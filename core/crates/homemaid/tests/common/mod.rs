//! Harness for plugin tests: builds a plugin to wasm32-wasip2 and runs it on a real core.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
    time::{Duration, Instant},
};

use homemaid::{Core, automations::Automations, config::PluginConfig, plugins::Plugins, reconciler::Reconciler};
use tokio::sync::mpsc;

pub fn build_plugin(package: &str) -> PathBuf {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let status = Command::new(env!("CARGO"))
        .args(["build", "-q", "-p", package, "--target", "wasm32-wasip2"])
        .current_dir(&workspace)
        .status()
        .expect("cargo runs");
    assert!(status.success(), "{package} build failed (rustup target add wasm32-wasip2?)");
    workspace.join(format!("target/wasm32-wasip2/debug/{}.wasm", package.replace('-', "_")))
}

pub struct Plugin<'a> {
    pub id: &'a str,
    pub package: &'a str,
    pub permissions: &'a [&'a str],
    pub settings: &'a [(&'a str, &'a str)],
    pub secrets: &'a [(&'a str, &'a str)],
}

#[allow(dead_code)] // Not every test binary uses it.
/// Runs the plugin on a fresh core. Keep the returned dir alive: it holds the plugin's KV store.
pub fn start(plugin: Plugin) -> (Arc<Core>, TempDir) {
    let path = build_plugin(plugin.package);
    start_wasm(plugin, path)
}

#[allow(dead_code)]
/// Same with an already built `.wasm` (an older plugin-api, for compatibility tests).
pub fn start_wasm(plugin: Plugin, path: PathBuf) -> (Arc<Core>, TempDir) {
    start_on(plugin, path, None)
}

/// Same with automations loaded first, as the core does (their schedules go to the devices).
#[allow(dead_code)] // Not every test binary uses it.
pub fn start_with(plugin: Plugin, automations: Automations) -> (Arc<Core>, TempDir) {
    let path = build_plugin(plugin.package);
    start_on(plugin, path, Some(automations))
}

fn start_on(plugin: Plugin, path: PathBuf, automations: Option<Automations>) -> (Arc<Core>, TempDir) {
    let _ = tracing_subscriber::fmt().with_test_writer().try_init();
    let data = TempDir::new();
    let (commands, mut command_rx) = mpsc::unbounded_channel();
    let core = Arc::new(Core::new(Reconciler::new(BTreeMap::new()), commands));
    if let Some(automations) = automations {
        core.set_automations(automations);
    }
    let pairs = |pairs: &[(&str, &str)]| pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    let config = PluginConfig {
        id: plugin.id.into(),
        path,
        permissions: plugin.permissions.iter().map(|p| p.to_string()).collect(),
        settings: pairs(plugin.settings),
        version: None,
        key: None,
    };
    let secrets = BTreeMap::from([(plugin.id.to_string(), pairs(plugin.secrets))]);
    let plugins = Arc::new(Plugins::start(core.clone(), vec![config], &data.0, secrets).unwrap());
    let _ = core.plugins.set(plugins.clone());
    let router = core.clone();
    tokio::spawn(async move {
        while let Some(command) = command_rx.recv().await {
            plugins.dispatch(&router, command);
        }
    });
    (core, data)
}

/// A hub with no plugin yet, as after a first start: plugins are installed through the API.
#[allow(dead_code)] // Not every test binary uses it.
pub fn hub() -> (Arc<Core>, TempDir) {
    let _ = tracing_subscriber::fmt().with_test_writer().try_init();
    let data = TempDir::new();
    let (commands, mut command_rx) = mpsc::unbounded_channel();
    let core = Arc::new(Core::new(Reconciler::new(BTreeMap::new()), commands));
    let plugins = Arc::new(Plugins::start(core.clone(), vec![], &data.0.join("data"), BTreeMap::new()).unwrap());
    let _ = core.plugins.set(plugins.clone());
    let router = core.clone();
    tokio::spawn(async move {
        while let Some(command) = command_rx.recv().await {
            plugins.dispatch(&router, command);
        }
    });
    (core, data)
}

/// Polls `check` until it holds. `timeout` is what a dev machine needs; shared CI runners compile
/// the plugins' wasm much slower, so it is stretched 4x (returning as soon as `check` holds).
pub async fn wait_for(what: &str, timeout: Duration, mut check: impl FnMut() -> bool) {
    let deadline = Instant::now() + timeout * 4;
    while !check() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// Minimal temp dir, removed on drop.
pub struct TempDir(pub PathBuf);

impl TempDir {
    fn new() -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("homemaid-test-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
