//! Desktop without a backend (Linux, macOS): records what it would do, so the agent runs and is
//! testable anywhere. ponytail: no real input on Linux yet, add enigo here when a Linux PC needs it.

use std::sync::Mutex;

use super::{Combo, Request};

const KEEP: usize = 1000;

static RECORDED: Mutex<Vec<Request>> = Mutex::new(Vec::new());

/// Last desktop requests handled by this process (at most 1000).
pub fn recorded() -> Vec<Request> {
    RECORDED.lock().unwrap().clone()
}

/// No app discovery here yet.
// ponytail: Linux could read the `.desktop` files of /usr/share/applications when a Linux PC needs it.
pub fn start_apps() -> std::collections::BTreeMap<String, String> {
    Default::default()
}

pub struct Desktop {
    volume: u8,
    muted: bool,
}

impl Desktop {
    pub fn new() -> Result<Self, String> {
        Ok(Self { volume: 50, muted: false })
    }

    pub fn handle(&mut self, request: &Request) -> Result<(), String> {
        tracing::debug!(?request, "no desktop backend on this platform");
        match request {
            Request::SetVolume { level } => self.volume = *level,
            Request::SetMute { muted } => self.muted = *muted,
            _ => {}
        }
        let mut recorded = RECORDED.lock().unwrap();
        if recorded.len() >= KEEP {
            recorded.drain(..KEEP / 2);
        }
        recorded.push(request.clone());
        Ok(())
    }

    pub fn press(&mut self, combo: &Combo) -> Result<(), String> {
        tracing::debug!(?combo, "no desktop backend on this platform");
        Ok(())
    }

    pub fn volume(&self) -> Option<(u8, bool)> {
        Some((self.volume, self.muted))
    }

    pub fn active_window(&self) -> Option<String> {
        None
    }
}
