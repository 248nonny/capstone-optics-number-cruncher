use crate::engine::{model::Model, values::Values};
use crate::parts::Selection;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Serialize, Deserialize, Default)]
struct Saved {
    #[serde(default)]
    values: BTreeMap<String, f64>,
    #[serde(default)]
    pinhole: Option<String>,
    #[serde(default)]
    lens: Option<String>,
    #[serde(default)]
    objective: Option<String>,
    #[serde(default)]
    grating: Option<String>,
}

pub fn save(m: &Model, v: &Values, sel: &Selection) -> Result<String, String> {
    let values = m
        .defs
        .iter()
        .enumerate()
        .filter(|(_, d)| !d.output)
        .map(|(i, d)| (d.id.clone(), v.v[i]))
        .collect();
    let saved = Saved {
        values,
        pinhole: sel.pinhole.clone(),
        lens: sel.lens.clone(),
        objective: sel.objective.clone(),
        grating: sel.grating.clone(),
    };
    toml::to_string(&saved).map_err(|e| e.to_string())
}

pub fn load(m: &Model, v: &mut Values, sel: &mut Selection, text: &str) -> Result<usize, String> {
    let saved: Saved = toml::from_str(text).map_err(|e| e.to_string())?;
    let mut n = 0;
    for (id, x) in &saved.values {
        if let Some(&i) = m.index.get(id) {
            v.v[i] = *x;
            n += 1;
        }
    }
    (sel.pinhole, sel.lens, sel.objective, sel.grating) =
        (saved.pinhole, saved.lens, saved.objective, saved.grating);
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scenario_round_trips_and_unknown_ids_are_skipped() {
        let m = crate::system::model();
        let mut v = m.defaults();
        v.set("opt.na", 0.25);
        let sel = Selection {
            objective: Some("Newport SFPX040 (N-SF11 PCX, uncoated)".into()),
            ..Default::default()
        };
        let text = save(&m, &v, &sel).unwrap();
        let (mut fresh, mut sel2) = (m.defaults(), Selection::default());
        assert!(load(&m, &mut fresh, &mut sel2, &text).unwrap() >= 15);
        assert_eq!(fresh.v, v.v);
        assert_eq!(sel2.objective, sel.objective);
        assert_eq!(
            load(&m, &mut fresh, &mut sel2, "[values]\n\"nope.x\" = 1.0\n").unwrap(),
            0
        );
    }
}
