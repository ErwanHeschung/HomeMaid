//! Compares desired and observed state and drives commands until they are confirmed.
//!
//! Pure state machine: time is passed in and outputs are collected with [`Reconciler::drain`],
//! so the runtime decides where commands and changes go.

use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

use serde::Serialize;
use uuid::Uuid;

use crate::model::{CapKind, CapSlot, CapValue, CommandStatus, Entity};

const BASE_DELAY: Duration = Duration::from_secs(1);
const MAX_DELAY: Duration = Duration::from_secs(30);
const MAX_ATTEMPTS: u32 = 5;

/// Order for the driver (plugin host) to apply a desired value.
#[derive(Debug, Clone, PartialEq)]
pub struct Command {
    pub entity: Uuid,
    pub cap: CapKind,
    pub value: CapValue,
}

/// A slot that changed, pushed to API subscribers.
#[derive(Debug, Clone, Serialize)]
pub struct Change {
    pub id: Uuid,
    pub cap: CapKind,
    pub slot: CapSlot,
}

#[derive(Debug)]
pub enum Output {
    Command(Command),
    Changed(Change),
    /// The entity itself changed: discovered, availability, name, room or tags.
    Entity(Entity),
}

pub struct Reconciler {
    pub entities: BTreeMap<Uuid, Entity>,
    outbox: Vec<Output>,
}

impl Reconciler {
    pub fn new(entities: BTreeMap<Uuid, Entity>) -> Self {
        Self { entities, outbox: Vec::new() }
    }

    pub fn drain(&mut self) -> Vec<Output> {
        std::mem::take(&mut self.outbox)
    }

    /// Adds or refreshes an entity discovered by its plugin. Known slots keep their state,
    /// capabilities and metadata come from the plugin. The entity comes online with its desired state.
    pub fn upsert(&mut self, mut entity: Entity, now: Instant) {
        let id = entity.id;
        if let Some(mut previous) = self.entities.remove(&id) {
            for (kind, slot) in &mut entity.caps {
                if let Some(old) = previous.caps.remove(kind) {
                    let meta = std::mem::take(&mut slot.meta);
                    *slot = old;
                    slot.meta = meta;
                }
            }
        }
        // Coming online emits the whole entity, slots included.
        entity.available = false;
        self.entities.insert(id, entity);
        self.set_available(id, true, now);
    }

    /// Changes user metadata (name, room, tags) and notifies subscribers.
    pub fn edit(&mut self, id: Uuid, edit: impl FnOnce(&mut Entity)) -> Result<(), String> {
        let entity = self.entities.get_mut(&id).ok_or("unknown entity")?;
        edit(entity);
        self.outbox.push(Output::Entity(entity.clone()));
        Ok(())
    }

    pub fn set_desired(
        &mut self,
        id: Uuid,
        cap: &CapKind,
        value: CapValue,
        now: Instant,
    ) -> Result<CommandStatus, String> {
        let entity = self.entities.get_mut(&id).ok_or("unknown entity")?;
        let slot = entity.caps.get_mut(cap).ok_or_else(|| format!("entity has no {cap} capability"))?;
        slot.meta.validate(cap, &value)?;
        slot.status = if slot.observed.as_ref() == Some(&value) {
            CommandStatus::Confirmed
        } else {
            CommandStatus::Pending { attempts: 0 }
        };
        slot.desired = Some(value);
        slot.next_retry = None;
        if entity.available {
            drive(id, cap, slot, now, &mut self.outbox);
        }
        changed(id, cap, slot, &mut self.outbox);
        Ok(slot.status.clone())
    }

    /// State reported by the device.
    pub fn observe(&mut self, id: Uuid, cap: &CapKind, value: CapValue) {
        let Some(slot) = self.entities.get_mut(&id).and_then(|e| e.caps.get_mut(cap)) else {
            return;
        };
        // Same value reported again (periodic refresh): nothing can change, don't wake subscribers or the snapshot.
        if slot.observed.as_ref() == Some(&value) {
            return;
        }
        if slot.desired.as_ref() == Some(&value) {
            slot.status = CommandStatus::Confirmed;
            slot.next_retry = None;
        } else if !slot.meta.read_only && !matches!(slot.status, CommandStatus::Pending { .. }) {
            // Changed outside the hub (physical button, vendor app): adopt it instead of fighting the
            // user. A failed wish too, or it would come back as pending on the next reconnection.
            slot.desired = Some(value.clone());
            slot.status = CommandStatus::Confirmed;
        }
        slot.observed = Some(value);
        changed(id, cap, slot, &mut self.outbox);
    }

    pub fn set_available(&mut self, id: Uuid, available: bool, now: Instant) {
        let Some(entity) = self.entities.get_mut(&id) else { return };
        if entity.available == available {
            return;
        }
        entity.available = available;
        if available {
            // Back online: it gets its desired state with a fresh retry budget.
            for (cap, slot) in &mut entity.caps {
                if slot.desired.is_some() && slot.desired != slot.observed {
                    slot.status = CommandStatus::Pending { attempts: 0 };
                    slot.next_retry = None;
                    drive(id, cap, slot, now, &mut self.outbox);
                }
            }
        }
        self.outbox.push(Output::Entity(entity.clone()));
    }

    /// Retries due commands. ponytail: O(n) scan of every slot, a priority queue if it ever matters.
    pub fn tick(&mut self, now: Instant) {
        for entity in self.entities.values_mut().filter(|e| e.available) {
            for (cap, slot) in &mut entity.caps {
                if drive(entity.id, cap, slot, now, &mut self.outbox) {
                    changed(entity.id, cap, slot, &mut self.outbox);
                }
            }
        }
    }
}

/// Sends the pending command of a slot when due, or fails it when out of attempts.
/// Returns whether the slot changed.
fn drive(id: Uuid, cap: &CapKind, slot: &mut CapSlot, now: Instant, outbox: &mut Vec<Output>) -> bool {
    let CommandStatus::Pending { attempts } = slot.status else { return false };
    let Some(value) = slot.desired.clone() else { return false };
    if slot.next_retry.is_some_and(|t| now < t) {
        return false;
    }
    if attempts >= MAX_ATTEMPTS {
        slot.status = CommandStatus::Failed { reason: format!("not confirmed after {attempts} attempts") };
        slot.next_retry = None;
        return true;
    }
    let attempts = attempts + 1;
    slot.status = CommandStatus::Pending { attempts };
    slot.next_retry = Some(now + backoff(attempts));
    outbox.push(Output::Command(Command { entity: id, cap: cap.clone(), value }));
    true
}

fn backoff(attempts: u32) -> Duration {
    BASE_DELAY.saturating_mul(1 << (attempts - 1).min(16)).min(MAX_DELAY)
}

fn changed(id: Uuid, cap: &CapKind, slot: &CapSlot, outbox: &mut Vec<Output>) {
    outbox.push(Output::Changed(Change { id, cap: cap.clone(), slot: slot.clone() }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::CapMeta;

    const LAMP: Uuid = Uuid::from_u128(1);

    fn lamp() -> Reconciler {
        let caps = BTreeMap::from([
            (CapKind::OnOff, CapSlot::default()),
            (
                CapKind::Brightness,
                CapSlot { meta: CapMeta { max: Some(100.0), ..Default::default() }, ..Default::default() },
            ),
            (CapKind::Power, CapSlot { meta: CapMeta { read_only: true, ..Default::default() }, ..Default::default() }),
        ]);
        let entity = Entity {
            id: LAMP,
            name: "lamp".into(),
            room: None,
            tags: vec![],
            available: true,
            plugin: "mock".into(),
            local_id: "lamp".into(),
            device: None,
            added: None,
            model: None,
            actions: vec![],
            caps,
        };
        Reconciler::new(BTreeMap::from([(LAMP, entity)]))
    }

    fn commands(rec: &mut Reconciler) -> Vec<Command> {
        rec.drain()
            .into_iter()
            .filter_map(|o| match o {
                Output::Command(c) => Some(c),
                _ => None,
            })
            .collect()
    }

    fn status(rec: &Reconciler) -> CommandStatus {
        rec.entities[&LAMP].caps[&CapKind::OnOff].status.clone()
    }

    fn turn_on() -> Command {
        Command { entity: LAMP, cap: CapKind::OnOff, value: CapValue::OnOff(true) }
    }

    #[test]
    fn new_desired_value_sends_a_command() {
        let mut rec = lamp();
        rec.set_desired(LAMP, &CapKind::OnOff, CapValue::OnOff(true), Instant::now()).unwrap();
        assert_eq!(commands(&mut rec), [turn_on()]);
        assert_eq!(status(&rec), CommandStatus::Pending { attempts: 1 });
    }

    #[test]
    fn matching_observation_confirms() {
        let mut rec = lamp();
        let now = Instant::now();
        rec.set_desired(LAMP, &CapKind::OnOff, CapValue::OnOff(true), now).unwrap();
        rec.observe(LAMP, &CapKind::OnOff, CapValue::OnOff(true));
        assert_eq!(status(&rec), CommandStatus::Confirmed);
        rec.drain();
        rec.tick(now + Duration::from_secs(3600));
        assert!(commands(&mut rec).is_empty());
    }

    #[test]
    fn unconfirmed_command_is_retried_then_fails() {
        let mut rec = lamp();
        let mut t = Instant::now();
        rec.set_desired(LAMP, &CapKind::OnOff, CapValue::OnOff(true), t).unwrap();
        assert_eq!(commands(&mut rec).len(), 1);
        rec.tick(t);
        assert!(commands(&mut rec).is_empty(), "retry before the backoff delay");
        for attempt in 2..=MAX_ATTEMPTS {
            t += MAX_DELAY;
            rec.tick(t);
            assert_eq!(commands(&mut rec), [turn_on()]);
            assert_eq!(status(&rec), CommandStatus::Pending { attempts: attempt });
        }
        t += MAX_DELAY;
        rec.tick(t);
        assert!(commands(&mut rec).is_empty());
        assert!(matches!(status(&rec), CommandStatus::Failed { .. }));

        // Then switched by hand: adopted, so a reconnection does not send the failed wish again.
        rec.observe(LAMP, &CapKind::OnOff, CapValue::OnOff(false));
        assert_eq!(status(&rec), CommandStatus::Confirmed);
        rec.set_available(LAMP, false, t);
        rec.set_available(LAMP, true, t);
        assert!(commands(&mut rec).is_empty());
    }

    #[test]
    fn offline_entity_gets_desired_state_when_back() {
        let mut rec = lamp();
        let now = Instant::now();
        rec.set_available(LAMP, false, now);
        rec.set_desired(LAMP, &CapKind::OnOff, CapValue::OnOff(true), now).unwrap();
        rec.tick(now + Duration::from_secs(600));
        assert!(commands(&mut rec).is_empty());
        assert_eq!(status(&rec), CommandStatus::Pending { attempts: 0 });
        rec.set_available(LAMP, true, now + Duration::from_secs(601));
        assert_eq!(commands(&mut rec), [turn_on()]);
    }

    #[test]
    fn external_change_becomes_desired() {
        let mut rec = lamp();
        rec.observe(LAMP, &CapKind::OnOff, CapValue::OnOff(true));
        let slot = &rec.entities[&LAMP].caps[&CapKind::OnOff];
        assert_eq!(slot.desired, Some(CapValue::OnOff(true)));
        assert_eq!(slot.status, CommandStatus::Confirmed);
        assert!(commands(&mut rec).is_empty());
    }

    #[test]
    fn invalid_values_are_rejected() {
        let mut rec = lamp();
        let now = Instant::now();
        assert!(rec.set_desired(LAMP, &CapKind::Brightness, CapValue::Brightness(150), now).is_err());
        assert!(rec.set_desired(LAMP, &CapKind::Brightness, CapValue::OnOff(true), now).is_err());
        assert!(rec.set_desired(LAMP, &CapKind::Power, CapValue::Power { watts: 1.0 }, now).is_err());
        assert!(rec.set_desired(LAMP, &CapKind::Custom("x".into()), CapValue::OnOff(true), now).is_err());
        assert!(rec.set_desired(Uuid::nil(), &CapKind::OnOff, CapValue::OnOff(true), now).is_err());
        assert!(rec.drain().is_empty());
        assert!(rec.set_desired(LAMP, &CapKind::Brightness, CapValue::Brightness(100), now).is_ok());
    }

    #[test]
    fn rediscovery_keeps_state_and_takes_new_meta() {
        let mut rec = lamp();
        let now = Instant::now();
        rec.set_available(LAMP, false, now);
        rec.set_desired(LAMP, &CapKind::OnOff, CapValue::OnOff(true), now).unwrap();
        let mut discovered = rec.entities[&LAMP].clone();
        discovered.caps = BTreeMap::from([
            (CapKind::OnOff, CapSlot { meta: CapMeta { unit: Some("x".into()), ..Default::default() }, ..Default::default() }),
            (CapKind::Custom("new".into()), CapSlot::default()),
        ]);
        rec.drain();
        rec.upsert(discovered, now);

        let entity = &rec.entities[&LAMP];
        assert!(entity.available);
        assert_eq!(entity.caps.len(), 2, "capabilities no longer declared are dropped");
        let slot = &entity.caps[&CapKind::OnOff];
        assert_eq!(slot.desired, Some(CapValue::OnOff(true)));
        assert_eq!(slot.meta.unit.as_deref(), Some("x"));
        assert_eq!(commands(&mut rec), [turn_on()], "desired state is sent once back online");
    }

    #[test]
    fn repeated_observation_is_silent() {
        let mut rec = lamp();
        rec.observe(LAMP, &CapKind::Power, CapValue::Power { watts: 3.0 });
        assert_eq!(rec.drain().len(), 1);
        rec.observe(LAMP, &CapKind::Power, CapValue::Power { watts: 3.0 });
        assert!(rec.drain().is_empty());
    }

    #[test]
    fn entity_events_on_availability_and_edit() {
        let mut rec = lamp();
        let now = Instant::now();
        let entities = |rec: &mut Reconciler| -> Vec<Entity> {
            rec.drain()
                .into_iter()
                .filter_map(|o| match o {
                    Output::Entity(e) => Some(e),
                    _ => None,
                })
                .collect()
        };
        rec.set_available(LAMP, false, now);
        assert!(entities(&mut rec).iter().any(|e| !e.available));
        rec.set_available(LAMP, false, now);
        assert!(entities(&mut rec).is_empty(), "no event without a change");
        rec.edit(LAMP, |e| e.room = Some("salon".into())).unwrap();
        assert_eq!(entities(&mut rec)[0].room.as_deref(), Some("salon"));
        assert!(rec.edit(Uuid::nil(), |_| {}).is_err());
    }
}
