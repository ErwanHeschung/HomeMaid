//! SDK for HomeMaid plugins (plugin-api v1).
//!
//! ```ignore
//! struct MyPlugin;
//! impl homemaid_plugin::Plugin for MyPlugin { /* ... */ }
//! homemaid_plugin::export!(MyPlugin with_types_in homemaid_plugin::bindings);
//! ```
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

pub use bindings::export;
use bindings::{Guest, homemaid::plugin::types};
pub use bindings::homemaid::plugin::host::{kv_get, kv_set, log, secret, set_available};
pub use bindings::homemaid::plugin::types::LogLevel;

/// Version of the WIT contract this SDK implements.
pub const API_VERSION: &str = "1.0.0";

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
