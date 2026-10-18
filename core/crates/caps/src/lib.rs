//! Capability types shared by the core and the plugin SDK.

use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

/// Capability type. Serialized as a string (`on_off`, `custom:<name>`) so it can key maps.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub enum CapKind {
    OnOff,
    Brightness,
    Power,
    Volume,
    Mute,
    /// Declared by a plugin, rendered generically from its schema.
    Custom(String),
}

impl fmt::Display for CapKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            CapKind::OnOff => f.write_str("on_off"),
            CapKind::Brightness => f.write_str("brightness"),
            CapKind::Power => f.write_str("power"),
            CapKind::Volume => f.write_str("volume"),
            CapKind::Mute => f.write_str("mute"),
            CapKind::Custom(name) => write!(f, "custom:{name}"),
        }
    }
}

impl FromStr for CapKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "on_off" => Ok(CapKind::OnOff),
            "brightness" => Ok(CapKind::Brightness),
            "power" => Ok(CapKind::Power),
            "volume" => Ok(CapKind::Volume),
            "mute" => Ok(CapKind::Mute),
            _ => match s.strip_prefix("custom:") {
                Some(name) if !name.is_empty() => Ok(CapKind::Custom(name.to_string())),
                _ => Err(format!("unknown capability `{s}`")),
            },
        }
    }
}

impl From<CapKind> for String {
    fn from(kind: CapKind) -> String {
        kind.to_string()
    }
}

impl TryFrom<String> for CapKind {
    type Error = String;

    fn try_from(s: String) -> Result<Self, String> {
        s.parse()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapValue {
    OnOff(bool),
    Brightness(u8),
    Power { watts: f32 },
    /// 0-100.
    Volume(u8),
    Mute(bool),
    Custom(serde_json::Value),
}

impl CapValue {
    fn fits(&self, kind: &CapKind) -> bool {
        matches!(
            (kind, self),
            (CapKind::OnOff, CapValue::OnOff(_))
                | (CapKind::Brightness, CapValue::Brightness(_))
                | (CapKind::Power, CapValue::Power { .. })
                | (CapKind::Volume, CapValue::Volume(_))
                | (CapKind::Mute, CapValue::Mute(_))
                | (CapKind::Custom(_), CapValue::Custom(_))
        )
    }

    fn number(&self) -> Option<f64> {
        match self {
            CapValue::Brightness(v) | CapValue::Volume(v) => Some(f64::from(*v)),
            CapValue::Power { watts } => Some(f64::from(*watts)),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CapMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    /// Sensors (power, temperature...) are observed only.
    #[serde(default)]
    pub read_only: bool,
    /// JSON schema of a custom capability, used by the UI for generic rendering.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<serde_json::Value>,
}

impl CapMeta {
    /// Checks that `value` can be set as the desired state of a `kind` capability.
    pub fn validate(&self, kind: &CapKind, value: &CapValue) -> Result<(), String> {
        if self.read_only {
            return Err(format!("{kind} is read-only"));
        }
        if !value.fits(kind) {
            return Err(format!("value does not match capability {kind}"));
        }
        if let Some(n) = value.number()
            && (self.min.is_some_and(|min| n < min) || self.max.is_some_and(|max| n > max))
        {
            return Err(format!("{n} is out of bounds for {kind}"));
        }
        // ponytail: custom values are not checked against their schema, add a JSON schema validator when a plugin needs it
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cap_kind_parses_and_prints() {
        assert_eq!("custom:fan".parse(), Ok(CapKind::Custom("fan".into())));
        assert_eq!(CapKind::Custom("fan".into()).to_string(), "custom:fan");
        assert_eq!("on_off".parse(), Ok(CapKind::OnOff));
        assert!("custom:".parse::<CapKind>().is_err());
        assert!("nope".parse::<CapKind>().is_err());
    }
}
