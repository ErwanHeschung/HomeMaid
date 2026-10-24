//! Scenes: `[[scene]]` in homemaid.toml, a named set of desired states ("Cinéma": the TV on, the
//! lights at 20 %), then one-shot actions (open Netflix on the PC). Applying one sets the states;
//! the reconciler does the rest, so a scene is not a snapshot replayed blindly: each device
//! confirms or fails, and one offline gets its state back when it returns. The app tells a scene
//! in force from the devices' observed states; actions are not states, they just run.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    Core,
    automations::{ActionResult, Automations, InvokeConfig, SetConfig, Target, invoke_each, nothing_matched, set_each},
    model::{CapKind, CapValue},
};

// Also written back (the app edits scenes): unset options are left out.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SceneConfig {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Shown in this room in the app; none: the whole home.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub room: Option<String>,
    #[serde(default, rename = "set", skip_serializing_if = "Vec::is_empty")]
    pub sets: Vec<SetConfig>,
    /// Run after the states are set, in order.
    #[serde(default, rename = "invoke", skip_serializing_if = "Vec::is_empty")]
    pub invokes: Vec<InvokeConfig>,
}

#[derive(Debug, Clone)]
struct Set {
    target: Target,
    cap: CapKind,
    value: CapValue,
}

#[derive(Debug, Clone)]
struct Invoke {
    target: Target,
    action: String,
    params: Value,
}

#[derive(Debug, Clone)]
pub struct Scene {
    pub id: String,
    pub name: String,
    sets: Vec<Set>,
    invokes: Vec<Invoke>,
    /// As written, for the app's editor.
    config: SceneConfig,
}

impl Scene {
    /// Checks everything that can be checked without the devices; errors stop the core.
    pub fn compile(c: &SceneConfig) -> Result<Self, String> {
        if c.id.is_empty() {
            return Err("a scene needs an `id`".into());
        }
        if c.sets.is_empty() && c.invokes.is_empty() {
            return Err(format!("scene `{}`: needs at least one [[scene.set]] or [[scene.invoke]]", c.id));
        }
        let sets = c
            .sets
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let at = |e: String| format!("scene `{}`, set {}: {e}", c.id, i + 1);
                if s.from.is_some() {
                    return Err(at("a scene sets values, not `from`".into()));
                }
                let cap: CapKind = s.cap.parse().map_err(at)?;
                let value = s.value.clone().ok_or_else(|| at("give a `value`".into()))?;
                Ok(Set { target: Target::parse(&s.select).map_err(at)?, value: CapValue::from_plain(&cap, value).map_err(at)?, cap })
            })
            .collect::<Result<_, String>>()?;
        let invokes = c
            .invokes
            .iter()
            .enumerate()
            .map(|(i, a)| {
                let at = |e: String| format!("scene `{}`, invoke {}: {e}", c.id, i + 1);
                if !(a.params.is_null() || a.params.is_object()) {
                    return Err(at("`params` must be a table".into()));
                }
                Ok(Invoke { target: Target::parse(&a.select).map_err(at)?, action: a.action.clone(), params: a.params.clone() })
            })
            .collect::<Result<_, String>>()?;
        Ok(Self { id: c.id.clone(), name: c.name.clone().unwrap_or_else(|| c.id.clone()), sets, invokes, config: c.clone() })
    }
}

pub struct Scenes {
    list: Vec<Scene>,
}

impl Scenes {
    /// Compiles the configured scenes: any error is a config error.
    pub fn new(configs: &[SceneConfig]) -> Result<Self, String> {
        let mut list: Vec<Scene> = vec![];
        for c in configs {
            if list.iter().any(|s| s.id == c.id) {
                return Err(format!("two scenes have the id `{}`", c.id));
            }
            list.push(Scene::compile(c)?);
        }
        Ok(Self { list })
    }

    pub fn none() -> Self {
        Self { list: vec![] }
    }

    pub fn contains(&self, id: &str) -> bool {
        self.list.iter().any(|s| s.id == id)
    }

    /// Sets every state of the scene, then runs its actions: what each device answered, or why not.
    pub async fn apply(&self, core: &Core, id: &str) -> Result<Vec<ActionResult>, String> {
        let scene = self.list.iter().find(|s| s.id == id).ok_or_else(|| format!("unknown scene `{id}`"))?;
        let mut results = core.with(|r| {
            let mut results = vec![];
            for set in &scene.sets {
                let label = format!("set {} {}", set.cap, set.value.plain());
                let ids = set.target.entities(r.entities.values()).iter().map(|e| (e.id, e.name.clone())).collect();
                results.extend(nothing_matched(set_each(r, ids, &set.cap, &set.value, &label), &set.target, &label));
            }
            results
        });
        for invoke in &scene.invokes {
            let label = format!("invoke {}", invoke.action);
            let ids = core.with(|r| invoke.target.entities(r.entities.values()).iter().map(|e| (e.id, e.name.clone())).collect());
            results.extend(nothing_matched(invoke_each(core, ids, &invoke.action, &invoke.params, &label).await, &invoke.target, &label));
        }
        Ok(results)
    }

    /// For the API: every scene as written, with the states it sets now (entity, capability,
    /// value), from which the app tells whether it is in force, and what may be wrong.
    pub fn list(&self, core: &Core) -> Value {
        core.with(|r| {
            let items: Vec<Value> = self
                .list
                .iter()
                .map(|s| {
                    let (mut targets, mut problems) = (vec![], vec![]);
                    for invoke in &s.invokes {
                        let matched = invoke.target.entities(r.entities.values());
                        if !matched.iter().any(|e| e.actions.iter().any(|d| d.name == invoke.action)) {
                            problems.push(format!("`{}` matches no device with the action `{}`", invoke.target.text, invoke.action));
                        }
                    }
                    for set in &s.sets {
                        let matched = set.target.entities(r.entities.values());
                        let with_cap: Vec<_> = matched.iter().filter(|e| e.caps.contains_key(&set.cap)).collect();
                        if matched.is_empty() {
                            problems.push(format!("`{}` matches no device", set.target.text));
                        } else if with_cap.is_empty() {
                            problems.push(format!("`{}` matches no device with {}", set.target.text, set.cap));
                        }
                        targets.extend(with_cap.iter().map(|e| json!({ "entity": e.id, "cap": set.cap.to_string(), "value": set.value.plain() })));
                    }
                    json!({ "id": s.id, "name": s.name, "targets": targets, "problems": problems, "config": s.config })
                })
                .collect();
            json!(items)
        })
    }
}

/// The scenes automations name must exist (a `scene` step).
pub fn check_refs(automations: &Automations, scenes: &Scenes) -> Result<(), String> {
    match automations.scene_refs().into_iter().find(|(_, _, scene)| !scenes.contains(scene)) {
        Some((automation, node, scene)) => Err(format!("automation `{automation}`, node `{node}`: no scene `{scene}`")),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, sync::Arc};

    use tokio::sync::mpsc;

    use super::*;
    use crate::{
        model::{CapSlot, Entity},
        reconciler::Reconciler,
    };

    fn scenes(text: &str) -> Result<Scenes, String> {
        #[derive(Deserialize)]
        struct File {
            scene: Vec<SceneConfig>,
        }
        Scenes::new(&toml::from_str::<File>(text).map_err(|e| e.to_string())?.scene)
    }

    fn lamp() -> Entity {
        Entity {
            id: crate::model::entity_uuid("mock", "lamp"),
            name: "Lampe".into(),
            room: Some("Salon".into()),
            tags: vec![],
            available: true,
            plugin: "mock".into(),
            local_id: "lamp".into(),
            device: None,
            added: None,
            model: None,
            actions: vec![],
            caps: BTreeMap::from([(CapKind::OnOff, CapSlot { observed: Some(CapValue::OnOff(false)), ..Default::default() })]),
        }
    }

    const CINEMA: &str = r#"
        [[scene]]
        id = "cinema"
        name = "Cinéma"
        room = "Salon"
        [[scene.set]]
        select = "room:Salon & cap:on_off"
        cap = "on_off"
        value = true
        [[scene.set]]
        select = "room:Cave"
        cap = "on_off"
        value = false
        [[scene.invoke]]
        select = "room:Salon"
        action = "blink"
    "#;

    #[test]
    fn errors_say_which_scene_and_set() {
        let set = |body: &str| format!("[[scene]]\nid = \"s\"\n[[scene.set]]\n{body}\n");
        for (text, what) in [
            (set(r#"select = "room:Salon"
cap = "brightnes"
value = 1"#), "scene `s`, set 1: unknown capability"),
            (set(r#"select = "room:Salon"
cap = "on_off""#), "give a `value`"),
            (set(r#"select = "room:Salon"
cap = "on_off"
from = "trigger""#), "not `from`"),
            ("[[scene]]\nid = \"s\"\n".into(), "at least one [[scene.set]] or [[scene.invoke]]"),
            ("[[scene]]\nid = \"s\"\n[[scene.invoke]]\nselect = \"plugin:pc\"\naction = \"launch\"\nparams = 3\n".into(), "scene `s`, invoke 1: `params` must be a table"),
            (format!("{CINEMA}{CINEMA}"), "two scenes"),
        ] {
            let err = scenes(&text).err().unwrap_or_default();
            assert!(err.contains(what), "{what}: got `{err}`");
        }
    }

    #[tokio::test]
    async fn applying_sets_desired_states_and_lists_what_it_sets() {
        let (tx, mut commands) = mpsc::unbounded_channel();
        let core = Arc::new(Core::new(Reconciler::new([(lamp().id, lamp())].into()), tx));
        let scenes = scenes(CINEMA).unwrap();

        let results = scenes.apply(&core, "cinema").await.unwrap();
        assert_eq!((results[0].target.as_str(), results[0].outcome.as_str()), ("Lampe", "pending"));
        assert_eq!(results[1].outcome, "matches no device", "a result, not a failure of the whole scene");
        assert_eq!(commands.try_recv().unwrap().value, CapValue::OnOff(true));
        assert!(scenes.apply(&core, "nope").await.is_err());
        // Then its actions, after the states: here no plugin runs, said per device.
        assert_eq!((results[2].action.as_str(), results[2].outcome.as_str()), ("invoke blink", "plugins are not running"));

        let list = scenes.list(&core);
        assert_eq!(list[0]["targets"], json!([{ "entity": lamp().id, "cap": "on_off", "value": true }]), "actions are not states");
        assert_eq!(list[0]["problems"], json!(["`room:Salon` matches no device with the action `blink`", "`room:Cave` matches no device"]));
        assert_eq!(list[0]["config"]["room"], "Salon");
    }
}
