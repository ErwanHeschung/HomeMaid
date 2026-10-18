// No console window when started at logon or as a service; `attach_console` keeps terminal logs.
#![cfg_attr(windows, windows_subsystem = "windows")]

//! The `homemaid` executable: parses the flags, then runs the server (`homemaid::server`) or,
//! on Windows without a role flag, the desktop client (`homemaid_desktop`).

use std::{future::Future, path::PathBuf, process::ExitCode};

use clap::Parser;
use homemaid::server::{self, init_logging};
use tokio::sync::watch;

/// HomeMaid home automation hub. Roles are enabled by flags and can be combined. On Windows,
/// without --core or --agent, it runs as a desktop app: hub and PC control, an icon in the
/// notification area and a window, with homemaid.toml next to the exe.
#[derive(Parser)]
struct Args {
    /// Run the core: entity registry, reconciler, plugins and API.
    #[arg(long)]
    core: bool,
    /// Run the PC agent: remote control of this PC. Must run in the user's session, not as a service.
    #[arg(long)]
    agent: bool,
    /// Configuration file [default: homemaid.toml, next to the exe in desktop mode].
    #[arg(long)]
    config: Option<PathBuf>,
    /// Write logs to this file instead of the console.
    #[arg(long)]
    log_file: Option<PathBuf>,
    /// Run under the Windows service control manager (set in the service's command line).
    #[cfg(windows)]
    #[arg(long)]
    service: bool,
    /// Desktop mode: start in the notification area without opening the window (at logon).
    #[cfg(windows)]
    #[arg(long)]
    background: bool,
}

impl Args {
    fn config(&self) -> PathBuf {
        self.config.clone().unwrap_or_else(|| "homemaid.toml".into())
    }

    /// The agent always on with --agent, absent otherwise.
    fn agent_switch(&self) -> Option<watch::Receiver<bool>> {
        self.agent.then(server::always_on)
    }
}

fn main() -> ExitCode {
    #[cfg(windows)]
    attach_console();
    let args = Args::parse();
    #[cfg(windows)]
    if !args.core && !args.agent && !args.service {
        return homemaid_desktop::main(args.config, args.log_file, args.background);
    }
    if let Err(e) = init_logging(args.log_file.as_deref()) {
        eprintln!("{e}");
        return ExitCode::FAILURE;
    }
    #[cfg(windows)]
    if args.service {
        return report(service::start());
    }
    report(block_on(server::run(args.config(), args.core, args.agent_switch(), async {
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

fn block_on(future: impl Future<Output = Result<(), String>>) -> Result<(), String> {
    tokio::runtime::Runtime::new().map_err(|e| e.to_string())?.block_on(future)
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
        let args = super::Args::parse();
        let result = super::block_on(homemaid::server::run(args.config(), args.core, args.agent_switch(), async {
            let _ = stopped.await;
        }));
        set(ServiceState::Stopped, ServiceControlAccept::empty()).map_err(|e| e.to_string())?;
        result
    }
}
