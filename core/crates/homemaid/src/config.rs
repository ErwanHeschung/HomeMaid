use std::{
    collections::BTreeMap,
    fs, io,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{Mutex, PoisonError},
};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{automations::AutomationConfig, model::Entity};

#[derive(Debug, Deserialize)]
pub struct Config {
    /// Required by `--core`.
    pub core: Option<CoreConfig>,
    /// Required by `--agent`.
    pub agent: Option<AgentConfig>,
    #[serde(default, rename = "entity")]
    pub entities: Vec<EntityOverride>,
    #[serde(default, rename = "plugin")]
    pub plugins: Vec<PluginConfig>,
    #[serde(default, rename = "room")]
    pub rooms: Vec<RoomConfig>,
    #[serde(default, rename = "automation")]
    pub automations: Vec<crate::automations::AutomationConfig>,
}

/// A room, by the name entities refer to. `kind` picks the app's furnished model of it
/// (`living`, `kitchen`...); empty goes back to the one inferred from the name.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct RoomConfig {
    pub name: String,
    #[serde(default)]
    pub kind: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CoreConfig {
    pub bind: SocketAddr,
    pub data_dir: PathBuf,
    pub token: String,
    /// Where the home is, in degrees: only for sunrise and sunset in automations.
    #[serde(default)]
    pub latitude: Option<f64>,
    #[serde(default)]
    pub longitude: Option<f64>,
    /// Where plugins are found (`index.json`); the official registry when absent.
    #[serde(default)]
    pub registry: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AgentConfig {
    /// Read in desktop mode only (the PC window's "PC control" switch); `--agent` always runs it.
    #[serde(default = "yes")]
    pub enabled: bool,
    pub bind: SocketAddr,
    /// Shared with the clients: the phone and the `pc` plugin.
    pub token: String,
    /// The only programs the remote can launch, by name. Bare names are looked up in PATH; an
    /// app pinned from the phone is `shell:AppsFolder\<AppID>`.
    #[serde(default)]
    pub apps: BTreeMap<String, PathBuf>,
    /// Opt-in: the apps of the Start menu can be launched too (and pinned from the phone).
    #[serde(default)]
    pub discover_apps: bool,
    /// Services the remote opens and drives (a streaming site, a player): nothing about them is
    /// built in, the presets live in the config.
    #[serde(default, rename = "shortcut")]
    pub shortcuts: Vec<Shortcut>,
}

/// A service on the remote: its logo, what opens it, and the keys its player answers to.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Shortcut {
    pub name: String,
    /// A logo bundled with the app (`app/assets/logos/<icon>.svg`); none shows the name's initials.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// A URL, opened in the default browser, or a program.
    pub open: String,
    /// Key combos sent to the focused window by the playback buttons, by action (`play_pause`,
    /// `back`, `forward`, `restart`, `next_episode`, `skip_intro`), e.g. `"shift+n"`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub keys: BTreeMap<String, String>,
}

/// User metadata layered over what the plugin discovered. The UUID is stable, the rest can change freely.
#[derive(Debug, Clone, Deserialize)]
pub struct EntityOverride {
    pub id: Uuid,
    pub name: Option<String>,
    pub room: Option<String>,
    pub tags: Option<Vec<String>>,
    /// The app's 3D model for this entity; empty goes back to the inferred one.
    #[serde(default)]
    pub model: Option<String>,
}

impl EntityOverride {
    pub fn apply(&self, entity: &mut Entity) {
        if let Some(name) = self.name.as_ref().filter(|n| !n.is_empty()) {
            entity.name.clone_from(name);
        }
        if let Some(room) = &self.room {
            // Empty clears the room.
            entity.room = Some(room.clone()).filter(|r| !r.is_empty());
        }
        if let Some(tags) = &self.tags {
            entity.tags.clone_from(tags);
        }
        if let Some(model) = &self.model {
            entity.model = Some(model.clone()).filter(|m| !m.is_empty());
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct PluginConfig {
    pub id: String,
    pub path: PathBuf,
    /// Permissions granted by the user; the plugin is refused if its manifest asks for more.
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub settings: BTreeMap<String, String>,
    /// Installed from the registry: its version, and the author's key it must stay signed with.
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub key: Option<String>,
    /// Devices the user added from the app (plugin-api 1.3), in `[[plugin.device]]`.
    #[serde(default, rename = "device")]
    pub devices: Vec<DeviceConfig>,
}

/// A device added from the app: its id, and what the user typed for it but its secrets (in
/// `secrets.toml`, table [`device_secrets`]).
#[derive(Debug, Clone, Deserialize)]
pub struct DeviceConfig {
    pub id: String,
    #[serde(default)]
    pub settings: BTreeMap<String, String>,
}

/// The `secrets.toml` table of a device added to `plugin`.
pub fn device_secrets(plugin: &str, device: &str) -> String {
    format!("{plugin}/{device}")
}

/// Relative paths in the file are resolved against the directory of the file.
pub fn load(path: &Path) -> Result<Config, String> {
    let text = read_text(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut config: Config = toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    for (section, token) in [("core", config.core.as_ref().map(|c| &c.token)), ("agent", config.agent.as_ref().map(|a| &a.token))] {
        if let Some(Err(e)) = token.map(|t| check_token(t)) {
            return Err(format!("{}: {section}.token {e}", path.display()));
        }
    }
    let dir = path.parent().unwrap_or(Path::new("."));
    if let Some(core) = &mut config.core {
        core.data_dir = dir.join(&core.data_dir);
    }
    for plugin in &mut config.plugins {
        plugin.path = dir.join(&plugin.path);
    }
    Ok(config)
}

/// A token guards the home, or the PC's keyboard, from the whole LAN.
fn check_token(token: &str) -> Result<(), &'static str> {
    if token.starts_with("change-me") {
        Err("is the example's placeholder: replace it with a long random string")
    } else if token.chars().count() < 16 {
        Err("is too short: use at least 16 random characters")
    } else {
        Ok(())
    }
}

fn yes() -> bool {
    true
}

/// A new access token: 122 random bits from the OS, as 32 hex digits.
pub fn new_token() -> String {
    Uuid::new_v4().simple().to_string()
}

/// The config written at the first start in desktop mode: hub and PC control on the default
/// ports with fresh tokens, and the `pc` plugin when its .wasm sits in `plugins/`. Nobody types
/// the tokens: the phone reads them from the pairing QR codes.
pub fn write_first(path: &Path) -> Result<(), String> {
    let dir = path.parent().unwrap_or(Path::new("."));
    let agent_token = new_token();
    let pc = if dir.join("plugins/pc_plugin.wasm").is_file() { FIRST_PC } else { "" };
    let text = FIRST.replace("{core}", &new_token()).replace("{agent}", &agent_token).replace("{pc}", pc);
    crate::store::write_atomic(path, text.as_bytes()).map_err(|e| format!("{}: {e}", path.display()))?;
    set_secret(&dir.join("data/secrets.toml"), "pc", "token", &agent_token)
}

const FIRST: &str = r#"# HomeMaid, written at the first start. Every option: homemaid.example.toml in the project.
# The tokens are access codes: the phone gets them from the QR codes of the HomeMaid window.

[core]
bind = "0.0.0.0:8123"
data_dir = "data"
token = "{core}"

[agent]
# The "PC control" switch of the HomeMaid window.
enabled = true
bind = "0.0.0.0:8124"
token = "{agent}"
{pc}"#;

const FIRST_PC: &str = r#"
# This PC as a device of the home (volume, mute, media). Its token is in data/secrets.toml.
[[plugin]]
id = "pc"
path = "plugins/pc_plugin.wasm"
# loopback: the agent runs on this PC.
permissions = ["local-network", "loopback"]
[plugin.settings]
agent = "127.0.0.1:8124"
"#;

/// Sets `enabled` in `[agent]`, keeping the rest of the file. Switched on without an `[agent]`
/// section, it adds one with a fresh token, also given to the `pc` plugin's secrets; it then
/// returns `true`: the section is read at start, so the process must restart.
pub fn set_agent_enabled(path: &Path, enabled: bool) -> Result<bool, String> {
    use toml_edit::{Item, Table, value};

    let created = edit_toml(path, |doc| {
        if let Some(agent) = doc.get_mut("agent").and_then(Item::as_table_mut) {
            agent["enabled"] = value(enabled);
            return Ok(None);
        }
        if !enabled {
            return Ok(None);
        }
        let token = new_token();
        let data_dir = doc.get("core").and_then(|c| c.get("data_dir")).and_then(Item::as_str).unwrap_or("data");
        let secrets = path.parent().unwrap_or(Path::new(".")).join(data_dir).join("secrets.toml");
        let mut agent = Table::new();
        agent["enabled"] = value(true);
        agent["bind"] = value("0.0.0.0:8124");
        agent["token"] = value(&token);
        doc["agent"] = Item::Table(agent);
        Ok(Some((secrets, token)))
    })?;
    match created {
        Some((secrets, token)) => set_secret(&secrets, "pc", "token", &token).map(|()| true),
        None => Ok(false),
    }
}

/// Sets `[section] key` in a secrets file (created with its folder if missing), keeping the rest.
pub fn set_secret(path: &Path, section: &str, key: &str, secret: &str) -> Result<(), String> {
    edit_toml(path, |doc| {
        let table = doc.entry(section).or_insert(toml_edit::table()).as_table_mut().ok_or(format!("`{section}` is not a table"))?;
        table[key] = toml_edit::value(secret);
        Ok(())
    })
}

/// Reads a TOML file (none is empty), lets `edit` change it, and writes it back with its comments
/// and layout. One edit at a time in the process: the API, the desktop window and the first start
/// all edit the same files, and each would otherwise lose the others' changes.
fn edit_toml<R>(path: &Path, edit: impl FnOnce(&mut toml_edit::DocumentMut) -> Result<R, String>) -> Result<R, String> {
    static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());
    let _guard = ONE_AT_A_TIME.lock().unwrap_or_else(PoisonError::into_inner);
    let fail = |e: &dyn std::fmt::Display| format!("{}: {e}", path.display());
    let text = match read_text(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(fail(&e)),
    };
    let mut doc: toml_edit::DocumentMut = text.parse().map_err(|e| fail(&e))?;
    let result = edit(&mut doc).map_err(|e| fail(&e))?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| fail(&e))?;
    }
    crate::store::write_atomic(path, doc.to_string().as_bytes()).map_err(|e| fail(&e))?;
    Ok(result)
}

/// Without the UTF-8 BOM some Windows editors add, which TOML parsers reject.
fn read_text(path: &Path) -> io::Result<String> {
    let text = fs::read_to_string(path)?;
    Ok(text.strip_prefix('\u{feff}').map(str::to_string).unwrap_or(text))
}

/// `secrets.toml` in the data dir, one table per plugin id. Kept out of the versioned config.
pub fn load_secrets(path: &Path) -> Result<BTreeMap<String, BTreeMap<String, String>>, String> {
    match fs::read_to_string(path) {
        Ok(text) => toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display())),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

/// Writes one entity's metadata into its `[[entity]]` table of the config file (created if
/// missing), keeping comments and layout: the UI and the user edit the same file.
pub fn save_override(path: &Path, o: &EntityOverride) -> Result<(), String> {
    use toml_edit::{Array, value};

    save_table(path, "entity", "id", &o.id.to_string(), |set| {
        set("name", o.name.as_ref().filter(|n| !n.is_empty()).map(value));
        set("room", o.room.as_ref().filter(|r| !r.is_empty()).map(value));
        set("tags", o.tags.as_ref().map(|tags| value(tags.iter().collect::<Array>())));
        set("model", o.model.as_ref().filter(|m| !m.is_empty()).map(value));
    })
}

/// Same for a room's `[[room]]` table.
pub fn save_room(path: &Path, room: &RoomConfig) -> Result<(), String> {
    save_table(path, "room", "name", &room.name, |set| {
        set("kind", room.kind.as_ref().filter(|k| !k.is_empty()).map(toml_edit::value));
    })
}

/// Writes a plugin installed from the registry into its `[[plugin]]` (found by id, added after the
/// others when new), with its `[plugin.settings]`. `file` is its `.wasm`, as written in the config.
pub fn save_plugin(path: &Path, plugin: &PluginConfig, file: &str) -> Result<(), String> {
    use toml_edit::{Array, Item, Table, value};

    save_table(path, "plugin", "id", &plugin.id, |set| {
        set("path", Some(value(file)));
        set("permissions", Some(value(plugin.permissions.iter().collect::<Array>())));
        set("version", plugin.version.as_deref().map(value));
        set("key", plugin.key.as_deref().map(value));
        let mut settings = Table::new();
        for (key, v) in &plugin.settings {
            settings[key] = value(v);
        }
        set("settings", Some(Item::Table(settings)));
    })
}

/// Adds a device to the `[[plugin]]` of `plugin`, after its others, as `[[plugin.device]]`.
pub fn add_device(path: &Path, plugin: &str, device: &DeviceConfig) -> Result<(), String> {
    use toml_edit::{ArrayOfTables, Item, Table, value};

    edit_toml(path, |doc| {
        let table = plugin_table(doc, plugin)?;
        let devices = table.entry("device").or_insert(Item::ArrayOfTables(ArrayOfTables::new()));
        let devices = devices.as_array_of_tables_mut().ok_or(format!("`device` of plugin `{plugin}` must be [[plugin.device]]"))?;
        let mut new = Table::new();
        new["id"] = value(&device.id);
        let mut settings = Table::new();
        for (key, v) in &device.settings {
            settings[key] = value(v);
        }
        new["settings"] = Item::Table(settings);
        devices.push(new);
        Ok(())
    })
}

/// Removes the `[[plugin.device]]` `device` of `plugin`.
pub fn delete_device(path: &Path, plugin: &str, device: &str) -> Result<(), String> {
    use toml_edit::Item;

    edit_toml(path, |doc| {
        let devices = plugin_table(doc, plugin)?.get_mut("device").and_then(Item::as_array_of_tables_mut);
        let devices = devices.ok_or(format!("plugin `{plugin}` has no device `{device}`"))?;
        let index = devices.iter().position(|t| t.get("id").and_then(Item::as_str) == Some(device));
        devices.remove(index.ok_or(format!("plugin `{plugin}` has no device `{device}`"))?);
        Ok(())
    })
}

/// Removes `[section]` from a secrets file, if there.
pub fn delete_secrets(path: &Path, section: &str) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    edit_toml(path, |doc| {
        doc.remove(section);
        Ok(())
    })
}

fn plugin_table<'a>(doc: &'a mut toml_edit::DocumentMut, id: &str) -> Result<&'a mut toml_edit::Table, String> {
    use toml_edit::Item;

    let tables = doc.get_mut("plugin").and_then(Item::as_array_of_tables_mut).ok_or(format!("no plugin `{id}`"))?;
    tables.iter_mut().find(|t| t.get("id").and_then(Item::as_str) == Some(id)).ok_or(format!("no plugin `{id}`"))
}

/// Removes the `[[plugin]]` of `id`.
pub fn delete_plugin(path: &Path, id: &str) -> Result<(), String> {
    use toml_edit::Item;

    edit_toml(path, |doc| {
        let tables = doc.get_mut("plugin").and_then(Item::as_array_of_tables_mut).ok_or(format!("no plugin `{id}`"))?;
        let index = tables.iter().position(|t| t.get("id").and_then(Item::as_str) == Some(id)).ok_or(format!("no plugin `{id}`"))?;
        tables.remove(index);
        Ok(())
    })
}

/// Sets `discover_apps` in `[agent]` (the window's switch), keeping the rest of the file.
pub fn set_agent_discover(path: &Path, on: bool) -> Result<(), String> {
    edit_toml(path, |doc| {
        let agent = doc.get_mut("agent").and_then(toml_edit::Item::as_table_mut).ok_or("no [agent] section")?;
        agent["discover_apps"] = toml_edit::value(on);
        Ok(())
    })
}

/// Writes a `[[agent.shortcut]]` (found by name, appended when new), its keys on one line.
pub fn save_shortcut(path: &Path, shortcut: &Shortcut) -> Result<(), String> {
    use toml_edit::{ArrayOfTables, InlineTable, Item, Table, value};

    edit_toml(path, |doc| {
        let agent = doc.get_mut("agent").and_then(Item::as_table_mut).ok_or("no [agent] section")?;
        let tables = agent
            .entry("shortcut")
            .or_insert(Item::ArrayOfTables(ArrayOfTables::new()))
            .as_array_of_tables_mut()
            .ok_or("`agent.shortcut` must be an array of tables ([[agent.shortcut]])")?;
        let mut table = Table::new();
        table["name"] = value(&shortcut.name);
        if let Some(icon) = &shortcut.icon {
            table["icon"] = value(icon);
        }
        table["open"] = value(&shortcut.open);
        if !shortcut.keys.is_empty() {
            table["keys"] = value(shortcut.keys.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect::<InlineTable>());
        }
        let found = tables.iter().position(|t| t.get("name").and_then(Item::as_str) == Some(&shortcut.name));
        match found.and_then(|i| tables.get_mut(i)) {
            Some(old) => {
                let (decor, position) = (old.decor().clone(), old.position());
                *old = table;
                *old.decor_mut() = decor;
                old.set_position(position);
            }
            None => tables.push(table),
        }
        Ok(())
    })
}

/// Removes the `[[agent.shortcut]]` named `name`.
pub fn delete_shortcut(path: &Path, name: &str) -> Result<(), String> {
    use toml_edit::Item;

    edit_toml(path, |doc| {
        let tables = doc
            .get_mut("agent")
            .and_then(|a| a.get_mut("shortcut"))
            .and_then(Item::as_array_of_tables_mut)
            .ok_or(format!("no shortcut `{name}`"))?;
        let index = tables.iter().position(|t| t.get("name").and_then(Item::as_str) == Some(name)).ok_or(format!("no shortcut `{name}`"))?;
        tables.remove(index);
        Ok(())
    })
}

/// Sets (or, with `None`, removes) an app of `[agent.apps]`.
pub fn set_app(path: &Path, name: &str, target: Option<&str>) -> Result<(), String> {
    use toml_edit::{Item, Table};

    edit_toml(path, |doc| {
        let agent = doc.get_mut("agent").and_then(Item::as_table_mut).ok_or("no [agent] section")?;
        let apps = agent.entry("apps").or_insert(Item::Table(Table::new())).as_table_mut().ok_or("`agent.apps` must be a table")?;
        match target {
            Some(target) => apps[name] = toml_edit::value(target),
            None => {
                apps.remove(name).ok_or(format!("no app `{name}` in [agent.apps]"))?;
            }
        }
        Ok(())
    })
}

/// Sets where the home is (`latitude`, `longitude` in `[core]`), for sunrise and sunset.
pub fn set_location(path: &Path, latitude: f64, longitude: f64) -> Result<(), String> {
    edit_toml(path, |doc| {
        let core = doc.get_mut("core").and_then(toml_edit::Item::as_table_mut).ok_or("no [core] section")?;
        core["latitude"] = toml_edit::value(latitude);
        core["longitude"] = toml_edit::value(longitude);
        Ok(())
    })
}

/// Writes one automation into its `[[automation]]` table (found by `id`, appended when new), or
/// removes it (`None`). `check` is given every automation of the file as it would become (hand
/// edits since the start included); the file is written only if it accepts them all.
pub fn save_automation<R>(
    path: &Path,
    id: &str,
    automation: Option<&AutomationConfig>,
    check: impl FnOnce(&[AutomationConfig]) -> Result<R, String>,
) -> Result<R, String> {
    use toml_edit::{ArrayOfTables, Item};

    edit_toml(path, |doc| {
        let tables = doc
            .entry("automation")
            .or_insert(Item::ArrayOfTables(ArrayOfTables::new()))
            .as_array_of_tables_mut()
            .ok_or("`automation` must be an array of tables ([[automation]])")?;
        let found = tables.iter().position(|t| t.get("id").and_then(Item::as_str) == Some(id));
        match (automation, found) {
            (Some(automation), Some(index)) => {
                let table = tables.get_mut(index).expect("index is in bounds");
                // Where it was, with the comment above its [[automation]].
                let (decor, position) = (table.decor().clone(), table.position());
                *table = automation_table(automation)?;
                *table.decor_mut() = decor;
                table.set_position(position);
            }
            (Some(automation), None) => {
                // At the end of the file, not after whatever table happens to precede the array.
                let mut table = automation_table(automation)?;
                table.set_position(Some(last_position(doc.as_table()) + 1));
                doc["automation"].as_array_of_tables_mut().expect("checked above").push(table);
            }
            (None, Some(index)) => drop(tables.remove(index)),
            (None, None) => return Err(format!("no automation `{id}`")),
        }
        #[derive(Deserialize)]
        struct File {
            #[serde(default)]
            automation: Vec<AutomationConfig>,
        }
        let file: File = toml::from_str(&doc.to_string()).map_err(|e| e.to_string())?;
        check(&file.automation)
    })
}

/// The highest position of a table in the document (where the file ends).
fn last_position(table: &toml_edit::Table) -> isize {
    use toml_edit::Item;

    table
        .iter()
        .flat_map(|(_, item)| match item {
            Item::Table(t) => vec![t.position().unwrap_or(0).max(last_position(t))],
            Item::ArrayOfTables(a) => a.iter().map(|t| t.position().unwrap_or(0).max(last_position(t))).collect(),
            _ => vec![],
        })
        .max()
        .unwrap_or(0)
}

/// An automation as a `[[automation]]` table, its triggers, conditions and actions on one line
/// each, as written by hand.
fn automation_table(automation: &AutomationConfig) -> Result<toml_edit::Table, String> {
    use toml_edit::{Item, Value};

    #[derive(Serialize)]
    struct File<'a> {
        automation: [&'a AutomationConfig; 1],
    }
    let text = toml::to_string(&File { automation: [automation] }).map_err(|e| e.to_string())?;
    let mut doc: toml_edit::DocumentMut = text.parse().map_err(|e: toml_edit::TomlError| e.to_string())?;
    let mut table = doc["automation"].as_array_of_tables_mut().and_then(|t| t.get_mut(0)).ok_or("no automation written")?.clone();
    // No position of its own: printed right after what comes before it, children after parent.
    table.set_position(None);
    for list in ["trigger", "condition", "action"] {
        for item in table.get_mut(list).and_then(Item::as_array_of_tables_mut).into_iter().flat_map(|t| t.iter_mut()) {
            item.set_position(None);
            item.decor_mut().set_prefix("");
            for (mut key, value) in item.iter_mut() {
                if let Some(sub) = value.as_table() {
                    *value = Item::Value(Value::InlineTable(sub.clone().into_inline_table()));
                    key.leaf_decor_mut().clear();
                }
            }
        }
    }
    table.decor_mut().clear();
    Ok(table)
}

/// Finds (or appends) the table of `[[array]]` whose `key` is `id`, lets [fill] set or remove
/// (`None`) its fields, and writes the file back.
fn save_table(
    path: &Path,
    array: &str,
    key: &str,
    id: &str,
    fill: impl FnOnce(&mut dyn FnMut(&str, Option<toml_edit::Item>)),
) -> Result<(), String> {
    use toml_edit::{ArrayOfTables, Item, Table, value};

    edit_toml(path, |doc| {
        let tables = doc
            .entry(array)
            .or_insert(Item::ArrayOfTables(ArrayOfTables::new()))
            .as_array_of_tables_mut()
            .ok_or(format!("`{array}` must be an array of tables ([[{array}]])"))?;
        let found = tables.iter().position(|t| t.get(key).and_then(Item::as_str) == Some(id));
        let index = match found {
            Some(index) => index,
            None => {
                let mut table = Table::new();
                table[key] = value(id);
                tables.push(table);
                tables.len() - 1
            }
        };
        let table = tables.get_mut(index).expect("index is in bounds");
        fill(&mut |field, item| match item {
            Some(item) => table[field] = item,
            None => drop(table.remove(field)),
        });
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saving_an_override_keeps_the_rest_of_the_file() {
        let path = std::env::temp_dir().join(format!("homemaid-config-test-{}.toml", std::process::id()));
        let id = Uuid::from_u128(9);
        fs::write(
            &path,
            "\u{feff}# my config\n[core] # inline comment\nbind = \"0.0.0.0:8123\"\ndata_dir = \"data\"\ntoken = \"0123456789abcdef\"\n",
        )
        .unwrap();

        let o = EntityOverride { id, name: Some("Lampe".into()), room: Some("salon".into()), tags: None, model: Some("kettle".into()) };
        save_override(&path, &o).unwrap();
        save_override(&path, &EntityOverride { room: Some(String::new()), ..o.clone() }).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        let config = load(&path).unwrap();
        fs::remove_file(&path).unwrap();

        assert!(text.starts_with("# my config\n[core] # inline comment\n"), "{text}");
        assert_eq!(text.matches("[[entity]]").count(), 1, "updated in place, not appended twice");
        assert_eq!(config.entities[0].id, id);
        assert_eq!(config.entities[0].name.as_deref(), Some("Lampe"));
        assert_eq!(config.entities[0].room, None, "an empty room removes it");
        assert_eq!(config.entities[0].model.as_deref(), Some("kettle"));
    }

    #[test]
    fn an_installed_plugin_goes_with_the_others_and_keeps_its_settings_on_update() {
        let path = std::env::temp_dir().join(format!("homemaid-plugin-test-{}.toml", std::process::id()));
        let text = "[core]\nbind = \"0.0.0.0:8123\"\ndata_dir = \"data\"\ntoken = \"0123456789abcdef\"\n\n\
                    # mine\n[[plugin]]\nid = \"pc\"\npath = \"pc.wasm\"\n[plugin.settings]\nagent = \"127.0.0.1:8124\"\n\n\
                    [[automation]]\nid = \"a\"\n";
        fs::write(&path, text).unwrap();
        let mut plugin = PluginConfig {
            id: "tplink".into(),
            path: PathBuf::new(),
            permissions: vec!["local-network".into()],
            settings: BTreeMap::from([("hosts".into(), "192.168.1.20".into())]),
            version: Some("0.1.0".into()),
            key: Some("RWQkey".into()),
            devices: vec![],
        };
        save_plugin(&path, &plugin, "data/plugins/tplink.wasm").unwrap();
        for (id, host) in [("strip1", "192.168.1.89"), ("strip2", "192.168.1.90")] {
            let device = DeviceConfig { id: id.into(), settings: BTreeMap::from([("host".into(), host.into())]) };
            add_device(&path, "tplink", &device).unwrap();
        }
        // An update rewrites the plugin's fields, not its devices.
        plugin.version = Some("0.2.0".into());
        save_plugin(&path, &plugin, "data/plugins/tplink.wasm").unwrap();
        let config = load(&path).unwrap();
        let written = fs::read_to_string(&path).unwrap();
        assert_eq!(written.matches("[[plugin]]").count(), 2, "{written}");
        assert!(written.find("id = \"tplink\"") < written.find("[[automation]]"), "after the other plugin: {written}");
        let t = &config.plugins[1];
        assert_eq!((t.version.as_deref(), t.key.as_deref()), (Some("0.2.0"), Some("RWQkey")));
        assert_eq!(t.settings["hosts"], "192.168.1.20");
        assert!(t.path.ends_with("data/plugins/tplink.wasm"));
        let hosts: Vec<_> = t.devices.iter().map(|d| (d.id.as_str(), d.settings["host"].as_str())).collect();
        assert_eq!(hosts, [("strip1", "192.168.1.89"), ("strip2", "192.168.1.90")], "{written}");

        delete_device(&path, "tplink", "strip1").unwrap();
        let devices = &load(&path).unwrap().plugins[1].devices;
        assert_eq!(devices.iter().map(|d| d.id.as_str()).collect::<Vec<_>>(), ["strip2"]);
        assert!(delete_device(&path, "tplink", "strip1").is_err(), "gone already");

        delete_plugin(&path, "tplink").unwrap();
        let config = load(&path).unwrap();
        fs::remove_file(&path).unwrap();
        assert_eq!(config.plugins.len(), 1);
        assert_eq!(config.plugins[0].settings["agent"], "127.0.0.1:8124");
    }

    #[test]
    fn switching_the_agent_keeps_the_file_and_defaults_to_on() {
        let path = std::env::temp_dir().join(format!("homemaid-agent-test-{}.toml", std::process::id()));
        fs::write(&path, "# pc\n[agent] # here\nbind = \"0.0.0.0:8124\"\ntoken = \"0123456789abcdef\"\n").unwrap();
        assert!(load(&path).unwrap().agent.unwrap().enabled);
        set_agent_enabled(&path, false).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("# pc\n[agent] # here\n") && text.contains("enabled = false"), "{text}");
        assert!(!load(&path).unwrap().agent.unwrap().enabled);
        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn the_first_config_runs_hub_and_pc_with_fresh_tokens() {
        let dir = std::env::temp_dir().join(format!("homemaid-first-{}", std::process::id()));
        fs::create_dir_all(dir.join("plugins")).unwrap();
        fs::write(dir.join("plugins/pc_plugin.wasm"), "").unwrap();
        let path = dir.join("homemaid.toml");
        write_first(&path).unwrap();
        let config = load(&path).unwrap();
        let (core, agent) = (config.core.unwrap(), config.agent.unwrap());
        assert!(agent.enabled && core.token.len() == 32 && agent.token.len() == 32 && core.token != agent.token);
        assert_eq!(config.plugins[0].id, "pc");
        let secrets = load_secrets(&dir.join("data/secrets.toml")).unwrap();
        assert_eq!(secrets["pc"]["token"], agent.token, "the pc plugin can talk to the agent");

        // A config without [agent]: switching PC control on adds it, with its secret.
        fs::write(&path, "# mine\n[core]\nbind = \"0.0.0.0:8123\"\ndata_dir = \"data\"\ntoken = \"0123456789abcdef\"\n").unwrap();
        assert_eq!(set_agent_enabled(&path, false), Ok(false));
        assert_eq!(set_agent_enabled(&path, true), Ok(true));
        let agent = load(&path).unwrap().agent.unwrap();
        assert!(agent.enabled && fs::read_to_string(&path).unwrap().starts_with("# mine\n"));
        assert_eq!(load_secrets(&dir.join("data/secrets.toml")).unwrap()["pc"]["token"], agent.token);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn concurrent_edits_are_all_kept() {
        let path = std::env::temp_dir().join(format!("homemaid-concurrent-test-{}.toml", std::process::id()));
        fs::write(&path, "# rooms\n").unwrap();
        std::thread::scope(|scope| {
            for i in 0..8 {
                let path = &path;
                scope.spawn(move || save_room(path, &RoomConfig { name: format!("room {i}"), kind: Some("generic".into()) }).unwrap());
            }
        });
        let rooms = load(&path).unwrap().rooms;
        fs::remove_file(&path).unwrap();
        assert_eq!(rooms.len(), 8, "{rooms:?}");
    }

    #[test]
    fn automations_are_saved_replaced_and_removed_keeping_the_rest() {
        use crate::automations::Automations;

        let path = std::env::temp_dir().join(format!("homemaid-automation-test-{}.toml", std::process::id()));
        fs::write(&path, "# mine\n[core] # here\nbind = \"0.0.0.0:8123\"\ndata_dir = \"data\"\ntoken = \"0123456789abcdef\"\n\n# by hand\n[[automation]]\nid = \"kept\"\n[[automation.trigger]]\nat = \"07:00\"\n[[automation.action]]\nset = { select = \"room:Salon\", cap = \"on_off\", value = true }\n\n# rooms\n[[room]]\nname = \"Salon\"\n").unwrap();
        let check = |all: &[AutomationConfig]| Automations::new(all, None).map(|_| all.len());
        let mut dusk: AutomationConfig = serde_json::from_value(serde_json::json!({
            "id": "dusk", "name": "Soir",
            "trigger": [{ "at": "19:30", "days": ["mon", "fri"] }],
            "condition": [{ "state": { "select": "room:Salon", "cap": "on_off", "is": false } }],
            "action": [{ "set": { "select": "room:Salon & cap:on_off", "cap": "on_off", "value": true } },
                       { "invoke": { "select": "plugin:pc", "action": "play_pause" } }],
        })).unwrap();

        assert_eq!(save_automation(&path, "dusk", Some(&dusk), check), Ok(2));
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("# mine\n[core] # here\n") && text.contains("# by hand\n[[automation]]\nid = \"kept\""), "{text}");
        assert!(text.contains(r#"set = { select = "room:Salon & cap:on_off", cap = "on_off", value = true }"#), "one line each: {text}");
        assert!(!text.contains("enabled"), "defaults are left out: {text}");
        assert!(text.find("id = \"dusk\"") > text.find("[[room]]"), "a new one goes at the end of the file: {text}");

        dusk.name = Some("Crépuscule".into());
        assert_eq!(save_automation(&path, "dusk", Some(&dusk), check), Ok(2), "replaced, not added");
        let loaded = load(&path).unwrap().automations;
        assert_eq!((loaded.len(), loaded[1].name.as_deref()), (2, Some("Crépuscule")));

        let before = fs::read_to_string(&path).unwrap();
        let broken = AutomationConfig { triggers: vec![], ..dusk.clone() };
        assert!(save_automation(&path, "dusk", Some(&broken), check).unwrap_err().contains("trigger"));
        let twin = AutomationConfig { id: "kept".into(), ..dusk.clone() };
        assert!(save_automation(&path, "other", Some(&twin), check).unwrap_err().contains("two automations"));
        assert_eq!(fs::read_to_string(&path).unwrap(), before, "refused: the file is untouched");

        assert_eq!(save_automation(&path, "dusk", None, check), Ok(1));
        assert!(save_automation(&path, "dusk", None, check).is_err(), "already gone");
        let text = fs::read_to_string(&path).unwrap();
        fs::remove_file(&path).unwrap();
        assert!(!text.contains("dusk") && text.contains("id = \"kept\""), "{text}");
    }

    #[test]
    fn the_phone_writes_shortcuts_and_pinned_apps() {
        let path = std::env::temp_dir().join(format!("homemaid-shortcut-test-{}.toml", std::process::id()));
        fs::write(&path, "# pc\n[agent]\nbind = \"0.0.0.0:8124\"\ntoken = \"0123456789abcdef\"\n\n# by hand\n[[agent.shortcut]]\nname = \"Netflix\"\nopen = \"https://www.netflix.com\"\n").unwrap();
        let plex = Shortcut { name: "Plex".into(), icon: Some("plex".into()), open: "https://app.plex.tv".into(), keys: BTreeMap::from([("skip_intro".into(), "s".into())]) };
        save_shortcut(&path, &plex).unwrap();
        save_shortcut(&path, &Shortcut { open: "https://app.plex.tv/desktop".into(), ..plex.clone() }).unwrap();
        set_app(&path, "Steam", Some("shell:AppsFolder\\Valve.Steam")).unwrap();
        set_agent_discover(&path, true).unwrap();
        let agent = load(&path).unwrap().agent.unwrap();
        assert_eq!(agent.shortcuts.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["Netflix", "Plex"], "replaced, not added");
        assert_eq!(agent.shortcuts[1].open, "https://app.plex.tv/desktop");
        assert_eq!(agent.shortcuts[1].keys["skip_intro"], "s");
        assert_eq!(agent.apps["Steam"], PathBuf::from("shell:AppsFolder\\Valve.Steam"));
        assert!(agent.discover_apps);
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("# pc\n[agent]\n") && text.contains("# by hand\n[[agent.shortcut]]\nname = \"Netflix\""), "{text}");
        assert!(text.contains(r#"keys = { skip_intro = "s" }"#), "{text}");

        delete_shortcut(&path, "Plex").unwrap();
        set_app(&path, "Steam", None).unwrap();
        assert!(delete_shortcut(&path, "Plex").is_err() && set_app(&path, "Steam", None).is_err(), "already gone");
        let agent = load(&path).unwrap().agent.unwrap();
        fs::remove_file(&path).unwrap();
        assert_eq!((agent.shortcuts.len(), agent.apps.len()), (1, 0));
    }

    #[test]
    fn weak_tokens_are_refused() {
        let path = std::env::temp_dir().join(format!("homemaid-token-test-{}.toml", std::process::id()));
        for (token, ok) in [("change-me-too", false), ("short", false), ("", false), ("0123456789abcdef", true)] {
            fs::write(&path, format!("[agent]\nbind = \"0.0.0.0:8124\"\ntoken = \"{token}\"\n")).unwrap();
            assert_eq!(load(&path).is_ok(), ok, "{token:?}");
        }
        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn a_room_kind_is_saved_and_cleared() {
        let path = std::env::temp_dir().join(format!("homemaid-room-test-{}.toml", std::process::id()));
        fs::write(&path, "# rooms\n").unwrap();
        let room = RoomConfig { name: "Salon".into(), kind: Some("living".into()) };
        save_room(&path, &room).unwrap();
        save_room(&path, &RoomConfig { name: "Garage".into(), kind: Some("generic".into()) }).unwrap();
        assert_eq!(load(&path).unwrap().rooms, [room.clone(), RoomConfig { name: "Garage".into(), kind: Some("generic".into()) }]);

        save_room(&path, &RoomConfig { kind: Some(String::new()), ..room }).unwrap();
        let rooms = load(&path).unwrap().rooms;
        fs::remove_file(&path).unwrap();
        assert_eq!(rooms[0].kind, None, "an empty kind goes back to the inferred one");
    }
}
