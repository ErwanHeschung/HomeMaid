pub mod agent;
pub mod automations;
pub mod api;
pub mod config;
pub mod model;
pub mod pairing;
pub mod plugins;
pub mod reconciler;
pub mod registry;
pub mod selector;
pub mod server;
pub mod store;

use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, Mutex, OnceLock, RwLock},
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
    /// Automations were added, edited or removed.
    Automations,
    /// A plugin was installed, configured or removed, started or failed.
    Plugins,
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
    /// The automations in force (none until started), swapped when edited: see [`Core::automations`].
    automations: RwLock<Arc<automations::Automations>>,
    /// The task running them, replaced with them.
    engine: Mutex<Option<tokio::task::JoinHandle<()>>>,
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
            automations: RwLock::new(Arc::new(automations::Automations::none())),
            engine: Mutex::default(),
        }
    }

    /// The automations in force. Hold the `Arc` for one use: an edit replaces them.
    pub fn automations(&self) -> Arc<automations::Automations> {
        self.automations.read().unwrap().clone()
    }

    /// Puts `next` in force: their runs carry over by id, the engine restarts from now (missed
    /// times are not caught up), devices get their new schedules, and subscribers are told.
    /// Needs a tokio runtime.
    pub fn set_automations(self: &Arc<Self>, next: automations::Automations) {
        let next = Arc::new(next);
        next.carry_runs(&self.automations());
        *self.automations.write().unwrap() = next.clone();
        let mut engine = self.engine.lock().unwrap();
        if let Some(old) = engine.take() {
            old.abort();
        }
        if !next.is_empty() {
            *engine = Some(tokio::spawn(next.drive(self.clone())));
        }
        drop(engine);
        if let Some(plugins) = self.plugins.get() {
            plugins.reschedule();
        }
        let _ = self.changes.send(Event::Automations);
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
