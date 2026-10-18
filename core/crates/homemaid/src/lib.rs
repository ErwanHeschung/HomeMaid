pub mod agent;
pub mod api;
pub mod config;
pub mod model;
pub mod pairing;
pub mod plugins;
pub mod reconciler;
pub mod store;

use std::{
    collections::{BTreeMap, HashMap},
    sync::Mutex,
};

use tokio::sync::{Notify, broadcast, mpsc};

use config::EntityOverride;
use model::Entity;
use uuid::Uuid;
use reconciler::{Change, Command, Output, Reconciler};

/// What API subscribers are told about.
#[derive(Debug, Clone)]
pub enum Event {
    Slot(Change),
    Entity(Entity),
    Room(config::RoomConfig),
}

/// Shared runtime around the reconciler: routes its outputs to the driver and the API subscribers.
pub struct Core {
    rec: Mutex<Reconciler>,
    commands: mpsc::UnboundedSender<Command>,
    pub changes: broadcast::Sender<Event>,
    /// Signaled when state changed and the snapshot should be saved.
    pub dirty: Notify,
    /// User metadata by entity, applied on every discovery. Edited live by `entity.update`.
    pub overrides: Mutex<HashMap<Uuid, EntityOverride>>,
    /// Room kinds chosen by the user, by room name. Edited live by `room.update`.
    pub rooms: Mutex<BTreeMap<String, String>>,
}

impl Core {
    pub fn new(rec: Reconciler, commands: mpsc::UnboundedSender<Command>) -> Self {
        Self {
            rec: Mutex::new(rec),
            commands,
            changes: broadcast::channel(256).0,
            dirty: Notify::new(),
            overrides: Mutex::default(),
            rooms: Mutex::default(),
        }
    }

    pub fn with<R>(&self, f: impl FnOnce(&mut Reconciler) -> R) -> R {
        let mut rec = self.rec.lock().expect("reconciler lock poisoned");
        let result = f(&mut rec);
        for output in rec.drain() {
            match output {
                Output::Command(c) => {
                    let _ = self.commands.send(c);
                }
                Output::Changed(change) => {
                    let _ = self.changes.send(Event::Slot(change));
                    self.dirty.notify_one();
                }
                Output::Entity(entity) => {
                    let _ = self.changes.send(Event::Entity(entity));
                    self.dirty.notify_one();
                }
            }
        }
        result
    }
}
