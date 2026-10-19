//! SDK for HomeMaid plugins (plugin-api 1.2).
//!
//! ```ignore
//! struct MyPlugin;
//! impl homemaid_plugin::Plugin for MyPlugin { /* ... */ }
//! homemaid_plugin::export!(MyPlugin with_types_in homemaid_plugin::bindings);
//! ```
//! A plugin with one-shot actions (1.1) also implements [`Actions`] and exports with
//! `homemaid_plugin::export_with_actions!(MyPlugin with_types_in homemaid_plugin::bindings_actions);`.
//! A plugin whose devices run schedules themselves (1.2) implements [`Actions`] and [`Schedules`]
//! and exports with
//! `homemaid_plugin::export_with_schedules!(MyPlugin with_types_in homemaid_plugin::bindings_schedules);`.
//! Build with `cargo build --target wasm32-wasip2`.

use std::collections::BTreeMap;

pub mod http;

pub use caps::{CapKind, CapMeta, CapValue};

#[doc(hidden)]
pub mod bindings {
    wit_bindgen::generate!({
        path: "wit",
        world: "plugin",
        pub_export_macro: true,
        export_macro_name: "export",
        default_bindings_module: "homemaid_plugin::bindings",
    });
}

/// The 1.1 world (plugin + actions), sharing the host and types of [`bindings`].
#[doc(hidden)]
pub mod bindings_actions {
    wit_bindgen::generate!({
        path: "wit",
        world: "plugin-with-actions",
        pub_export_macro: true,
        export_macro_name: "export_with_actions",
        default_bindings_module: "homemaid_plugin::bindings_actions",
        with: {
            "homemaid:plugin/types@1.2.0": crate::bindings::homemaid::plugin::types,
            "homemaid:plugin/host@1.2.0": crate::bindings::homemaid::plugin::host,
        },
    });
}

/// The 1.2 world (plugin + actions + schedules), sharing the host and types of [`bindings`].
#[doc(hidden)]
pub mod bindings_schedules {
    wit_bindgen::generate!({
        path: "wit",
        world: "plugin-with-schedules",
        pub_export_macro: true,
        export_macro_name: "export_with_schedules",
        default_bindings_module: "homemaid_plugin::bindings_schedules",
        with: {
            "homemaid:plugin/types@1.2.0": crate::bindings::homemaid::plugin::types,
            "homemaid:plugin/host@1.2.0": crate::bindings::homemaid::plugin::host,
            "homemaid:plugin/action-types@1.2.0": crate::bindings_actions::homemaid::plugin::action_types,
        },
    });
}

pub use bindings::export;
pub use bindings_actions::export_with_actions;
pub use bindings_schedules::export_with_schedules;
use bindings::{Guest, homemaid::plugin::types};
pub use bindings::homemaid::plugin::host::{kv_get, kv_set, log, secret, set_available};
pub use bindings::homemaid::plugin::types::LogLevel;

/// Version of the WIT contract this SDK implements.
pub const API_VERSION: &str = "1.2.0";

/// Permission to open sockets to the local network (the core refuses public addresses).
pub const LOCAL_NETWORK: &str = "local-network";

pub struct Manifest {
    pub id: String,
    pub version: String,
    pub permissions: Vec<String>,
}

pub struct DiscoveredEntity {
    pub local_id: String,
    pub name: String,
    pub device: Option<String>,
    pub caps: Vec<(CapKind, CapMeta)>,
}

pub trait Plugin {
    fn manifest() -> Manifest;
    fn init(settings: BTreeMap<String, String>) -> Result<(), String>;
    fn discover() -> Result<Vec<DiscoveredEntity>, String>;
    /// Applies a desired value. Confirm it with [`report_state`] once the device reports it,
    /// typically from [`Plugin::refresh`], which the host calls right after.
    fn apply(local_id: &str, kind: CapKind, value: CapValue) -> Result<(), String>;
    /// Called right after commands and between them. Returns the delay in ms before the next call.
    fn refresh() -> u32;
}

/// A one-shot action of an entity (media next, lock...): not a state, never retried.
pub struct ActionDecl {
    pub local_id: String,
    pub name: String,
    /// JSON schema of the parameters; none takes no parameters.
    pub params: Option<serde_json::Value>,
}

/// Plugins with actions (plugin-api 1.1).
pub trait Actions: Plugin {
    fn actions() -> Vec<ActionDecl>;
    /// Does it now. `params` is `{}` for an action without parameters.
    fn invoke(local_id: &str, name: &str, params: serde_json::Value) -> Result<(), String>;
}

/// When a schedule fires, in the device's own time zone (plugin-api 1.2).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TimeOfDay {
    /// Minutes after midnight.
    Clock(u16),
    /// Minutes after sunrise, negative before; the device computes its sunrise.
    Sunrise(i16),
    Sunset(i16),
}

/// A schedule handed over by the core: set `kind` to `value` at `at` on `days` (Monday = bit 0).
#[derive(Debug, Clone, PartialEq)]
pub struct Schedule {
    /// The automation it comes from.
    pub id: String,
    pub kind: CapKind,
    pub value: CapValue,
    pub at: TimeOfDay,
    pub days: u8,
}

/// Plugins whose devices run schedules themselves (plugin-api 1.2).
pub trait Schedules: Actions {
    /// Replaces every schedule handed over before for this entity (empty: remove them all).
    /// Never touch the device's other rules. Ok only once the device holds them; Err otherwise
    /// (unsupported, unreachable: you may still apply them when it answers).
    fn set_schedules(local_id: &str, schedules: Vec<Schedule>) -> Result<(), String>;
}

pub fn report_state(local_id: &str, kind: &CapKind, value: &CapValue) {
    let value = serde_json::to_string(value).expect("CapValue always serializes");
    bindings::homemaid::plugin::host::report_state(local_id, &kind.to_string(), &value);
}

impl<T: Plugin> Guest for T {
    fn manifest() -> types::Manifest {
        let m = T::manifest();
        types::Manifest { id: m.id, version: m.version, api_version: API_VERSION.into(), permissions: m.permissions }
    }

    fn init(settings: Vec<(String, String)>) -> Result<(), String> {
        T::init(settings.into_iter().collect())
    }

    fn discover() -> Result<Vec<types::Entity>, String> {
        let entities = T::discover()?
            .into_iter()
            .map(|e| types::Entity {
                local_id: e.local_id,
                name: e.name,
                device: e.device,
                capabilities: e
                    .caps
                    .into_iter()
                    .map(|(kind, meta)| types::Capability {
                        kind: kind.to_string(),
                        meta: serde_json::to_string(&meta).expect("CapMeta always serializes"),
                    })
                    .collect(),
            })
            .collect();
        Ok(entities)
    }

    fn apply(local_id: String, kind: String, value: String) -> Result<(), String> {
        let kind = kind.parse()?;
        let value = serde_json::from_str(&value).map_err(|e| format!("invalid value: {e}"))?;
        T::apply(&local_id, kind, value)
    }

    fn refresh() -> u32 {
        T::refresh()
    }
}

impl<T: Actions> bindings_actions::Guest for T {
    fn manifest() -> types::Manifest {
        <T as Guest>::manifest()
    }

    fn init(settings: Vec<(String, String)>) -> Result<(), String> {
        <T as Guest>::init(settings)
    }

    fn discover() -> Result<Vec<types::Entity>, String> {
        <T as Guest>::discover()
    }

    fn apply(local_id: String, kind: String, value: String) -> Result<(), String> {
        <T as Guest>::apply(local_id, kind, value)
    }

    fn refresh() -> u32 {
        <T as Guest>::refresh()
    }

    fn list_actions() -> Vec<bindings_actions::homemaid::plugin::action_types::Action> {
        T::actions()
            .into_iter()
            .map(|a| bindings_actions::homemaid::plugin::action_types::Action {
                local_id: a.local_id,
                name: a.name,
                params: a.params.map(|p| p.to_string()),
            })
            .collect()
    }

    fn invoke(local_id: String, name: String, params: String) -> Result<(), String> {
        let params = serde_json::from_str(&params).map_err(|e| format!("invalid params: {e}"))?;
        T::invoke(&local_id, &name, params)
    }
}

impl<T: Schedules> bindings_schedules::Guest for T {
    fn manifest() -> types::Manifest {
        <T as Guest>::manifest()
    }

    fn init(settings: Vec<(String, String)>) -> Result<(), String> {
        <T as Guest>::init(settings)
    }

    fn discover() -> Result<Vec<types::Entity>, String> {
        <T as Guest>::discover()
    }

    fn apply(local_id: String, kind: String, value: String) -> Result<(), String> {
        <T as Guest>::apply(local_id, kind, value)
    }

    fn refresh() -> u32 {
        <T as Guest>::refresh()
    }

    fn list_actions() -> Vec<bindings_actions::homemaid::plugin::action_types::Action> {
        <T as bindings_actions::Guest>::list_actions()
    }

    fn invoke(local_id: String, name: String, params: String) -> Result<(), String> {
        <T as bindings_actions::Guest>::invoke(local_id, name, params)
    }

    fn set_schedules(local_id: String, schedules: Vec<bindings_schedules::homemaid::plugin::schedule_types::Schedule>) -> Result<(), String> {
        use bindings_schedules::homemaid::plugin::schedule_types::TimeOfDay as Wit;
        let schedules = schedules
            .into_iter()
            .map(|s| {
                let kind: CapKind = s.kind.parse()?;
                let value = serde_json::from_str(&s.value).map_err(|e| format!("invalid value: {e}"))?;
                let at = match s.at {
                    Wit::Clock(m) => TimeOfDay::Clock(m),
                    Wit::Sunrise(o) => TimeOfDay::Sunrise(o),
                    Wit::Sunset(o) => TimeOfDay::Sunset(o),
                };
                Ok(Schedule { id: s.id, kind, value, at, days: s.days })
            })
            .collect::<Result<_, String>>()?;
        T::set_schedules(&local_id, schedules)
    }
}
