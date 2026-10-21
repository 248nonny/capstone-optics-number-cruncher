use crate::engine::{model::Model, state::State};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Serialize, Deserialize, Default)]
struct Saved {
    #[serde(default)]
    values: BTreeMap<String, f64>,
    #[serde(default)]
    components: BTreeMap<String, String>,
    #[serde(default)]
    constraints: BTreeMap<String, bool>,
    #[serde(default)]
    links: Vec<(String, String)>,
}

pub fn save(m: &Model, s: &State) -> Result<String, String> {
    let inputs = m.defs.iter().enumerate().filter(|(_, d)| !d.output);
    let saved = Saved {
        values: inputs.clone().map(|(i, d)| (d.id.clone(), s.values.v[i])).collect(),
        components: inputs.filter_map(|(i, d)| Some((d.id.clone(), s.locked[i].clone()?))).collect(),
        constraints: m.formulas.iter().zip(&s.active).map(|(f, &on)| (f.id.clone(), on)).collect(),
        links: s.links.clone(),
    };
    toml::to_string(&saved).map_err(|e| e.to_string())
}

pub fn load(m: &Model, s: &mut State, text: &str) -> Result<usize, String> {
    let saved: Saved = toml::from_str(text).map_err(|e| e.to_string())?;
    let mut n = 0;
    for (id, x) in &saved.values {
        if let Some(&i) = m.index.get(id) {
            s.values.v[i] = *x;
            s.locked[i] = saved.components.get(id).cloned();
            n += 1;
        }
    }
    for (f, on) in m.formulas.iter().zip(s.active.iter_mut()) {
        *on = saved.constraints.get(&f.id).copied().unwrap_or(*on);
    }
    s.links = saved.links.into_iter().filter(|(a, b)| m.index.contains_key(a) && m.index.contains_key(b)).collect();
    s.settle(m, &[]);
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::build;

    #[test]
    fn scenario_round_trips() {
        let m = build();
        let mut s = State::new(&m);
        s.set(&m, "pin1.d", 33e-6);
        s.apply_component(&m, "adc", "LTC2225 (12-bit 10 MS/s parallel) + differential epc901");
        s.add_link(&m, "dcoll.f", "coll.f");
        let text = save(&m, &s).unwrap();
        let mut fresh = State::new(&m);
        assert!(load(&m, &mut fresh, &text).unwrap() > 50);
        assert_eq!(fresh.values.v.iter().zip(&s.values.v).filter(|(a, b)| (*a - *b).abs() > 1e-12 * b.abs().max(1.0)).count(), 0);
        assert_eq!(fresh.locked, s.locked);
        assert_eq!(fresh.links, s.links);
    }

    #[test]
    fn unknown_ids_are_skipped() {
        let m = build();
        let mut s = State::new(&m);
        assert_eq!(load(&m, &mut s, "[values]\n\"nope.x\" = 1.0\n").unwrap(), 0);
    }
}
