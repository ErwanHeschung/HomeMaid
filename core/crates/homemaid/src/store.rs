//! Persistent snapshot of the current state, so desired values survive a restart.

use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{self, Write},
    path::Path,
    sync::atomic::{AtomicU32, Ordering},
};

use uuid::Uuid;

use crate::{config::EntityOverride, model::Entity};

/// A snapshot that cannot be read back (cut short, an unknown format) is only a cache: it is set
/// aside as `<name>.bad` and the hub starts without it rather than not at all.
pub fn load(path: &Path) -> io::Result<BTreeMap<Uuid, Entity>> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(e) => return Err(e),
    };
    serde_json::from_slice(&bytes).or_else(|e| {
        let mut bad = path.as_os_str().to_owned();
        bad.push(".bad");
        tracing::error!("{}: {e}; set aside as {}, starting without the saved state", path.display(), bad.display());
        fs::rename(path, &bad)?;
        Ok(BTreeMap::new())
    })
}

pub fn save(path: &Path, entities: &BTreeMap<Uuid, Entity>) -> io::Result<()> {
    write_atomic(path, &serde_json::to_vec_pretty(entities)?)
}

/// Writes to a temporary file, flushed to disk, then renames it: a crash or a power loss leaves
/// the old file or the new one, never a truncated one. Each write has its own temporary file.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(format!(".{}-{}.tmp", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
    let written = File::create(&tmp).and_then(|mut file| {
        file.write_all(bytes)?;
        file.sync_all()
    });
    written.and_then(|()| fs::rename(&tmp, path)).inspect_err(|_| drop(fs::remove_file(&tmp)))
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
            added: None,
            model: None,
            actions: vec![],
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

    #[test]
    fn a_corrupt_snapshot_is_set_aside() {
        let path = std::env::temp_dir().join(format!("homemaid-store-bad-{}.json", std::process::id()));
        fs::write(&path, b"{\"trunc").unwrap();
        assert!(load(&path).unwrap().is_empty());
        let mut bad = path.as_os_str().to_owned();
        bad.push(".bad");
        assert!(!path.exists() && fs::remove_file(&bad).is_ok(), "kept aside for a look");
    }
}
