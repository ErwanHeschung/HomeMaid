//! The plugin registry: an `index.json` on GitHub listing plugins, their versions and where their
//! `.wasm` is. Read only when the user browses or installs (`registry.list`, `plugin.install`):
//! the hub never needs the Internet to run.
//!
//! Trust: each plugin names its author's minisign key. A download must match its sha256 and be
//! signed by that key; once installed, the key is pinned in the config, so an update signed by
//! another key is refused even if the index changes it.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::plugins::{API_MAJOR, API_MINOR};

pub const DEFAULT_URL: &str = "https://raw.githubusercontent.com/ErwanHeschung/HomeMaid-plugins/main/index.json";
const INDEX_LIMIT: u64 = 1 << 20;
const WASM_LIMIT: u64 = 32 << 20;

#[derive(Debug, Deserialize)]
pub struct Index {
    pub format: u32,
    pub plugins: Vec<Entry>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Entry {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub repo: String,
    /// The author's minisign public key (base64, as in the second line of a `.pub` file).
    pub key: String,
    /// What the user fills in at install.
    #[serde(default)]
    pub settings: Vec<Setting>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub versions: Vec<Version>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Setting {
    pub key: String,
    pub label: String,
    /// Kept in `secrets.toml`, never shown again.
    #[serde(default)]
    pub secret: bool,
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Version {
    pub version: String,
    /// The plugin-api it is built for (`1.2`).
    pub api: String,
    pub permissions: Vec<String>,
    pub url: String,
    pub sha256: String,
    /// The whole `.minisig` file.
    pub signature: String,
}

impl Entry {
    /// The newest version this core can run: same major plugin-api, minor not newer.
    pub fn compatible(&self) -> Option<&Version> {
        self.versions.iter().filter(|v| supported(&v.api)).max_by(|a, b| compare(&a.version, &b.version))
    }
}

fn supported(api: &str) -> bool {
    let minor = api.strip_prefix(API_MAJOR).and_then(|m| m.split('.').next()).and_then(|m| m.parse::<u32>().ok());
    minor.is_some_and(|m| m <= API_MINOR)
}

/// Dotted numbers compared as numbers (`0.10.0` > `0.9.1`); anything else as text.
pub fn compare(a: &str, b: &str) -> std::cmp::Ordering {
    let parts = |v: &str| v.split('.').map(|p| p.parse::<u64>().map_err(|_| p.to_string())).collect::<Vec<_>>();
    parts(a).cmp(&parts(b))
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(60))).build().into()
}

fn get(url: &str, limit: u64) -> Result<Vec<u8>, String> {
    let fail = |e: ureq::Error| format!("{url}: {e}");
    agent().get(url).call().map_err(fail)?.body_mut().with_config().limit(limit).read_to_vec().map_err(fail)
}

pub fn fetch(url: &str) -> Result<Index, String> {
    let index: Index = serde_json::from_slice(&get(url, INDEX_LIMIT)?).map_err(|e| format!("{url}: {e}"))?;
    if index.format != 1 {
        return Err(format!("{url}: index format {} is not supported, update HomeMaid", index.format));
    }
    Ok(index)
}

/// Downloads a version and checks it. `pinned` is the key of the version already installed.
pub fn download(entry: &Entry, version: &Version, pinned: Option<&str>) -> Result<Vec<u8>, String> {
    if let Some(pinned) = pinned.filter(|k| *k != entry.key) {
        return Err(format!("`{}` is now signed by another key than when it was installed ({pinned}): refused", entry.id));
    }
    let wasm = get(&version.url, WASM_LIMIT)?;
    verify(entry, version, &wasm)?;
    Ok(wasm)
}

pub fn verify(entry: &Entry, version: &Version, wasm: &[u8]) -> Result<(), String> {
    let sha: String = Sha256::digest(wasm).iter().map(|b| format!("{b:02x}")).collect();
    if !sha.eq_ignore_ascii_case(&version.sha256) {
        return Err(format!("{} {}: checksum mismatch, download refused", entry.id, version.version));
    }
    let key = minisign_verify::PublicKey::from_base64(&entry.key).map_err(|e| format!("{}: bad key: {e}", entry.id))?;
    let signature = minisign_verify::Signature::decode(&version.signature).map_err(|e| format!("{}: bad signature: {e}", entry.id))?;
    key.verify(wasm, &signature, false)
        .map_err(|e| format!("{} {}: signature refused ({e}), download refused", entry.id, version.version))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A fresh author key (base64), and the `.minisig` of some bytes: what an author does with the CLI.
    pub(crate) fn author() -> (String, impl Fn(&[u8]) -> String) {
        let minisign::KeyPair { pk, sk } = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
        (pk.to_base64(), move |data: &[u8]| minisign::sign(None, &sk, data, None, None).unwrap().into_string())
    }

    pub(crate) fn sha256(data: &[u8]) -> String {
        Sha256::digest(data).iter().map(|b| format!("{b:02x}")).collect()
    }

    const WASM: &[u8] = b"\0asm fixture";

    fn entry(key: &str, signature: &str, versions: &[(&str, &str)]) -> Entry {
        let version = |(version, api): &(&str, &str)| Version {
            version: version.to_string(),
            api: api.to_string(),
            permissions: vec![],
            url: String::new(),
            sha256: sha256(WASM),
            signature: signature.into(),
        };
        Entry {
            id: "demo".into(),
            name: "Demo".into(),
            description: String::new(),
            repo: String::new(),
            key: key.into(),
            settings: vec![],
            versions: versions.iter().map(version).collect(),
        }
    }

    #[test]
    fn a_signed_file_is_accepted_a_changed_one_is_not() {
        let (key, sign) = author();
        let e = entry(&key, &sign(WASM), &[("1.0.0", "1.0")]);
        verify(&e, &e.versions[0], WASM).unwrap();
        assert!(verify(&e, &e.versions[0], b"\0asm fixturE").unwrap_err().contains("checksum"));

        let mut other = e.versions[0].clone();
        other.sha256 = sha256(b"other");
        assert!(verify(&e, &other, b"other").unwrap_err().contains("signature"), "right checksum, signature of another file");
        let (_, impostor) = author();
        other.signature = impostor(b"other");
        assert!(verify(&e, &other, b"other").unwrap_err().contains("signature"), "signed by someone else");
    }

    #[test]
    fn a_new_key_is_refused_before_downloading() {
        let (key, sign) = author();
        let e = entry(&key, &sign(WASM), &[("1.0.0", "1.0")]);
        let err = download(&e, &e.versions[0], Some("RWQother")).unwrap_err();
        assert!(err.contains("another key"), "{err}");
    }

    #[test]
    fn the_newest_version_this_core_runs_is_picked() {
        let e = entry("", "", &[("0.9.0", "1.0"), ("0.10.0", "1.2"), ("0.11.0", "1.9"), ("1.0.0", "2.0")]);
        assert_eq!(e.compatible().unwrap().version, "0.10.0");
        assert!(entry("", "", &[("1.0.0", "2.0")]).compatible().is_none());
    }
}
