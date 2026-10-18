//! Desktop mode (Windows, `homemaid.exe` without --core or --agent): the hub and PC control in
//! one process, the logo in the notification area, and a window (a WebView2 page,
//! `desktop.html`) with the state, the pairing QR codes and the switches. The config and the log
//! live next to the exe. Started again while running, it shows the running one's window.

use std::{
    borrow::Cow,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
    thread,
};

use homemaid::{config, pairing};
use serde_json::{Value, json};
use tao::{
    dpi::LogicalSize,
    event::{Event, StartCause, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder},
    platform::windows::IconExtWindows,
    window::{Icon, Theme, Window, WindowBuilder},
};
use tokio::sync::{oneshot, watch};
use tray_icon::{
    MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
};
use windows::{
    Win32::{
        Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, WAIT_OBJECT_0},
        Globalization::GetUserDefaultUILanguage,
        System::Threading::{CreateEventW, INFINITE, SetEvent, WaitForSingleObject},
        UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW},
    },
    core::{HSTRING, w},
};
use wry::{WebViewBuilder, http::Response};

use super::{Args, CORE};

enum UserEvent {
    Show,
    Ipc(String),
    Tray(TrayIconEvent),
    Menu(MenuEvent),
    Stopped(Result<(), String>),
}

pub fn main(args: Args) -> ExitCode {
    let dir = std::env::current_exe().ok().and_then(|exe| exe.parent().map(Path::to_path_buf)).unwrap_or_else(|| ".".into());
    let config_path = args.config.clone().unwrap_or_else(|| dir.join("homemaid.toml"));
    if let Err(e) = super::init_logging(Some(&args.log_file.clone().unwrap_or_else(|| dir.join("homemaid.log")))) {
        return fail(&e);
    }
    let Some(show_event) = single_instance() else { return ExitCode::SUCCESS };
    if !config_path.exists() {
        if let Err(e) = config::write_first(&config_path) {
            return fail(&e);
        }
        tracing::info!("first start: wrote {}", config_path.display());
    }
    match run(args, dir, config_path, show_event) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => fail(&e),
    }
}

fn fail(message: &str) -> ExitCode {
    alert(message);
    ExitCode::FAILURE
}

/// No console in desktop mode: errors go to the log and to a message box.
fn alert(message: &str) {
    tracing::error!("{message}");
    unsafe { MessageBoxW(None, &HSTRING::from(message), w!("HomeMaid"), MB_ICONERROR | MB_OK) };
}

/// The event a second instance sets to show this one's window; `None` if one is already running.
fn single_instance() -> Option<HANDLE> {
    match unsafe { CreateEventW(None, false, false, w!("Local\\HomeMaid.Show")) } {
        Ok(event) if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS => {
            let _ = unsafe { SetEvent(event) };
            None
        }
        Ok(event) => Some(event),
        Err(e) => {
            tracing::warn!("cannot check for another instance: {e}");
            Some(HANDLE::default())
        }
    }
}

fn french() -> bool {
    let language = unsafe { GetUserDefaultUILanguage() };
    language & 0x3ff == 0x0c
}

fn run(args: Args, dir: PathBuf, config_path: PathBuf, show_event: HANDLE) -> Result<(), String> {
    let config = config::load(&config_path)?;
    let core = config.core.as_ref().ok_or_else(|| format!("{}: no [core] section", config_path.display()))?;
    let core_bind = core.bind;
    let qr = json!({
        "core": pairing::qr_svg("core", core.bind, &core.token),
        "agent": config.agent.as_ref().map(|a| pairing::qr_svg("agent", a.bind, &a.token)),
    });
    let agent_port = config.agent.as_ref().map(|a| a.bind.port());
    let (switch, switch_rx) = watch::channel(config.agent.as_ref().is_some_and(|a| a.enabled));

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    // The hub and the agent, on a tokio runtime of their own.
    let (stop, stopped) = oneshot::channel::<()>();
    let runner = proxy.clone();
    let path = config_path.clone();
    thread::spawn(move || {
        let result = super::block_on(super::run(path, true, agent_port.map(|_| switch_rx), async {
            let _ = stopped.await;
        }));
        let _ = runner.send_event(UserEvent::Stopped(result));
    });

    // A second instance sets the event: show the window.
    let shower = proxy.clone();
    let raw = show_event.0 as usize;
    thread::spawn(move || {
        while unsafe { WaitForSingleObject(HANDLE(raw as _), INFINITE) } == WAIT_OBJECT_0 {
            if shower.send_event(UserEvent::Show).is_err() {
                break;
            }
        }
    });
    let to_loop = proxy.clone();
    TrayIconEvent::set_event_handler(Some(move |e| drop(to_loop.send_event(UserEvent::Tray(e)))));
    let to_loop = proxy.clone();
    MenuEvent::set_event_handler(Some(move |e| drop(to_loop.send_event(UserEvent::Menu(e)))));

    let window = WindowBuilder::new()
        .with_title("HomeMaid")
        .with_inner_size(LogicalSize::new(440.0, 820.0))
        .with_min_inner_size(LogicalSize::new(360.0, 520.0))
        .with_window_icon(Icon::from_resource(1, None).ok())
        .with_theme(Some(Theme::Dark))
        .with_visible(false)
        .build(&event_loop)
        .map_err(|e| e.to_string())?;
    // WebView2's own files (cache, settings) in the data folder, not next to the exe.
    let mut context = wry::WebContext::new(Some(core.data_dir.join("webview")));
    let to_loop = proxy.clone();
    let webview = WebViewBuilder::new_with_web_context(&mut context)
        .with_custom_protocol("homemaid".into(), |_, request| asset(request.uri().path()))
        .with_url("homemaid://localhost/")
        .with_background_color((0x15, 0x17, 0x1B, 0xFF))
        .with_ipc_handler(move |request| drop(to_loop.send_event(UserEvent::Ipc(request.into_body()))))
        .build(&window)
        .map_err(|e| format!("cannot open the window (is the WebView2 runtime installed?): {e}"))?;

    let fr = french();
    let open = MenuItem::new(if fr { "Ouvrir HomeMaid" } else { "Open HomeMaid" }, true, None);
    let quit = MenuItem::new(if fr { "Quitter" } else { "Quit" }, true, None);
    let mut tray: Option<TrayIcon> = None;
    let mut stop = Some(stop);
    let mut restart = false;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::NewEvents(StartCause::Init) => {
                let menu = Menu::new();
                let _ = menu.append_items(&[&open, &PredefinedMenuItem::separator(), &quit]);
                tray = TrayIconBuilder::new()
                    .with_icon(tray_icon::Icon::from_resource(1, None).expect("the exe has its icon (build.rs)"))
                    .with_tooltip(tooltip(fr, *switch.borrow()))
                    .with_menu(Box::new(menu))
                    .with_menu_on_left_click(false)
                    .build()
                    .map_err(|e| tracing::error!("no notification-area icon: {e}"))
                    .ok();
                if !args.background {
                    show(&window);
                }
            }
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => window.set_visible(false),
            Event::UserEvent(UserEvent::Show)
            | Event::UserEvent(UserEvent::Tray(TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            })) => show(&window),
            Event::UserEvent(UserEvent::Menu(e)) if e.id == *open.id() => show(&window),
            Event::UserEvent(UserEvent::Menu(e)) if e.id == *quit.id() => request_stop(&mut stop),
            Event::UserEvent(UserEvent::Ipc(message)) => {
                let message: Value = serde_json::from_str(&message).unwrap_or_default();
                let on = message["on"].as_bool().unwrap_or(false);
                let status = || status(core_bind, agent_port, *switch.borrow(), &dir);
                let reply = match message["cmd"].as_str().unwrap_or_default() {
                    "init" => {
                        let mut s = status();
                        s["qr"] = qr.clone();
                        s
                    }
                    "status" => status(),
                    "agent" => match config::set_agent_enabled(&config_path, on) {
                        // A new [agent] section is read at start.
                        Ok(true) => {
                            restart = true;
                            return request_stop(&mut stop);
                        }
                        Ok(false) => {
                            switch.send_replace(on);
                            if let Some(tray) = &tray {
                                let _ = tray.set_tooltip(Some(tooltip(fr, on)));
                            }
                            status()
                        }
                        Err(e) => json!({ "error": e }),
                    },
                    "startup" => match startup::set(on) {
                        Ok(()) => status(),
                        Err(e) => json!({ "error": e }),
                    },
                    "folder" => {
                        let _ = Command::new("explorer").arg(&dir).spawn();
                        return;
                    }
                    "restart" => {
                        restart = true;
                        return request_stop(&mut stop);
                    }
                    "quit" => return request_stop(&mut stop),
                    _ => return,
                };
                let _ = webview.evaluate_script(&format!("hm({reply})"));
            }
            Event::UserEvent(UserEvent::Stopped(result)) => {
                tray = None;
                window.set_visible(false);
                match result {
                    Err(e) => alert(&e),
                    Ok(()) if restart => {
                        // Free the single-instance event first, or the new process would just show us.
                        let _ = unsafe { CloseHandle(show_event) };
                        let mut again = Command::new(std::env::current_exe().unwrap_or_else(|_| "homemaid.exe".into()));
                        if let Some(config) = &args.config {
                            again.arg("--config").arg(config);
                        }
                        if let Err(e) = again.spawn() {
                            alert(&format!("cannot restart: {e}"));
                        }
                    }
                    Ok(()) => {}
                }
                *control_flow = ControlFlow::Exit;
            }
            _ => {}
        }
    })
}

/// Stops the hub and the agent; the loop exits once they are down (`UserEvent::Stopped`).
fn request_stop(stop: &mut Option<oneshot::Sender<()>>) {
    if let Some(stop) = stop.take() {
        let _ = stop.send(());
    }
}

fn show(window: &Window) {
    window.set_visible(true);
    window.set_minimized(false);
    window.set_focus();
}

fn tooltip(fr: bool, agent: bool) -> &'static str {
    match (fr, agent) {
        (true, true) => "HomeMaid · maison et contrôle du PC",
        (true, false) => "HomeMaid · maison",
        (false, true) => "HomeMaid · home and PC control",
        (false, false) => "HomeMaid · home",
    }
}

fn status(core_bind: std::net::SocketAddr, agent_port: Option<u16>, agent_on: bool, dir: &Path) -> Value {
    let devices = CORE.get().map(|core| core.with(|r| (r.entities.len(), r.entities.values().filter(|e| e.available).count())));
    json!({
        "host": pairing::hostname(),
        "ip": pairing::lan_ip(core_bind).to_string(),
        "hub": { "port": core_bind.port(), "ready": devices.is_some(), "devices": devices.map(|d| d.0), "online": devices.map(|d| d.1) },
        "agent": agent_port.map(|port| json!({ "port": port, "on": agent_on })),
        "startup": startup::is_on(),
        "version": env!("CARGO_PKG_VERSION"),
        "dir": dir.display().to_string(),
    })
}

/// The window's files, served from the exe (`homemaid://localhost/...`).
fn asset(path: &str) -> Response<Cow<'static, [u8]>> {
    let (body, kind): (&'static [u8], &str) = match path {
        "/" => (include_bytes!("desktop.html"), "text/html; charset=utf-8"),
        "/logo.png" => (include_bytes!("../../../../app/assets/brand/logo.png"), "image/png"),
        "/barlow.ttf" => (include_bytes!("../../../../app/assets/fonts/Barlow-Regular.ttf"), "font/ttf"),
        "/barlow-semibold.ttf" => (include_bytes!("../../../../app/assets/fonts/Barlow-SemiBold.ttf"), "font/ttf"),
        "/barlow-condensed.ttf" => (include_bytes!("../../../../app/assets/fonts/BarlowSemiCondensed-Medium.ttf"), "font/ttf"),
        _ => return Response::builder().status(404).body(Cow::Borrowed(&[][..])).expect("valid response"),
    };
    Response::builder().header("Content-Type", kind).body(Cow::Borrowed(body)).expect("valid response")
}

/// "Start with Windows": the `HomeMaid` value of the user's Run key, opening in the background.
mod startup {
    use windows::{
        Win32::{
            Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, WIN32_ERROR},
            System::Registry::{HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW},
        },
        core::{PCWSTR, w},
    };

    const RUN: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
    const NAME: PCWSTR = w!("HomeMaid");

    fn command() -> String {
        let exe = std::env::current_exe().unwrap_or_default();
        format!("\"{}\" --background", exe.display())
    }

    /// On only if the value starts this exe: a stale one (the exe moved) counts as off.
    pub fn is_on() -> bool {
        let mut buffer = [0u16; 1024];
        let mut size = size_of_val(&buffer) as u32;
        let found = unsafe {
            RegGetValueW(HKEY_CURRENT_USER, RUN, NAME, RRF_RT_REG_SZ, None, Some(buffer.as_mut_ptr().cast()), Some(&mut size))
        };
        let len = (size as usize / 2).saturating_sub(1).min(buffer.len());
        found == ERROR_SUCCESS && String::from_utf16_lossy(&buffer[..len]) == command()
    }

    pub fn set(on: bool) -> Result<(), String> {
        let result = if on {
            let value: Vec<u16> = command().encode_utf16().chain([0]).collect();
            unsafe {
                RegSetKeyValueW(HKEY_CURRENT_USER, RUN, NAME, REG_SZ.0, Some(value.as_ptr().cast()), size_of_val(value.as_slice()) as u32)
            }
        } else {
            match unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN, NAME) } {
                ERROR_FILE_NOT_FOUND => ERROR_SUCCESS,
                other => other,
            }
        };
        check(result)
    }

    fn check(result: WIN32_ERROR) -> Result<(), String> {
        if result == ERROR_SUCCESS { Ok(()) } else { Err(windows::core::Error::from(result.to_hresult()).message()) }
    }
}
