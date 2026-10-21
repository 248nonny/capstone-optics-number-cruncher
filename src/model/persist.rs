use crate::engine::{ctx::Model, source::Source};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Serialize, Deserialize, PartialEq, Debug)]
enum Saved {
    Manual(f64),
    Link(String),
    Rule(String),
    Component(String),
}

fn to_saved(m: &Model, s: &Source) -> Saved {
    match s {
        Source::Manual(x) => Saved::Manual(*x),
        Source::Link(id) => Saved::Link(id.clone()),
        Source::Rule(r) => Saved::Rule(m.rules[*r].out().into()),
        Source::Component(c) => Saved::Component(c.clone()),
    }
}

fn from_saved(m: &Model, s: Saved) -> Option<Source> {
    Some(match s {
        Saved::Manual(x) => Source::Manual(x),
        Saved::Link(id) => Source::Link(id),
        Saved::Rule(out) => Source::Rule(m.rules.iter().position(|r| r.out() == out)?),
        Saved::Component(c) => Source::Component(c),
    })
}

pub fn save(m: &Model) -> Result<String, String> {
    let all: BTreeMap<_, _> = m.sources.iter().map(|(k, s)| (k.clone(), to_saved(m, s))).collect();
    toml::to_string(&all).map_err(|e| e.to_string())
}

pub fn load(m: &mut Model, text: &str) -> Result<usize, String> {
    let saved: BTreeMap<String, Saved> = toml::from_str(text).map_err(|e| e.to_string())?;
    let sources: Vec<_> = saved.into_iter().filter(|(k, _)| m.index.contains_key(k)).filter_map(|(k, s)| Some((k, from_saved(m, s)?))).collect();
    let n = sources.len();
    m.sources.extend(sources);
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{apply_component, build};

    #[test]
    fn scenario_round_trips_by_rule_id() {
        let mut m = build();
        m.set_value("pin1.d", 33e-6);
        apply_component(&mut m, "adc", "LTC2225 (12-bit 10 MS/s parallel) + differential epc901");
        let text = save(&m).unwrap();
        assert!(text.contains("Rule = \"obj.na\""));
        let mut fresh = build();
        assert!(load(&mut fresh, &text).unwrap() > 50);
        assert_eq!(fresh.sources, m.sources);
    }

    #[test]
    fn unknown_ids_are_skipped() {
        let mut m = build();
        assert_eq!(load(&mut m, "[\"nope.x\"]\nManual = 1.0\n").unwrap(), 0);
    }
}
