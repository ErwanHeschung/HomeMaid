//! An automation's steps as a graph of nodes (`[[automation.node]]`), as in n8n: each node does
//! one thing, then goes on through one of its ports. A port naming several nodes runs them side
//! by side; a `merge` joins branches back.
//!
//! The graph has no cycle (loops go through `repeat`, whose body is a closed part of the graph),
//! and only a merge has several inputs. A port not taken (the other side of an `if`, a timeout
//! that did not happen) passes a "dead" token on, running nothing, down to the merges: a merge
//! waiting for all its inputs then knows which ones will never come (dead-path elimination).

use std::{
    collections::{HashMap, HashSet},
    future::Future,
    pin::Pin,
    sync::Mutex,
    time::Duration,
};

use futures_util::{StreamExt, stream::FuturesUnordered};
use jiff::Zoned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use super::{
    ActionResult, Condition, ConditionConfig, InvokeConfig, Need, Planned, PlannedTarget, SetConfig, Target, Trigger, TriggerConfig, check,
    condition_needs, crosses, invoke_each, is_false, next_at, nothing_matched, set_each, trigger_needs,
};
use crate::{
    Core, Event,
    model::{CapKind, CapValue, Entity},
    reconciler::Reconciler,
};

/// Node runs in one run at most: a `repeat` with nothing to wait on stops there.
const MAX_STEPS: usize = 10_000;
/// Turns of one `repeat` at most.
const MAX_TURNS: u32 = 1000;
/// Longest delay or timeout: longer belongs to a time trigger.
const MAX_DURATION: Duration = Duration::from_secs(30 * 24 * 3600);
/// A `wait` on conditions checks them at least this often (time ranges change on their own).
const RECHECK: Duration = Duration::from_secs(30);

// ------------------------------------------------------------------------------------- config

/// One node: an `id`, one kind (`set`, `invoke`, `delay`, `wait`, `if`, `repeat`, `merge`, `stop`),
/// and the nodes each of its ports leads to.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NodeConfig {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub set: Option<SetConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invoke: Option<InvokeConfig>,
    /// A scene's id: sets all its states.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scene: Option<String>,
    /// `45s`, `10m`, `1h30m`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delay: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wait: Option<WaitConfig>,
    /// Conditions: `then` when they pass, `else` otherwise.
    #[serde(rename = "if", skip_serializing_if = "Option::is_none")]
    pub test: Option<Vec<ConditionConfig>>,
    /// For `if`: one passing condition is enough (default: all of them).
    #[serde(default, skip_serializing_if = "is_false")]
    pub any: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repeat: Option<RepeatConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merge: Option<MergeMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<StopConfig>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub next: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub error: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub timeout: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub then: Vec<String>,
    #[serde(default, rename = "else", skip_serializing_if = "Vec::is_empty")]
    pub otherwise: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub body: Vec<String>,
}

/// Until the conditions pass, or a trigger fires; `timeout` at most.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WaitConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub until: Option<Vec<ConditionConfig>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger: Option<TriggerConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,
}

/// One of: `count` turns, `while` (checked before each turn), `until` (checked after each), or
/// `for_each` device a selector matches (its body targets it with `select = "item"`).
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RepeatConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,
    #[serde(rename = "while", skip_serializing_if = "Option::is_none")]
    pub during: Option<Vec<ConditionConfig>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub until: Option<Vec<ConditionConfig>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub for_each: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MergeMode {
    /// Goes on once every branch arrived (or is known not to come).
    All,
    /// Goes on with the first branch that arrives.
    Any,
}

/// Ends the run, failed when it gives an `error`.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StopConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

// ------------------------------------------------------------------------------------- compiled

#[derive(Debug, Clone, Copy, PartialEq)]
enum Port {
    Next,
    Error,
    Timeout,
    Then,
    Else,
    Body,
}

impl Port {
    fn name(self) -> &'static str {
        match self {
            Port::Next => "next",
            Port::Error => "error",
            Port::Timeout => "timeout",
            Port::Then => "then",
            Port::Else => "else",
            Port::Body => "body",
        }
    }
}

/// Which devices a `set` or an `invoke` acts on.
#[derive(Debug, Clone)]
enum Pick {
    Select(Target),
    /// The device whose change started the run.
    Trigger,
    /// The device of this turn of a `for_each`.
    Item,
}

impl Pick {
    fn parse(text: &str) -> Result<Self, String> {
        match text {
            "trigger" => Ok(Pick::Trigger),
            "item" => Ok(Pick::Item),
            _ => Target::parse(text).map(Pick::Select),
        }
    }

    fn text(&self) -> &str {
        match self {
            Pick::Select(t) => &t.text,
            Pick::Trigger => "trigger",
            Pick::Item => "item",
        }
    }
}

#[derive(Debug, Clone)]
enum Source {
    Value(CapValue),
    /// The value that started the run.
    Trigger,
}

#[derive(Debug, Clone)]
enum Kind {
    Set { pick: Pick, cap: CapKind, value: Source },
    Invoke { pick: Pick, action: String, params: Value },
    Scene(String),
    Delay(Duration),
    Wait { until: Vec<Condition>, trigger: Option<Trigger>, timeout: Option<Duration> },
    If { conditions: Vec<Condition>, any: bool },
    Repeat(Repeat),
    Merge(MergeMode),
    Stop(Option<String>),
}

#[derive(Debug, Clone)]
enum Repeat {
    Count(u32),
    While(Vec<Condition>),
    Until(Vec<Condition>),
    ForEach(Target),
}

#[derive(Debug, Clone)]
struct Node {
    id: String,
    kind: Kind,
    /// Targets by index.
    ports: Vec<(Port, Vec<usize>)>,
}

impl Node {
    fn out(&self, port: Port) -> &[usize] {
        self.ports.iter().find(|(p, _)| *p == port).map_or(&[], |(_, t)| t)
    }

    /// The tokens it passes on, `taken` live and the others dead. An `error` or a `timeout` port
    /// leading nowhere goes on through `next`. A body is not left this way: the repeat runs it.
    fn tokens(&self, taken: Port) -> Vec<(usize, bool)> {
        let taken = if matches!(taken, Port::Error | Port::Timeout) && self.out(taken).is_empty() { Port::Next } else { taken };
        self.ports.iter().filter(|(p, _)| *p != Port::Body).flat_map(|(p, t)| t.iter().map(move |&t| (t, *p == taken))).collect()
    }
}

/// The steps of an automation, checked.
#[derive(Debug, Clone)]
pub(super) struct Flow {
    nodes: Vec<Node>,
    start: Vec<usize>,
    /// Edges coming in, `start` included, by node.
    inputs: Vec<usize>,
}

/// How the parent compiles what nodes share with triggers and conditions.
pub(super) struct Compilers<'a> {
    pub condition: &'a dyn Fn(&ConditionConfig) -> Result<Condition, String>,
    pub trigger: &'a dyn Fn(&TriggerConfig) -> Result<Trigger, String>,
    /// A `when` trigger exists: a run may have a device and a value that started it.
    pub fired_by_device: bool,
}

/// `45s`, `10m`, `1h30m`: numbers with their unit (h, m, s).
pub(super) fn duration(text: &str) -> Result<Duration, String> {
    let bad = || format!("`{text}` is not a duration (45s, 10m, 1h30m)");
    let (mut total, mut digits) = (0u64, String::new());
    for c in text.chars() {
        match c {
            '0'..='9' => digits.push(c),
            'h' | 'm' | 's' if !digits.is_empty() => {
                let unit = match c {
                    'h' => 3600,
                    'm' => 60,
                    _ => 1,
                };
                let n: u64 = digits.parse().map_err(|_| bad())?;
                total = n.checked_mul(unit).and_then(|n| total.checked_add(n)).ok_or_else(bad)?;
                digits.clear();
            }
            _ => return Err(bad()),
        }
    }
    if !digits.is_empty() || total == 0 {
        return Err(bad());
    }
    let d = Duration::from_secs(total);
    if d > MAX_DURATION {
        return Err(format!("`{text}` is more than 30 days: use a time trigger"));
    }
    Ok(d)
}

fn show(d: Duration) -> String {
    let s = d.as_secs();
    [(s / 3600, "h"), (s / 60 % 60, "m"), (s % 60, "s")].iter().filter(|(n, _)| *n > 0).map(|(n, unit)| format!("{n}{unit}")).collect()
}

impl Flow {
    /// The legacy list of actions (`[[automation.action]]`) is a chain of nodes.
    pub(super) fn legacy(actions: &[super::ActionConfig]) -> (Vec<NodeConfig>, Vec<String>) {
        let id = |i: usize| format!("action-{}", i + 1);
        let nodes = actions
            .iter()
            .enumerate()
            .map(|(i, a)| NodeConfig {
                id: id(i),
                set: a.set.clone(),
                invoke: a.invoke.clone(),
                next: if i + 1 < actions.len() { vec![id(i + 1)] } else { vec![] },
                ..NodeConfig::empty()
            })
            .collect();
        (nodes, if actions.is_empty() { vec![] } else { vec![id(0)] })
    }

    /// Everything that can be checked without the devices. Errors name the node.
    pub(super) fn compile(configs: &[NodeConfig], start: &[String], c: &Compilers) -> Result<Self, String> {
        let mut index = HashMap::new();
        for (i, n) in configs.iter().enumerate() {
            if n.id.is_empty() {
                return Err(format!("node {}: needs an `id`", i + 1));
            }
            if index.insert(n.id.as_str(), i).is_some() {
                return Err(format!("two nodes have the id `{}`", n.id));
            }
        }
        let resolve = |from: &str, port: &str, ids: &[String]| -> Result<Vec<usize>, String> {
            ids.iter().map(|id| index.get(id.as_str()).copied().ok_or_else(|| format!("{from}`{port}` leads to `{id}`, which is no node"))).collect()
        };
        if start.is_empty() {
            return Err("`start` names the first node(s)".into());
        }
        let start = resolve("", "start", start)?;

        let mut nodes = vec![];
        for n in configs {
            let at = |e: String| format!("node `{}`: {e}", n.id);
            let kind = node_kind(n, c).map_err(at)?;
            let allowed: &[Port] = match kind {
                Kind::Set { .. } | Kind::Invoke { .. } | Kind::Scene(_) => &[Port::Next, Port::Error],
                Kind::Delay(_) | Kind::Merge(_) => &[Port::Next],
                Kind::Wait { .. } => &[Port::Next, Port::Timeout],
                Kind::If { .. } => &[Port::Then, Port::Else],
                Kind::Repeat(_) => &[Port::Body, Port::Next],
                Kind::Stop(_) => &[],
            };
            let mut ports = vec![];
            for (port, ids) in [
                (Port::Next, &n.next),
                (Port::Error, &n.error),
                (Port::Timeout, &n.timeout),
                (Port::Then, &n.then),
                (Port::Else, &n.otherwise),
                (Port::Body, &n.body),
            ] {
                if ids.is_empty() {
                    continue;
                }
                if !allowed.contains(&port) {
                    let names: Vec<&str> = allowed.iter().map(|p| p.name()).collect();
                    let has = if names.is_empty() { "none".into() } else { names.join(", ") };
                    return Err(at(format!("this node has no `{}` port (it has: {has})", port.name())));
                }
                ports.push((port, resolve("", port.name(), ids).map_err(at)?));
            }
            if matches!(kind, Kind::Repeat(_)) && n.body.is_empty() {
                return Err(at("a `repeat` needs a `body`: the nodes it repeats".into()));
            }
            nodes.push(Node { id: n.id.clone(), kind, ports });
        }

        let mut inputs = vec![0; nodes.len()];
        // Where each edge comes from: a node and its port (none: `start`).
        let mut from: Vec<Vec<Option<(usize, Port)>>> = vec![vec![]; nodes.len()];
        for &s in &start {
            inputs[s] += 1;
            from[s].push(None);
        }
        for (i, n) in nodes.iter().enumerate() {
            for (port, targets) in &n.ports {
                for &t in targets {
                    inputs[t] += 1;
                    from[t].push(Some((i, *port)));
                }
            }
        }
        for (n, &count) in nodes.iter().zip(&inputs) {
            match (count, &n.kind) {
                (0, _) => return Err(format!("node `{}`: nothing leads to it", n.id)),
                (1, _) | (_, Kind::Merge(_)) => {}
                _ => return Err(format!("node `{}`: several nodes lead to it: join them with a `merge` node", n.id)),
            }
        }
        let flow = Flow { nodes, start, inputs };
        if let Some(n) = flow.cycle() {
            return Err(format!("node `{}` leads back to itself: loop with a `repeat` node", flow.nodes[n].id));
        }
        // A body is closed: what it reaches, nothing else reaches (it is entered only from its
        // repeat, and left only by ending).
        let mut in_for_each = HashSet::new();
        for (r, node) in flow.nodes.iter().enumerate() {
            let Kind::Repeat(repeat) = &node.kind else { continue };
            let body = flow.reachable(node.out(Port::Body));
            for &b in &body {
                if from[b].iter().any(|f| f.is_none_or(|(f, port)| !(f == r && port == Port::Body) && !body.contains(&f))) {
                    return Err(format!(
                        "node `{}`: the body of `{}` leads to it, and so does the rest: a body ends by itself, go on from the repeat's `next`",
                        flow.nodes[b].id, node.id
                    ));
                }
            }
            if matches!(repeat, Repeat::ForEach(_)) {
                in_for_each.extend(body);
            }
        }
        for (i, n) in flow.nodes.iter().enumerate() {
            let (pick, from_trigger) = match &n.kind {
                Kind::Set { pick, value, .. } => (Some(pick), matches!(value, Source::Trigger)),
                Kind::Invoke { pick, .. } => (Some(pick), false),
                _ => (None, false),
            };
            if matches!(pick, Some(Pick::Item)) && !in_for_each.contains(&i) {
                return Err(format!("node `{}`: `item` is only for the body of a `repeat` with `for_each`", n.id));
            }
            if (matches!(pick, Some(Pick::Trigger)) || from_trigger) && !c.fired_by_device {
                return Err(format!("node `{}`: `trigger` needs a `when` trigger (a device that starts the run)", n.id));
            }
        }
        Ok(flow)
    }

    /// A node on a cycle, if any.
    fn cycle(&self) -> Option<usize> {
        // 0: not seen, 1: on the path, 2: done.
        fn visit(flow: &Flow, n: usize, color: &mut [u8]) -> Option<usize> {
            match color[n] {
                1 => return Some(n),
                2 => return None,
                _ => {}
            }
            color[n] = 1;
            for (_, targets) in &flow.nodes[n].ports {
                for &t in targets {
                    if let Some(c) = visit(flow, t, color) {
                        return Some(c);
                    }
                }
            }
            color[n] = 2;
            None
        }
        let mut color = vec![0; self.nodes.len()];
        (0..self.nodes.len()).find_map(|n| visit(self, n, &mut color))
    }

    fn reachable(&self, from: &[usize]) -> HashSet<usize> {
        let mut seen = HashSet::new();
        let mut todo = from.to_vec();
        while let Some(n) = todo.pop() {
            if seen.insert(n) {
                todo.extend(self.nodes[n].ports.iter().flat_map(|(_, t)| t));
            }
        }
        seen
    }

    /// Its `set` nodes, when it is made of nothing but `set` and `invoke` (no wait, no branch, no
    /// value from the trigger): what a device can run on its own. The core runs the invokes.
    pub(super) fn plain_sets(&self) -> Option<Vec<(&Target, &CapKind, &CapValue)>> {
        let mut sets = vec![];
        for n in &self.nodes {
            match &n.kind {
                _ if !n.out(Port::Error).is_empty() => return None,
                Kind::Set { pick: Pick::Select(t), cap, value: Source::Value(v) } => sets.push((t, cap, v)),
                Kind::Invoke { .. } => {}
                _ => return None,
            }
        }
        Some(sets)
    }

    /// The scenes it sets: (node, scene).
    pub(super) fn scene_refs(&self) -> impl Iterator<Item = (&str, &str)> {
        self.nodes.iter().filter_map(|n| match &n.kind {
            Kind::Scene(id) => Some((n.id.as_str(), id.as_str())),
            _ => None,
        })
    }

    /// Its `set` and `invoke` nodes, described for the app.
    pub(super) fn describe_actions(&self) -> Vec<Value> {
        self.nodes.iter().filter_map(describe_action).collect()
    }

    /// What its nodes need of the devices, for the problems of an automation.
    pub(super) fn needs(&self, need: &mut Need) {
        for n in &self.nodes {
            let conditions: &[Condition] = match &n.kind {
                Kind::Set { pick: Pick::Select(target), cap, .. } => {
                    need(target, cap.to_string(), &|e: &Entity| e.caps.contains_key(cap));
                    &[]
                }
                Kind::Invoke { pick: Pick::Select(target), action, .. } => {
                    need(target, format!("the action `{action}`"), &|e: &Entity| e.actions.iter().any(|d| d.name == *action));
                    &[]
                }
                Kind::Wait { until, trigger, .. } => {
                    if let Some(t) = trigger {
                        trigger_needs(t, need);
                    }
                    until
                }
                Kind::If { conditions, .. } => conditions,
                Kind::Repeat(Repeat::While(c) | Repeat::Until(c)) => c,
                Kind::Repeat(Repeat::ForEach(target)) => {
                    need(target, "a device".into(), &|_| true);
                    &[]
                }
                _ => &[],
            };
            for c in conditions {
                condition_needs(c, need);
            }
        }
    }

    /// The path a run would take now, without waiting nor doing anything: an `if` decides on the
    /// current state, a `wait` is passed through, a body is shown once. With what each `set` and
    /// `invoke` on the way would do.
    pub(super) fn preview(&self, r: &Reconciler, now: &Zoned) -> (Vec<PathStep>, Vec<Planned>) {
        let (mut path, mut planned) = (vec![], vec![]);
        let mut seen = HashSet::new();
        let mut todo: Vec<usize> = self.start.iter().rev().copied().collect();
        let holds = |conditions: &[Condition], any: bool| all_or_any(conditions, any, |c| check(c, now, r.entities.values(), None));
        while let Some(n) = todo.pop() {
            if !seen.insert(n) {
                continue;
            }
            let node = &self.nodes[n];
            let (ports, ok, reason): (&[Port], bool, String) = match &node.kind {
                Kind::Set { .. } | Kind::Invoke { .. } => {
                    planned.push(planned_for(node, r));
                    (&[Port::Next], true, String::new())
                }
                Kind::Scene(id) => (&[Port::Next], true, format!("sets the scene `{id}`")),
                Kind::Delay(d) => (&[Port::Next], true, format!("waits {}", show(*d))),
                Kind::Wait { until, trigger, timeout } => {
                    let limit = timeout.map_or(String::new(), |t| format!(", {} at most", show(t)));
                    match trigger {
                        Some(t) => (&[Port::Next], true, format!("waits for {}{limit}", super::describe_trigger(t))),
                        None => {
                            let (ok, reason) = holds(until, false);
                            (&[Port::Next], ok, if ok { format!("goes on: {reason}") } else { format!("waits: {reason}{limit}") })
                        }
                    }
                }
                Kind::If { conditions, any } => {
                    let (ok, reason) = holds(conditions, *any);
                    (if ok { &[Port::Then] } else { &[Port::Else] }, ok, format!("{}: {reason}", if ok { "then" } else { "else" }))
                }
                Kind::Repeat(repeat) => (&[Port::Body, Port::Next], true, describe_repeat(repeat)),
                Kind::Merge(_) => (&[Port::Next], true, String::new()),
                Kind::Stop(error) => (&[], error.is_none(), error.clone().unwrap_or_else(|| "stops".into())),
            };
            path.push(PathStep { node: node.id.clone(), ok, reason });
            // Depth first, in the order written.
            for port in ports.iter().rev() {
                todo.extend(node.out(*port).iter().rev());
            }
        }
        (path, planned)
    }
}

impl NodeConfig {
    fn empty() -> Self {
        NodeConfig {
            id: String::new(),
            set: None,
            invoke: None,
            scene: None,
            delay: None,
            wait: None,
            test: None,
            any: false,
            repeat: None,
            merge: None,
            stop: None,
            next: vec![],
            error: vec![],
            timeout: vec![],
            then: vec![],
            otherwise: vec![],
            body: vec![],
        }
    }
}

fn node_kind(n: &NodeConfig, c: &Compilers) -> Result<Kind, String> {
    let conditions = |list: &[ConditionConfig]| -> Result<Vec<Condition>, String> {
        if list.is_empty() {
            return Err("give at least one condition".into());
        }
        list.iter().map(c.condition).collect()
    };
    let kinds = [
        n.set.is_some(),
        n.invoke.is_some(),
        n.scene.is_some(),
        n.delay.is_some(),
        n.wait.is_some(),
        n.test.is_some(),
        n.repeat.is_some(),
        n.merge.is_some(),
        n.stop.is_some(),
    ];
    if kinds.iter().filter(|k| **k).count() != 1 {
        return Err("give exactly one of `set`, `invoke`, `scene`, `delay`, `wait`, `if`, `repeat`, `merge`, `stop`".into());
    }
    if n.any && n.test.is_none() {
        return Err("`any` goes with `if`".into());
    }
    if let Some(set) = &n.set {
        let cap: CapKind = set.cap.parse()?;
        let value = match (&set.value, set.from.as_deref()) {
            (Some(v), None) => Source::Value(CapValue::from_plain(&cap, v.clone())?),
            (None, Some("trigger")) => Source::Trigger,
            (None, Some(other)) => return Err(format!("`from = \"{other}\"`: only `from = \"trigger\"` is known")),
            _ => return Err("give either `value` or `from`".into()),
        };
        return Ok(Kind::Set { pick: Pick::parse(&set.select)?, cap, value });
    }
    if let Some(inv) = &n.invoke {
        if !(inv.params.is_null() || inv.params.is_object()) {
            return Err("`params` must be a table".into());
        }
        return Ok(Kind::Invoke { pick: Pick::parse(&inv.select)?, action: inv.action.clone(), params: inv.params.clone() });
    }
    if let Some(scene) = &n.scene {
        return Ok(Kind::Scene(scene.clone()));
    }
    if let Some(d) = &n.delay {
        return Ok(Kind::Delay(duration(d)?));
    }
    if let Some(w) = &n.wait {
        let timeout = w.timeout.as_deref().map(duration).transpose()?;
        return match (&w.until, &w.trigger) {
            (Some(until), None) => Ok(Kind::Wait { until: conditions(until)?, trigger: None, timeout }),
            (None, Some(t)) if t.id.is_some() => Err("a `wait` trigger has no `id`".into()),
            (None, Some(t)) => Ok(Kind::Wait { until: vec![], trigger: Some((c.trigger)(t)?), timeout }),
            _ => Err("a `wait` takes either `until` (conditions) or `trigger`".into()),
        };
    }
    if let Some(test) = &n.test {
        return Ok(Kind::If { conditions: conditions(test)?, any: n.any });
    }
    if let Some(r) = &n.repeat {
        let repeat = match (r.count, &r.during, &r.until, &r.for_each) {
            (Some(count), None, None, None) if (1..=MAX_TURNS).contains(&count) => Repeat::Count(count),
            (Some(_), None, None, None) => return Err(format!("`count` is from 1 to {MAX_TURNS}")),
            (None, Some(during), None, None) => Repeat::While(conditions(during)?),
            (None, None, Some(until), None) => Repeat::Until(conditions(until)?),
            (None, None, None, Some(select)) => Repeat::ForEach(Target::parse(select)?),
            _ => return Err("a `repeat` takes one of `count`, `while`, `until`, `for_each`".into()),
        };
        return Ok(Kind::Repeat(repeat));
    }
    if let Some(mode) = n.merge {
        return Ok(Kind::Merge(mode));
    }
    let stop = n.stop.as_ref().expect("one kind is set");
    Ok(Kind::Stop(stop.error.clone()))
}

fn describe_action(n: &Node) -> Option<Value> {
    match &n.kind {
        Kind::Set { pick, cap, value } => {
            let value = match value {
                Source::Value(v) => json!({ "value": v.plain() }),
                Source::Trigger => json!({ "from": "trigger" }),
            };
            let mut set = json!({ "select": pick.text(), "cap": cap.to_string() });
            set.as_object_mut().expect("an object").extend(value.as_object().expect("an object").clone());
            Some(json!({ "set": set }))
        }
        Kind::Invoke { pick, action, params } => Some(json!({ "invoke": { "select": pick.text(), "action": action, "params": params } })),
        _ => None,
    }
}

fn describe_repeat(r: &Repeat) -> String {
    match r {
        Repeat::Count(n) => format!("{n} times"),
        Repeat::While(_) => "while its conditions pass".into(),
        Repeat::Until(_) => "until its conditions pass".into(),
        Repeat::ForEach(t) => format!("for each of `{}`", t.text),
    }
}

/// What a `set` or an `invoke` would do now.
fn planned_for(node: &Node, r: &Reconciler) -> Planned {
    let targets = match &node.kind {
        Kind::Set { pick: Pick::Select(target), cap, value } => target
            .entities(r.entities.values())
            .into_iter()
            .map(|e| {
                let slot = e.caps.get(cap);
                let (error, changes) = match (slot, value) {
                    (None, _) => (Some(format!("{} has no {cap}", e.name)), false),
                    (Some(slot), Source::Value(v)) => (slot.meta.validate(cap, v).err(), slot.desired.as_ref() != Some(v)),
                    (Some(_), Source::Trigger) => (None, true),
                };
                PlannedTarget { entity: e.id, name: e.name.clone(), changes, error }
            })
            .collect(),
        Kind::Invoke { pick: Pick::Select(target), action, .. } => target
            .entities(r.entities.values())
            .into_iter()
            .map(|e| PlannedTarget {
                entity: e.id,
                name: e.name.clone(),
                changes: true,
                error: (!e.actions.iter().any(|d| d.name == *action)).then(|| format!("{} has no action `{action}`", e.name)),
            })
            .collect(),
        // Known only when it runs.
        _ => vec![],
    };
    Planned { action: describe_action(node).unwrap_or_default(), targets }
}

/// All the conditions pass (or one of them, `any`), with why.
fn all_or_any(conditions: &[Condition], any: bool, check: impl Fn(&Condition) -> (bool, String)) -> (bool, String) {
    let checks: Vec<(bool, String)> = conditions.iter().map(check).collect();
    let ok = if any { checks.iter().any(|(ok, _)| *ok) } else { checks.iter().all(|(ok, _)| *ok) };
    (ok, checks.into_iter().map(|(_, why)| why).collect::<Vec<_>>().join("; "))
}

// ------------------------------------------------------------------------------------- run

/// What started a run.
#[derive(Debug, Clone, Default)]
pub(super) struct Fired {
    /// `"19:30"`, `"Lampe on_off"`, `"manual"`.
    pub label: String,
    /// The trigger's `id`, for the `trigger` condition.
    pub id: Option<String>,
    /// The device whose change started it, and its new value.
    pub entity: Option<Uuid>,
    pub value: Option<CapValue>,
}

/// A node that ran, for the trace.
#[derive(Debug, Clone, Serialize)]
pub struct Step {
    pub node: String,
    /// RFC 3339, local time: when it ended.
    pub at: String,
    pub ok: bool,
    pub outcome: String,
}

/// A node on the path a dry run would take.
#[derive(Debug, Serialize)]
pub struct PathStep {
    pub node: String,
    pub ok: bool,
    pub reason: String,
}

/// What a run did so far.
#[derive(Debug, Default)]
pub(super) struct Trace {
    pub steps: Vec<Step>,
    pub results: Vec<ActionResult>,
    /// The nodes running now (a wait shows here).
    pub current: Vec<String>,
    count: usize,
}

/// Why a run ends before its last node.
pub(super) enum Halt {
    /// A `stop` without an error.
    Stop,
    Fail(String),
}

type Boxed<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub(super) struct Exec<'a> {
    pub core: &'a Core,
    pub flow: &'a Flow,
    pub fired: &'a Fired,
    pub trace: &'a Mutex<Trace>,
}

impl Exec<'_> {
    pub async fn run(&self) -> Result<(), Halt> {
        match self.region(&self.flow.start, None).await {
            Err(Halt::Stop) => Ok(()),
            other => other,
        }
    }

    /// Runs from `entry` until nothing is left to run: the whole flow, or one turn of a body.
    fn region<'b>(&'b self, entry: &'b [usize], item: Option<Uuid>) -> Boxed<'b, Result<(), Halt>> {
        Box::pin(async move {
            // By merge: tokens arrived, one of them live, gone on.
            let mut merges: HashMap<usize, (usize, bool, bool)> = HashMap::new();
            let mut tokens: Vec<(usize, bool)> = entry.iter().rev().map(|&n| (n, true)).collect();
            let mut running = FuturesUnordered::new();
            loop {
                while let Some((n, live)) = tokens.pop() {
                    let node = &self.flow.nodes[n];
                    if let Kind::Merge(mode) = node.kind {
                        let (count, any_live, gone) = merges.entry(n).or_default();
                        *count += 1;
                        *any_live |= live;
                        let complete = *count == self.flow.inputs[n];
                        let go = !*gone && match mode {
                            MergeMode::All => complete,
                            MergeMode::Any => live || complete,
                        };
                        if go {
                            *gone = true;
                            let live = *any_live;
                            if live {
                                self.step(node, true, String::new());
                            }
                            tokens.extend(node.out(Port::Next).iter().map(|&t| (t, live)));
                        }
                    } else if live {
                        running.push(self.node(n, item));
                    } else {
                        // Nothing taken: every port passes the dead token on.
                        tokens.extend(node.tokens(Port::Body));
                    }
                }
                match running.next().await {
                    None => return Ok(()),
                    Some(next) => tokens.extend(next?),
                }
            }
        })
    }

    fn node<'b>(&'b self, n: usize, item: Option<Uuid>) -> Boxed<'b, Result<Vec<(usize, bool)>, Halt>> {
        Box::pin(async move {
            let node = &self.flow.nodes[n];
            {
                let mut trace = self.trace.lock().unwrap();
                trace.count += 1;
                if trace.count > MAX_STEPS {
                    return Err(Halt::Fail(format!("stopped: more than {MAX_STEPS} steps (a repeat with nothing to wait for?)")));
                }
                trace.current.push(node.id.clone());
            }
            let (port, ok, outcome) = match &node.kind {
                Kind::Set { pick, cap, value } => {
                    let results = self.set(pick, cap, value, item);
                    let ok = results.iter().all(|r| r.ok);
                    (if ok { Port::Next } else { Port::Error }, ok, self.results(results))
                }
                Kind::Invoke { pick, action, params } => {
                    let results = self.invoke(pick, action, params, item).await;
                    let ok = results.iter().all(|r| r.ok);
                    (if ok { Port::Next } else { Port::Error }, ok, self.results(results))
                }
                Kind::Scene(id) => {
                    let scenes = self.core.scenes();
                    let results = scenes.apply(self.core, id).await.unwrap_or_else(|e| {
                        vec![ActionResult { entity: None, target: id.clone(), action: "scene".into(), ok: false, outcome: e }]
                    });
                    let ok = results.iter().all(|r| r.ok);
                    (if ok { Port::Next } else { Port::Error }, ok, self.results(results))
                }
                Kind::Delay(d) => {
                    tokio::time::sleep(*d).await;
                    (Port::Next, true, show(*d))
                }
                Kind::Wait { until, trigger, timeout } => {
                    let (passed, outcome) = self.wait(until, trigger.as_ref(), *timeout).await;
                    (if passed { Port::Next } else { Port::Timeout }, true, outcome)
                }
                Kind::If { conditions, any } => {
                    let (ok, reason) = self.holds(conditions, *any);
                    (if ok { Port::Then } else { Port::Else }, true, format!("{}: {reason}", if ok { "then" } else { "else" }))
                }
                Kind::Repeat(repeat) => {
                    let turns = self.repeat(repeat, node.out(Port::Body), item).await?;
                    (Port::Next, true, format!("{turns} turns"))
                }
                Kind::Merge(_) => unreachable!("merges are joined by the region"),
                Kind::Stop(error) => {
                    self.step(node, error.is_none(), error.clone().unwrap_or_else(|| "stopped".into()));
                    return Err(error.clone().map_or(Halt::Stop, Halt::Fail));
                }
            };
            self.step(node, ok, outcome);
            Ok(node.tokens(port))
        })
    }

    fn step(&self, node: &Node, ok: bool, outcome: String) {
        let mut trace = self.trace.lock().unwrap();
        if let Some(i) = trace.current.iter().position(|c| *c == node.id) {
            trace.current.remove(i);
        }
        let at = Zoned::now().strftime("%Y-%m-%dT%H:%M:%S%:z").to_string();
        trace.steps.push(Step { node: node.id.clone(), at, ok, outcome });
    }

    /// Keeps the results for the run, and sums them up for the step.
    fn results(&self, results: Vec<ActionResult>) -> String {
        let outcome = results.iter().map(|r| format!("{}: {}", r.target, r.outcome)).collect::<Vec<_>>().join(", ");
        self.trace.lock().unwrap().results.extend(results);
        outcome
    }

    fn holds(&self, conditions: &[Condition], any: bool) -> (bool, String) {
        let now = Zoned::now();
        self.core.with(|r| all_or_any(conditions, any, |c| check(c, &now, r.entities.values(), self.fired.id.as_deref())))
    }

    fn pick(&self, pick: &Pick, item: Option<Uuid>, r: &Reconciler) -> Result<Vec<(Uuid, String)>, String> {
        let one = |id: Option<Uuid>, missing: &str| {
            id.and_then(|id| r.entities.get(&id)).map(|e| vec![(e.id, e.name.clone())]).ok_or_else(|| missing.to_string())
        };
        match pick {
            Pick::Select(t) => Ok(t.entities(r.entities.values()).iter().map(|e| (e.id, e.name.clone())).collect()),
            Pick::Trigger => one(self.fired.entity, "no device started this run"),
            Pick::Item => one(item, "the device of this turn is gone"),
        }
    }

    fn set(&self, pick: &Pick, cap: &CapKind, value: &Source, item: Option<Uuid>) -> Vec<ActionResult> {
        let value = match value {
            Source::Value(v) => Ok(v.clone()),
            Source::Trigger => self
                .fired
                .value
                .as_ref()
                .ok_or_else(|| "no value started this run".to_string())
                .and_then(|v| CapValue::from_plain(cap, v.plain())),
        };
        let label = format!("set {cap} {}", value.as_ref().map_or_else(|_| "?".into(), |v| v.plain().to_string()));
        let results = self.core.with(|r| {
            let ids = match (self.pick(pick, item, r), &value) {
                (Ok(ids), Ok(_)) => ids,
                (Err(e), _) => return vec![failed(pick, &label, e)],
                (_, Err(e)) => return vec![failed(pick, &label, e.clone())],
            };
            set_each(r, ids, cap, value.as_ref().expect("checked above"), &label)
        });
        empty_is_failed(results, pick, &label)
    }

    async fn invoke(&self, pick: &Pick, action: &str, params: &Value, item: Option<Uuid>) -> Vec<ActionResult> {
        let label = format!("invoke {action}");
        let ids = match self.core.with(|r| self.pick(pick, item, r)) {
            Ok(ids) => ids,
            Err(e) => return vec![failed(pick, &label, e)],
        };
        empty_is_failed(invoke_each(self.core, ids, action, params, &label).await, pick, &label)
    }

    /// Whether it went on (`false`: timed out), and how.
    async fn wait(&self, until: &[Condition], trigger: Option<&Trigger>, timeout: Option<Duration>) -> (bool, String) {
        // Before looking: a change in between is not missed.
        let mut changes = self.core.changes.subscribe();
        let deadline = timeout.map(|t| tokio::time::Instant::now() + t);
        let at = match trigger {
            Some(Trigger::At { time, days }) => next_at(*time, *days, &Zoned::now()),
            _ => None,
        };
        // A `when` fires on an edge from the values seen when the wait began.
        let mut seen: HashMap<Uuid, CapValue> = match trigger {
            Some(Trigger::When(w)) => self.core.with(|r| {
                r.entities
                    .values()
                    .filter(|e| w.target.select.matches(e))
                    .filter_map(|e| Some((e.id, e.caps.get(&w.cap)?.observed.clone()?)))
                    .collect()
            }),
            _ => HashMap::new(),
        };
        loop {
            if !until.is_empty() {
                let (ok, reason) = self.holds(until, false);
                if ok {
                    return (true, reason);
                }
            }
            let mut nap = RECHECK;
            if let Some(at) = &at {
                let now = Zoned::now();
                if now >= *at {
                    return (true, at.strftime("%H:%M").to_string());
                }
                nap = nap.min(Duration::try_from(at.duration_since(&now)).unwrap_or_default());
            }
            if let Some(deadline) = deadline {
                let now = tokio::time::Instant::now();
                if now >= deadline {
                    return (false, "timed out".into());
                }
                nap = nap.min(deadline - now);
            }
            tokio::select! {
                () = tokio::time::sleep(nap) => {}
                event = changes.recv() => match event {
                    Ok(Event::Slot(change)) => {
                        let Some(Trigger::When(w)) = trigger else { continue };
                        let Some(now) = change.slot.observed.clone().filter(|_| change.cap == w.cap) else { continue };
                        let Some(name) = self.core.with(|r| r.entities.get(&change.id).filter(|e| w.target.select.matches(e)).map(|e| e.name.clone())) else {
                            continue;
                        };
                        let before = seen.insert(change.id, now.clone());
                        if crosses(&w.test, before.as_ref(), &now) {
                            return (true, format!("{name} {}", w.cap));
                        }
                    }
                    Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return (false, "the hub is stopping".into()),
                },
            }
        }
    }

    /// Runs the body as `repeat` says; how many turns it did.
    async fn repeat(&self, repeat: &Repeat, body: &[usize], item: Option<Uuid>) -> Result<u32, Halt> {
        let too_many = || Halt::Fail(format!("stopped: a repeat goes round {MAX_TURNS} times at most"));
        let mut turns = 0;
        match repeat {
            Repeat::Count(n) => {
                for _ in 0..*n {
                    self.region(body, item).await?;
                    turns += 1;
                }
            }
            Repeat::While(conditions) => {
                while self.holds(conditions, false).0 {
                    if turns == MAX_TURNS {
                        return Err(too_many());
                    }
                    self.region(body, item).await?;
                    turns += 1;
                }
            }
            Repeat::Until(conditions) => loop {
                if turns == MAX_TURNS {
                    return Err(too_many());
                }
                self.region(body, item).await?;
                turns += 1;
                if self.holds(conditions, false).0 {
                    break;
                }
            },
            Repeat::ForEach(target) => {
                let ids: Vec<Uuid> = self.core.with(|r| target.entities(r.entities.values()).iter().map(|e| e.id).collect());
                for id in ids.into_iter().take(MAX_TURNS as usize) {
                    self.region(body, Some(id)).await?;
                    turns += 1;
                }
            }
        }
        Ok(turns)
    }
}

fn failed(pick: &Pick, label: &str, outcome: String) -> ActionResult {
    ActionResult { entity: None, target: pick.text().into(), action: label.into(), ok: false, outcome }
}

fn empty_is_failed(results: Vec<ActionResult>, pick: &Pick, label: &str) -> Vec<ActionResult> {
    match pick {
        Pick::Select(target) => nothing_matched(results, target, label),
        _ => results,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_read_as_written() {
        assert_eq!(duration("45s"), Ok(Duration::from_secs(45)));
        assert_eq!(duration("1h30m"), Ok(Duration::from_secs(5400)));
        assert_eq!(show(Duration::from_secs(5400)), "1h30m");
        assert_eq!(show(Duration::from_secs(50 * 3600 + 5)), "50h5s");
        for bad in ["", "10", "m", "10x", "1h-3m"] {
            assert!(duration(bad).unwrap_err().contains("not a duration"), "{bad}");
        }
        assert!(duration("31d").is_err() && duration("800h").unwrap_err().contains("30 days"));
    }
}
