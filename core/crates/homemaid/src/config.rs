use std::{collections::BTreeMap, fs, io, net::SocketAddr, path::Path, path::PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::model::Entity;

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
}

#[derive(Debug, Clone, Deserialize)]
pub struct AgentConfig {
    /// Read in desktop mode only (the PC window's "PC control" switch); `--agent` always runs it.
    #[serde(default = "yes")]
    pub enabled: bool,
    pub bind: SocketAddr,
    /// Shared with the clients: the phone and the `pc` plugin.
    pub token: String,
    /// The only programs the remote can launch, by name. Bare names are looked up in PATH.
    #[serde(default)]
    pub apps: BTreeMap<String, PathBuf>,
    /// Services the remote opens and drives (a streaming site, a player): nothing about them is
    /// built in, the presets live in the config.
    #[serde(default, rename = "shortcut")]
    pub shortcuts: Vec<Shortcut>,
}

/// A service on the remote: its logo, what opens it, and the keys its player answers to.
#[derive(Debug, Clone, Deserialize)]
pub struct Shortcut {
    pub name: String,
    /// A logo bundled with the app (`app/assets/logos/<icon>.svg`); none shows the name's initials.
    #[serde(default)]
    pub icon: Option<String>,
    /// A URL, opened in the default browser, or a program.
    pub open: String,
    /// Key combos sent to the focused window by the playback buttons, by action (`play_pause`,
    /// `back`, `forward`, `restart`, `next_episode`, `skip_intro`), e.g. `"shift+n"`.
    #[serde(default)]
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
}

/// Relative paths in the file are resolved against the directory of the file.
pub fn load(path: &Path) -> Result<Config, String> {
    let text = read_text(path)?;
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

    let fail = |e: &dyn std::fmt::Display| format!("{}: {e}", path.display());
    let mut doc: toml_edit::DocumentMut = read_text(path)?.parse().map_err(|e| fail(&e))?;
    let created = match doc.get_mut("agent").and_then(Item::as_table_mut) {
        Some(agent) => {
            agent["enabled"] = value(enabled);
            false
        }
        None if !enabled => return Ok(false),
        None => {
            let token = new_token();
            let data_dir = doc.get("core").and_then(|c| c.get("data_dir")).and_then(Item::as_str).unwrap_or("data");
            let secrets = path.parent().unwrap_or(Path::new(".")).join(data_dir).join("secrets.toml");
            set_secret(&secrets, "pc", "token", &token)?;
            let mut agent = Table::new();
            agent["enabled"] = value(true);
            agent["bind"] = value("0.0.0.0:8124");
            agent["token"] = value(token);
            doc["agent"] = Item::Table(agent);
            true
        }
    };
    crate::store::write_atomic(path, doc.to_string().as_bytes()).map_err(|e| fail(&e))?;
    Ok(created)
}

/// Sets `[section] key` in a secrets file (created with its folder if missing), keeping the rest.
pub fn set_secret(path: &Path, section: &str, key: &str, secret: &str) -> Result<(), String> {
    let fail = |e: &dyn std::fmt::Display| format!("{}: {e}", path.display());
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(fail(&e)),
    };
    let mut doc: toml_edit::DocumentMut = text.parse().map_err(|e| fail(&e))?;
    let table = doc.entry(section).or_insert(toml_edit::table()).as_table_mut().ok_or_else(|| fail(&format!("`{section}` is not a table")))?;
    table[key] = toml_edit::value(secret);
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| fail(&e))?;
    }
    crate::store::write_atomic(path, doc.to_string().as_bytes()).map_err(|e| fail(&e))
}

/// Without the UTF-8 BOM some Windows editors add, which TOML parsers reject.
fn read_text(path: &Path) -> Result<String, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
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

/// Finds (or appends) the table of `[[array]]` whose `key` is `id`, lets [fill] set or remove
/// (`None`) its fields, and writes the file back.
fn save_table(
    path: &Path,
    array: &str,
    key: &str,
    id: &str,
    fill: impl FnOnce(&mut dyn FnMut(&str, Option<toml_edit::Item>)),
) -> Result<(), String> {
    use toml_edit::{ArrayOfTables, DocumentMut, Item, Table, value};

    let fail = |e: &dyn std::fmt::Display| format!("{}: {e}", path.display());
    let mut doc: DocumentMut = read_text(path)?.parse().map_err(|e| fail(&e))?;
    let tables = doc
        .entry(array)
        .or_insert(Item::ArrayOfTables(ArrayOfTables::new()))
        .as_array_of_tables_mut()
        .ok_or_else(|| fail(&format!("`{array}` must be an array of tables ([[{array}]])")))?;
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
    crate::store::write_atomic(path, doc.to_string().as_bytes()).map_err(|e| fail(&e))
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
