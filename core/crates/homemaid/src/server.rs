//! Running the hub (core) and the PC agent. The `homemaid` binary runs them from its flags
//! ([`run`]); the desktop client drives them through a [`Handle`], and knows nothing else of them.

use std::{
    fs::{self, File},
    future::Future,
    io,
    net::SocketAddr,
    panic::AssertUnwindSafe,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
    thread,
    time::{Duration, Instant},
};

use serde::Serialize;
use tokio::sync::{mpsc, oneshot, watch};

use crate::{Core, agent, api, automations::{Automations, Location}, config, pairing, plugins::Plugins, reconciler::Reconciler, store};

/// Logs to `file` (appended), or to the console.
pub fn init_logging(file: Option<&Path>) -> Result<(), String> {
    match file {
        Some(path) => {
            let file = LogFile::open(path.into()).map_err(|e| format!("{}: {e}", path.display()))?;
            tracing_subscriber::fmt().with_writer(Mutex::new(file)).with_ansi(false).init();
        }
        None => tracing_subscriber::fmt::init(),
    }
    // Into the log: the desktop exe has no console to print them to.
    std::panic::set_hook(Box::new(|info| tracing::error!("{info}")));
    Ok(())
}

/// Past this size the log is moved to `<name>.old` (replacing the previous one) and starts over.
const LOG_SIZE: u64 = 10 << 20;

/// An appended log file that never grows past twice [`LOG_SIZE`] on disk.
struct LogFile {
    path: PathBuf,
    file: File,
    size: u64,
}

impl LogFile {
    fn open(path: PathBuf) -> io::Result<Self> {
        let file = File::options().create(true).append(true).open(&path)?;
        let size = file.metadata()?.len();
        Ok(Self { path, file, size })
    }
}

impl io::Write for LogFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.size > LOG_SIZE {
            let mut old = self.path.clone().into_os_string();
            old.push(".old");
            fs::rename(&self.path, old)?;
            *self = Self::open(self.path.clone())?;
        }
        let n = self.file.write(buf)?;
        self.size += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

/// The agent switch of `--agent`: always on.
pub fn always_on() -> watch::Receiver<bool> {
    watch::channel(true).1
}

/// Runs the requested roles until `shutdown` completes, then stops them cleanly. The agent runs
/// while its switch is on (with --agent it stays on).
pub async fn run(
    config_path: PathBuf,
    run_core: bool,
    agent_switch: Option<watch::Receiver<bool>>,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> Result<(), String> {
    serve(config_path, run_core, agent_switch, shutdown, Arc::default(), Arc::default()).await
}

async fn serve(
    config_path: PathBuf,
    run_core: bool,
    agent_switch: Option<watch::Receiver<bool>>,
    shutdown: impl Future<Output = ()> + Send + 'static,
    published: Arc<OnceLock<Arc<Core>>>,
    agent_restart: Arc<tokio::sync::Notify>,
) -> Result<(), String> {
    if !run_core && agent_switch.is_none() {
        return Err("nothing to run: pass --core and/or --agent".into());
    }
    let mut config = config::load(&config_path)?;
    let (stop, stopped) = watch::channel(());
    tokio::spawn(async move {
        shutdown.await;
        let _ = stop.send(());
    });
    let until_stopped = || {
        let mut stopped = stopped.clone();
        async move {
            let _ = stopped.changed().await;
        }
    };

    let core = async {
        if !run_core {
            return Ok(());
        }
        let core_config = config.core.take().ok_or("--core needs a [core] section in the config")?;
        // Checked before anything starts: a broken automation is a config error.
        let location = match (core_config.latitude, core_config.longitude) {
            (Some(latitude), Some(longitude)) => Some(Location { latitude, longitude }),
            (None, None) => None,
            _ => return Err(format!("{}: [core] needs both latitude and longitude", config_path.display())),
        };
        let automations = Automations::new(&config.automations, location).map_err(|e| format!("{}: {e}", config_path.display()))?;
        start_core(core_config, &config_path, config.entities, config.rooms, config.plugins, automations, until_stopped(), &published).await
    };
    let agent = async {
        let Some(mut switch) = agent_switch else { return Ok(()) };
        let mut agent_config = config.agent.take().ok_or("--agent needs an [agent] section in the config")?;
        let manage = agent::Manage { config_path: config_path.clone(), restart: agent_restart };
        loop {
            if *switch.borrow_and_update() {
                // Read again at each start: the phone or the window may have changed it.
                match config::load(&config_path).map(|c| c.agent) {
                    Ok(Some(fresh)) => agent_config = fresh,
                    Ok(None) => tracing::warn!("no [agent] section any more: the agent keeps the one it had"),
                    Err(e) => tracing::warn!("{e}: the agent keeps the config it had"),
                }
                // Until switched off (a dropped switch never is) or stopped.
                let mut off = switch.clone();
                let switched_off = async move {
                    loop {
                        if off.changed().await.is_err() {
                            std::future::pending::<()>().await;
                        }
                        if !*off.borrow() {
                            break;
                        }
                    }
                };
                let stop = until_stopped();
                let restarted = agent::run(agent_config.clone(), Some(manage.clone()), async move {
                    tokio::select! { () = stop => {}, () = switched_off => {} }
                })
                .await?;
                tracing::info!("agent stopped{}", if restarted { ", restarting with its new config" } else { "" });
                if restarted {
                    continue;
                }
            } else {
                tokio::select! {
                    changed = switch.changed() => if changed.is_err() { return Ok(()) },
                    () = until_stopped() => {}
                }
            }
            if stopped.has_changed().unwrap_or(true) {
                return Ok(());
            }
        }
    };
    tokio::try_join!(core, agent).map(|_| ())
}

// The pieces of the config the hub needs, plus where to publish it: grouping them would only rename them.
#[allow(clippy::too_many_arguments)]
async fn start_core(
    config: config::CoreConfig,
    config_path: &Path,
    overrides: Vec<config::EntityOverride>,
    rooms: Vec<config::RoomConfig>,
    plugins: Vec<config::PluginConfig>,
    automations: Automations,
    shutdown: impl Future<Output = ()> + Send + 'static,
    published: &OnceLock<Arc<Core>>,
) -> Result<(), String> {
    let data_dir = &config.data_dir;
    std::fs::create_dir_all(data_dir).map_err(|e| format!("{}: {e}", data_dir.display()))?;
    let snapshot_path = data_dir.join("state.json");
    let snapshot = store::load(&snapshot_path).map_err(|e| format!("{}: {e}", snapshot_path.display()))?;

    let secrets = config::load_secrets(&data_dir.join("secrets.toml"))?;

    let (commands, mut command_rx) = mpsc::unbounded_channel();
    let entities = store::restore(snapshot, &overrides);
    let core = Arc::new(Core::new(Reconciler::new(entities), commands));
    let _ = published.set(core.clone());
    *core.overrides.lock().unwrap() = overrides.into_iter().map(|o| (o.id, o)).collect();
    *core.rooms.lock().unwrap() = rooms.into_iter().filter_map(|r| Some((r.name, r.kind.filter(|k| !k.is_empty())?))).collect();

    // Before the plugins: they hand the automations' schedules to their devices at start.
    core.set_automations(automations);
    let plugins = Arc::new(Plugins::start(core.clone(), plugins, data_dir, secrets)?);
    let _ = core.plugins.set(plugins.clone());
    let router = core.clone();
    tokio::spawn(async move {
        while let Some(command) = command_rx.recv().await {
            plugins.dispatch(&router, command);
        }
    });


    let ticker = core.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        loop {
            interval.tick().await;
            ticker.with(|r| r.tick(Instant::now()));
        }
    });

    let (saver, path) = (core.clone(), snapshot_path.clone());
    tokio::spawn(async move {
        loop {
            saver.dirty.notified().await;
            // At most one write per 5 s. A clean stop saves anyway.
            tokio::time::sleep(Duration::from_secs(5)).await;
            save(&saver, &path);
        }
    });

    let listener = tokio::net::TcpListener::bind(config.bind).await.map_err(|e| format!("{}: {e}", config.bind))?;
    tracing::info!("core listening on {}", config.bind);
    let _announcement = pairing::announce("core", config.bind.port());
    tracing::info!("to pair the app, open http://localhost:{}/pair on this PC", config.bind.port());
    let app = api::router(core.clone(), &config, config_path);
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(shutdown)
        .await
        .map_err(|e| e.to_string())?;
    save(&core, &snapshot_path);
    Ok(())
}

fn save(core: &Core, path: &Path) {
    let mut entities = core.with(|r| r.entities.clone());
    // Sensors are read again at start; and a window title has no business on disk.
    for slot in entities.values_mut().flat_map(|e| e.caps.values_mut()).filter(|s| s.meta.read_only) {
        slot.observed = None;
    }
    if let Err(e) = store::save(path, &entities) {
        tracing::error!("cannot save {}: {e}", path.display());
    }
}


/// A running server, driven from another thread (the desktop client).
pub struct Handle {
    config_path: PathBuf,
    core: Arc<OnceLock<Arc<Core>>>,
    core_bind: SocketAddr,
    data_dir: PathBuf,
    agent: Option<(watch::Sender<bool>, u16)>,
    /// Restarts the agent so it reads its config again.
    agent_restart: Arc<tokio::sync::Notify>,
    /// The Start menu's apps offered to the phone (`[agent] discover_apps`).
    discover: std::sync::atomic::AtomicBool,
    qr: (String, Option<String>),
    stop: Mutex<Option<oneshot::Sender<()>>>,
}

/// What the desktop window shows of the server.
#[derive(Debug, Serialize)]
pub struct Status {
    /// The hub is up (plugins started, API listening soon).
    pub ready: bool,
    pub devices: usize,
    pub online: usize,
    /// Where the phone reaches the hub: LAN IP and port.
    pub address: String,
    pub host: String,
    pub agent: Option<AgentStatus>,
}

#[derive(Debug, Serialize)]
pub struct AgentStatus {
    pub on: bool,
    pub port: u16,
    /// The PC's apps offered to the phone.
    pub discover: bool,
}

impl Handle {
    /// Starts the hub, and the agent if the config has one (as its `enabled` says), on a thread
    /// and runtime of their own. `stopped` is called once both are down, with how they ended.
    pub fn spawn(config_path: PathBuf, stopped: impl FnOnce(Result<(), String>) + Send + 'static) -> Result<Self, String> {
        let config = config::load(&config_path)?;
        let core = config.core.as_ref().ok_or_else(|| format!("{}: no [core] section", config_path.display()))?;
        let qr = (
            pairing::qr_svg("core", core.bind, &core.token),
            config.agent.as_ref().map(|a| pairing::qr_svg("agent", a.bind, &a.token)),
        );
        let agent = config.agent.as_ref().map(|a| (watch::channel(a.enabled), a.bind.port()));
        let discover = config.agent.as_ref().is_some_and(|a| a.discover_apps);
        let agent_restart = Arc::new(tokio::sync::Notify::new());
        let restart = agent_restart.clone();
        let switch = agent.as_ref().map(|((_, rx), _)| rx.clone());
        let (stop, stop_rx) = oneshot::channel();
        let published = Arc::new(OnceLock::new());
        let (path, core_slot) = (config_path.clone(), published.clone());
        thread::Builder::new()
            .name("server".into())
            .spawn(move || {
                // A panic is reported like any error, or the window would wait forever.
                let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
                    let runtime = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
                    runtime.block_on(serve(path, true, switch, async { drop(stop_rx.await) }, core_slot, restart))
                }));
                stopped(result.unwrap_or_else(|panic| {
                    let what = panic.downcast_ref::<&str>().copied().or(panic.downcast_ref::<String>().map(String::as_str));
                    Err(format!("internal error: {}", what.unwrap_or("panic")))
                }));
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            core_bind: core.bind,
            data_dir: core.data_dir.clone(),
            config_path,
            core: published,
            agent: agent.map(|((tx, _), port)| (tx, port)),
            agent_restart,
            discover: discover.into(),
            qr,
            stop: Mutex::new(Some(stop)),
        })
    }

    pub fn status(&self) -> Status {
        let counts = self.core.get().map(|core| core.with(|r| (r.entities.len(), r.entities.values().filter(|e| e.available).count())));
        Status {
            ready: counts.is_some(),
            devices: counts.map_or(0, |c| c.0),
            online: counts.map_or(0, |c| c.1),
            address: format!("{}:{}", pairing::lan_ip(self.core_bind), self.core_bind.port()),
            host: pairing::hostname(),
            agent: self.agent.as_ref().map(|(switch, port)| AgentStatus {
                on: *switch.borrow(),
                port: *port,
                discover: self.discover.load(std::sync::atomic::Ordering::Relaxed),
            }),
        }
    }

    /// The hub's data folder, where a client may keep its own files too.
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// The pairing QR codes (SVG) of the hub and of the agent. They hold the tokens: show them
    /// on this PC only.
    pub fn pairing_qr(&self) -> (&str, Option<&str>) {
        (&self.qr.0, self.qr.1.as_deref())
    }

    /// Switches the agent on or off, and saves it in the config. `Ok(true)` when the config had
    /// no `[agent]` yet and gained one: only a restart picks it up.
    pub fn set_agent(&self, on: bool) -> Result<bool, String> {
        let created = config::set_agent_enabled(&self.config_path, on)?;
        if let Some((switch, _)) = &self.agent {
            switch.send_replace(on);
        }
        Ok(created)
    }

    /// Offers the PC's apps to the phone, or stops: saved in the config, then the agent restarts
    /// to read it.
    pub fn set_discover(&self, on: bool) -> Result<(), String> {
        config::set_agent_discover(&self.config_path, on)?;
        self.discover.store(on, std::sync::atomic::Ordering::Relaxed);
        self.agent_restart.notify_one();
        Ok(())
    }

    /// Stops the hub and the agent cleanly (the state is saved); `stopped` follows.
    pub fn stop(&self) {
        if let Some(stop) = self.stop.lock().unwrap().take() {
            let _ = stop.send(());
        }
    }
}
