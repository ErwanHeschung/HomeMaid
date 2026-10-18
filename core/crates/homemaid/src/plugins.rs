//! Plugin host (plugin-api 1.x): each WASM plugin runs sandboxed in its own thread. A plugin that
//! exports `invoke` (plugin-api 1.1) also offers one-shot actions.
//!
//! Commands are applied as soon as they arrive, then the plugin is refreshed to read the new state
//! back; in between, it is refreshed at the interval it asks for. A plugin that traps is marked offline and restarted with backoff.

use std::{
    collections::{BTreeMap, HashMap},
    fs, io,
    net::IpAddr,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, RecvTimeoutError},
    },
    thread,
    time::{Duration, Instant},
};

use uuid::Uuid;
use wasmtime::{
    Engine, Store, StoreLimits, StoreLimitsBuilder,
    component::{Component, HasSelf, Linker, ResourceTable},
};
use wasmtime_wasi::{WasiCtx, WasiCtxView, WasiView, sockets::SocketAddrUse};

use crate::{
    Core,
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
use wit_actions::PluginWithActions;

/// A running plugin, with or without actions; the calls they share go through [`call!`].
enum Instance {
    Plain(Plugin),
    Actions(PluginWithActions),
}

macro_rules! call {
    ($instance:expr, $method:ident($($arg:expr),*)) => {
        match $instance {
            Instance::Plain(p) => p.$method($($arg),*),
            Instance::Actions(p) => p.$method($($arg),*),
        }
    };
}

/// What a plugin's thread is asked to do.
enum Job {
    Apply(String, Command),
    Invoke { local_id: String, name: String, params: String, reply: tokio::sync::oneshot::Sender<Result<(), String>> },
}

const API_MAJOR: &str = "1.";
const LOCAL_NETWORK: &str = "local-network";
const MEMORY_LIMIT: usize = 64 << 20;
const EPOCH_TICK: Duration = Duration::from_millis(100);
/// Budget of one call, in epoch ticks (about 60 s). Epochs advance in wall-clock time, including
/// time blocked in socket calls, so it must leave room for slow devices; it only catches runaway plugins.
const CALL_BUDGET: u64 = 600;
const MIN_RESTART: Duration = Duration::from_secs(1);
const MAX_RESTART: Duration = Duration::from_secs(60);

type Secrets = BTreeMap<String, BTreeMap<String, String>>;

pub struct Plugins {
    senders: HashMap<String, mpsc::Sender<Job>>,
}

impl Plugins {
    /// Starts every configured plugin. One that cannot be loaded is logged and skipped.
    pub fn start(
        core: Arc<Core>,
        configs: Vec<PluginConfig>,
        data_dir: &Path,
        secrets: Secrets,
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
        let mut senders = HashMap::new();
        for cfg in configs {
            let (tx, rx) = mpsc::channel();
            let runner = Runner {
                engine: engine.clone(),
                core: core.clone(),
                secrets: secrets.get(&cfg.id).cloned().unwrap_or_default(),
                kv_path: kv_dir.join(format!("{}.kv.json", cfg.id)),
                cfg,
            };
            senders.insert(runner.cfg.id.clone(), tx);
            thread::Builder::new()
                .name(format!("plugin-{}", runner.cfg.id))
                .spawn(move || runner.supervise(&rx))
                .map_err(|e| e.to_string())?;
        }
        Ok(Self { senders })
    }

    /// Routes a reconciler command to the plugin driving its entity.
    pub fn dispatch(&self, core: &Core, command: Command) {
        let target = core.with(|r| r.entities.get(&command.entity).map(|e| (e.plugin.clone(), e.local_id.clone())));
        match target.and_then(|(plugin, local_id)| Some((self.senders.get(&plugin)?, local_id))) {
            // A stopped plugin drops the command; the reconciler fails it after its retries.
            Some((tx, local_id)) => drop(tx.send(Job::Apply(local_id, command))),
            None => tracing::warn!(?command, "no plugin for entity, command dropped"),
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
        let tx = self.senders.get(&plugin).ok_or_else(|| format!("plugin `{plugin}` is not running"))?;
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
}

impl Runner {
    fn supervise(&self, rx: &mpsc::Receiver<Job>) {
        // Compiled here rather than in `start`: it takes seconds and must not hold up the core.
        let component = match Component::from_file(&self.engine, &self.cfg.path) {
            Ok(component) => component,
            Err(e) => {
                tracing::error!(plugin = %self.cfg.id, "cannot load {}: {e:#}", self.cfg.path.display());
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
                    return;
                }
                Err(e) => {
                    tracing::error!(plugin = %self.cfg.id, "plugin failed, restart in {delay:?}: {e:?}");
                    self.set_offline();
                    thread::sleep(delay);
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
        // The 1.1 linker is a superset: it also satisfies 1.0 plugins.
        PluginWithActions::add_to_linker::<_, HasSelf<_>>(&mut linker, |state| state)?;
        let has_actions = component.component_type().exports(&self.engine).any(|(name, _)| name == "invoke");
        let plugin = if has_actions {
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
        let mut actions: HashMap<String, Vec<ActionDecl>> = HashMap::new();
        if let Instance::Actions(p) = &plugin {
            store.set_epoch_deadline(CALL_BUDGET);
            for a in p.call_list_actions(&mut store)? {
                let params = match a.params.as_deref().map(serde_json::from_str).transpose() {
                    Ok(params) => params,
                    Err(e) => {
                        tracing::warn!(plugin = %self.cfg.id, "{}: action `{}` ignored, invalid params schema: {e}", a.local_id, a.name);
                        continue;
                    }
                };
                actions.entry(a.local_id).or_default().push(ActionDecl { name: a.name, params });
            }
        }
        let entities: Vec<Entity> =
            discovered.into_iter().map(|e| {
                let mine = actions.remove(&e.local_id).unwrap_or_default();
                self.to_entity(e, mine)
            }).collect();
        tracing::info!(plugin = %self.cfg.id, version = %manifest.version, "started with {} entities", entities.len());
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
                        Instance::Plain(_) => Err("this plugin has no actions (plugin-api 1.0)".into()),
                    };
                    if let Err(e) = &result {
                        tracing::warn!(plugin = %self.cfg.id, %local_id, %name, "action failed: {e}");
                    }
                    let _ = reply.send(result);
                    // An action may change states (pause, lock): read them back.
                    next_refresh = Instant::now();
                }
                Err(RecvTimeoutError::Timeout) => {
                    store.set_epoch_deadline(CALL_BUDGET);
                    let ms = call!(&plugin, call_refresh(&mut store))?;
                    next_refresh = Instant::now() + Duration::from_millis(ms.into());
                }
                Err(RecvTimeoutError::Disconnected) => return Ok(Exit::Stopped),
            }
        }
    }

    fn store(&self, network: Arc<AtomicBool>) -> wasmtime::Result<Store<HostState>> {
        // No preopened dirs, env or args: the plugin only sees what the host interface gives it.
        let wasi = WasiCtx::builder()
            .inherit_stderr()
            .allow_tcp(true)
            .allow_udp(true)
            .allow_ip_name_lookup(false)
            .socket_addr_check(move |addr, usage| {
                let ok = network.load(Ordering::Relaxed) && is_local(addr.ip(), usage);
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
            limits: StoreLimitsBuilder::new().memory_size(MEMORY_LIMIT).build(),
            kv,
            kv_path: self.kv_path.clone(),
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
    kv_path: PathBuf,
    secrets: BTreeMap<String, String>,
}

impl WasiView for HostState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView { ctx: &mut self.wasi, table: &mut self.table }
    }
}

impl types::Host for HostState {}

impl wit_actions::homemaid::plugin::action_types::Host for HostState {}

impl host::Host for HostState {
    fn log(&mut self, level: types::LogLevel, message: String) {
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
        self.kv.insert(key, value);
        let saved = serde_json::to_vec(&self.kv).map_err(io::Error::from).and_then(|b| store::write_atomic(&self.kv_path, &b));
        if let Err(e) = saved {
            tracing::error!(plugin = %self.plugin, "cannot save {}: {e}", self.kv_path.display());
        }
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
        assert!(!is_local(ip("0.0.0.0"), SocketAddrUse::TcpConnect));
    }
}
