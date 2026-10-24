//! Entity selectors: `room:Salon & cap:on_off`, `plugin:pc | tag:"night light"`, `!model:lamp`.
//!
//! Automations refer to entities by what they are, never by UUID or a name that may change.
//! Terms: `room`, `tag`, `cap`, `plugin`, `device`, `model`, `id`, `name`; values are bare words
//! or "quoted". `!` binds tighter than `&`, which binds tighter than `|`; parentheses group.

use std::fmt;

use uuid::Uuid;

use crate::model::{CapKind, Entity};

#[derive(Debug, Clone, PartialEq)]
pub enum Selector {
    Room(String),
    Tag(String),
    Cap(CapKind),
    Plugin(String),
    Device(String),
    Model(String),
    Id(Uuid),
    Name(String),
    Not(Box<Selector>),
    And(Box<Selector>, Box<Selector>),
    Or(Box<Selector>, Box<Selector>),
}

impl Selector {
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut parser = Parser { text, pos: 0 };
        let selector = parser.or()?;
        parser.skip_space();
        if parser.pos < text.len() {
            return Err(parser.error("expected `&`, `|` or the end"));
        }
        Ok(selector)
    }

    pub fn matches(&self, e: &Entity) -> bool {
        let same = |a: &str, b: &str| a.eq_ignore_ascii_case(b);
        match self {
            Selector::Room(r) => e.room.as_deref().is_some_and(|room| same(room, r)),
            Selector::Tag(t) => e.tags.iter().any(|tag| same(tag, t)),
            Selector::Cap(c) => e.caps.contains_key(c),
            Selector::Plugin(p) => e.plugin == *p,
            Selector::Device(d) => e.device.as_deref() == Some(d.as_str()),
            Selector::Model(m) => e.model.as_deref() == Some(m.as_str()),
            Selector::Id(id) => e.id == *id,
            Selector::Name(n) => same(&e.name, n),
            Selector::Not(s) => !s.matches(e),
            Selector::And(a, b) => a.matches(e) && b.matches(e),
            Selector::Or(a, b) => a.matches(e) || b.matches(e),
        }
    }
}

impl fmt::Display for Selector {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let term = |f: &mut fmt::Formatter, key: &str, value: &str| {
            if value.chars().all(|c| c.is_alphanumeric() || "_-:.".contains(c)) {
                write!(f, "{key}:{value}")
            } else {
                write!(f, "{key}:{value:?}")
            }
        };
        match self {
            Selector::Room(v) => term(f, "room", v),
            Selector::Tag(v) => term(f, "tag", v),
            Selector::Cap(v) => term(f, "cap", &v.to_string()),
            Selector::Plugin(v) => term(f, "plugin", v),
            Selector::Device(v) => term(f, "device", v),
            Selector::Model(v) => term(f, "model", v),
            Selector::Id(v) => term(f, "id", &v.to_string()),
            Selector::Name(v) => term(f, "name", v),
            Selector::Not(s) => write!(f, "!({s})"),
            Selector::And(a, b) => write!(f, "({a} & {b})"),
            Selector::Or(a, b) => write!(f, "({a} | {b})"),
        }
    }
}

struct Parser<'a> {
    text: &'a str,
    pos: usize,
}

impl Parser<'_> {
    fn error(&self, what: &str) -> String {
        format!("selector `{}`: {what} at column {}", self.text, self.pos + 1)
    }

    fn skip_space(&mut self) {
        while let Some(c) = self.peek().filter(|c| c.is_whitespace()) {
            self.pos += c.len_utf8();
        }
    }

    fn peek(&self) -> Option<char> {
        self.text[self.pos..].chars().next()
    }

    fn eat(&mut self, c: char) -> bool {
        self.skip_space();
        if self.peek() == Some(c) {
            self.pos += c.len_utf8();
            true
        } else {
            false
        }
    }

    fn or(&mut self) -> Result<Selector, String> {
        let mut left = self.and()?;
        while self.eat('|') {
            left = Selector::Or(Box::new(left), Box::new(self.and()?));
        }
        Ok(left)
    }

    fn and(&mut self) -> Result<Selector, String> {
        let mut left = self.not()?;
        while self.eat('&') {
            left = Selector::And(Box::new(left), Box::new(self.not()?));
        }
        Ok(left)
    }

    fn not(&mut self) -> Result<Selector, String> {
        if self.eat('!') {
            return Ok(Selector::Not(Box::new(self.not()?)));
        }
        if self.eat('(') {
            let inner = self.or()?;
            if !self.eat(')') {
                return Err(self.error("expected `)`"));
            }
            return Ok(inner);
        }
        self.term()
    }

    fn term(&mut self) -> Result<Selector, String> {
        self.skip_space();
        let start = self.pos;
        while self.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
            self.pos += 1;
        }
        let key = &self.text[start..self.pos];
        if key.is_empty() {
            return Err(self.error("expected a term such as `room:Salon`"));
        }
        if self.peek() != Some(':') {
            return Err(self.error(&format!("expected `:` after `{key}`")));
        }
        self.pos += 1;
        let value = self.value()?;
        Ok(match key {
            "room" => Selector::Room(value),
            "tag" => Selector::Tag(value),
            "cap" => Selector::Cap(value.parse().map_err(|e: String| self.error(&e))?),
            "plugin" => Selector::Plugin(value),
            "device" => Selector::Device(value),
            "model" => Selector::Model(value),
            "id" => Selector::Id(value.parse().map_err(|_| self.error(&format!("`{value}` is not a UUID")))?),
            "name" => Selector::Name(value),
            _ => {
                self.pos = start;
                return Err(self.error(&format!(
                    "unknown term `{key}` (room, tag, cap, plugin, device, model, id, name)"
                )));
            }
        })
    }

    fn value(&mut self) -> Result<String, String> {
        if self.peek() == Some('"') {
            self.pos += 1;
            let start = self.pos;
            while let Some(c) = self.peek() {
                if c == '"' {
                    let value = self.text[start..self.pos].to_string();
                    self.pos += 1;
                    return Ok(value);
                }
                self.pos += c.len_utf8();
            }
            return Err(self.error("unclosed `\"`"));
        }
        let start = self.pos;
        while self.peek().is_some_and(|c| !c.is_whitespace() && !"&|!()\"".contains(c)) {
            self.pos += self.peek().map_or(1, char::len_utf8);
        }
        if start == self.pos {
            return Err(self.error("expected a value"));
        }
        Ok(self.text[start..self.pos].to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::model::CapSlot;

    fn entity(name: &str, room: Option<&str>, plugin: &str, caps: &[CapKind]) -> Entity {
        Entity {
            id: crate::model::entity_uuid(plugin, name),
            name: name.into(),
            room: room.map(Into::into),
            tags: vec!["night light".into()],
            available: true,
            plugin: plugin.into(),
            local_id: name.into(),
            device: None,
            added: None,
            model: Some("lamp".into()),
            actions: vec![],
            caps: caps.iter().map(|c| (c.clone(), CapSlot::default())).collect::<BTreeMap<_, _>>(),
        }
    }

    #[test]
    fn operators_bind_not_and_or_in_that_order() {
        let s = Selector::parse("room:Salon & cap:on_off | !plugin:mock").unwrap();
        assert_eq!(s.to_string(), "((room:Salon & cap:on_off) | !(plugin:mock))");
        let grouped = Selector::parse("room:Salon & (cap:on_off | plugin:pc)").unwrap();
        assert_eq!(grouped.to_string(), "(room:Salon & (cap:on_off | plugin:pc))");
    }

    #[test]
    fn matches_rooms_tags_and_names_ignoring_case() {
        let lamp = entity("Lampe du salon", Some("Salon"), "tplink", &[CapKind::OnOff]);
        assert!(Selector::parse("room:salon & cap:on_off").unwrap().matches(&lamp));
        assert!(Selector::parse(r#"name:"lampe du salon""#).unwrap().matches(&lamp));
        assert!(Selector::parse(r#"tag:"Night light" & model:lamp"#).unwrap().matches(&lamp));
        assert!(!Selector::parse("room:Cuisine | cap:volume").unwrap().matches(&lamp));
        assert!(!Selector::parse("!plugin:tplink").unwrap().matches(&lamp));
    }

    #[test]
    fn errors_say_where() {
        for (text, what) in [
            ("room:", "expected a value"),
            ("rooms:Salon", "unknown term `rooms`"),
            ("room Salon", "expected `:`"),
            ("cap:on-off", "unknown capability"),
            ("(room:Salon", "expected `)`"),
            ("room:Salon room:Cuisine", "expected `&`, `|`"),
            (r#"name:"Lampe"#, "unclosed"),
            ("id:nope", "not a UUID"),
            ("room:Salon\u{a0}&\u{2009}", "expected a term"),
        ] {
            let err = Selector::parse(text).unwrap_err();
            assert!(err.contains(what) && err.contains("column"), "{text}: {err}");
        }
    }
}
