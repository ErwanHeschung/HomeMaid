use std::{collections::BTreeMap, time::Instant};

pub use caps::{CapKind, CapMeta, CapValue};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum CommandStatus {
    #[default]
    Idle,
    Pending {
        attempts: u32,
    },
    Confirmed,
    Failed {
        reason: String,
    },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CapSlot {
    pub meta: CapMeta,
    pub observed: Option<CapValue>,
    pub desired: Option<CapValue>,
    pub status: CommandStatus,
    /// When the pending command is (re)sent; `None` means as soon as possible.
    #[serde(skip)]
    pub next_retry: Option<Instant>,
}

/// A controllable or measurable element, e.g. one outlet of a power strip.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    pub id: Uuid,
    pub name: String,
    pub room: Option<String>,
    pub tags: Vec<String>,
    pub available: bool,
    /// Plugin that drives this entity, and the entity id inside that plugin.
    pub plugin: String,
    pub local_id: String,
    /// Physical device grouping entities (the power strip of an outlet), as named by the plugin.
    pub device: Option<String>,
    /// The device the user added it with, from the app (`[[plugin.device]]` id, plugin-api 1.3):
    /// removing that device removes it.
    #[serde(default)]
    pub added: Option<String>,
    /// Which 3D model the app draws (`kettle`, `tv`...), chosen by the user; the app infers one
    /// from the capabilities when absent.
    #[serde(default)]
    pub model: Option<String>,
    pub caps: BTreeMap<CapKind, CapSlot>,
    /// One-shot actions the plugin offers (media next, lock...): not states, never retried.
    #[serde(default)]
    pub actions: Vec<ActionDecl>,
}

/// An action an entity can do, declared by its plugin (plugin-api 1.1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionDecl {
    pub name: String,
    /// JSON schema of the parameters; none takes no parameters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

const NAMESPACE: Uuid = Uuid::from_u128(0x5d1e_8c4a_9b3f_4e27_a6d0_1f2c_3b4a_5e6f);

/// Stable id derived from the plugin's own ids, so nothing has to be stored to keep it.
pub fn entity_uuid(plugin: &str, local_id: &str) -> Uuid {
    Uuid::new_v5(&NAMESPACE, format!("{plugin}/{local_id}").as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_uuid_is_stable_and_scoped_by_plugin() {
        assert_eq!(entity_uuid("tplink", "a/1"), entity_uuid("tplink", "a/1"));
        assert_ne!(entity_uuid("tplink", "a/1"), entity_uuid("other", "a/1"));
        // Pinned: changing the namespace would break every reference to every entity.
        assert_eq!(entity_uuid("mock", "lamp").to_string(), "b2ab420e-3e8d-569b-80ee-016956537f82");
    }
}
