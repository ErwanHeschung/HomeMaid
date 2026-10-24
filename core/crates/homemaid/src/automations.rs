//! Typed automations: `[[automation]]` in homemaid.toml, checked when the config loads.
//!
//! A trigger (a time of day, sunrise or sunset included, or an observed state crossing a value)
//! runs the automation; its conditions decide; then its nodes run, a workflow (see [flow]): set
//! desired states (the reconciler does the rest), invoke one-shot actions, wait, branch, repeat.
//! Entities are chosen by selectors, never by name. A dry run tells what would happen without
//! doing it. Each automation keeps its last runs in memory, with the path they took.

mod flow;

use std::{
    collections::{BTreeSet, HashMap, VecDeque},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

pub use flow::{MergeMode, NodeConfig, PathStep, RepeatConfig, Step, StopConfig, WaitConfig};
use flow::{Exec, Fired, Flow, Halt, Trace};

use jiff::{
    Zoned,
    civil::{Date, Time},
    tz::TimeZone,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    Core, Event,
    model::{CapKind, CapValue, Entity},
    reconciler::{Change, Reconciler},
    selector::Selector,
};

const KEEP_RUNS: usize = 20;
/// More runs than this in a minute is a loop (two automations answering each other): stop it.
const MAX_RUNS_PER_MINUTE: usize = 10;
/// An `at` trigger found due later than this (the PC was asleep at that time) is skipped, not run late.
const GRACE: Duration = Duration::from_secs(5 * 60);
/// Largest sunrise/sunset offset, in minutes: what the devices that run schedules accept.
const MAX_SUN_OFFSET: i16 = 360;
/// Runs of one automation at the same time, waiting ones included (`queued`, `parallel`).
const MAX_RUNNING: usize = 10;
/// `automation.run` answers when the run ends, or after this with where it is.
const ANSWER_WITHIN: Duration = Duration::from_secs(10);

// ------------------------------------------------------------------------------------- config

// Also written back (the app edits automations): unset options are left out, as a person would.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AutomationConfig {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Mode::is_single")]
    pub mode: Mode,
    /// The first node(s).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub start: Vec<String>,
    #[serde(default, rename = "trigger", skip_serializing_if = "Vec::is_empty")]
    pub triggers: Vec<TriggerConfig>,
    #[serde(default, rename = "condition", skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<ConditionConfig>,
    #[serde(default, rename = "node", skip_serializing_if = "Vec::is_empty")]
    pub nodes: Vec<NodeConfig>,
    /// Before nodes: a list run in order, still read (a chain of nodes).
    #[serde(default, rename = "action", skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<ActionConfig>,
}

/// What a trigger does while a run is not over (a wait, a delay).
#[derive(Debug, Clone, Copy, Default, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Nothing: the new run is skipped.
    #[default]
    Single,
    /// The run is stopped, and a new one starts.
    Restart,
    /// The new run starts when the others have ended.
    Queued,
    /// The new run starts beside it.
    Parallel,
}

impl Mode {
    fn is_single(&self) -> bool {
        *self == Mode::Single
    }
}

fn yes() -> bool {
    true
}

fn is_true(b: &bool) -> bool {
    *b
}

fn is_false(b: &bool) -> bool {
    !*b
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerConfig {
    /// For the `trigger` condition: which one started the run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub days: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub when: Option<TestConfig>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConditionConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub between: Option<[String; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub days: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<TestConfig>,
    /// The `id` of the trigger that started the run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger: Option<String>,
}

/// `{ select, cap, ... }` with one test: `becomes`/`is` (a value), `above`, `below`, `changes`.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TestConfig {
    pub select: String,
    pub cap: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub becomes: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub above: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub below: Option<f64>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub changes: bool,
    /// For conditions: every selected entity must pass (default: any).
    #[serde(default, skip_serializing_if = "is_false")]
    pub all: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ActionConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub set: Option<SetConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invoke: Option<InvokeConfig>,
}

/// `select = "trigger"` is the device that started the run, `"item"` the one of a `for_each`.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SetConfig {
    pub select: String,
    pub cap: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
    /// `"trigger"`: the value that started the run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InvokeConfig {
    pub select: String,
    pub action: String,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub params: Value,
}

// ------------------------------------------------------------------------------------- compiled

/// Days of the week, Monday = bit 0.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Days(u8);

const DAY_NAMES: [&str; 7] = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];

impl Days {
    const ALL: Days = Days(0x7f);

    fn parse(names: &[String]) -> Result<Self, String> {
        let mut mask = 0;
        for name in names {
            let day = DAY_NAMES
                .iter()
                .position(|d| d.eq_ignore_ascii_case(name))
                .ok_or_else(|| format!("unknown day `{name}` (mon, tue, wed, thu, fri, sat, sun)"))?;
            mask |= 1 << day;
        }
        if mask == 0 {
            return Err("`days` is empty".into());
        }
        Ok(Days(mask))
    }

    fn contains(self, z: &Zoned) -> bool {
        self.0 & (1 << z.weekday().to_monday_zero_offset()) != 0
    }

    fn names(self) -> Vec<&'static str> {
        DAY_NAMES.iter().enumerate().filter(|(i, _)| self.0 & (1 << i) != 0).map(|(_, d)| *d).collect()
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Target {
    pub(crate) select: Selector,
    /// As written, for people.
    pub(crate) text: String,
}

impl Target {
    pub(crate) fn parse(text: &str) -> Result<Self, String> {
        Ok(Self { select: Selector::parse(text)?, text: text.into() })
    }

    pub(crate) fn entities<'a>(&self, entities: impl IntoIterator<Item = &'a Entity>) -> Vec<&'a Entity> {
        entities.into_iter().filter(|e| self.select.matches(e)).collect()
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Test {
    Is(CapValue),
    Above(f64),
    Below(f64),
    Changes,
}

#[derive(Debug, Clone)]
struct StateTest {
    target: Target,
    cap: CapKind,
    test: Test,
    all: bool,
}

/// Where the hub is (`latitude` and `longitude` in `[core]`), for sunrise and sunset.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Location {
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Sun {
    Rise,
    Set,
}

/// A time of day: `19:30`, or `sunrise`/`sunset` shifted by minutes (`sunset-30`).
#[derive(Debug, Clone, Copy, PartialEq)]
enum TimeOf {
    Clock(Time),
    Sun(Sun, i16, Location),
}

impl TimeOf {
    fn parse(text: &str, location: Option<Location>) -> Result<Self, String> {
        let sun = [("sunrise", Sun::Rise), ("sunset", Sun::Set)].into_iter().find_map(|(name, sun)| Some((sun, text.strip_prefix(name)?)));
        let Some((sun, offset)) = sun else {
            return text.parse().map(TimeOf::Clock).map_err(|_| format!("`{text}` is not a time (HH:MM, sunrise, sunset, sunset-30)"));
        };
        let offset: i16 = match offset {
            "" => 0,
            o if o.starts_with(['+', '-']) => o.parse().map_err(|_| format!("`{text}`: the offset is in minutes, as in sunset-30"))?,
            _ => return Err(format!("`{text}`: write the offset with its sign, as in sunset-30 or sunrise+15")),
        };
        if offset.abs() > MAX_SUN_OFFSET {
            return Err(format!("`{text}`: the offset is {MAX_SUN_OFFSET} minutes at most"));
        }
        let location = location.ok_or("sunrise and sunset need `latitude` and `longitude` in [core]")?;
        sunrise::Coordinates::new(location.latitude, location.longitude).ok_or("[core] latitude or longitude is out of range")?;
        Ok(TimeOf::Sun(sun, offset, location))
    }

    /// When it is on `date`, in `tz`; none when the sun does not rise or set that day (polar
    /// regions). A clock time that does not exist that day (spring forward) is just after the gap.
    fn on(&self, date: Date, tz: &TimeZone) -> Option<Zoned> {
        match *self {
            TimeOf::Clock(time) => date.to_datetime(time).to_zoned(tz.clone()).ok(),
            TimeOf::Sun(sun, offset, location) => {
                let coordinates = sunrise::Coordinates::new(location.latitude, location.longitude)?;
                let day = chrono::NaiveDate::from_ymd_opt(date.year().into(), date.month() as u32, date.day() as u32)?;
                let event = match sun {
                    Sun::Rise => sunrise::SolarEvent::Sunrise,
                    Sun::Set => sunrise::SolarEvent::Sunset,
                };
                let utc = sunrise::SolarDay::new(coordinates, day).event_time(event)?;
                let at = jiff::Timestamp::from_second(utc.timestamp()).ok()?.to_zoned(tz.clone());
                at.checked_add(jiff::SignedDuration::from_mins(offset.into())).ok()
            }
        }
    }
}

impl TimeOf {
    fn schedule_time(&self) -> ScheduleTime {
        match *self {
            TimeOf::Clock(t) => ScheduleTime::Clock(t.hour() as u16 * 60 + t.minute() as u16),
            TimeOf::Sun(Sun::Rise, offset, _) => ScheduleTime::Sunrise(offset),
            TimeOf::Sun(Sun::Set, offset, _) => ScheduleTime::Sunset(offset),
        }
    }
}

/// A schedule a device can run on its own, so it still happens while the hub is off: one `at`
/// trigger of an automation that needs nothing but the time, and one of its `set` actions.
#[derive(Debug, Clone, PartialEq)]
pub struct Schedule {
    /// The automation it comes from.
    pub automation: String,
    pub cap: CapKind,
    pub value: CapValue,
    pub at: ScheduleTime,
    /// Monday = bit 0.
    pub days: u8,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScheduleTime {
    /// Minutes after midnight.
    Clock(u16),
    /// Minutes after sunrise (negative: before).
    Sunrise(i16),
    Sunset(i16),
}

impl std::fmt::Display for TimeOf {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            TimeOf::Clock(time) => write!(f, "{}", time.strftime("%H:%M")),
            TimeOf::Sun(sun, offset, _) => {
                write!(f, "{}", if *sun == Sun::Rise { "sunrise" } else { "sunset" })?;
                if *offset != 0 {
                    write!(f, "{offset:+}")?;
                }
                Ok(())
            }
        }
    }
}

#[derive(Debug, Clone)]
enum Trigger {
    At { time: TimeOf, days: Days },
    When(StateTest),
}

#[derive(Debug, Clone)]
enum Condition {
    Between(TimeOf, TimeOf),
    Days(Days),
    State(StateTest),
    Trigger(String),
}

#[derive(Debug, Clone)]
pub struct Automation {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    mode: Mode,
    triggers: Vec<Trigger>,
    /// By trigger.
    trigger_ids: Vec<Option<String>>,
    conditions: Vec<Condition>,
    flow: Flow,
    /// As written, for the app's editor.
    config: AutomationConfig,
}

fn state_test(c: &TestConfig, trigger: bool) -> Result<StateTest, String> {
    let cap: CapKind = c.cap.parse()?;
    let value = if trigger { &c.becomes } else { &c.is };
    let wrong = if trigger { &c.is } else { &c.becomes };
    if wrong.is_some() {
        return Err(if trigger { "a trigger tests with `becomes`, not `is`" } else { "a condition tests with `is`, not `becomes`" }.into());
    }
    let mut tests = vec![];
    if let Some(v) = value {
        tests.push(Test::Is(CapValue::from_plain(&cap, v.clone())?));
    }
    if let Some(x) = c.above {
        tests.push(Test::Above(x));
    }
    if let Some(x) = c.below {
        tests.push(Test::Below(x));
    }
    if c.changes {
        if !trigger {
            return Err("`changes` only makes sense for a trigger".into());
        }
        tests.push(Test::Changes);
    }
    let [test] = <[Test; 1]>::try_from(tests).map_err(|_| {
        format!("give exactly one test: {}, `above`, `below`{}", if trigger { "`becomes`" } else { "`is`" }, if trigger { ", `changes`" } else { "" })
    })?;
    if matches!(test, Test::Above(_) | Test::Below(_)) && !CapValue::is_numeric(&cap) {
        return Err(format!("{cap} is not a number: use {}", if trigger { "`becomes`" } else { "`is`" }));
    }
    if trigger && c.all {
        return Err("`all` only applies to conditions".into());
    }
    Ok(StateTest { target: Target::parse(&c.select)?, cap, test, all: c.all })
}

fn trigger(t: &TriggerConfig, location: Option<Location>) -> Result<Trigger, String> {
    match (&t.at, &t.when) {
        (Some(at), None) => Ok(Trigger::At { time: TimeOf::parse(at, location)?, days: t.days.as_deref().map(Days::parse).transpose()?.unwrap_or(Days::ALL) }),
        (None, Some(when)) if t.days.is_none() => Ok(Trigger::When(state_test(when, true)?)),
        (None, Some(_)) => Err("`days` goes with `at`".into()),
        _ => Err("give either `at` or `when`".into()),
    }
}

fn condition(k: &ConditionConfig, location: Option<Location>, trigger_ids: &[Option<String>]) -> Result<Condition, String> {
    let time = |text: &str| TimeOf::parse(text, location);
    match (&k.between, &k.days, &k.state, &k.trigger) {
        (Some([from, to]), None, None, None) => Ok(Condition::Between(time(from)?, time(to)?)),
        (None, Some(days), None, None) => Ok(Condition::Days(Days::parse(days)?)),
        (None, None, Some(state), None) => Ok(Condition::State(state_test(state, false)?)),
        (None, None, None, Some(id)) if trigger_ids.iter().flatten().any(|t| t == id) => Ok(Condition::Trigger(id.clone())),
        (None, None, None, Some(id)) => Err(format!("no trigger has the id `{id}`")),
        _ => Err("give one of `between`, `days`, `state`, `trigger`".into()),
    }
}

impl Automation {
    /// Checks everything that can be checked without the devices; errors stop the core.
    pub fn compile(c: &AutomationConfig, location: Option<Location>) -> Result<Self, String> {
        let at = |what: &str, i: usize, e: String| format!("automation `{}`, {what} {}: {e}", c.id, i + 1);
        if c.id.is_empty() {
            return Err("an automation needs an `id`".into());
        }
        if c.triggers.is_empty() || (c.nodes.is_empty() && c.actions.is_empty()) {
            return Err(format!("automation `{}`: needs at least one [[automation.trigger]] and one [[automation.node]]", c.id));
        }
        let mut triggers = vec![];
        let mut trigger_ids: Vec<Option<String>> = vec![];
        for (i, t) in c.triggers.iter().enumerate() {
            triggers.push(trigger(t, location).map_err(|e| at("trigger", i, e))?);
            if t.id.as_ref().is_some_and(|id| id.is_empty() || trigger_ids.contains(&Some(id.clone()))) {
                return Err(at("trigger", i, "its `id` is empty or another trigger's".into()));
            }
            trigger_ids.push(t.id.clone());
        }
        let conditions = c
            .conditions
            .iter()
            .enumerate()
            .map(|(i, k)| condition(k, location, &trigger_ids).map_err(|e| at("condition", i, e)))
            .collect::<Result<Vec<_>, _>>()?;
        let compilers = flow::Compilers {
            condition: &|k| condition(k, location, &trigger_ids),
            trigger: &|t| trigger(t, location),
            fired_by_device: triggers.iter().any(|t| matches!(t, Trigger::When(_))),
        };
        let flow = match (c.nodes.is_empty(), c.actions.is_empty()) {
            (false, false) => return Err(format!("automation `{}`: write [[automation.node]] or [[automation.action]], not both", c.id)),
            (true, _) if !c.start.is_empty() => return Err(format!("automation `{}`: `start` names nodes, and there are none", c.id)),
            (true, _) => {
                let (nodes, start) = Flow::legacy(&c.actions);
                Flow::compile(&nodes, &start, &compilers)
            }
            (false, _) => Flow::compile(&c.nodes, &c.start, &compilers),
        }
        .map_err(|e| format!("automation `{}`, {e}", c.id))?;
        Ok(Self {
            id: c.id.clone(),
            name: c.name.clone().unwrap_or_else(|| c.id.clone()),
            enabled: c.enabled,
            mode: c.mode,
            triggers,
            trigger_ids,
            conditions,
            flow,
            config: c.clone(),
        })
    }

    /// The next time one of its `at` triggers fires, strictly after `after`.
    fn next_time(&self, after: &Zoned) -> Option<Zoned> {
        self.next_trigger(after).map(|(at, _)| at)
    }

    /// Same, with what fired: labelled `19:30`, or `sunset (21:58)`.
    fn next_trigger(&self, after: &Zoned) -> Option<(Zoned, Fired)> {
        self.triggers
            .iter()
            .zip(&self.trigger_ids)
            .filter_map(|(t, id)| match t {
                Trigger::At { time, days } => next_at(*time, *days, after).map(|at| {
                    let clock = at.strftime("%H:%M").to_string();
                    let label = if matches!(time, TimeOf::Clock(_)) { clock } else { format!("{time} ({clock})") };
                    (at, Fired { label, id: id.clone(), ..Fired::default() })
                }),
                Trigger::When(_) => None,
            })
            .min_by(|a, b| a.0.cmp(&b.0))
    }
}

/// Next occurrence of `time` on one of `days`, strictly after `after`, in its time zone.
fn next_at(time: TimeOf, days: Days, after: &Zoned) -> Option<Zoned> {
    (0..8).find_map(|offset| {
        let date = after.date().checked_add(jiff::Span::new().days(offset)).ok()?;
        let at = time.on(date, after.time_zone())?;
        (at > *after && days.contains(&at)).then_some(at)
    })
}

/// Whether `now` is in [from, to), today's sunrise or sunset when they are one; across midnight
/// when `to` is earlier. Never, a day the sun does not set or rise.
fn between(now: &Zoned, from: TimeOf, to: TimeOf) -> bool {
    let time = |t: TimeOf| t.on(now.date(), now.time_zone()).map(|z| z.time());
    let (Some(from), Some(to), now) = (time(from), time(to), now.time()) else { return false };
    if from <= to { now >= from && now < to } else { now >= from || now < to }
}

/// A state trigger fires on the edge: the observed value now passes, and did not before.
fn crosses(test: &Test, before: Option<&CapValue>, now: &CapValue) -> bool {
    let Some(before) = before else { return false };
    match test {
        Test::Is(v) => now == v && before != v,
        Test::Above(x) => now.number().is_some_and(|n| n > *x) && before.number().is_some_and(|b| b <= *x),
        Test::Below(x) => now.number().is_some_and(|n| n < *x) && before.number().is_some_and(|b| b >= *x),
        Test::Changes => now != before,
    }
}

fn passes(test: &Test, value: &CapValue) -> bool {
    match test {
        Test::Is(v) => value == v,
        Test::Above(x) => value.number().is_some_and(|n| n > *x),
        Test::Below(x) => value.number().is_some_and(|n| n < *x),
        Test::Changes => false,
    }
}

// ------------------------------------------------------------------------------------- output

/// What an automation did, or why it did nothing.
#[derive(Debug, Clone, Serialize)]
pub struct Run {
    /// RFC 3339, local time: when it started.
    pub at: String,
    /// What started it: `"19:30"`, `"Lampe du salon on_off"`, `"manual"`.
    pub trigger: String,
    pub skipped: Option<String>,
    /// Why it failed before its last node: a `stop` with an error, a limit.
    pub error: Option<String>,
    /// Why it was stopped before its end: by hand, started again, its automation edited.
    pub stopped: Option<String>,
    pub results: Vec<ActionResult>,
    /// The nodes it went through, in the order they ended.
    pub steps: Vec<Step>,
    /// Not over yet (`automation.run` answered before the end).
    #[serde(skip_serializing_if = "is_false")]
    pub running: bool,
}

impl Run {
    fn skipped(at: String, trigger: String, why: String) -> Self {
        Run { at, trigger, skipped: Some(why), error: None, stopped: None, results: vec![], steps: vec![], running: false }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ActionResult {
    pub entity: Option<Uuid>,
    /// The entity's name, or the selector when it matched nothing.
    pub target: String,
    /// `set brightness 40` or `invoke play_pause`.
    pub action: String,
    pub ok: bool,
    /// Command status (`pending`, `confirmed`), `sent`, or the error.
    pub outcome: String,
}

#[derive(Debug, Serialize)]
pub struct DryRun {
    pub conditions: Vec<Check>,
    /// The nodes it would go through.
    pub path: Vec<PathStep>,
    pub actions: Vec<Planned>,
    /// Would it run now (every condition passes)?
    pub would_run: bool,
}

#[derive(Debug, Serialize)]
pub struct Check {
    pub condition: Value,
    pub ok: bool,
    pub reason: String,
}

#[derive(Debug, Serialize)]
pub struct Planned {
    pub action: Value,
    pub targets: Vec<PlannedTarget>,
}

#[derive(Debug, Serialize)]
pub struct PlannedTarget {
    pub entity: Uuid,
    pub name: String,
    /// The desired state differs from the value (a `set`); always true for an action.
    pub changes: bool,
    pub error: Option<String>,
}

fn describe_test(t: &StateTest) -> Value {
    let test = match &t.test {
        Test::Is(v) => json!({ "is": v.plain() }),
        Test::Above(x) => json!({ "above": x }),
        Test::Below(x) => json!({ "below": x }),
        Test::Changes => json!({ "changes": true }),
    };
    json!({ "select": t.target.text, "cap": t.cap.to_string(), "test": test, "all": t.all })
}

fn describe_trigger(t: &Trigger) -> Value {
    match t {
        Trigger::At { time, days } => json!({ "at": time.to_string(), "days": days.names() }),
        Trigger::When(test) => json!({ "when": describe_test(test) }),
    }
}

fn describe_condition(c: &Condition) -> Value {
    match c {
        Condition::Between(from, to) => json!({ "between": [from.to_string(), to.to_string()] }),
        Condition::Days(days) => json!({ "days": days.names() }),
        Condition::State(test) => json!({ "state": describe_test(test) }),
        Condition::Trigger(id) => json!({ "trigger": id }),
    }
}

// ------------------------------------------------------------------------------------- engine

#[derive(Default)]
struct History {
    runs: VecDeque<Run>,
    recent: VecDeque<Instant>,
    /// Stopped by the loop guard, until the core restarts or the automations are edited.
    stopped: Option<String>,
}

/// A run not over yet.
struct Running {
    seq: u64,
    at: String,
    trigger: String,
    trace: Arc<Mutex<Trace>>,
    abort: tokio::task::AbortHandle,
    /// Who waits for its end (`automation.run`).
    done: Option<tokio::sync::oneshot::Sender<Run>>,
}

impl Running {
    /// The run as it is: ended with `end` (a failure, or why it was stopped), or still `running`.
    fn so_far(&self, end: End, running: bool) -> Run {
        let trace = self.trace.lock().unwrap();
        let (error, stopped) = match end {
            End::Done => (None, None),
            End::Failed(e) => (Some(e), None),
            End::Stopped(why) => (None, Some(why.to_string())),
        };
        Run { at: self.at.clone(), trigger: self.trigger.clone(), skipped: None, error, stopped, results: trace.results.clone(), steps: trace.steps.clone(), running }
    }
}

enum End {
    Done,
    Failed(String),
    Stopped(&'static str),
}

static SEQ: AtomicU64 = AtomicU64::new(0);

/// Shared by the versions of the automations an edit replaces: a run that goes on (its
/// automation did not change) ends in the same history.
#[derive(Default)]
struct Shared {
    history: Mutex<HashMap<String, History>>,
    running: Mutex<HashMap<String, Vec<Running>>>,
    /// One at a time, by automation in `queued` mode.
    queues: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

pub struct Automations {
    list: Vec<Automation>,
    /// Kept to compile edited automations the same way.
    pub location: Option<Location>,
    shared: Arc<Shared>,
    /// Last observed value per entity and capability, to see edges.
    observed: Mutex<HashMap<(Uuid, CapKind), CapValue>>,
    /// The devices that run an automation themselves too, by automation.
    delegated: Mutex<HashMap<String, BTreeSet<Uuid>>>,
}

fn stamp(z: &Zoned) -> String {
    z.strftime("%Y-%m-%dT%H:%M:%S%:z").to_string()
}

impl Automations {
    /// Compiles the configured automations: any error is a config error.
    pub fn new(configs: &[AutomationConfig], location: Option<Location>) -> Result<Self, String> {
        let mut list: Vec<Automation> = vec![];
        for c in configs {
            if list.iter().any(|a| a.id == c.id) {
                return Err(format!("two automations have the id `{}`", c.id));
            }
            list.push(Automation::compile(c, location)?);
        }
        Ok(Self { list, location, shared: Arc::default(), observed: Mutex::default(), delegated: Mutex::default() })
    }

    /// What the device of `entity` can run itself, from the automations simple enough for it:
    /// every trigger an `at`, no condition but days (a device cannot check a state or a time
    /// range), nothing but `set` nodes. The core runs the whole automation anyway.
    pub fn schedules_for(&self, entity: &Entity) -> Vec<Schedule> {
        let mut out = vec![];
        for a in self.list.iter().filter(|a| a.enabled) {
            let mut days = Days::ALL.0;
            for c in &a.conditions {
                days &= match c {
                    Condition::Days(d) => d.0,
                    Condition::Between(..) | Condition::State(_) | Condition::Trigger(_) => 0,
                };
            }
            let times: Option<Vec<(ScheduleTime, u8)>> = a
                .triggers
                .iter()
                .map(|t| match t {
                    Trigger::At { time, days: d } => Some((time.schedule_time(), d.0 & days)),
                    Trigger::When(_) => None,
                })
                .collect();
            let (Some(times), Some(sets)) = (times, a.flow.plain_sets()) else { continue };
            for (target, cap, value) in sets {
                let writable = entity.caps.get(cap).is_some_and(|slot| !slot.meta.read_only);
                if !writable || !target.select.matches(entity) {
                    continue;
                }
                for &(at, days) in times.iter().filter(|(_, days)| *days != 0) {
                    out.push(Schedule { automation: a.id.clone(), cap: cap.clone(), value: value.clone(), at, days });
                }
            }
        }
        out
    }

    /// Whether the device of `entity` took `schedules` (it replaced what it had from the core).
    pub fn set_delegated(&self, entity: Uuid, schedules: &[Schedule], ok: bool) {
        let mut delegated = self.delegated.lock().unwrap();
        for entities in delegated.values_mut() {
            entities.remove(&entity);
        }
        if ok {
            for s in schedules {
                delegated.entry(s.automation.clone()).or_default().insert(entity);
            }
        }
    }

    /// No automation (before the config is read, or none written).
    pub fn none() -> Self {
        Self::new(&[], None).expect("an empty list has nothing to refuse")
    }

    /// Takes over from `old`: the runs of the automations still here (by id), and the runs not
    /// over of those that did not change. Those that changed are stopped. An edited automation
    /// stopped by the loop guard starts again.
    pub(crate) fn carry(&mut self, old: &Automations, core: &Core) {
        self.shared = old.shared.clone();
        let same = |id: &str| {
            let now = self.list.iter().find(|a| a.id == id).map(|a| &a.config);
            now.is_some() && now == old.list.iter().find(|a| a.id == id).map(|a| &a.config)
        };
        let ended: Vec<(String, Vec<Running>)> = {
            let mut running = self.shared.running.lock().unwrap();
            let ids: Vec<String> = running.keys().filter(|id| !same(id)).cloned().collect();
            ids.into_iter().filter_map(|id| running.remove_entry(&id)).collect()
        };
        {
            let mut history = self.shared.history.lock().unwrap();
            history.retain(|id, _| self.list.iter().any(|a| a.id == *id));
            for h in history.values_mut() {
                h.recent.clear();
                h.stopped = None;
            }
        }
        for (id, runs) in ended {
            for r in runs {
                r.abort.abort();
                if self.list.iter().any(|a| a.id == id) {
                    self.close(core, &id, r, End::Stopped("the automation was edited"));
                }
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// The scenes their `scene` steps name: (automation, node, scene).
    pub fn scene_refs(&self) -> Vec<(String, String, String)> {
        self.list.iter().flat_map(|a| a.flow.scene_refs().map(|(node, scene)| (a.id.clone(), node.into(), scene.into()))).collect()
    }

    fn index(&self, id: &str) -> Result<usize, String> {
        self.list.iter().position(|a| a.id == id).ok_or_else(|| format!("unknown automation `{id}`"))
    }

    /// For the API: every automation, what it is, when it runs next, what may be wrong, its runs.
    pub fn list(&self, core: &Core, now: &Zoned) -> Value {
        let history = self.shared.history.lock().unwrap();
        let running = self.shared.running.lock().unwrap();
        let items: Vec<Value> = self
            .list
            .iter()
            .map(|a| {
                let h = history.get(&a.id);
                let (mut problems, delegated) = core.with(|r| {
                    let on_devices = self.delegated.lock().unwrap().get(&a.id).cloned().unwrap_or_default();
                    let names: Vec<String> = on_devices.iter().filter_map(|id| r.entities.get(id).map(|e| e.name.clone())).collect();
                    (problems(a, r.entities.values()), names)
                });
                if let Some(stopped) = h.and_then(|h| h.stopped.clone()) {
                    problems.insert(0, stopped);
                }
                let running: Vec<Value> = running
                    .get(&a.id)
                    .into_iter()
                    .flatten()
                    .map(|r| {
                        let trace = r.trace.lock().unwrap();
                        json!({ "at": r.at, "trigger": r.trigger, "nodes": trace.current, "steps": trace.steps })
                    })
                    .collect();
                json!({
                    "id": a.id,
                    "name": a.name,
                    "enabled": a.enabled && h.is_none_or(|h| h.stopped.is_none()),
                    "mode": a.mode,
                    "triggers": a.triggers.iter().map(describe_trigger).collect::<Vec<_>>(),
                    "conditions": a.conditions.iter().map(describe_condition).collect::<Vec<_>>(),
                    "actions": a.flow.describe_actions(),
                    "next_run": a.enabled.then(|| a.next_time(now)).flatten().map(|z| z.timestamp().to_string()),
                    "problems": problems,
                    // These devices run it themselves too: it still happens while the hub is off.
                    "delegated": delegated,
                    "running": running,
                    "runs": h.map(|h| h.runs.iter().rev().cloned().collect::<Vec<_>>()).unwrap_or_default(),
                    "config": a.config,
                })
            })
            .collect();
        json!(items)
    }

    /// What it would do now, without doing anything.
    pub fn dry_run(&self, core: &Core, id: &str, now: &Zoned) -> Result<DryRun, String> {
        let a = &self.list[self.index(id)?];
        let conditions: Vec<Check> = a
            .conditions
            .iter()
            .map(|c| {
                let (ok, reason) = core.with(|r| check(c, now, r.entities.values(), None));
                Check { condition: describe_condition(c), ok, reason }
            })
            .collect();
        let (path, actions) = core.with(|r| a.flow.preview(r, now));
        let would_run = conditions.iter().all(|c| c.ok);
        Ok(DryRun { conditions, path, actions, would_run })
    }

    /// Runs it now (conditions and mode still apply), as if triggered by `trigger`. Answers when
    /// it ends, or after a while with where it is (`running`).
    pub async fn run(self: &Arc<Self>, core: &Arc<Core>, id: &str, trigger: String, now: &Zoned) -> Result<Run, String> {
        let i = self.index(id)?;
        let (seq, done) = self.start(core, i, Fired { label: trigger, ..Fired::default() }, now);
        match tokio::time::timeout(ANSWER_WITHIN, done).await {
            Ok(Ok(run)) => Ok(run),
            Ok(Err(_)) => Err("the run was dropped".into()),
            Err(_) => {
                let running = self.shared.running.lock().unwrap();
                let r = running.get(id).into_iter().flatten().find(|r| Some(r.seq) == seq).ok_or("the run ended meanwhile")?;
                Ok(r.so_far(End::Done, true))
            }
        }
    }

    /// Stops its runs not over yet; how many.
    pub fn stop(&self, core: &Core, id: &str) -> Result<usize, String> {
        self.index(id)?;
        let runs = self.shared.running.lock().unwrap().remove(id).unwrap_or_default();
        let count = runs.len();
        for r in runs {
            r.abort.abort();
            self.close(core, id, r, End::Stopped("by hand"));
        }
        Ok(count)
    }

    /// Why it would not run now: the loop guard, a condition.
    fn skip(&self, core: &Core, a: &Automation, fired: &Fired, now: &Zoned) -> Option<String> {
        {
            let mut history = self.shared.history.lock().unwrap();
            let h = history.entry(a.id.clone()).or_default();
            if let Some(stopped) = &h.stopped {
                return Some(stopped.clone());
            }
            let minute_ago = Instant::now() - Duration::from_secs(60);
            while h.recent.front().is_some_and(|t| *t < minute_ago) {
                h.recent.pop_front();
            }
            if h.recent.len() >= MAX_RUNS_PER_MINUTE {
                let why = format!("stopped: ran {MAX_RUNS_PER_MINUTE} times in a minute (a loop?); restart the core to resume");
                tracing::error!(automation = %a.id, "{why}");
                h.stopped = Some(why.clone());
                return Some(why);
            }
            h.recent.push_back(Instant::now());
        }
        a.conditions.iter().find_map(|c| {
            let (ok, reason) = core.with(|r| check(c, now, r.entities.values(), fired.id.as_deref()));
            (!ok).then_some(reason)
        })
    }

    /// Starts a run aside, as its mode says: its number (none: skipped), and its end.
    fn start(self: &Arc<Self>, core: &Arc<Core>, i: usize, fired: Fired, now: &Zoned) -> (Option<u64>, tokio::sync::oneshot::Receiver<Run>) {
        let (done, end) = tokio::sync::oneshot::channel();
        let a = &self.list[i];
        let mut skipped = self.skip(core, a, &fired, now);
        let mut running = self.shared.running.lock().unwrap();
        let others = running.entry(a.id.clone()).or_default();
        if skipped.is_none() {
            skipped = match a.mode {
                Mode::Single if !others.is_empty() => Some("skipped: it is still running".into()),
                Mode::Queued | Mode::Parallel if others.len() >= MAX_RUNNING => Some(format!("skipped: it is running {MAX_RUNNING} times already")),
                _ => None,
            };
        }
        if let Some(why) = skipped {
            drop(running);
            let run = Run::skipped(stamp(now), fired.label, why);
            self.record(core, &a.id, run.clone());
            let _ = done.send(run);
            return (None, end);
        }
        if a.mode == Mode::Restart {
            for r in std::mem::take(others) {
                r.abort.abort();
                self.close(core, &a.id, r, End::Stopped("started again"));
            }
        }
        let seq = SEQ.fetch_add(1, Ordering::Relaxed);
        let trace = Arc::<Mutex<Trace>>::default();
        let queue = (a.mode == Mode::Queued).then(|| self.shared.queues.lock().unwrap().entry(a.id.clone()).or_default().clone());
        let trigger = fired.label.clone();
        let task = tokio::spawn({
            let (this, core, trace) = (self.clone(), core.clone(), trace.clone());
            async move {
                let _turn = match &queue {
                    Some(queue) => Some(queue.lock().await),
                    None => None,
                };
                let a = &this.list[i];
                let ended = Exec { core: &core, flow: &a.flow, fired: &fired, trace: &trace }.run().await;
                let end = match ended {
                    Err(Halt::Fail(e)) => End::Failed(e),
                    Ok(()) | Err(Halt::Stop) => End::Done,
                };
                // Not there: stopped meanwhile, and recorded by who stopped it.
                let me = {
                    let mut running = this.shared.running.lock().unwrap();
                    let runs = running.get_mut(&a.id);
                    runs.and_then(|runs| runs.iter().position(|r| r.seq == seq).map(|i| runs.remove(i)))
                };
                if let Some(me) = me {
                    this.close(&core, &a.id, me, end);
                }
            }
        });
        // Under the lock: the task cannot look for itself before it is here.
        others.push(Running { seq, at: stamp(now), trigger, trace, abort: task.abort_handle(), done: Some(done) });
        (Some(seq), end)
    }

    /// Records a run that ended, and tells who waits for it.
    fn close(&self, core: &Core, id: &str, mut r: Running, end: End) {
        let run = r.so_far(end, false);
        self.record(core, id, run.clone());
        if let Some(done) = r.done.take() {
            let _ = done.send(run);
        }
    }

    fn record(&self, core: &Core, id: &str, run: Run) {
        tracing::info!(automation = %id, trigger = %run.trigger, skipped = ?run.skipped, error = ?run.error, stopped = ?run.stopped, "automation ran");
        let mut history = self.shared.history.lock().unwrap();
        let runs = &mut history.entry(id.into()).or_default().runs;
        if runs.len() == KEEP_RUNS {
            runs.pop_front();
        }
        runs.push_back(run.clone());
        drop(history);
        let _ = core.changes.send(Event::Automation { id: id.into(), run });
    }

    /// The automations a state change triggers, with what triggered them.
    fn triggered_by(&self, core: &Core, change: &Change) -> Vec<(usize, Fired)> {
        let Some(now) = change.slot.observed.clone() else { return vec![] };
        let before = self.observed.lock().unwrap().insert((change.id, change.cap.clone()), now.clone());
        if before.as_ref() == Some(&now) {
            return vec![]; // Not an observed change (a wish, a status).
        }
        let Some(entity) = core.with(|r| r.entities.get(&change.id).cloned()) else { return vec![] };
        self.list
            .iter()
            .enumerate()
            .filter(|(_, a)| a.enabled)
            .filter_map(|(i, a)| {
                a.triggers.iter().zip(&a.trigger_ids).find_map(|(t, id)| match t {
                    Trigger::When(w) if w.cap == change.cap && w.target.select.matches(&entity) && crosses(&w.test, before.as_ref(), &now) => {
                        let label = format!("{} {}", entity.name, change.cap);
                        Some((i, Fired { label, id: id.clone(), entity: Some(entity.id), value: Some(now.clone()) }))
                    }
                    _ => None,
                })
            })
            .collect()
    }

    /// The engine: sleeps until the next `at`, wakes on observed changes. Runs forever.
    pub async fn drive(self: Arc<Self>, core: Arc<Core>) {
        let mut changes = core.changes.subscribe();
        // A value already true at start is not an edge.
        self.seed(&core);
        let mut checked = Zoned::now();
        loop {
            let next = self.list.iter().filter(|a| a.enabled).filter_map(|a| a.next_time(&checked)).min();
            // At most a minute: follows clock and time-zone changes. Already due: no wait.
            let wait = next
                .map(|n| Duration::try_from(n.duration_since(&Zoned::now())).unwrap_or(Duration::ZERO))
                .unwrap_or(Duration::MAX)
                .min(Duration::from_secs(60));
            tokio::select! {
                () = tokio::time::sleep(wait) => {
                    let now = Zoned::now();
                    for (i, a) in self.list.iter().enumerate().filter(|(_, a)| a.enabled) {
                        let Some((due, fired)) = a.next_trigger(&checked).filter(|(due, _)| *due <= now) else { continue };
                        let late = Duration::try_from(now.duration_since(&due)).unwrap_or(Duration::ZERO);
                        if late > GRACE {
                            tracing::warn!(automation = %a.id, "{} skipped: the hub was not running then", fired.label);
                        } else {
                            self.start(&core, i, fired, &now);
                        }
                    }
                    checked = now;
                }
                event = changes.recv() => match event {
                    Ok(Event::Slot(change)) => {
                        for (i, fired) in self.triggered_by(&core, &change) {
                            self.start(&core, i, fired, &Zoned::now());
                        }
                    }
                    Ok(_) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        // The edges in between are lost; start again from the current values, so
                        // the next ones are seen right.
                        tracing::warn!("automations missed {n} changes");
                        self.seed(&core);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                },
            }
        }
    }

    /// Takes the observed values as they are now.
    fn seed(&self, core: &Core) {
        core.with(|r| {
            let mut observed = self.observed.lock().unwrap();
            for e in r.entities.values() {
                for (cap, slot) in &e.caps {
                    if let Some(v) = &slot.observed {
                        observed.insert((e.id, cap.clone()), v.clone());
                    }
                }
            }
        });
    }
}

/// Sets `value` as the desired state of each entity: its command status, or why not.
pub(crate) fn set_each(r: &mut Reconciler, ids: Vec<(Uuid, String)>, cap: &CapKind, value: &CapValue, label: &str) -> Vec<ActionResult> {
    ids.into_iter()
        .map(|(id, name)| {
            let status = r.set_desired(id, cap, value.clone(), Instant::now());
            ActionResult {
                entity: Some(id),
                target: name,
                action: label.into(),
                ok: status.is_ok(),
                outcome: match status {
                    Ok(s) => serde_json::to_value(&s).ok().and_then(|v| v["state"].as_str().map(String::from)).unwrap_or_default(),
                    Err(e) => e,
                },
            }
        })
        .collect()
}

/// Invokes `action` on each entity, one after the other: sent, or why not.
pub(crate) async fn invoke_each(core: &Core, ids: Vec<(Uuid, String)>, action: &str, params: &Value, label: &str) -> Vec<ActionResult> {
    let mut results = vec![];
    for (id, name) in ids {
        let outcome = match core.plugins.get() {
            Some(plugins) => plugins.invoke(core, id, action, params.clone()).await,
            None => Err("plugins are not running".into()),
        };
        let (ok, outcome) = (outcome.is_ok(), outcome.map_or_else(|e| e, |()| "sent".into()));
        results.push(ActionResult { entity: Some(id), target: name, action: label.into(), ok, outcome });
    }
    results
}

pub(crate) fn nothing_matched(results: Vec<ActionResult>, target: &Target, label: &str) -> Vec<ActionResult> {
    if !results.is_empty() {
        return results;
    }
    vec![ActionResult { entity: None, target: target.text.clone(), action: label.into(), ok: false, outcome: "matches no device".into() }]
}

/// Whether a condition holds now, and why (or why not), in words for the dry run and the log.
/// `fired`: the id of the trigger that started the run.
fn check<'a>(c: &Condition, now: &Zoned, entities: impl IntoIterator<Item = &'a Entity>, fired: Option<&str>) -> (bool, String) {
    match c {
        Condition::Between(from, to) => {
            let ok = between(now, *from, *to);
            // Sunrise and sunset are shown with today's time.
            let shown = |t: &TimeOf| match t {
                TimeOf::Clock(_) => t.to_string(),
                TimeOf::Sun(..) => t.on(now.date(), now.time_zone()).map_or(format!("{t} (none today)"), |z| format!("{t} ({})", z.strftime("%H:%M"))),
            };
            (ok, format!("{} is {}between {} and {}", now.strftime("%H:%M"), if ok { "" } else { "not " }, shown(from), shown(to)))
        }
        Condition::Days(days) => {
            let ok = days.contains(now);
            (ok, format!("{} is {}one of {}", now.strftime("%a"), if ok { "" } else { "not " }, days.names().join(", ")))
        }
        Condition::State(t) => {
            let values: Vec<(String, Option<CapValue>)> = t
                .target
                .entities(entities)
                .into_iter()
                .filter_map(|e| e.caps.get(&t.cap).map(|s| (e.name.clone(), s.observed.clone())))
                .collect();
            if values.is_empty() {
                return (false, format!("`{}` matches no device with {}", t.target.text, t.cap));
            }
            let pass = |v: &Option<CapValue>| v.as_ref().is_some_and(|v| passes(&t.test, v));
            let ok = if t.all { values.iter().all(|(_, v)| pass(v)) } else { values.iter().any(|(_, v)| pass(v)) };
            let shown: Vec<String> = values
                .iter()
                .map(|(name, v)| format!("{name}: {}", v.as_ref().map_or("unknown".into(), |v| v.plain().to_string())))
                .collect();
            (ok, format!("{} {}", shown.join(", "), if ok { "(passes)" } else { "(does not pass)" }))
        }
        Condition::Trigger(id) => {
            let ok = fired == Some(id.as_str());
            (ok, format!("{} by the trigger `{id}`", if ok { "started" } else { "not started" }))
        }
    }
}

/// Called with what a selector must find: a device, with `what` (a capability, an action).
type Need<'a> = dyn FnMut(&Target, String, &dyn Fn(&Entity) -> bool) + 'a;

fn trigger_needs(t: &Trigger, need: &mut Need) {
    if let Trigger::When(w) = t {
        need(&w.target, w.cap.to_string(), &|e| e.caps.contains_key(&w.cap));
    }
}

fn condition_needs(c: &Condition, need: &mut Need) {
    if let Condition::State(s) = c {
        need(&s.target, s.cap.to_string(), &|e| e.caps.contains_key(&s.cap));
    }
}

/// What may be wrong with an automation given the devices now: selectors matching nothing,
/// devices without the capability or the action.
fn problems<'a>(a: &Automation, entities: impl IntoIterator<Item = &'a Entity> + Clone) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    let mut need = |target: &Target, what: String, has: &dyn Fn(&Entity) -> bool| {
        let matched = target.entities(entities.clone());
        let problem = if matched.is_empty() {
            format!("`{}` matches no device", target.text)
        } else if !matched.iter().any(|e| has(e)) {
            format!("`{}` matches no device with {what}", target.text)
        } else {
            return;
        };
        if !out.contains(&problem) {
            out.push(problem);
        }
    };
    for t in &a.triggers {
        trigger_needs(t, &mut need);
    }
    for c in &a.conditions {
        condition_needs(c, &mut need);
    }
    a.flow.needs(&mut need);
    out
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use jiff::tz::TimeZone;
    use tokio::sync::mpsc;

    use super::*;
    use crate::{
        model::{CapMeta, CapSlot},
        reconciler::Reconciler,
    };

    fn config(toml_text: &str) -> Result<Arc<Automations>, String> {
        #[derive(Deserialize)]
        struct File {
            automation: Vec<AutomationConfig>,
        }
        let file: File = toml::from_str(toml_text).map_err(|e| e.to_string())?;
        Automations::new(&file.automation, Some(PARIS)).map(Arc::new)
    }

    const PARIS: Location = Location { latitude: 48.8566, longitude: 2.3522 };

    fn paris(text: &str) -> Zoned {
        let dt: jiff::civil::DateTime = text.parse().unwrap();
        dt.to_zoned(TimeZone::get("Europe/Paris").unwrap()).unwrap()
    }

    fn lamp(on: bool) -> Entity {
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
            caps: BTreeMap::from([
                (CapKind::OnOff, CapSlot { observed: Some(CapValue::OnOff(on)), desired: Some(CapValue::OnOff(on)), ..Default::default() }),
                (
                    CapKind::Brightness,
                    CapSlot { meta: CapMeta { max: Some(100.0), ..Default::default() }, observed: Some(CapValue::Brightness(10)), ..Default::default() },
                ),
            ]),
        }
    }

    fn core(entities: Vec<Entity>) -> (Arc<Core>, mpsc::UnboundedReceiver<crate::reconciler::Command>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Arc::new(Core::new(Reconciler::new(entities.into_iter().map(|e| (e.id, e)).collect()), tx)), rx)
    }

    const EVENING: &str = r#"
        [[automation]]
        id = "evening"
        name = "Lampe du soir"
        [[automation.trigger]]
        at = "19:30"
        days = ["mon", "tue", "wed", "thu", "fri"]
        [[automation.condition]]
        between = ["18:00", "01:00"]
        [[automation.action]]
        set = { select = "room:Salon & cap:brightness", cap = "brightness", value = 40 }
    "#;

    #[test]
    fn the_examples_of_the_example_config_load() {
        // Its commented [[automation]] tables, without the prose around them.
        let text = include_str!("../../../homemaid.example.toml");
        let start = text.find("# [[automation]]").unwrap();
        let toml: Vec<&str> = text[start..]
            .lines()
            .take_while(|l| l.starts_with('#'))
            .filter_map(|l| l.strip_prefix("# "))
            .filter(|l| l.starts_with("[[") || l.split_once(" = ").is_some_and(|(key, _)| key.chars().all(|c| c.is_ascii_lowercase() || c == '_')))
            .collect();
        let text = toml.join("\n");
        let automations = config(&text).unwrap();
        assert!(automations.list.iter().any(|a| a.id == "living-at-night"));
        #[derive(Deserialize)]
        struct File {
            scene: Vec<crate::scenes::SceneConfig>,
        }
        let scenes = crate::scenes::Scenes::new(&toml::from_str::<File>(&text).unwrap().scene).unwrap();
        assert!(scenes.contains("cinema"));
    }

    #[test]
    fn a_complete_automation_loads() {
        let automations = config(EVENING).unwrap();
        assert_eq!(automations.list[0].name, "Lampe du soir");
    }

    #[test]
    fn config_errors_say_what_and_where() {
        let base = |extra: &str, action: &str| format!(
            "[[automation]]\nid = \"a\"\n[[automation.trigger]]\n{extra}\n[[automation.action]]\n{action}\n"
        );
        let ok_action = r#"set = { select = "room:Salon", cap = "on_off", value = true }"#;
        for (text, what) in [
            (base(r#"at = "25:00""#, ok_action), "not a time"),
            (base(r#"at = "19:30"
days = ["monday"]"#, ok_action), "unknown day"),
            (base(r#"when = { select = "room:Salon", cap = "on_off", above = 1 }"#, ok_action), "not a number"),
            (base(r#"when = { select = "room:Salon", cap = "on_off" }"#, ok_action), "exactly one test"),
            (base(r#"at = "19:30""#, r#"set = { select = "room:Salon", cap = "brightnes", value = 40 }"#), "unknown capability"),
            (base(r#"at = "19:30""#, r#"set = { select = "room:Salon", cap = "brightness", value = "high" }"#), "not a brightness"),
            (base(r#"at = "19:30""#, r#"set = { select = "rooms:Salon", cap = "on_off", value = true }"#), "unknown term"),
            (base(r#"at = "19:30""#, r#"sett = { select = "room:Salon", cap = "on_off", value = true }"#), "unknown field"),
            (format!("{}\n{}", base(r#"at = "19:30""#, ok_action), base(r#"at = "19:30""#, ok_action)), "two automations"),
        ] {
            let err = config(&text).err().unwrap_or_default();
            assert!(err.contains(what), "{what}: got `{err}` for\n{text}");
        }
    }

    #[test]
    fn next_time_respects_days_midnight_and_daylight_saving() {
        let a = &config(EVENING).unwrap().list[0];
        // Friday 20:00 -> Monday 19:30.
        assert_eq!(a.next_time(&paris("2026-10-16T20:00")).unwrap().to_string(), "2026-10-19T19:30:00+02:00[Europe/Paris]");
        // Same day, later.
        assert_eq!(a.next_time(&paris("2026-10-19T08:00")).unwrap().datetime().to_string(), "2026-10-19T19:30:00");
        // Exactly at the time: strictly after, so the next working day.
        assert_eq!(a.next_time(&paris("2026-10-19T19:30")).unwrap().datetime().to_string(), "2026-10-20T19:30:00");
        // A 02:30 trigger on the spring-forward night happens at 03:30.
        let night = next_at(TimeOf::Clock("02:30".parse().unwrap()), Days::ALL, &paris("2026-03-29T00:00")).unwrap();
        assert_eq!(night.to_string(), "2026-03-29T03:30:00+02:00[Europe/Paris]");
        let clock = |t: &str| TimeOf::Clock(t.parse().unwrap());
        assert!(between(&paris("2026-10-19T00:30"), clock("18:00"), clock("01:00")), "across midnight");
    }

    #[test]
    fn sunrise_and_sunset_follow_the_location_and_the_date() {
        let sun = |t: &str| TimeOf::parse(t, Some(PARIS)).unwrap();
        let near = |at: Zoned, expected: &str| {
            let expected = paris(expected);
            let gap = at.duration_since(&expected).abs();
            assert!(gap < jiff::SignedDuration::from_mins(2), "{at} is not near {expected}");
        };
        // Paris, summer solstice: sunset at 21:58, sunrise at 05:47 (local time).
        let june = |t: &str| next_at(sun(t), Days::ALL, &paris("2026-06-21T00:00")).unwrap();
        near(june("sunset"), "2026-06-21T21:58");
        near(june("sunrise"), "2026-06-21T05:47");
        near(june("sunset-30"), "2026-06-21T21:28");
        // Winter solstice: 16:56.
        near(next_at(sun("sunset"), Days::ALL, &paris("2026-12-21T12:00")).unwrap(), "2026-12-21T16:56");
        // Already past today's: tomorrow's.
        assert_eq!(next_at(sun("sunset"), Days::ALL, &paris("2026-06-21T23:00")).unwrap().date().to_string(), "2026-06-22");

        assert!(between(&paris("2026-06-21T23:00"), sun("sunset"), TimeOf::Clock("01:00".parse().unwrap())));
        assert!(!between(&paris("2026-06-21T21:00"), sun("sunset"), TimeOf::Clock("01:00".parse().unwrap())));
        assert_eq!(sun("sunrise+15").to_string(), "sunrise+15");

        // No sunset at the North Cape in June: never, without panicking.
        let north = Location { latitude: 71.17, longitude: 25.78 };
        assert!(TimeOf::parse("sunset", Some(north)).unwrap().on(paris("2026-06-21T00:00").date(), &TimeZone::UTC).is_none());
    }

    #[test]
    fn only_automations_a_device_can_run_alone_are_handed_over() {
        let automations = config(r#"
            [[automation]]
            id = "dusk"
            [[automation.trigger]]
            at = "sunset-15"
            days = ["mon", "tue", "wed"]
            [[automation.condition]]
            days = ["tue", "wed", "thu"]
            [[automation.action]]
            set = { select = "room:Salon", cap = "on_off", value = true }
            [[automation.action]]
            invoke = { select = "room:Salon", action = "blink" }
            [[automation.action]]
            set = { select = "room:Cuisine", cap = "on_off", value = true }

            [[automation]]
            id = "morning"
            [[automation.trigger]]
            at = "07:30"
            [[automation.action]]
            set = { select = "room:Salon", cap = "brightness", value = 40 }

            [[automation]]
            id = "ranged"
            [[automation.trigger]]
            at = "07:30"
            [[automation.condition]]
            between = ["07:00", "08:00"]
            [[automation.action]]
            set = { select = "room:Salon", cap = "on_off", value = false }

            [[automation]]
            id = "edge"
            [[automation.trigger]]
            when = { select = "room:Salon", cap = "on_off", becomes = true }
            [[automation.action]]
            set = { select = "room:Salon", cap = "brightness", value = 10 }

            [[automation]]
            id = "then-off"
            start = ["on"]
            [[automation.trigger]]
            at = "07:30"
            [[automation.node]]
            id = "on"
            set = { select = "room:Salon", cap = "on_off", value = true }
            next = ["later"]
            [[automation.node]]
            id = "later"
            delay = "1h"
        "#).unwrap();
        let schedules = automations.schedules_for(&lamp(false));
        assert_eq!(schedules, [
            Schedule { automation: "dusk".into(), cap: CapKind::OnOff, value: CapValue::OnOff(true), at: ScheduleTime::Sunset(-15), days: 0b110 },
            Schedule { automation: "morning".into(), cap: CapKind::Brightness, value: CapValue::Brightness(40), at: ScheduleTime::Clock(450), days: 0x7f },
        ], "a range, a state or a delay needs the hub; another room is not this device");

        let id = lamp(false).id;
        automations.set_delegated(id, &schedules, true);
        automations.set_delegated(id, &schedules[1..], true);
        let delegated = automations.delegated.lock().unwrap().clone();
        assert!(delegated["morning"].contains(&id) && !delegated["dusk"].contains(&id), "replaced, not added");
    }

    #[test]
    fn sun_errors_say_what_is_missing() {
        assert!(TimeOf::parse("sunset", None).unwrap_err().contains("latitude"));
        assert!(TimeOf::parse("sunset-400", Some(PARIS)).unwrap_err().contains("360"));
        assert!(TimeOf::parse("sunset30", Some(PARIS)).unwrap_err().contains("sign"));
        assert!(TimeOf::parse("sunset-x", Some(PARIS)).unwrap_err().contains("minutes"));
        assert!(TimeOf::parse("sunset", Some(Location { latitude: 120.0, longitude: 0.0 })).unwrap_err().contains("range"));
    }

    #[test]
    fn state_triggers_fire_on_the_edge_only() {
        let on = Test::Is(CapValue::OnOff(true));
        assert!(crosses(&on, Some(&CapValue::OnOff(false)), &CapValue::OnOff(true)));
        assert!(!crosses(&on, Some(&CapValue::OnOff(true)), &CapValue::OnOff(true)), "already on");
        assert!(!crosses(&on, None, &CapValue::OnOff(true)), "first sighting is not an edge");
        let above = Test::Above(50.0);
        assert!(crosses(&above, Some(&CapValue::Volume(40)), &CapValue::Volume(60)));
        assert!(!crosses(&above, Some(&CapValue::Volume(55)), &CapValue::Volume(60)), "was above already");
    }

    #[tokio::test]
    async fn dry_run_changes_nothing_and_run_sets_desired_states() {
        let automations = config(EVENING).unwrap();
        let (core, mut commands) = core(vec![lamp(true)]);
        let monday_evening = paris("2026-10-19T19:30");

        let dry = automations.dry_run(&core, "evening", &monday_evening).unwrap();
        assert!(dry.would_run);
        assert_eq!(dry.actions[0].targets[0].name, "Lampe");
        assert!(dry.actions[0].targets[0].changes);
        assert!(commands.try_recv().is_err(), "a dry run sends nothing");

        let run = automations.run(&core, "evening", "19:30".into(), &monday_evening).await.unwrap();
        assert_eq!(run.skipped, None);
        assert_eq!(run.results[0].outcome, "pending");
        assert_eq!(commands.try_recv().unwrap().value, CapValue::Brightness(40));

        let noon = automations.run(&core, "evening", "manual".into(), &paris("2026-10-19T12:00")).await.unwrap();
        assert!(noon.skipped.unwrap().contains("not between 18:00 and 01:00"));
    }

    #[tokio::test]
    async fn problems_name_selectors_that_match_nothing_and_the_log_is_bounded() {
        let automations = config(
            r#"
            [[automation]]
            id = "ghost"
            [[automation.trigger]]
            when = { select = "room:Cave", cap = "on_off", becomes = true }
            [[automation.action]]
            invoke = { select = "room:Salon", action = "blink" }
        "#,
        )
        .unwrap();
        let (core, _commands) = core(vec![lamp(false)]);
        let now = paris("2026-10-19T12:00");
        let list = automations.list(&core, &now);
        let problems = list[0]["problems"].as_array().unwrap();
        assert!(problems.iter().any(|p| p.as_str().unwrap().contains("`room:Cave` matches no device")));
        assert!(problems.iter().any(|p| p.as_str().unwrap().contains("the action `blink`")));

        for _ in 0..(MAX_RUNS_PER_MINUTE + 5) {
            automations.run(&core, "ghost", "manual".into(), &now).await.unwrap();
        }
        let list = automations.list(&core, &now);
        assert_eq!(list[0]["runs"].as_array().unwrap().len(), MAX_RUNS_PER_MINUTE + 5);
        assert_eq!(list[0]["enabled"], false, "stopped by the loop guard");
        assert!(list[0]["runs"][0]["skipped"].as_str().unwrap().contains("a loop?"));
    }

    #[tokio::test]
    async fn an_observed_change_triggers_but_a_wish_does_not() {
        let automations = config(
            r#"
            [[automation]]
            id = "follow"
            [[automation.trigger]]
            when = { select = "room:Salon", cap = "on_off", becomes = true }
            [[automation.action]]
            set = { select = "room:Salon", cap = "brightness", value = 80 }
        "#,
        )
        .unwrap();
        let (core, _commands) = core(vec![lamp(false)]);
        let id = lamp(false).id;
        let change = |on: bool, observed: bool| Change {
            id,
            cap: CapKind::OnOff,
            slot: CapSlot { observed: observed.then_some(CapValue::OnOff(on)), desired: Some(CapValue::OnOff(on)), ..Default::default() },
        };
        assert!(automations.triggered_by(&core, &change(false, true)).is_empty(), "first sighting");
        assert!(automations.triggered_by(&core, &change(true, false)).is_empty(), "a wish only");
        assert_eq!(automations.triggered_by(&core, &change(true, true)).len(), 1, "now really on");
        assert!(automations.triggered_by(&core, &change(true, true)).is_empty(), "still on");
    }

    // ------------------------------------------------------------------------------- workflows

    const AT_7: &str = "[[automation.trigger]]\nat = \"07:00\"\n";

    /// One automation `f`; `rest` starts with its own keys (`start`, `mode`), then its tables.
    fn flow(rest: &str) -> Result<Arc<Automations>, String> {
        config(&format!("[[automation]]\nid = \"f\"\n{rest}"))
    }

    fn node(id: &str, body: &str) -> String {
        format!("[[automation.node]]\nid = \"{id}\"\n{body}\n")
    }

    fn set(id: &str, cap: &str, value: &str, ports: &str) -> String {
        node(id, &format!("set = {{ select = \"room:Salon\", cap = \"{cap}\", value = {value} }}\n{ports}"))
    }

    fn names(run: &Run) -> Vec<&str> {
        run.steps.iter().map(|s| s.node.as_str()).collect()
    }

    fn sent(commands: &mut mpsc::UnboundedReceiver<crate::reconciler::Command>) -> Vec<CapValue> {
        std::iter::from_fn(|| commands.try_recv().ok()).map(|c| c.value).collect()
    }

    fn last_run(a: &Automations, id: &str) -> Option<Run> {
        a.shared.history.lock().unwrap().get(id).and_then(|h| h.runs.back().cloned())
    }

    fn other_lamp() -> Entity {
        Entity { id: crate::model::entity_uuid("mock", "lamp2"), local_id: "lamp2".into(), name: "Lampe 2".into(), ..lamp(false) }
    }

    #[test]
    fn flow_errors_say_which_node() {
        let on = |id: &str, ports: &str| set(id, "on_off", "true", ports);
        let cases = [
            (format!("start = [\"a\"]\n{AT_7}{}", on("a", "next = [\"b\"]")), "`next` leads to `b`, which is no node"),
            (format!("start = [\"a\"]\n{AT_7}{}", on("a", "then = [\"a\"]")), "no `then` port (it has: next, error)"),
            (format!("start = [\"a\"]\n{AT_7}{}{}", on("a", ""), on("b", "")), "node `b`: nothing leads to it"),
            (format!("start = [\"a\", \"b\"]\n{AT_7}{}{}{}", on("a", "next = [\"c\"]"), on("b", "next = [\"c\"]"), on("c", "")), "join them with a `merge`"),
            (format!("start = [\"m\"]\n{AT_7}{}{}", on("a", "next = [\"m\"]"), node("m", "merge = \"any\"\nnext = [\"a\"]")), "leads back to itself"),
            (
                format!("start = [\"r\"]\n{AT_7}{}{}{}", node("r", "repeat = { count = 2 }\nbody = [\"a\"]\nnext = [\"m\"]"), on("a", "next = [\"m\"]"), node("m", "merge = \"all\"")),
                "node `m`: the body of `r` leads to it, and so does the rest",
            ),
            (format!("start = [\"a\"]\n{AT_7}{}", node("a", "set = { select = \"item\", cap = \"on_off\", value = true }")), "`item` is only for"),
            (format!("start = [\"a\"]\n{AT_7}{}", node("a", "set = { select = \"trigger\", cap = \"on_off\", value = true }")), "needs a `when` trigger"),
            (format!("start = [\"a\"]\n{AT_7}{}[[automation.action]]\nset = {{ select = \"room:Salon\", cap = \"on_off\", value = true }}\n", on("a", "")), "not both"),
            (format!("{AT_7}{}", on("a", "")), "`start` names the first node"),
            (format!("start = [\"a\"]\n{AT_7}{}", node("a", "delay = \"10\"")), "not a duration"),
            (format!("start = [\"a\"]\n{AT_7}{}", node("a", "wait = { timeout = \"1m\" }")), "either `until`"),
            (format!("start = [\"a\"]\n{AT_7}{}", node("a", "if = [{ trigger = \"x\" }]\nthen = []")), "no trigger has the id `x`"),
            (format!("start = [\"a\"]\n{AT_7}{}", node("a", "delay = \"1m\"\nstop = {}")), "exactly one of"),
            (format!("start = [\"a\"]\n{AT_7}{}", node("a", "repeat = { count = 2 }")), "needs a `body`"),
        ];
        for (text, what) in cases {
            let err = flow(&text).err().unwrap_or_default();
            assert!(err.contains(what) && err.starts_with("automation `f`"), "{what}: got `{err}` for\n{text}");
        }
    }

    #[tokio::test(start_paused = true)]
    async fn branches_run_side_by_side_and_merges_skip_the_paths_not_taken() {
        let a = flow(&format!(
            "start = [\"check\", \"side\"]\n{AT_7}{}{}{}{}{}{}",
            node("check", "if = [{ state = { select = \"room:Salon\", cap = \"on_off\", is = true } }]\nthen = [\"dim\"]\nelse = [\"bright\"]"),
            set("dim", "brightness", "20", "next = [\"join\"]"),
            set("bright", "brightness", "90", "next = [\"join\"]"),
            node("side", "delay = \"1s\"\nnext = [\"join\"]"),
            node("join", "merge = \"all\"\nnext = [\"off\"]"),
            set("off", "on_off", "false", ""),
        ))
        .unwrap();
        let (core, mut commands) = core(vec![lamp(true)]);
        let run = a.run(&core, "f", "manual".into(), &Zoned::now()).await.unwrap();
        assert_eq!(names(&run), ["check", "dim", "side", "join", "off"], "`bright` is dead, so `join` does not wait for it");
        assert_eq!(sent(&mut commands), [CapValue::Brightness(20), CapValue::OnOff(false)]);
        assert_eq!(run.steps[0].outcome, "then: Lampe: true (passes)");

        let dry = a.dry_run(&core, "f", &Zoned::now()).unwrap();
        let path: Vec<&str> = dry.path.iter().map(|p| p.node.as_str()).collect();
        assert_eq!(path, ["check", "dim", "join", "off", "side"]);
        assert_eq!(dry.actions.len(), 2, "the `set`s on the way");

        let any = flow(&format!(
            "start = [\"fast\", \"slow\"]\n{AT_7}{}{}{}{}",
            node("fast", "delay = \"1s\"\nnext = [\"join\"]"),
            node("slow", "delay = \"5s\"\nnext = [\"join\"]"),
            node("join", "merge = \"any\"\nnext = [\"on\"]"),
            set("on", "on_off", "true", ""),
        ))
        .unwrap();
        let run = any.run(&core, "f", "manual".into(), &Zoned::now()).await.unwrap();
        assert_eq!(names(&run), ["fast", "join", "on", "slow"], "on with the first, once");
    }

    #[tokio::test(start_paused = true)]
    async fn a_wait_times_out_or_goes_on_and_the_mode_decides_about_new_runs() {
        let steps = format!(
            "{AT_7}{}{}{}{}",
            set("on", "on_off", "true", "next = [\"wait\"]"),
            node("wait", "wait = { until = [{ state = { select = \"room:Salon\", cap = \"on_off\", is = false } }], timeout = \"10m\" }\nnext = [\"done\"]\ntimeout = [\"late\"]"),
            set("done", "brightness", "50", ""),
            set("late", "brightness", "5", ""),
        );
        let a = flow(&format!("start = [\"on\"]\n{steps}")).unwrap();
        let (core, mut commands) = core(vec![lamp(false)]);
        // The lamp stays off until the 1st run's desired state is observed: the wait passes at once.
        let run = a.run(&core, "f", "manual".into(), &Zoned::now()).await.unwrap();
        assert_eq!(names(&run), ["on", "wait", "done"]);
        sent(&mut commands);

        core.with(|r| r.observe(lamp(false).id, &CapKind::OnOff, CapValue::OnOff(true)));
        let run = a.run(&core, "f", "manual".into(), &Zoned::now()).await.unwrap();
        assert!(run.running && names(&run) == ["on"], "answered before the end: {run:?}");
        let list = a.list(&core, &Zoned::now());
        assert_eq!(list[0]["running"][0]["nodes"], json!(["wait"]));
        let second = a.run(&core, "f", "manual".into(), &Zoned::now()).await.unwrap();
        assert_eq!(second.skipped.as_deref(), Some("skipped: it is still running"), "single, by default");

        tokio::time::sleep(Duration::from_secs(11 * 60)).await;
        let run = last_run(&a, "f").unwrap();
        assert_eq!(names(&run), ["on", "wait", "late"]);
        assert_eq!(run.steps[1].outcome, "timed out");

        // Seen off while it waits: it goes on.
        a.run(&core, "f", "manual".into(), &Zoned::now()).await.unwrap();
        core.with(|r| r.observe(lamp(false).id, &CapKind::OnOff, CapValue::OnOff(false)));
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert_eq!(names(&last_run(&a, "f").unwrap()), ["on", "wait", "done"]);

        // Restart: the run waiting is stopped.
        core.with(|r| r.observe(lamp(false).id, &CapKind::OnOff, CapValue::OnOff(true)));
        let restart = flow(&format!("mode = \"restart\"\nstart = [\"on\"]\n{steps}")).unwrap();
        restart.run(&core, "f", "manual".into(), &Zoned::now()).await.unwrap();
        restart.run(&core, "f", "manual".into(), &Zoned::now()).await.unwrap();
        assert_eq!(last_run(&restart, "f").unwrap().stopped.as_deref(), Some("started again"));
        assert_eq!(restart.stop(&core, "f"), Ok(1));
        assert_eq!(last_run(&restart, "f").unwrap().stopped.as_deref(), Some("by hand"));

        // Queued: the second starts when the first ends.
        let queued = flow(&format!("mode = \"queued\"\nstart = [\"on\"]\n{steps}")).unwrap();
        queued.run(&core, "f", "manual".into(), &Zoned::now()).await.unwrap();
        queued.run(&core, "f", "manual".into(), &Zoned::now()).await.unwrap();
        let list = queued.list(&core, &Zoned::now());
        assert_eq!(list[0]["running"][1]["nodes"], json!([]), "waits for its turn");
        tokio::time::sleep(Duration::from_secs(11 * 60)).await;
        assert!(queued.shared.history.lock().unwrap()["f"].runs.iter().all(|r| r.stopped.is_none()));
        assert_eq!(queued.shared.running.lock().unwrap()["f"].len(), 1, "the second waits now");
    }

    #[tokio::test(start_paused = true)]
    async fn repeats_go_round_each_device_and_stop_at_their_limit() {
        let a = flow(&format!(
            "start = [\"each\"]\n{AT_7}{}{}{}{}",
            node("each", "repeat = { for_each = \"cap:on_off\" }\nbody = [\"dim\"]\nnext = [\"thrice\"]"),
            node("dim", "set = { select = \"item\", cap = \"brightness\", value = 30 }"),
            node("thrice", "repeat = { count = 3 }\nbody = [\"pause\"]"),
            node("pause", "delay = \"1s\""),
        ))
        .unwrap();
        let (core, mut commands) = core(vec![lamp(true), other_lamp()]);
        let run = a.run(&core, "f", "manual".into(), &Zoned::now()).await.unwrap();
        assert_eq!(names(&run), ["dim", "dim", "each", "pause", "pause", "pause", "thrice"]);
        assert_eq!(sent(&mut commands), [CapValue::Brightness(30), CapValue::Brightness(30)]);
        assert_eq!(run.results.iter().map(|r| r.target.as_str()).collect::<Vec<_>>(), ["Lampe", "Lampe 2"]);

        let forever = flow(&format!(
            "start = [\"loop\"]\n{AT_7}{}{}",
            node("loop", "repeat = { while = [{ state = { select = \"room:Salon\", cap = \"on_off\", is = true } }] }\nbody = [\"dim\"]"),
            set("dim", "brightness", "30", ""),
        ))
        .unwrap();
        let run = forever.run(&core, "f", "manual".into(), &Zoned::now()).await.unwrap();
        assert!(run.error.unwrap().contains("1000 times at most"));
    }

    #[tokio::test]
    async fn a_failure_takes_the_error_port_and_a_stop_ends_the_run() {
        let a = flow(&format!(
            "start = [\"cellar\"]\n{AT_7}{}{}{}",
            node("cellar", "set = { select = \"room:Cave\", cap = \"on_off\", value = true }\nnext = [\"never\"]\nerror = [\"fail\"]"),
            set("never", "on_off", "true", ""),
            node("fail", "stop = { error = \"the cellar is gone\" }"),
        ))
        .unwrap();
        let (core, mut commands) = core(vec![lamp(false)]);
        let run = a.run(&core, "f", "manual".into(), &Zoned::now()).await.unwrap();
        assert_eq!(names(&run), ["cellar", "fail"]);
        assert_eq!(run.error.as_deref(), Some("the cellar is gone"));
        assert!(sent(&mut commands).is_empty());

        // No error port: the failure is in the results, and the run goes on.
        let legacy = config(&format!(
            "[[automation]]\nid = \"f\"\n{AT_7}[[automation.action]]\nset = {{ select = \"room:Cave\", cap = \"on_off\", value = true }}\n[[automation.action]]\nset = {{ select = \"room:Salon\", cap = \"on_off\", value = true }}\n"
        ))
        .unwrap();
        let run = legacy.run(&core, "f", "manual".into(), &Zoned::now()).await.unwrap();
        assert_eq!(names(&run), ["action-1", "action-2"], "actions are a chain");
        assert_eq!((run.results[0].ok, run.results[1].ok, run.error), (false, true, None));
    }

    #[tokio::test]
    async fn the_trigger_says_which_device_and_value_started_the_run() {
        let a = config(&format!(
            "[[automation]]\nid = \"f\"\nstart = [\"which\"]\n[[automation.trigger]]\nid = \"dimmed\"\nwhen = {{ select = \"cap:brightness\", cap = \"brightness\", changes = true }}\n{AT_7}{}{}",
            node("which", "if = [{ trigger = \"dimmed\" }]\nthen = [\"copy\"]"),
            node("copy", "set = { select = \"trigger\", cap = \"brightness\", from = \"trigger\" }"),
        ))
        .unwrap();
        let (core, mut commands) = core(vec![lamp(false), other_lamp()]);
        let change = Change {
            id: other_lamp().id,
            cap: CapKind::Brightness,
            slot: CapSlot { observed: Some(CapValue::Brightness(70)), ..Default::default() },
        };
        a.seed(&core);
        let (i, fired) = a.triggered_by(&core, &change).remove(0);
        let (_, end) = a.start(&core, i, fired, &Zoned::now());
        let run = end.await.unwrap();
        assert_eq!(names(&run), ["which", "copy"]);
        assert_eq!(run.results[0].target, "Lampe 2");
        assert_eq!(sent(&mut commands), [CapValue::Brightness(70)]);

        let manual = a.run(&core, "f", "manual".into(), &Zoned::now()).await.unwrap();
        assert_eq!(names(&manual), ["which"], "not started by `dimmed`: else, which leads nowhere");
    }

    #[tokio::test(start_paused = true)]
    async fn an_edit_stops_the_runs_of_what_changed_only() {
        let (core, _commands) = core(vec![lamp(true)]);
        let waits = |delay: &str| format!("[[automation]]\nid = \"{delay}\"\nstart = [\"w\"]\n{AT_7}{}", node("w", &format!("delay = \"{delay}\"")));
        let before = config(&format!("{}{}", waits("1h"), waits("2h"))).unwrap();
        core.set_automations(Arc::try_unwrap(before).ok().unwrap());
        let a = core.automations();
        a.run(&core, "1h", "manual".into(), &Zoned::now()).await.ok();
        a.run(&core, "2h", "manual".into(), &Zoned::now()).await.ok();
        let edited = config(&format!("{}{}", waits("1h"), waits("2h").replace("id = \"2h\"", "id = \"2h\"\nname = \"x\""))).unwrap();
        core.set_automations(Arc::try_unwrap(edited).ok().unwrap());
        let a = core.automations();
        let running = a.shared.running.lock().unwrap();
        assert_eq!((running["1h"].len(), running.get("2h").map_or(0, Vec::len)), (1, 0));
        drop(running);
        assert_eq!(last_run(&a, "2h").unwrap().stopped.as_deref(), Some("the automation was edited"));
    }


    #[tokio::test]
    async fn a_scene_step_sets_the_scene_and_must_name_one() {
        let a = flow(&format!("start = [\"night\"]\n{AT_7}{}", node("night", "scene = \"night\""))).unwrap();
        #[derive(Deserialize)]
        struct File {
            scene: Vec<crate::scenes::SceneConfig>,
        }
        let file: File = toml::from_str("[[scene]]\nid = \"night\"\n[[scene.set]]\nselect = \"room:Salon\"\ncap = \"on_off\"\nvalue = false\n").unwrap();
        let scenes = crate::scenes::Scenes::new(&file.scene).unwrap();
        assert!(crate::scenes::check_refs(&a, &crate::scenes::Scenes::none()).unwrap_err().contains("automation `f`, node `night`: no scene `night`"));
        assert!(crate::scenes::check_refs(&a, &scenes).is_ok());

        let (core, mut commands) = core(vec![lamp(true)]);
        core.set_scenes(scenes);
        let run = a.run(&core, "f", "manual".into(), &Zoned::now()).await.unwrap();
        assert_eq!((names(&run), run.results[0].outcome.as_str()), (vec!["night"], "pending"));
        assert_eq!(sent(&mut commands), [CapValue::OnOff(false)]);
    }
}
