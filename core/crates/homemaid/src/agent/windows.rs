//! Windows desktop: input with enigo, volume with Core Audio, the window in front with Win32 (its
//! program) and UI Automation (the browser's address), what plays with the media sessions.

use enigo::{Axis, Button as MouseButton, Coordinate, Direction::Click, Enigo, Key as EnigoKey, Keyboard, Mouse, Settings};
use windows::{
    Media::Control::{
        GlobalSystemMediaTransportControlsSession as Session, GlobalSystemMediaTransportControlsSessionManager as Sessions,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
    },
    Win32::{
        Foundation::{CloseHandle, ERROR_SUCCESS, HANDLE, HWND},
        Media::Audio::{Endpoints::IAudioEndpointVolume, IMMDeviceEnumerator, MMDeviceEnumerator, eConsole, eRender},
        Storage::{EnhancedStorage::PKEY_AppUserModel_ID, Packaging::Appx::GetApplicationUserModelId},
        System::{
            Com::{
                CLSCTX_ALL, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
                StructuredStorage::{PropVariantClear, PropVariantToStringAlloc},
            },
            SystemInformation::GetSystemTimeAsFileTime,
            Threading::{OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW},
            Variant::VARIANT,
        },
        UI::{
            Accessibility::{
                CUIAutomation, IUIAutomation, IUIAutomationValuePattern, TreeScope_Descendants, UIA_ControlTypePropertyId,
                UIA_EditControlTypeId, UIA_ValuePatternId,
            },
            Shell::{ASSOCF_IS_PROTOCOL, ASSOCSTR_EXECUTABLE, AssocQueryStringW, PropertiesSystem::{IPropertyStore, SHGetPropertyStoreForWindow}},
            WindowsAndMessaging::{GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId},
        },
    },
    core::{PWSTR, w},
};

use super::{Button, Combo, ComboKey, Foreground, Key, Media, Modifier, NowPlaying, Power, Request, seek_to, take_whole};

/// What `back` and `forward` seek by, in the media sessions' 100 ns ticks.
const SEEK: i64 = 5 * 10_000_000;

pub struct Desktop {
    enigo: Enigo,
    /// ponytail: default output device at startup; follow device changes if switching outputs matters.
    volume: Option<IAudioEndpointVolume>,
    move_rest: (f64, f64),
    scroll_rest: (f64, f64),
    /// None when Windows refuses (older than 10 1809): the media keys then.
    media: Option<Sessions>,
    automation: Option<IUIAutomation>,
    /// The browser's address last read, and the window and title it was read in: read again only
    /// when they change (a tab switch changes the title), as UI Automation costs the browser.
    address: Option<(Seen, Option<String>)>,
}

/// A window (its handle) and its title.
type Seen = (usize, Option<String>);

impl Desktop {
    pub fn new() -> Result<Self, String> {
        let enigo = Enigo::new(&Settings::default()).map_err(|e| e.to_string())?;
        // Multithreaded: the media sessions' calls then wait in place (`join`).
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.ok().map_err(|e| e.to_string())?;
        // No audio device is not an error: volume is just unavailable.
        let volume = unsafe { audio_endpoint() }.map_err(|e| tracing::warn!("no audio endpoint: {e}")).ok();
        let media = Sessions::RequestAsync().and_then(|m| m.join()).map_err(|e| tracing::warn!("no media sessions: {e}")).ok();
        let automation = unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }
            .map_err(|e| tracing::warn!("no UI Automation: {e}"))
            .ok();
        Ok(Self { enigo, volume, move_rest: (0.0, 0.0), scroll_rest: (0.0, 0.0), media, automation, address: None })
    }

    pub fn handle(&mut self, request: &Request) -> Result<(), String> {
        let result = match request {
            Request::Move { dx, dy } => {
                let (x, y) = take_whole(&mut self.move_rest, *dx, *dy);
                self.enigo.move_mouse(x, y, Coordinate::Rel)
            }
            Request::Scroll { dx, dy } => {
                let (x, y) = take_whole(&mut self.scroll_rest, *dx, *dy);
                let vertical = if y != 0 { self.enigo.scroll(y, Axis::Vertical) } else { Ok(()) };
                vertical.and_then(|()| if x != 0 { self.enigo.scroll(x, Axis::Horizontal) } else { Ok(()) })
            }
            Request::Click { button } => self.enigo.button(mouse_button(*button), Click),
            Request::Text { text } => self.enigo.text(text),
            Request::Key { key } => self.enigo.key(key_of(*key), Click),
            Request::Media { action } => return self.media(*action),
            Request::SetVolume { level } => {
                let endpoint = self.endpoint()?;
                return unsafe { endpoint.SetMasterVolumeLevelScalar(f32::from(*level) / 100.0, std::ptr::null()) }
                    .map_err(|e| e.to_string());
            }
            Request::SetMute { muted } => {
                let endpoint = self.endpoint()?;
                return unsafe { endpoint.SetMute(*muted, std::ptr::null()) }.map_err(|e| e.to_string());
            }
            Request::Power { action } => return power(*action),
            // Handled by the desktop thread itself.
            Request::Launch { .. } | Request::Open { .. } | Request::Control { .. } | Request::State | Request::ListApps => return Ok(()),
            Request::SaveShortcut { .. } | Request::DeleteShortcut { .. } | Request::PinApp { .. } | Request::UnpinApp { .. } => return Ok(()),
        };
        result.map_err(|e| e.to_string())
    }

    /// Holds the modifiers, taps the key, releases them in reverse.
    pub fn press(&mut self, combo: &Combo) -> Result<(), String> {
        use enigo::Direction::{Press, Release};
        let modifier = |m: &Modifier| match m {
            Modifier::Shift => EnigoKey::Shift,
            Modifier::Ctrl => EnigoKey::Control,
            Modifier::Alt => EnigoKey::Alt,
            Modifier::Meta => EnigoKey::Meta,
        };
        let key = match combo.key {
            ComboKey::Named(key) => key_of(key),
            ComboKey::Char(c) => EnigoKey::Unicode(c),
        };
        let mut result = Ok(());
        let mut held = Vec::new();
        for m in &combo.modifiers {
            result = self.enigo.key(modifier(m), Press);
            if result.is_err() {
                break;
            }
            held.push(m);
        }
        if result.is_ok() {
            result = self.enigo.key(key, Click);
        }
        for m in held.into_iter().rev() {
            let _ = self.enigo.key(modifier(m), Release);
        }
        result.map_err(|e| e.to_string())
    }

    pub fn volume(&self) -> Option<(u8, bool)> {
        let endpoint = self.volume.as_ref()?;
        unsafe {
            let level = endpoint.GetMasterVolumeLevelScalar().ok()?;
            let muted = endpoint.GetMute().ok()?.as_bool();
            Some(((level * 100.0).round() as u8, muted))
        }
    }

    /// The window in front, with the active tab's address when it is the default browser's.
    pub fn foreground(&mut self) -> Foreground {
        let (window, mut front) = unsafe { front() };
        front.browser = front.exe.is_some() && front.exe == default_browser();
        if front.browser {
            let key = (window.0 as usize, front.title.clone());
            front.url = match &self.address {
                Some((read, url)) if *read == key => url.clone(),
                _ => {
                    let url = self.automation.as_ref().and_then(|a| unsafe { address_bar(a, window) }.ok()).filter(|u| !u.is_empty());
                    self.address = Some((key, url.clone()));
                    url
                }
            };
        }
        front
    }

    pub fn now_playing(&self, front: &Foreground) -> Option<NowPlaying> {
        let session = self.session(front)?;
        let properties = session.TryGetMediaPropertiesAsync().and_then(|p| p.join()).ok()?;
        let title = properties.Title().ok()?.to_string();
        let playing = session.GetPlaybackInfo().and_then(|i| i.PlaybackStatus()).is_ok_and(|s| s == Status::Playing);
        (!title.is_empty()).then(|| NowPlaying { title, artist: properties.Artist().map(|a| a.to_string()).unwrap_or_default(), playing })
    }

    /// The media session of the app in front, else the one Windows' media flyout shows.
    fn session(&self, front: &Foreground) -> Option<Session> {
        let media = self.media.as_ref()?;
        let mine = media.GetSessions().ok()?.into_iter().find(|s| s.SourceAppUserModelId().is_ok_and(|id| super::plays_in(&id.to_string(), front)));
        mine.or_else(|| media.GetCurrentSession().ok())
    }

    /// Through the media session when it takes it, else the media key (the arrows to seek).
    fn media(&mut self, action: Media) -> Result<(), String> {
        let (_, front) = unsafe { front() };
        let session = self.session(&front);
        match session.map(|s| control(&s, action)) {
            Some(Ok(true)) => return Ok(()),
            Some(Err(e)) => tracing::debug!("media session refused {action:?}: {e}"),
            _ => {}
        }
        self.enigo.key(media_key(action), Click).map_err(|e| e.to_string())
    }

    fn endpoint(&self) -> Result<&IAudioEndpointVolume, String> {
        self.volume.as_ref().ok_or_else(|| "no audio output device".to_string())
    }
}

/// `Ok(false)` when the session does not offer it.
fn control(session: &Session, action: Media) -> windows::core::Result<bool> {
    let controls = session.GetPlaybackInfo()?.Controls()?;
    let request = match action {
        Media::PlayPause if controls.IsPlayPauseToggleEnabled()? => session.TryTogglePlayPauseAsync()?,
        Media::Next if controls.IsNextEnabled()? => session.TrySkipNextAsync()?,
        Media::Previous if controls.IsPreviousEnabled()? => session.TrySkipPreviousAsync()?,
        Media::Back | Media::Forward if controls.IsPlaybackPositionEnabled()? => {
            let timeline = session.GetTimelineProperties()?;
            let playing = session.GetPlaybackInfo()?.PlaybackStatus()? == Status::Playing;
            let now = unsafe { GetSystemTimeAsFileTime() };
            let now = ((u64::from(now.dwHighDateTime) << 32) | u64::from(now.dwLowDateTime)) as i64;
            let delta = if action == Media::Back { -SEEK } else { SEEK };
            let to = seek_to(timeline.Position()?.Duration, timeline.LastUpdatedTime()?.UniversalTime, now, playing, delta, timeline.EndTime()?.Duration);
            session.TryChangePlaybackPositionAsync(to)?
        }
        _ => return Ok(false),
    };
    request.join()
}

/// The foreground window and its program: file name, and AppUserModelID (the window's own, else
/// its package's).
unsafe fn front() -> (HWND, Foreground) {
    unsafe {
        let window = GetForegroundWindow();
        let mut title = [0u16; 512];
        let len = GetWindowTextW(window, &mut title);
        let title = (len > 0).then(|| String::from_utf16_lossy(&title[..len as usize]));
        let mut pid = 0;
        GetWindowThreadProcessId(window, Some(&mut pid));
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok();
        let exe = process.and_then(|p| image_name(p));
        let aumid = window_aumid(window).or_else(|| process.and_then(|p| package_aumid(p)));
        if let Some(p) = process {
            let _ = CloseHandle(p);
        }
        (window, Foreground { title, exe, aumid, ..Default::default() })
    }
}

unsafe fn image_name(process: HANDLE) -> Option<String> {
    let mut path = [0u16; 1024];
    let mut len = path.len() as u32;
    unsafe { QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(path.as_mut_ptr()), &mut len) }.ok()?;
    file_name(&path[..len as usize])
}

unsafe fn window_aumid(window: HWND) -> Option<String> {
    unsafe {
        let store: IPropertyStore = SHGetPropertyStoreForWindow(window).ok()?;
        let mut value = store.GetValue(&PKEY_AppUserModel_ID).ok()?;
        let text = PropVariantToStringAlloc(&value).ok();
        let _ = PropVariantClear(&mut value);
        let text = text?;
        let id = text.to_string().ok();
        CoTaskMemFree(Some(text.0 as _));
        id.filter(|id| !id.is_empty())
    }
}

unsafe fn package_aumid(process: HANDLE) -> Option<String> {
    let mut id = [0u16; 512];
    let mut len = id.len() as u32;
    (unsafe { GetApplicationUserModelId(process, &mut len, Some(PWSTR(id.as_mut_ptr()))) } == ERROR_SUCCESS)
        .then(|| String::from_utf16_lossy(&id[..len.saturating_sub(1) as usize]))
}

/// The program `https` links open with: sites open there.
fn default_browser() -> Option<String> {
    let mut path = [0u16; 1024];
    let mut len = path.len() as u32;
    unsafe { AssocQueryStringW(ASSOCF_IS_PROTOCOL, ASSOCSTR_EXECUTABLE, w!("https"), w!("open"), Some(PWSTR(path.as_mut_ptr())), &mut len) }.ok().ok()?;
    file_name(&path[..len.saturating_sub(1) as usize])
}

/// A path's file name, lowercase.
fn file_name(path: &[u16]) -> Option<String> {
    let path = String::from_utf16_lossy(path);
    path.rsplit('\\').next().filter(|n| !n.is_empty()).map(str::to_lowercase)
}

/// The window's first edit field: the address bar, in browsers.
unsafe fn address_bar(automation: &IUIAutomation, window: HWND) -> windows::core::Result<String> {
    unsafe {
        let edit = automation.CreatePropertyCondition(UIA_ControlTypePropertyId, &VARIANT::from(UIA_EditControlTypeId.0))?;
        let bar = automation.ElementFromHandle(window)?.FindFirst(TreeScope_Descendants, &edit)?;
        let value: IUIAutomationValuePattern = bar.GetCurrentPatternAs(UIA_ValuePatternId)?;
        Ok(value.CurrentValue()?.to_string())
    }
}

unsafe fn audio_endpoint() -> windows::core::Result<IAudioEndpointVolume> {
    unsafe {
        let devices: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        devices.GetDefaultAudioEndpoint(eRender, eConsole)?.Activate(CLSCTX_ALL, None)
    }
}

/// The Start menu's apps (`Get-StartApps`: programs, Store apps, games), by name.
pub fn start_apps() -> std::collections::BTreeMap<String, String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    // UTF-8, or the console code page garbles accents ("Accès vocal").
    let script = "[Console]::OutputEncoding = [Text.Encoding]::UTF8; Get-StartApps | ConvertTo-Json -Compress";
    match std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
    {
        Ok(out) if out.status.success() => super::parse_start_apps(&String::from_utf8_lossy(&out.stdout)),
        Ok(out) => {
            tracing::warn!("Get-StartApps failed: {}", String::from_utf8_lossy(&out.stderr));
            Default::default()
        }
        Err(e) => {
            tracing::warn!("cannot list the Start menu's apps: {e}");
            Default::default()
        }
    }
}

/// Sets this thread up as the Windows shell wants it (an STA), for [`app_icon`].
pub fn init_icon_thread() {
    // Already set up is fine.
    let _ = unsafe { CoInitializeEx(None, windows::Win32::System::Com::COINIT_APARTMENTTHREADED) };
}

/// The icon the Start menu shows for an app, 96 px, as PNG.
pub fn app_icon(app_id: &str) -> Option<Vec<u8>> {
    unsafe { icon_png(app_id) }.map_err(|e| tracing::debug!("no icon for {app_id}: {e}")).ok()
}

unsafe fn icon_png(app_id: &str) -> windows::core::Result<Vec<u8>> {
    use windows::{
        Win32::{
            Foundation::{E_FAIL, HGLOBAL, SIZE},
            Graphics::{
                Gdi::{DeleteObject, HPALETTE},
                Imaging::{CLSID_WICImagingFactory, GUID_ContainerFormatPng, IWICImagingFactory, WICBitmapEncoderNoCache, WICBitmapUsePremultipliedAlpha},
            },
            System::{
                Com::{CLSCTX_INPROC_SERVER, STREAM_SEEK_CUR, StructuredStorage::{CreateStreamOnHGlobal, GetHGlobalFromStream}},
                Memory::{GlobalLock, GlobalUnlock},
            },
            UI::Shell::{IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_BIGGERSIZEOK, SIIGBF_ICONONLY},
        },
        core::{Error, HSTRING},
    };
    unsafe {
        // The Start menu's own item: programs, Store apps and games alike.
        let item: IShellItemImageFactory = SHCreateItemFromParsingName(&HSTRING::from(format!("shell:AppsFolder\\{app_id}")), None)?;
        let bitmap = item.GetImage(SIZE { cx: 96, cy: 96 }, SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK)?;
        let wic: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
        let source = wic.CreateBitmapFromHBITMAP(bitmap, HPALETTE::default(), WICBitmapUsePremultipliedAlpha);
        let _ = DeleteObject(bitmap.into());
        let source = source?;
        let stream = CreateStreamOnHGlobal(HGLOBAL::default(), true)?;
        let encoder = wic.CreateEncoder(&GUID_ContainerFormatPng, std::ptr::null())?;
        encoder.Initialize(&stream, WICBitmapEncoderNoCache)?;
        let (mut frame, mut options) = (None, None);
        encoder.CreateNewFrame(&mut frame, &mut options)?;
        let frame = frame.ok_or_else(|| Error::from(E_FAIL))?;
        frame.Initialize(options.as_ref())?;
        frame.WriteSource(&source, std::ptr::null())?;
        frame.Commit()?;
        encoder.Commit()?;
        // What was written: up to the stream's position.
        let mut length = 0u64;
        stream.Seek(0, STREAM_SEEK_CUR, Some(&mut length))?;
        let memory = GetHGlobalFromStream(&stream)?;
        let data = GlobalLock(memory) as *const u8;
        if data.is_null() {
            return Err(Error::from(E_FAIL));
        }
        let png = std::slice::from_raw_parts(data, length as usize).to_vec();
        let _ = GlobalUnlock(memory);
        Ok(png)
    }
}

fn power(action: Power) -> Result<(), String> {
    match action {
        Power::Lock => unsafe { windows::Win32::System::Shutdown::LockWorkStation() }.map_err(|e| e.to_string()),
        // Sleep (not hibernate), wake events allowed.
        Power::Sleep => match unsafe { windows::Win32::System::Power::SetSuspendState(false, false, false) } {
            true => Ok(()),
            false => Err(windows::core::Error::from_thread().to_string()),
        },
        Power::Shutdown => std::process::Command::new("shutdown")
            .args(["/s", "/t", "0"])
            .spawn()
            .map(drop)
            .map_err(|e| format!("shutdown: {e}")),
    }
}

fn mouse_button(button: Button) -> MouseButton {
    match button {
        Button::Left => MouseButton::Left,
        Button::Right => MouseButton::Right,
        Button::Middle => MouseButton::Middle,
    }
}

fn key_of(key: Key) -> EnigoKey {
    match key {
        Key::Enter => EnigoKey::Return,
        Key::Backspace => EnigoKey::Backspace,
        Key::Delete => EnigoKey::Delete,
        Key::Escape => EnigoKey::Escape,
        Key::Tab => EnigoKey::Tab,
        Key::Space => EnigoKey::Space,
        Key::Up => EnigoKey::UpArrow,
        Key::Down => EnigoKey::DownArrow,
        Key::Left => EnigoKey::LeftArrow,
        Key::Right => EnigoKey::RightArrow,
        Key::Home => EnigoKey::Home,
        Key::End => EnigoKey::End,
        Key::PageUp => EnigoKey::PageUp,
        Key::PageDown => EnigoKey::PageDown,
    }
}

fn media_key(action: Media) -> EnigoKey {
    match action {
        Media::PlayPause => EnigoKey::MediaPlayPause,
        Media::Next => EnigoKey::MediaNextTrack,
        Media::Previous => EnigoKey::MediaPrevTrack,
        Media::VolumeUp => EnigoKey::VolumeUp,
        Media::VolumeDown => EnigoKey::VolumeDown,
        Media::Mute => EnigoKey::VolumeMute,
        Media::Back => EnigoKey::LeftArrow,
        Media::Forward => EnigoKey::RightArrow,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What the remote would see, printed: `cargo test --target x86_64-pc-windows-gnu -p homemaid
    /// --lib -- --ignored --nocapture what_is_in_front`.
    #[test]
    #[ignore = "needs a Windows desktop session"]
    fn what_is_in_front() {
        let mut desktop = Desktop::new().unwrap();
        println!("default browser: {:?}", default_browser());
        let front = desktop.foreground();
        println!("{front:?}\n{:?}", desktop.now_playing(&front));
        println!("address: {:?}", unsafe { address_bar(desktop.automation.as_ref().unwrap(), GetForegroundWindow()) });
        if let Some(media) = &desktop.media {
            for s in media.GetSessions().unwrap() {
                println!("session {:?}", s.SourceAppUserModelId().map(|id| id.to_string()));
            }
        }
    }

    /// Run on a Windows session: `cargo test --target x86_64-pc-windows-gnu -p homemaid --lib -- --ignored icons`.
    /// With `HOMEMAID_ICON_DIR`, the icons are written there to look at; `HOMEMAID_ICON_IDS` adds
    /// AppIDs (separated by `|`).
    #[test]
    #[ignore = "needs a Windows desktop session"]
    fn start_menu_icons_are_png() {
        init_icon_thread();
        let more = std::env::var("HOMEMAID_ICON_IDS").unwrap_or_default();
        let ids = ["Microsoft.WindowsCalculator_8wekyb3d8bbwe!App", "Microsoft.WindowsNotepad_8wekyb3d8bbwe!App"]
            .into_iter()
            .chain(more.split('|').filter(|id| !id.is_empty()));
        for id in ids {
            let png = app_icon(id).unwrap_or_else(|| panic!("no icon for {id}"));
            assert!(png.starts_with(b"\x89PNG\r\n\x1a\n") && png.len() > 200, "{id}: {} bytes", png.len());
            if let Ok(dir) = std::env::var("HOMEMAID_ICON_DIR") {
                let name: String = id.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
                std::fs::write(std::path::Path::new(&dir).join(format!("{name}.png")), &png).unwrap();
            }
        }
        assert!(app_icon("No.Such.App!App").is_none());
    }
}
