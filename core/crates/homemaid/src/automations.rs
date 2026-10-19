//! Typed automations: `[[automation]]` in homemaid.toml, checked when the config loads.
//!
//! A trigger (a time of day, sunrise or sunset included, or an observed state crossing a value) runs the automation; its
//! conditions decide; its actions set desired states (the reconciler does the rest) or invoke
//! one-shot actions. Entities are chosen by selectors, never by name. A dry run tells what would
//! happen without doing it. Each automation keeps its last runs in memory.

use std::{
    collections::{BTreeSet, HashMap, VecDeque},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

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
    reconciler::Change,
    selector::Selector,
};

const KEEP_RUNS: usize = 20;
/// More runs than this in a minute is a loop (two automations answering each other): stop it.
const MAX_RUNS_PER_MINUTE: usize = 10;
/// An `at` trigger found due later than this (the PC was asleep at that time) is skipped, not run late.
const GRACE: Duration = Duration::from_secs(5 * 60);
/// Largest sunrise/sunset offset, in minutes: what the devices that run schedules accept.
const MAX_SUN_OFFSET: i16 = 360;

// ------------------------------------------------------------------------------------- config

// Also written back (the app edits automations): unset options are left out, as a person would.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AutomationConfig {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub enabled: bool,
    #[serde(default, rename = "trigger", skip_serializing_if = "Vec::is_empty")]
    pub triggers: Vec<TriggerConfig>,
    #[serde(default, rename = "condition", skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<ConditionConfig>,
    #[serde(default, rename = "action", skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<ActionConfig>,
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

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub days: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub when: Option<TestConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConditionConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub between: Option<[String; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub days: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<TestConfig>,
}

/// `{ select, cap, ... }` with one test: `becomes`/`is` (a value), `above`, `below`, `changes`.
#[derive(Debug, Clone, Deserialize, Serialize)]
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

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ActionConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub set: Option<SetConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invoke: Option<InvokeConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SetConfig {
    pub select: String,
    pub cap: String,
    pub value: Value,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
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
struct Target {
    select: Selector,
    /// As written, for people.
    text: String,
}

impl Target {
    fn parse(text: &str) -> Result<Self, String> {
        Ok(Self { select: Selector::parse(text)?, text: text.into() })
    }

    fn entities<'a>(&self, entities: impl IntoIterator<Item = &'a Entity>) -> Vec<&'a Entity> {
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
}

#[derive(Debug, Clone)]
enum Action {
    Set { target: Target, cap: CapKind, value: CapValue },
    Invoke { target: Target, action: String, params: Value },
}

#[derive(Debug, Clone)]
pub struct Automation {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    triggers: Vec<Trigger>,
    conditions: Vec<Condition>,
    actions: Vec<Action>,
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

impl Automation {
    /// Checks everything that can be checked without the devices; errors stop the core.
    pub fn compile(c: &AutomationConfig, location: Option<Location>) -> Result<Self, String> {
        let time = |text: &str| TimeOf::parse(text, location);
        let at = |what: String, i: usize, e: String| format!("automation `{}`, {what} {}: {e}", c.id, i + 1);
        if c.id.is_empty() {
            return Err("an automation needs an `id`".into());
        }
        if c.triggers.is_empty() || c.actions.is_empty() {
            return Err(format!("automation `{}`: needs at least one [[automation.trigger]] and one [[automation.action]]", c.id));
        }
        let mut triggers = vec![];
        for (i, t) in c.triggers.iter().enumerate() {
            let trigger = match (&t.at, &t.when) {
                (Some(at_time), None) => {
                    let time = time(at_time).map_err(|e| at("trigger".into(), i, e))?;
                    Trigger::At { time, days: t.days.as_deref().map(Days::parse).transpose()?.unwrap_or(Days::ALL) }
                }
                (None, Some(when)) if t.days.is_none() => Trigger::When(state_test(when, true)?),
                (None, Some(_)) => return Err(at("trigger".into(), i, "`days` goes with `at`".into())),
                _ => return Err(at("trigger".into(), i, "give either `at` or `when`".into())),
            };
            triggers.push(trigger);
        }
        let triggers = triggers;
        let mut conditions = vec![];
        for (i, k) in c.conditions.iter().enumerate() {
            let condition = match (&k.between, &k.days, &k.state) {
                (Some([from, to]), None, None) => Condition::Between(time(from).map_err(|e| at("condition".into(), i, e))?, time(to).map_err(|e| at("condition".into(), i, e))?),
                (None, Some(days), None) => Condition::Days(Days::parse(days)?),
                (None, None, Some(state)) => Condition::State(state_test(state, false).map_err(|e| at("condition".into(), i, e))?),
                _ => return Err(at("condition".into(), i, "give one of `between`, `days`, `state`".into())),
            };
            conditions.push(condition);
        }
        let mut actions = vec![];
        for (i, a) in c.actions.iter().enumerate() {
            let action = match (&a.set, &a.invoke) {
                (Some(set), None) => {
                    let cap: CapKind = set.cap.parse().map_err(|e| at("action".into(), i, e))?;
                    let value = CapValue::from_plain(&cap, set.value.clone()).map_err(|e| at("action".into(), i, e))?;
                    Action::Set { target: Target::parse(&set.select)?, cap, value }
                }
                (None, Some(inv)) => {
                    if !(inv.params.is_null() || inv.params.is_object()) {
                        return Err(at("action".into(), i, "`params` must be a table".into()));
                    }
                    Action::Invoke { target: Target::parse(&inv.select)?, action: inv.action.clone(), params: inv.params.clone() }
                }
                _ => return Err(at("action".into(), i, "give either `set` or `invoke`".into())),
            };
            actions.push(action);
        }
        Ok(Self {
            id: c.id.clone(),
            name: c.name.clone().unwrap_or_else(|| c.id.clone()),
            enabled: c.enabled,
            triggers,
            conditions,
            actions,
            config: c.clone(),
        })
    }

    /// The next time one of its `at` triggers fires, strictly after `after`.
    fn next_time(&self, after: &Zoned) -> Option<Zoned> {
        self.next_trigger(after).map(|(at, _)| at)
    }

    /// Same, with how the run is labelled: `19:30`, or `sunset (21:58)`.
    fn next_trigger(&self, after: &Zoned) -> Option<(Zoned, String)> {
        self.triggers
            .iter()
            .filter_map(|t| match t {
                Trigger::At { time, days } => next_at(*time, *days, after).map(|at| {
                    let clock = at.strftime("%H:%M").to_string();
                    let label = if matches!(time, TimeOf::Clock(_)) { clock } else { format!("{time} ({clock})") };
                    (at, label)
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
    /// RFC 3339, local time.
    pub at: String,
    /// What started it: `"19:30"`, `"Lampe du salon on_off"`, `"manual"`.
    pub trigger: String,
    pub skipped: Option<String>,
    pub results: Vec<ActionResult>,
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
    }
}

fn describe_action(a: &Action) -> Value {
    match a {
        Action::Set { target, cap, value } => json!({ "set": { "select": target.text, "cap": cap.to_string(), "value": value.plain() } }),
        Action::Invoke { target, action, params } => json!({ "invoke": { "select": target.text, "action": action, "params": params } }),
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

pub struct Automations {
    list: Vec<Automation>,
    /// Kept to compile edited automations the same way.
    pub location: Option<Location>,
    history: Mutex<HashMap<String, History>>,
    /// Last observed value per entity and capability, to see edges.
    observed: Mutex<HashMap<(Uuid, CapKind), CapValue>>,
    /// The devices that run an automation themselves too, by automation.
    delegated: Mutex<HashMap<String, BTreeSet<Uuid>>>,
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
        Ok(Self { list, location, history: Mutex::default(), observed: Mutex::default(), delegated: Mutex::default() })
    }

    /// What the device of `entity` can run itself, from the automations simple enough for it:
    /// every trigger an `at`, no condition but days (a device cannot check a state or a time
    /// range). Only `set` actions go; the core runs the whole automation anyway.
    pub fn schedules_for(&self, entity: &Entity) -> Vec<Schedule> {
        let mut out = vec![];
        for a in self.list.iter().filter(|a| a.enabled) {
            let mut days = Days::ALL.0;
            for c in &a.conditions {
                days &= match c {
                    Condition::Days(d) => d.0,
                    Condition::Between(..) | Condition::State(_) => 0,
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
            let Some(times) = times else { continue };
            for action in &a.actions {
                let Action::Set { target, cap, value } = action else { continue };
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

    /// Keeps the runs of `old` for the automations still here (by id). An edited automation
    /// stopped by the loop guard starts again.
    pub(crate) fn carry_runs(&self, old: &Automations) {
        let old = old.history.lock().unwrap();
        let mut history = self.history.lock().unwrap();
        for a in &self.list {
            if let Some(h) = old.get(&a.id) {
                history.entry(a.id.clone()).or_default().runs.clone_from(&h.runs);
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    fn get(&self, id: &str) -> Result<&Automation, String> {
        self.list.iter().find(|a| a.id == id).ok_or_else(|| format!("unknown automation `{id}`"))
    }

    /// For the API: every automation, what it is, when it runs next, what may be wrong, its runs.
    pub fn list(&self, core: &Core, now: &Zoned) -> Value {
        let history = self.history.lock().unwrap();
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
                json!({
                    "id": a.id,
                    "name": a.name,
                    "enabled": a.enabled && h.is_none_or(|h| h.stopped.is_none()),
                    "triggers": a.triggers.iter().map(describe_trigger).collect::<Vec<_>>(),
                    "conditions": a.conditions.iter().map(describe_condition).collect::<Vec<_>>(),
                    "actions": a.actions.iter().map(describe_action).collect::<Vec<_>>(),
                    "next_run": a.enabled.then(|| a.next_time(now)).flatten().map(|z| z.timestamp().to_string()),
                    "problems": problems,
                    // These devices run it themselves too: it still happens while the hub is off.
                    "delegated": delegated,
                    "runs": h.map(|h| h.runs.iter().rev().cloned().collect::<Vec<_>>()).unwrap_or_default(),
                    "config": a.config,
                })
            })
            .collect();
        json!(items)
    }

    /// What it would do now, without doing anything.
    pub fn dry_run(&self, core: &Core, id: &str, now: &Zoned) -> Result<DryRun, String> {
        let a = self.get(id)?;
        let conditions: Vec<Check> = a
            .conditions
            .iter()
            .map(|c| {
                let (ok, reason) = core.with(|r| check(c, now, r.entities.values()));
                Check { condition: describe_condition(c), ok, reason }
            })
            .collect();
        let actions = core.with(|r| {
            a.actions
                .iter()
                .map(|action| {
                    let targets = match action {
                        Action::Set { target, cap, value } => target
                            .entities(r.entities.values())
                            .into_iter()
                            .map(|e| {
                                let slot = e.caps.get(cap);
                                let error = match slot {
                                    None => Some(format!("{} has no {cap}", e.name)),
                                    Some(slot) => slot.meta.validate(cap, value).err(),
                                };
                                let changes = slot.is_some_and(|s| s.desired.as_ref() != Some(value));
                                PlannedTarget { entity: e.id, name: e.name.clone(), changes, error }
                            })
                            .collect(),
                        Action::Invoke { target, action, .. } => target
                            .entities(r.entities.values())
                            .into_iter()
                            .map(|e| PlannedTarget {
                                entity: e.id,
                                name: e.name.clone(),
                                changes: true,
                                error: (!e.actions.iter().any(|d| d.name == *action)).then(|| format!("{} has no action `{action}`", e.name)),
                            })
                            .collect(),
                    };
                    Planned { action: describe_action(action), targets }
                })
                .collect()
        });
        let would_run = conditions.iter().all(|c| c.ok);
        Ok(DryRun { conditions, actions, would_run })
    }

    /// Runs it now (conditions still apply), as if triggered by `trigger`.
    pub async fn run(&self, core: &Core, id: &str, trigger: String, now: &Zoned) -> Result<Run, String> {
        let a = self.get(id)?;
        let run = self.execute(core, a, trigger, now).await;
        self.record(core, &a.id, run.clone());
        Ok(run)
    }

    async fn execute(&self, core: &Core, a: &Automation, trigger: String, now: &Zoned) -> Run {
        let mut run = Run { at: now.strftime("%Y-%m-%dT%H:%M:%S%:z").to_string(), trigger, skipped: None, results: vec![] };
        {
            let mut history = self.history.lock().unwrap();
            let h = history.entry(a.id.clone()).or_default();
            if let Some(stopped) = &h.stopped {
                run.skipped = Some(stopped.clone());
                return run;
            }
            let minute_ago = Instant::now() - Duration::from_secs(60);
            while h.recent.front().is_some_and(|t| *t < minute_ago) {
                h.recent.pop_front();
            }
            if h.recent.len() >= MAX_RUNS_PER_MINUTE {
                let why = format!("stopped: ran {MAX_RUNS_PER_MINUTE} times in a minute (a loop?); restart the core to resume");
                tracing::error!(automation = %a.id, "{why}");
                h.stopped = Some(why.clone());
                run.skipped = Some(why);
                return run;
            }
            h.recent.push_back(Instant::now());
        }
        for c in &a.conditions {
            let (ok, reason) = core.with(|r| check(c, now, r.entities.values()));
            if !ok {
                run.skipped = Some(reason);
                return run;
            }
        }
        for action in &a.actions {
            match action {
                Action::Set { target, cap, value } => {
                    let label = format!("set {cap} {}", value.plain());
                    let results = core.with(|r| {
                        let ids: Vec<(Uuid, String)> = target.entities(r.entities.values()).iter().map(|e| (e.id, e.name.clone())).collect();
                        ids.into_iter()
                            .map(|(id, name)| {
                                let status = r.set_desired(id, cap, value.clone(), Instant::now());
                                ActionResult {
                                    entity: Some(id),
                                    target: name,
                                    action: label.clone(),
                                    ok: status.is_ok(),
                                    outcome: match status {
                                        Ok(s) => serde_json::to_value(&s).ok().and_then(|v| v["state"].as_str().map(String::from)).unwrap_or_default(),
                                        Err(e) => e,
                                    },
                                }
                            })
                            .collect::<Vec<_>>()
                    });
                    run.results.extend(nothing_matched(results, target, &label));
                }
                Action::Invoke { target, action, params } => {
                    let label = format!("invoke {action}");
                    let ids: Vec<(Uuid, String)> =
                        core.with(|r| target.entities(r.entities.values()).iter().map(|e| (e.id, e.name.clone())).collect());
                    let mut results = vec![];
                    for (id, name) in ids {
                        let outcome = match core.plugins.get() {
                            Some(plugins) => plugins.invoke(core, id, action, params.clone()).await,
                            None => Err("plugins are not running".into()),
                        };
                        results.push(ActionResult {
                            entity: Some(id),
                            target: name,
                            action: label.clone(),
                            ok: outcome.is_ok(),
                            outcome: outcome.map_or_else(|e| e, |()| "sent".into()),
                        });
                    }
                    run.results.extend(nothing_matched(results, target, &label));
                }
            }
        }
        run
    }

    fn record(&self, core: &Core, id: &str, run: Run) {
        tracing::info!(automation = %id, trigger = %run.trigger, skipped = ?run.skipped, "automation ran");
        let mut history = self.history.lock().unwrap();
        let runs = &mut history.entry(id.into()).or_default().runs;
        if runs.len() == KEEP_RUNS {
            runs.pop_front();
        }
        runs.push_back(run.clone());
        let _ = core.changes.send(Event::Automation { id: id.into(), run });
    }

    /// The automations a state change triggers, with what triggered them.
    fn triggered_by(&self, core: &Core, change: &Change) -> Vec<(usize, String)> {
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
                a.triggers.iter().find_map(|t| match t {
                    Trigger::When(w) if w.cap == change.cap && w.target.select.matches(&entity) && crosses(&w.test, before.as_ref(), &now) => {
                        Some((i, format!("{} {}", entity.name, change.cap)))
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
                        let Some((due, label)) = a.next_trigger(&checked).filter(|(due, _)| *due <= now) else { continue };
                        let late = Duration::try_from(now.duration_since(&due)).unwrap_or(Duration::ZERO);
                        if late > GRACE {
                            tracing::warn!(automation = %a.id, "{label} skipped: the hub was not running then");
                        } else {
                            self.spawn_run(&core, i, label);
                        }
                    }
                    checked = now;
                }
                event = changes.recv() => match event {
                    Ok(Event::Slot(change)) => {
                        for (i, trigger) in self.triggered_by(&core, &change) {
                            self.spawn_run(&core, i, trigger);
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

    /// Runs one aside: an action may wait for its plugin, and the engine keeps listening meanwhile.
    fn spawn_run(self: &Arc<Self>, core: &Arc<Core>, i: usize, trigger: String) {
        let (this, core) = (self.clone(), core.clone());
        tokio::spawn(async move {
            let a = &this.list[i];
            let run = this.execute(&core, a, trigger, &Zoned::now()).await;
            this.record(&core, &a.id, run);
        });
    }
}

fn nothing_matched(results: Vec<ActionResult>, target: &Target, label: &str) -> Vec<ActionResult> {
    if !results.is_empty() {
        return results;
    }
    vec![ActionResult { entity: None, target: target.text.clone(), action: label.into(), ok: false, outcome: "matches no device".into() }]
}

/// Whether a condition holds now, and why (or why not), in words for the dry run and the log.
fn check<'a>(c: &Condition, now: &Zoned, entities: impl IntoIterator<Item = &'a Entity>) -> (bool, String) {
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
    }
}

/// What may be wrong with an automation given the devices now: selectors matching nothing,
/// devices without the capability or the action.
fn problems<'a>(a: &Automation, entities: impl IntoIterator<Item = &'a Entity> + Clone) -> Vec<String> {
    let mut out = vec![];
    let mut empty = |target: &Target, need: &str, has: &dyn Fn(&Entity) -> bool| {
        let matched = target.entities(entities.clone());
        if matched.is_empty() {
            out.push(format!("`{}` matches no device", target.text));
        } else if !matched.iter().any(|e| has(e)) {
            out.push(format!("`{}` matches no device with {need}", target.text));
        }
    };
    for t in &a.triggers {
        if let Trigger::When(w) = t {
            empty(&w.target, &w.cap.to_string(), &|e| e.caps.contains_key(&w.cap));
        }
    }
    for c in &a.conditions {
        if let Condition::State(s) = c {
            empty(&s.target, &s.cap.to_string(), &|e| e.caps.contains_key(&s.cap));
        }
    }
    for action in &a.actions {
        match action {
            Action::Set { target, cap, .. } => empty(target, &cap.to_string(), &|e| e.caps.contains_key(cap)),
            Action::Invoke { target, action, .. } => {
                empty(target, &format!("the action `{action}`"), &|e| e.actions.iter().any(|d| d.name == *action))
            }
        }
    }
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

    fn config(toml_text: &str) -> Result<Automations, String> {
        #[derive(Deserialize)]
        struct File {
            automation: Vec<AutomationConfig>,
        }
        let file: File = toml::from_str(toml_text).map_err(|e| e.to_string())?;
        Automations::new(&file.automation, Some(PARIS))
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

    fn core(entities: Vec<Entity>) -> (Core, mpsc::UnboundedReceiver<crate::reconciler::Command>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Core::new(Reconciler::new(entities.into_iter().map(|e| (e.id, e)).collect()), tx), rx)
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
        "#).unwrap();
        let schedules = automations.schedules_for(&lamp(false));
        assert_eq!(schedules, [
            Schedule { automation: "dusk".into(), cap: CapKind::OnOff, value: CapValue::OnOff(true), at: ScheduleTime::Sunset(-15), days: 0b110 },
            Schedule { automation: "morning".into(), cap: CapKind::Brightness, value: CapValue::Brightness(40), at: ScheduleTime::Clock(450), days: 0x7f },
        ], "a range or a state needs the hub; another room is not this device");

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
}
