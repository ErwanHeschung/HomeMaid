//! Plugin host (plugin-api 1.x): each WASM plugin runs sandboxed in its own thread. A plugin that
//! exports `invoke` (plugin-api 1.1) also offers one-shot actions; one that exports
//! `set-schedules` (1.2) is handed the schedules its devices can run themselves.
//!
//! Commands are applied as soon as they arrive, then the plugin is refreshed to read the new state
//! back; in between, it is refreshed at the interval it asks for. A plugin that traps is marked offline and restarted with backoff.

use std::{
    collections::{BTreeMap, HashMap},
    fs, io,
    net::IpAddr,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, RecvTimeoutError},
    },
    thread,
    time::{Duration, Instant},
};

use serde::Serialize;
use uuid::Uuid;
use wasmtime::{
    Engine, Store, StoreLimits, StoreLimitsBuilder,
    component::{Component, HasSelf, Linker, ResourceTable},
};
use wasmtime_wasi::{WasiCtx, WasiCtxView, WasiView, sockets::SocketAddrUse};

use crate::{
    Core,
    automations::{Schedule, ScheduleTime},
    config::PluginConfig,
    model::{ActionDecl, CapKind, CapMeta, CapSlot, CapValue, Entity, entity_uuid},
    reconciler::Command,
    store,
};

mod wit {
    wasmtime::component::bindgen!({ path: "../../plugin-api/wit", world: "plugin" });
}

/// The 1.1 world: the same host and types, plus actions.
mod wit_actions {
    wasmtime::component::bindgen!({
        path: "../../plugin-api/wit",
        world: "plugin-with-actions",
        with: {
            "homemaid:plugin/types": super::wit::homemaid::plugin::types,
            "homemaid:plugin/host": super::wit::homemaid::plugin::host,
        },
    });
}

use wit::{
    Plugin,
    homemaid::plugin::{host, types},
};
/// The 1.2 world: the same again, plus schedules.
mod wit_schedules {
    wasmtime::component::bindgen!({
        path: "../../plugin-api/wit",
        world: "plugin-with-schedules",
        with: {
            "homemaid:plugin/types": super::wit::homemaid::plugin::types,
            "homemaid:plugin/host": super::wit::homemaid::plugin::host,
            "homemaid:plugin/action-types": super::wit_actions::homemaid::plugin::action_types,
        },
    });
}

use wit_actions::PluginWithActions;
use wit_schedules::{PluginWithSchedules, homemaid::plugin::schedule_types};

/// A running plugin of any 1.x world; the calls they share go through [`call!`].
enum Instance {
    Plain(Plugin),
    Actions(PluginWithActions),
    Schedules(PluginWithSchedules),
}

macro_rules! call {
    ($instance:expr, $method:ident($($arg:expr),*)) => {
        match $instance {
            Instance::Plain(p) => p.$method($($arg),*),
            Instance::Actions(p) => p.$method($($arg),*),
            Instance::Schedules(p) => p.$method($($arg),*),
        }
    };
}

/// What a plugin's thread is asked to do.
enum Job {
    Apply(String, Command),
    Invoke { local_id: String, name: String, params: String, reply: tokio::sync::oneshot::Sender<Result<(), String>> },
    /// Automations or entities changed: hand the devices their schedules again.
    Reschedule,
}

pub const API_MAJOR: &str = "1.";
/// The newest 1.x this host implements.
pub const API_MINOR: u32 = 2;
const LOCAL_NETWORK: &str = "local-network";
/// Granted in the config only (`permissions`), for a plugin whose device runs on this PC (`pc`):
/// the core, the agent and their pairing pages listen on loopback.
const LOOPBACK: &str = "loopback";
const MEMORY_LIMIT: usize = 64 << 20;
/// Total size of a plugin's KV store (keys and values).
const KV_LIMIT: usize = 1 << 20;
/// Log lines a plugin may write per minute; more are counted and dropped.
const LOG_LIMIT: u32 = 60;
const LOG_LINE: usize = 1000;
const EPOCH_TICK: Duration = Duration::from_millis(100);
/// Budget of one call, in epoch ticks (about 60 s). Epochs advance in wall-clock time, including
/// time blocked in socket calls, so it must leave room for slow devices; it only catches runaway plugins.
const CALL_BUDGET: u64 = 600;
const MIN_RESTART: Duration = Duration::from_secs(1);
const MAX_RESTART: Duration = Duration::from_secs(60);

type Secrets = BTreeMap<String, BTreeMap<String, String>>;

pub struct Plugins {
    engine: Engine,
    core: Arc<Core>,
    kv_dir: PathBuf,
    running: Mutex<HashMap<String, Running>>,
}

struct Running {
    jobs: mpsc::Sender<Job>,
    thread: thread::JoinHandle<()>,
    state: Arc<Mutex<PluginState>>,
}

/// How a plugin is doing, for `plugins.list`.
#[derive(Debug, Clone, Default, Serialize)]
pub struct PluginState {
    /// From its manifest, once started.
    pub version: Option<String>,
    /// Why it is not running (cannot load, refused, crashed); `None` once started again.
    pub error: Option<String>,
}

impl Plugins {
    /// Starts every configured plugin. One that cannot be loaded is logged and skipped.
    pub fn start(
        core: Arc<Core>,
        configs: Vec<PluginConfig>,
        data_dir: &Path,
        mut secrets: Secrets,
    ) -> Result<Self, String> {
        let mut config = wasmtime::Config::new();
        config.epoch_interruption(true);
        let engine = Engine::new(&config).map_err(|e| e.to_string())?;
        let ticker = engine.clone();
        thread::spawn(move || {
            loop {
                thread::sleep(EPOCH_TICK);
                ticker.increment_epoch();
            }
        });

        let kv_dir = data_dir.join("plugins");
        fs::create_dir_all(&kv_dir).map_err(|e| format!("{}: {e}", kv_dir.display()))?;
        let plugins = Self { engine, core, kv_dir, running: Mutex::default() };
        for cfg in configs {
            let secrets = secrets.remove(&cfg.id).unwrap_or_default();
            plugins.spawn(cfg, secrets)?;
        }
        Ok(plugins)
    }

    /// Starts one more plugin, or restarts it with a new config. Blocks while the old instance
    /// finishes its call (60 s at worst), so the two never write its KV store at once.
    pub fn spawn(&self, cfg: PluginConfig, secrets: BTreeMap<String, String>) -> Result<(), String> {
        self.stop(&cfg.id);
        let (jobs, rx) = mpsc::channel();
        let state = Arc::new(Mutex::new(PluginState::default()));
        let runner = Runner {
            engine: self.engine.clone(),
            core: self.core.clone(),
            secrets,
            kv_path: self.kv_dir.join(format!("{}.kv.json", cfg.id)),
            state: state.clone(),
            cfg,
        };
        let id = runner.cfg.id.clone();
        let thread = thread::Builder::new()
            .name(format!("plugin-{id}"))
            .spawn(move || runner.supervise(&rx))
            .map_err(|e| e.to_string())?;
        self.running.lock().unwrap().insert(id, Running { jobs, thread, state });
        Ok(())
    }

    /// Stops a plugin, waiting for its thread (see [`Plugins::spawn`]). Its entities go offline.
    pub fn stop(&self, id: &str) {
        let Some(running) = self.running.lock().unwrap().remove(id) else { return };
        drop(running.jobs);
        let _ = running.thread.join();
    }

    /// Stops a plugin for good: its entities and KV store go too.
    pub fn remove(&self, id: &str) {
        self.stop(id);
        self.core.with(|r| r.entities.retain(|_, e| e.plugin != id));
        self.core.dirty.notify_one();
        let _ = fs::remove_file(self.kv_dir.join(format!("{id}.kv.json")));
    }

    /// The running plugins and how they are doing, by id.
    pub fn states(&self) -> BTreeMap<String, PluginState> {
        self.running.lock().unwrap().iter().map(|(id, r)| (id.clone(), r.state.lock().unwrap().clone())).collect()
    }

    fn sender(&self, plugin: &str) -> Option<mpsc::Sender<Job>> {
        self.running.lock().unwrap().get(plugin).map(|r| r.jobs.clone())
    }

    /// Routes a reconciler command to the plugin driving its entity.
    pub fn dispatch(&self, core: &Core, command: Command) {
        let target = core.with(|r| r.entities.get(&command.entity).map(|e| (e.plugin.clone(), e.local_id.clone())));
        match target.and_then(|(plugin, local_id)| Some((self.sender(&plugin)?, local_id))) {
            // A stopped plugin drops the command; the reconciler fails it after its retries.
            Some((tx, local_id)) => drop(tx.send(Job::Apply(local_id, command))),
            None => tracing::warn!(?command, "no plugin for entity, command dropped"),
        }
    }

    /// Every plugin hands its devices their schedules again, as they now are.
    pub fn reschedule(&self) {
        for running in self.running.lock().unwrap().values() {
            let _ = running.jobs.send(Job::Reschedule);
        }
    }

    /// Runs one of an entity's declared actions now. `params` is JSON (`{}` for none).
    pub async fn invoke(&self, core: &Core, entity: Uuid, action: &str, params: serde_json::Value) -> Result<(), String> {
        let (plugin, local_id, decl) = core.with(|r| {
            let e = r.entities.get(&entity).ok_or("unknown entity")?;
            if !e.available {
                return Err(format!("{} is offline", e.name));
            }
            let decl = e.actions.iter().find(|a| a.name == action).cloned();
            let decl = decl.ok_or_else(|| format!("{} has no action `{action}`", e.name))?;
            Ok((e.plugin.clone(), e.local_id.clone(), decl))
        })?;
        let empty = params.is_null() || params.as_object().is_some_and(|o| o.is_empty());
        if decl.params.is_none() && !empty {
            return Err(format!("`{action}` takes no parameters"));
        }
        // ponytail: params are not checked against their JSON schema; add a validator when a plugin needs it.
        let tx = self.sender(&plugin).ok_or_else(|| format!("plugin `{plugin}` is not running"))?;
        let (reply, answer) = tokio::sync::oneshot::channel();
        let params = if params.is_null() { "{}".into() } else { params.to_string() };
        tx.send(Job::Invoke { local_id, name: action.into(), params, reply }).map_err(|_| format!("plugin `{plugin}` stopped"))?;
        match tokio::time::timeout(Duration::from_secs(70), answer).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(format!("plugin `{plugin}` stopped")),
            Err(_) => Err(format!("plugin `{plugin}` did not answer")),
        }
    }
}

enum Exit {
    Stopped,
    Disabled(String),
}

struct Runner {
    cfg: PluginConfig,
    engine: Engine,
    core: Arc<Core>,
    secrets: BTreeMap<String, String>,
    kv_path: PathBuf,
    state: Arc<Mutex<PluginState>>,
}

impl Runner {
    fn supervise(&self, rx: &mpsc::Receiver<Job>) {
        // Compiled here rather than in `start`: it takes seconds and must not hold up the core.
        let component = match Component::from_file(&self.engine, &self.cfg.path) {
            Ok(component) => component,
            Err(e) => {
                tracing::error!(plugin = %self.cfg.id, "cannot load {}: {e:#}", self.cfg.path.display());
                self.failed(format!("cannot load {}: {e:#}", self.cfg.path.display()));
                return;
            }
        };
        let mut delay = MIN_RESTART;
        loop {
            match self.run(&component, rx, &mut delay) {
                Ok(Exit::Stopped) => return,
                Ok(Exit::Disabled(reason)) => {
                    tracing::error!(plugin = %self.cfg.id, "plugin disabled: {reason}");
                    self.set_offline();
                    self.failed(reason);
                    return;
                }
                Err(e) => {
                    tracing::error!(plugin = %self.cfg.id, "plugin failed, restart in {delay:?}: {e:?}");
                    self.set_offline();
                    self.failed(format!("{e:#}"));
                    // Waits on the jobs to stop at once when told to. A job arriving meanwhile is
                    // dropped: the reconciler retries, and a restart sends the desired state anyway.
                    if let Err(RecvTimeoutError::Disconnected) = rx.recv_timeout(delay) {
                        return;
                    }
                    delay = (delay * 2).min(MAX_RESTART);
                }
            }
        }
    }

    fn run(&self, component: &Component, rx: &mpsc::Receiver<Job>, delay: &mut Duration) -> wasmtime::Result<Exit> {
        let network = Arc::new(AtomicBool::new(false));
        let mut store = self.store(network.clone())?;
        let mut linker = Linker::new(&self.engine);
        wasmtime_wasi::p2::add_to_linker_sync(&mut linker)?;
        // The 1.2 linker is a superset: it also satisfies 1.0 and 1.1 plugins.
        PluginWithSchedules::add_to_linker::<_, HasSelf<_>>(&mut linker, |state| state)?;
        let exports = |export: &str| component.component_type().exports(&self.engine).any(|(name, _)| name == export);
        let plugin = if exports("set-schedules") {
            Instance::Schedules(PluginWithSchedules::instantiate(&mut store, component, &linker)?)
        } else if exports("invoke") {
            Instance::Actions(PluginWithActions::instantiate(&mut store, component, &linker)?)
        } else {
            Instance::Plain(Plugin::instantiate(&mut store, component, &linker)?)
        };

        store.set_epoch_deadline(CALL_BUDGET);
        let manifest = call!(&plugin, call_manifest(&mut store))?;
        if let Err(reason) = check_manifest(&manifest, &self.cfg) {
            return Ok(Exit::Disabled(reason));
        }
        // Granted only now: the manifest is read from a running instance.
        network.store(manifest.permissions.iter().any(|p| p == LOCAL_NETWORK), Ordering::Relaxed);

        let settings: Vec<(String, String)> = self.cfg.settings.clone().into_iter().collect();
        store.set_epoch_deadline(CALL_BUDGET);
        call!(&plugin, call_init(&mut store, &settings))?.map_err(|e| wasmtime::Error::msg(format!("init: {e}")))?;
        store.set_epoch_deadline(CALL_BUDGET);
        let discovered = call!(&plugin, call_discover(&mut store))?.map_err(|e| wasmtime::Error::msg(format!("discover: {e}")))?;
        store.data_mut().save_kv();
        let mut actions: HashMap<String, Vec<ActionDecl>> = HashMap::new();
        store.set_epoch_deadline(CALL_BUDGET);
        let declared = match &plugin {
            Instance::Plain(_) => vec![],
            Instance::Actions(p) => p.call_list_actions(&mut store)?,
            Instance::Schedules(p) => p.call_list_actions(&mut store)?,
        };
        for a in declared {
            let params = match a.params.as_deref().map(serde_json::from_str).transpose() {
                Ok(params) => params,
                Err(e) => {
                    tracing::warn!(plugin = %self.cfg.id, "{}: action `{}` ignored, invalid params schema: {e}", a.local_id, a.name);
                    continue;
                }
            };
            actions.entry(a.local_id).or_default().push(ActionDecl { name: a.name, params });
        }
        let entities: Vec<Entity> =
            discovered.into_iter().map(|e| {
                let mine = actions.remove(&e.local_id).unwrap_or_default();
                self.to_entity(e, mine)
            }).collect();
        tracing::info!(plugin = %self.cfg.id, version = %manifest.version, "started with {} entities", entities.len());
        *self.state.lock().unwrap() = PluginState { version: Some(manifest.version.clone()), error: None };
        let _ = self.core.changes.send(crate::Event::Plugins);
        if let Instance::Schedules(p) = &plugin {
            self.hand_schedules(p, &mut store, &entities)?;
        }
        let now = Instant::now();
        self.core.with(|r| entities.into_iter().for_each(|e| r.upsert(e, now)));
        *delay = MIN_RESTART;

        let mut next_refresh = Instant::now();
        loop {
            match rx.recv_timeout(next_refresh.saturating_duration_since(Instant::now())) {
                Ok(Job::Apply(local_id, command)) => {
                    let value = serde_json::to_string(&command.value)?;
                    store.set_epoch_deadline(CALL_BUDGET);
                    if let Err(e) = call!(&plugin, call_apply(&mut store, &local_id, &command.cap.to_string(), &value))? {
                        tracing::warn!(plugin = %self.cfg.id, ?command, "apply failed: {e}");
                    }
                    // Refresh right after, to read back the new state; queued commands are applied first.
                    next_refresh = Instant::now();
                }
                Ok(Job::Invoke { local_id, name, params, reply }) => {
                    let result = match &plugin {
                        Instance::Actions(p) => {
                            store.set_epoch_deadline(CALL_BUDGET);
                            p.call_invoke(&mut store, &local_id, &name, &params)?
                        }
                        Instance::Schedules(p) => {
                            store.set_epoch_deadline(CALL_BUDGET);
                            p.call_invoke(&mut store, &local_id, &name, &params)?
                        }
                        Instance::Plain(_) => Err("this plugin has no actions (plugin-api 1.0)".into()),
                    };
                    if let Err(e) = &result {
                        tracing::warn!(plugin = %self.cfg.id, %local_id, %name, "action failed: {e}");
                    }
                    store.data_mut().save_kv();
                    let _ = reply.send(result);
                    // An action may change states (pause, lock): read them back.
                    next_refresh = Instant::now();
                }
                Ok(Job::Reschedule) => {
                    if let Instance::Schedules(p) = &plugin {
                        let mine: Vec<Entity> = self.core.with(|r| r.entities.values().filter(|e| e.plugin == self.cfg.id).cloned().collect());
                        self.hand_schedules(p, &mut store, &mine)?;
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    store.set_epoch_deadline(CALL_BUDGET);
                    let ms = call!(&plugin, call_refresh(&mut store))?;
                    next_refresh = Instant::now() + Duration::from_millis(ms.into());
                }
                Err(RecvTimeoutError::Disconnected) => return Ok(Exit::Stopped),
            }
            // Once per call, not per kv-set: a plugin writing in a loop rewrites the file once.
            store.data_mut().save_kv();
        }
    }

    /// Gives each entity the schedules its device can run alone (none clears the old ones), so
    /// they keep happening while the hub is off. At start, and again on [`Job::Reschedule`].
    fn hand_schedules(&self, plugin: &PluginWithSchedules, store: &mut Store<HostState>, entities: &[Entity]) -> wasmtime::Result<()> {
        let automations = self.core.automations();
        for entity in entities {
            let schedules = automations.schedules_for(entity);
            let wit: Vec<schedule_types::Schedule> = schedules.iter().map(to_wit).collect();
            store.set_epoch_deadline(CALL_BUDGET);
            let result = plugin.call_set_schedules(&mut *store, &entity.local_id, &wit)?;
            match &result {
                Ok(()) if !schedules.is_empty() => {
                    tracing::info!(plugin = %self.cfg.id, "{}: {} schedules run on the device too", entity.name, schedules.len());
                }
                Ok(()) => {}
                Err(e) => tracing::warn!(plugin = %self.cfg.id, "{}: schedules stay on the hub only: {e}", entity.name),
            }
            automations.set_delegated(entity.id, &schedules, result.is_ok());
        }
        store.data_mut().save_kv();
        Ok(())
    }

    fn store(&self, network: Arc<AtomicBool>) -> wasmtime::Result<Store<HostState>> {
        let loopback = self.cfg.permissions.iter().any(|p| p == LOOPBACK);
        // No preopened dirs, env or args: the plugin only sees what the host interface gives it.
        let wasi = WasiCtx::builder()
            .inherit_stderr()
            .allow_tcp(true)
            .allow_udp(true)
            .allow_ip_name_lookup(false)
            .socket_addr_check(move |addr, usage| {
                let ip = addr.ip();
                let ok = network.load(Ordering::Relaxed) && is_local(ip, usage) && (loopback || !ip.to_canonical().is_loopback());
                Box::pin(async move { ok })
            })
            .build();
        let kv = match fs::read(&self.kv_path) {
            Ok(bytes) => serde_json::from_slice(&bytes)?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => BTreeMap::new(),
            Err(e) => return Err(e.into()),
        };
        let state = HostState {
            plugin: self.cfg.id.clone(),
            core: self.core.clone(),
            wasi,
            table: ResourceTable::new(),
            // The memory limit is per linear memory: cap their number too (and instances, tables).
            limits: StoreLimitsBuilder::new()
                .memory_size(MEMORY_LIMIT)
                .memories(2)
                .instances(64)
                .tables(64)
                .table_elements(100_000)
                .build(),
            kv,
            kv_dirty: false,
            kv_path: self.kv_path.clone(),
            logs: (Instant::now(), 0),
            secrets: self.secrets.clone(),
        };
        let mut store = Store::new(&self.engine, state);
        store.limiter(|state| &mut state.limits);
        // The deadline defaults to 0, which would interrupt instantiation itself.
        store.set_epoch_deadline(CALL_BUDGET);
        Ok(store)
    }

    fn to_entity(&self, e: types::Entity, actions: Vec<ActionDecl>) -> Entity {
        let id = entity_uuid(&self.cfg.id, &e.local_id);
        let caps = e
            .capabilities
            .into_iter()
            .filter_map(|c| {
                let parsed = c.kind.parse::<CapKind>().and_then(|kind| {
                    serde_json::from_str::<CapMeta>(&c.meta).map(|meta| (kind, meta)).map_err(|e| e.to_string())
                });
                match parsed {
                    Ok((kind, meta)) => Some((kind, CapSlot { meta, ..Default::default() })),
                    Err(err) => {
                        tracing::warn!(plugin = %self.cfg.id, "{}: capability `{}` ignored: {err}", e.local_id, c.kind);
                        None
                    }
                }
            })
            .collect();
        let mut entity = Entity {
            id,
            name: e.name,
            room: None,
            tags: vec![],
            available: true,
            plugin: self.cfg.id.clone(),
            local_id: e.local_id,
            device: e.device,
            model: None,
            actions,
            caps,
        };
        if let Some(o) = self.core.overrides.lock().unwrap().get(&id) {
            o.apply(&mut entity);
        }
        entity
    }

    fn failed(&self, error: String) {
        self.state.lock().unwrap().error = Some(error);
        let _ = self.core.changes.send(crate::Event::Plugins);
    }

    fn set_offline(&self) {
        let now = Instant::now();
        self.core.with(|r| {
            let ids: Vec<Uuid> = r.entities.values().filter(|e| e.plugin == self.cfg.id).map(|e| e.id).collect();
            for id in ids {
                r.set_available(id, false, now);
            }
        });
    }
}

fn to_wit(s: &Schedule) -> schedule_types::Schedule {
    schedule_types::Schedule {
        id: s.automation.clone(),
        kind: s.cap.to_string(),
        value: serde_json::to_string(&s.value).expect("CapValue always serializes"),
        at: match s.at {
            ScheduleTime::Clock(m) => schedule_types::TimeOfDay::Clock(m),
            ScheduleTime::Sunrise(o) => schedule_types::TimeOfDay::Sunrise(o),
            ScheduleTime::Sunset(o) => schedule_types::TimeOfDay::Sunset(o),
        },
        days: s.days,
    }
}

fn check_manifest(manifest: &types::Manifest, cfg: &PluginConfig) -> Result<(), String> {
    if manifest.id != cfg.id {
        return Err(format!("manifest id `{}` does not match the configured id", manifest.id));
    }
    if !manifest.api_version.starts_with(API_MAJOR) {
        return Err(format!("plugin-api {} is not supported, this core implements 1.x", manifest.api_version));
    }
    let missing: Vec<&String> = manifest.permissions.iter().filter(|p| !cfg.permissions.contains(p)).collect();
    if !missing.is_empty() {
        return Err(format!("permissions not granted: {missing:?} (add them to `permissions` in the config to accept)"));
    }
    Ok(())
}

/// Local network only: private, loopback, link-local, broadcast and link-local multicast (mDNS).
/// Binds may use the wildcard address, the OS then picks a local interface.
fn is_local(ip: IpAddr, usage: SocketAddrUse) -> bool {
    if ip.is_unspecified() {
        return matches!(usage, SocketAddrUse::TcpBind | SocketAddrUse::TcpListen | SocketAddrUse::UdpBind);
    }
    match ip {
        IpAddr::V4(ip) => {
            ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_broadcast()
                || (ip.is_multicast() && ip.octets()[..3] == [224, 0, 0])
        }
        IpAddr::V6(ip) => match ip.to_ipv4_mapped() {
            Some(v4) => is_local(IpAddr::V4(v4), usage),
            None => {
                ip.is_loopback()
                    || ip.is_unique_local()
                    || ip.is_unicast_link_local()
                    || (ip.is_multicast() && ip.segments()[0] == 0xff02)
            }
        },
    }
}

struct HostState {
    plugin: String,
    core: Arc<Core>,
    wasi: WasiCtx,
    table: ResourceTable,
    limits: StoreLimits,
    kv: BTreeMap<String, String>,
    kv_dirty: bool,
    kv_path: PathBuf,
    secrets: BTreeMap<String, String>,
    /// Start of the current minute, and lines logged in it.
    logs: (Instant, u32),
}

impl HostState {
    fn save_kv(&mut self) {
        if !std::mem::take(&mut self.kv_dirty) {
            return;
        }
        let saved = serde_json::to_vec(&self.kv).map_err(io::Error::from).and_then(|b| store::write_atomic(&self.kv_path, &b));
        if let Err(e) = saved {
            tracing::error!(plugin = %self.plugin, "cannot save {}: {e}", self.kv_path.display());
        }
    }

    /// Lets through [`LOG_LIMIT`] lines a minute, and says how many were dropped.
    fn may_log(&mut self) -> bool {
        let (start, count) = &mut self.logs;
        if start.elapsed() >= Duration::from_secs(60) {
            if *count > LOG_LIMIT {
                tracing::warn!(plugin = %self.plugin, "{} log lines dropped (more than {LOG_LIMIT} a minute)", *count - LOG_LIMIT);
            }
            *start = Instant::now();
            *count = 0;
        }
        *count += 1;
        *count <= LOG_LIMIT
    }
}

/// What a plugin stored before trapping is kept too.
impl Drop for HostState {
    fn drop(&mut self) {
        self.save_kv();
    }
}

fn kv_size(kv: &BTreeMap<String, String>) -> usize {
    kv.iter().map(|(k, v)| k.len() + v.len()).sum()
}

impl WasiView for HostState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView { ctx: &mut self.wasi, table: &mut self.table }
    }
}

impl types::Host for HostState {}

impl wit_actions::homemaid::plugin::action_types::Host for HostState {}

impl schedule_types::Host for HostState {}

impl host::Host for HostState {
    fn log(&mut self, level: types::LogLevel, mut message: String) {
        if !self.may_log() {
            return;
        }
        if message.len() > LOG_LINE {
            message.truncate(message.floor_char_boundary(LOG_LINE));
            message.push('…');
        }
        let plugin = &self.plugin;
        match level {
            types::LogLevel::Trace => tracing::trace!(%plugin, "{message}"),
            types::LogLevel::Debug => tracing::debug!(%plugin, "{message}"),
            types::LogLevel::Info => tracing::info!(%plugin, "{message}"),
            types::LogLevel::Warn => tracing::warn!(%plugin, "{message}"),
            types::LogLevel::Error => tracing::error!(%plugin, "{message}"),
        }
    }

    fn report_state(&mut self, local_id: String, kind: String, value: String) {
        let parsed = kind.parse::<CapKind>().and_then(|kind| {
            serde_json::from_str::<CapValue>(&value).map(|value| (kind, value)).map_err(|e| e.to_string())
        });
        match parsed {
            Ok((kind, value)) => {
                let id = entity_uuid(&self.plugin, &local_id);
                self.core.with(|r| r.observe(id, &kind, value));
            }
            Err(e) => tracing::warn!(plugin = %self.plugin, "invalid report-state for {local_id}: {e}"),
        }
    }

    fn set_available(&mut self, local_id: String, available: bool) {
        let id = entity_uuid(&self.plugin, &local_id);
        self.core.with(|r| r.set_available(id, available, Instant::now()));
    }

    fn kv_get(&mut self, key: String) -> Option<String> {
        self.kv.get(&key).cloned()
    }

    fn kv_set(&mut self, key: String, value: String) {
        let old = self.kv.get(&key).map_or(0, |v| key.len() + v.len());
        if kv_size(&self.kv) - old + key.len() + value.len() > KV_LIMIT {
            tracing::error!(plugin = %self.plugin, "kv-set `{key}` refused: the store would exceed {KV_LIMIT} bytes");
            return;
        }
        self.kv.insert(key, value);
        self.kv_dirty = true;
    }

    fn secret(&mut self, name: String) -> Option<String> {
        self.secrets.get(&name).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_local_addresses_are_reachable() {
        let ip = |s: &str| s.parse::<IpAddr>().unwrap();
        for local in ["192.168.1.20", "10.0.0.1", "172.16.5.4", "127.0.0.1", "169.254.1.1", "255.255.255.255", "224.0.0.251", "::1", "fd00::1", "fe80::1", "ff02::fb", "::ffff:192.168.1.2"] {
            assert!(is_local(ip(local), SocketAddrUse::TcpConnect), "{local} should be allowed");
        }
        for public in ["1.1.1.1", "8.8.8.8", "100.64.0.1", "239.1.2.3", "2606:4700::1111", "::ffff:8.8.8.8"] {
            assert!(!is_local(ip(public), SocketAddrUse::UdpSend), "{public} should be refused");
        }
        assert!(is_local(ip("0.0.0.0"), SocketAddrUse::UdpBind));
        assert!(ip("::ffff:127.0.0.1").to_canonical().is_loopback(), "mapped loopback needs `loopback` too");
        assert!(!is_local(ip("0.0.0.0"), SocketAddrUse::TcpConnect));
    }
}
