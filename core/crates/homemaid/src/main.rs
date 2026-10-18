// No console window when started at logon or as a service; `attach_console` keeps terminal logs.
#![cfg_attr(windows, windows_subsystem = "windows")]

use std::{
    fs::File,
    future::Future,
    net::SocketAddr,
    path::{Path, PathBuf},
    process::ExitCode,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use clap::Parser;
use homemaid::{Core, agent, api, config, pairing, plugins::Plugins, reconciler::Reconciler, store};
use tokio::sync::{mpsc, watch};

/// HomeMaid home automation hub. Roles are enabled by flags and can be combined.
#[derive(Parser)]
struct Args {
    /// Run the core: entity registry, reconciler, plugins and API.
    #[arg(long)]
    core: bool,
    /// Run the PC agent: remote control of this PC. Must run in the user's session, not as a service.
    #[arg(long)]
    agent: bool,
    /// Configuration file.
    #[arg(long, default_value = "homemaid.toml")]
    config: PathBuf,
    /// Write logs to this file instead of the console.
    #[arg(long)]
    log_file: Option<PathBuf>,
    /// Run under the Windows service control manager (set in the service's command line).
    #[cfg(windows)]
    #[arg(long)]
    service: bool,
}

fn main() -> ExitCode {
    #[cfg(windows)]
    attach_console();
    let args = Args::parse();
    if let Err(e) = init_logging(args.log_file.as_deref()) {
        eprintln!("{e}");
        return ExitCode::FAILURE;
    }
    #[cfg(windows)]
    if args.service {
        return report(service::start());
    }
    report(block_on(run(args, async {
        let _ = tokio::signal::ctrl_c().await;
    })))
}

fn report(result: Result<(), String>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!("{e}");
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn init_logging(log_file: Option<&Path>) -> Result<(), String> {
    match log_file {
        Some(path) => {
            let file = File::options()
                .create(true)
                .append(true)
                .open(path)
                .map_err(|e| format!("{}: {e}", path.display()))?;
            tracing_subscriber::fmt().with_writer(Mutex::new(file)).with_ansi(false).init();
        }
        None => tracing_subscriber::fmt::init(),
    }
    Ok(())
}

fn block_on(future: impl Future<Output = Result<(), String>>) -> Result<(), String> {
    tokio::runtime::Runtime::new().map_err(|e| e.to_string())?.block_on(future)
}

/// Runs the requested roles until `shutdown` completes, then stops them cleanly.
async fn run(args: Args, shutdown: impl Future<Output = ()> + Send + 'static) -> Result<(), String> {
    if !args.core && !args.agent {
        return Err("nothing to run: pass --core and/or --agent".into());
    }
    let mut config = config::load(&args.config)?;
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
        if !args.core {
            return Ok(());
        }
        let core_config = config.core.take().ok_or("--core needs a [core] section in the config")?;
        // Checked before anything starts: a broken automation is a config error.
        let automations = homemaid::automations::Automations::new(&config.automations).map_err(|e| format!("{}: {e}", args.config.display()))?;
        run_core(core_config, &args.config, config.entities, config.rooms, config.plugins, automations, until_stopped()).await
    };
    let agent = async {
        if !args.agent {
            return Ok(());
        }
        let agent_config = config.agent.take().ok_or("--agent needs an [agent] section in the config")?;
        agent::run(agent_config, until_stopped()).await
    };
    tokio::try_join!(core, agent).map(|_| ())
}

async fn run_core(
    config: config::CoreConfig,
    config_path: &Path,
    overrides: Vec<config::EntityOverride>,
    rooms: Vec<config::RoomConfig>,
    plugins: Vec<config::PluginConfig>,
    automations: homemaid::automations::Automations,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> Result<(), String> {
    let data_dir = &config.data_dir;
    std::fs::create_dir_all(data_dir).map_err(|e| format!("{}: {e}", data_dir.display()))?;
    let snapshot_path = data_dir.join("state.json");
    let snapshot = store::load(&snapshot_path).map_err(|e| format!("{}: {e}", snapshot_path.display()))?;

    let secrets = config::load_secrets(&data_dir.join("secrets.toml"))?;

    let (commands, mut command_rx) = mpsc::unbounded_channel();
    let entities = store::restore(snapshot, &overrides);
    let core = Arc::new(Core::new(Reconciler::new(entities), commands));
    *core.overrides.lock().unwrap() = overrides.into_iter().map(|o| (o.id, o)).collect();
    *core.rooms.lock().unwrap() = rooms.into_iter().filter_map(|r| Some((r.name, r.kind.filter(|k| !k.is_empty())?))).collect();

    let plugins = Arc::new(Plugins::start(core.clone(), plugins, data_dir, secrets)?);
    let _ = core.plugins.set(plugins.clone());
    let router = core.clone();
    tokio::spawn(async move {
        while let Some(command) = command_rx.recv().await {
            plugins.dispatch(&router, command);
        }
    });

    let automations = Arc::new(automations);
    let _ = core.automations.set(automations.clone());
    if !automations.is_empty() {
        tokio::spawn(automations.drive(core.clone()));
    }

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
            // At most one write per 5 s: sensors change every few seconds. A clean stop saves anyway.
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
    let entities = core.with(|r| r.entities.clone());
    if let Err(e) = store::save(path, &entities) {
        tracing::error!("cannot save {}: {e}", path.display());
    }
}

/// Reuses the console of the terminal we were started from, if any (GUI-subsystem binary).
#[cfg(windows)]
fn attach_console() {
    use windows::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
    let _ = unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
}

/// Windows service entry: `homemaid.exe --core --service --config ... --log-file ...`, installed with `sc.exe create`.
#[cfg(windows)]
mod service {
    use std::{ffi::OsString, time::Duration};

    use clap::Parser;
    use windows_service::{
        define_windows_service,
        service::{ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus, ServiceType},
        service_control_handler::{self, ServiceControlHandlerResult},
        service_dispatcher,
    };

    const NAME: &str = "HomeMaid";

    define_windows_service!(ffi_service_main, service_main);

    pub fn start() -> Result<(), String> {
        service_dispatcher::start(NAME, ffi_service_main).map_err(|e| format!("not started by the service manager: {e}"))
    }

    fn service_main(_service_args: Vec<OsString>) {
        if let Err(e) = run_service() {
            tracing::error!("service failed: {e}");
        }
    }

    fn run_service() -> Result<(), String> {
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let mut stop = Some(stop);
        let handler = move |control| match control {
            ServiceControl::Stop | ServiceControl::Shutdown => {
                if let Some(stop) = stop.take() {
                    let _ = stop.send(());
                }
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        };
        let status = service_control_handler::register(NAME, handler).map_err(|e| e.to_string())?;
        let set = |state, accept| {
            status.set_service_status(ServiceStatus {
                service_type: ServiceType::OWN_PROCESS,
                current_state: state,
                controls_accepted: accept,
                exit_code: ServiceExitCode::Win32(0),
                checkpoint: 0,
                wait_hint: Duration::from_secs(10),
                process_id: None,
            })
        };
        set(ServiceState::Running, ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN).map_err(|e| e.to_string())?;
        // The service's own command line (binPath), not the start parameters.
        let result = super::block_on(super::run(super::Args::parse(), async {
            let _ = stopped.await;
        }));
        set(ServiceState::Stopped, ServiceControlAccept::empty()).map_err(|e| e.to_string())?;
        result
    }
}
