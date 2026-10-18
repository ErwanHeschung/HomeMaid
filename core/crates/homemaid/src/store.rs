//! Persistent snapshot of the current state, so desired values survive a restart.

use std::{collections::BTreeMap, fs, io, path::Path};

use uuid::Uuid;

use crate::{config::EntityOverride, model::Entity};

pub fn load(path: &Path) -> io::Result<BTreeMap<Uuid, Entity>> {
    match fs::read(path) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(e) => Err(e),
    }
}

pub fn save(path: &Path, entities: &BTreeMap<Uuid, Entity>) -> io::Result<()> {
    write_atomic(path, &serde_json::to_vec_pretty(entities)?)
}

/// Writes to a temporary file then renames it, so a crash never leaves a truncated file.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(tmp, path)
}

/// Entities stay unavailable until their plugin discovers them again.
pub fn restore(snapshot: BTreeMap<Uuid, Entity>, overrides: &[EntityOverride]) -> BTreeMap<Uuid, Entity> {
    let mut entities = snapshot;
    for entity in entities.values_mut() {
        entity.available = false;
    }
    for o in overrides {
        if let Some(entity) = entities.get_mut(&o.id) {
            o.apply(entity);
        }
    }
    entities
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{CapKind, CapSlot, CapValue, CommandStatus};

    #[test]
    fn desired_state_survives_a_restart() {
        let id = Uuid::from_u128(7);
        let slot = CapSlot {
            desired: Some(CapValue::OnOff(true)),
            status: CommandStatus::Pending { attempts: 2 },
            ..Default::default()
        };
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
            caps: BTreeMap::from([(CapKind::OnOff, slot)]),
        };

        let path = std::env::temp_dir().join(format!("homemaid-store-test-{}.json", std::process::id()));
        save(&path, &BTreeMap::from([(id, entity)])).unwrap();
        let overrides = [EntityOverride { id, name: None, room: Some("salon".into()), tags: None, model: None }];
        let restored = restore(load(&path).unwrap(), &overrides);
        fs::remove_file(&path).unwrap();

        let entity = &restored[&id];
        assert!(!entity.available);
        assert_eq!(entity.room.as_deref(), Some("salon"));
        let slot = &entity.caps[&CapKind::OnOff];
        assert_eq!(slot.desired, Some(CapValue::OnOff(true)));
        assert_eq!(slot.status, CommandStatus::Pending { attempts: 2 });
        assert!(slot.next_retry.is_none(), "restored commands are resent right away");
    }
}
