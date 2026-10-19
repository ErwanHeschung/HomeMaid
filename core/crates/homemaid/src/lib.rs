pub mod agent;
pub mod automations;
pub mod api;
pub mod config;
pub mod model;
pub mod pairing;
pub mod plugins;
pub mod reconciler;
pub mod selector;
pub mod server;
pub mod store;

use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, Mutex, OnceLock},
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
    /// An automation ran (or was skipped, with why).
    Automation { id: String, run: automations::Run },
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
    /// The plugin host, set once started: actions are invoked through it.
    pub plugins: OnceLock<Arc<plugins::Plugins>>,
    /// The configured automations, set once started.
    pub automations: OnceLock<Arc<automations::Automations>>,
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
            plugins: OnceLock::new(),
            automations: OnceLock::new(),
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
                    // A sensor (read-only) has nothing to keep: no desired state, and its value is
                    // read again at start. Saving it would rewrite the snapshot every few seconds.
                    if !change.slot.meta.read_only {
                        self.dirty.notify_one();
                    }
                    let _ = self.changes.send(Event::Slot(change));
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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use model::{CapKind, CapMeta, CapSlot, CapValue};

    #[tokio::test]
    async fn sensor_readings_do_not_rewrite_the_snapshot() {
        let id = Uuid::from_u128(3);
        let sensor = CapSlot { meta: CapMeta { read_only: true, ..Default::default() }, ..Default::default() };
        let entity = Entity {
            id,
            name: "plug".into(),
            room: None,
            tags: vec![],
            available: true,
            plugin: "mock".into(),
            local_id: "plug".into(),
            device: None,
            model: None,
            actions: vec![],
            caps: BTreeMap::from([(CapKind::Power, sensor), (CapKind::OnOff, CapSlot::default())]),
        };
        let core = Core::new(Reconciler::new(BTreeMap::from([(id, entity)])), mpsc::unbounded_channel().0);
        let dirty = async || tokio::time::timeout(Duration::from_millis(10), core.dirty.notified()).await.is_ok();

        core.with(|r| r.observe(id, &CapKind::Power, CapValue::Power { watts: 4.2 }));
        assert!(!dirty().await);
        core.with(|r| r.observe(id, &CapKind::OnOff, CapValue::OnOff(true)));
        assert!(dirty().await, "a switch pressed by hand becomes the desired state: saved");
    }
}
