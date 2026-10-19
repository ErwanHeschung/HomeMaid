//! Windows desktop: input with enigo, volume with Core Audio, active window with Win32.

use enigo::{Axis, Button as MouseButton, Coordinate, Direction::Click, Enigo, Key as EnigoKey, Keyboard, Mouse, Settings};
use windows::Win32::{
    Media::Audio::{Endpoints::IAudioEndpointVolume, IMMDeviceEnumerator, MMDeviceEnumerator, eConsole, eRender},
    System::Com::{CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx},
    UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowTextW},
};

use super::{Button, Combo, ComboKey, Key, Media, Modifier, Power, Request, take_whole};

pub struct Desktop {
    enigo: Enigo,
    /// ponytail: default output device at startup; follow device changes if switching outputs matters.
    volume: Option<IAudioEndpointVolume>,
    move_rest: (f64, f64),
    scroll_rest: (f64, f64),
}

impl Desktop {
    pub fn new() -> Result<Self, String> {
        let enigo = Enigo::new(&Settings::default()).map_err(|e| e.to_string())?;
        // No audio device is not an error: volume is just unavailable.
        let volume = unsafe { audio_endpoint() }.map_err(|e| tracing::warn!("no audio endpoint: {e}")).ok();
        Ok(Self { enigo, volume, move_rest: (0.0, 0.0), scroll_rest: (0.0, 0.0) })
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
            Request::Media { action } => self.enigo.key(media_key(*action), Click),
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

    pub fn active_window(&self) -> Option<String> {
        let mut title = [0u16; 512];
        let len = unsafe { GetWindowTextW(GetForegroundWindow(), &mut title) };
        (len > 0).then(|| String::from_utf16_lossy(&title[..len as usize]))
    }

    fn endpoint(&self) -> Result<&IAudioEndpointVolume, String> {
        self.volume.as_ref().ok_or_else(|| "no audio output device".to_string())
    }
}

unsafe fn audio_endpoint() -> windows::core::Result<IAudioEndpointVolume> {
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
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
    }
}
