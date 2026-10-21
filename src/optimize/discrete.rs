use super::problem::{metrics, DesignVar, Metrics, Objective};
use crate::engine::{model::Model, state::State};
use crate::model::{outputs::Eval, train::Instance};
use std::{sync::Arc, thread};

#[derive(Clone)]
pub struct Slot {
    pub label: String,
    pub kind: &'static str,
    pub instances: Vec<&'static str>,
    pub include: bool,
}

pub type Parts = Vec<(&'static str, String)>;

pub struct Row {
    pub parts: Parts,
    pub cost: f64,
    pub metrics: Option<Metrics>,
}

const EXCLUDED_KINDS: [&str; 2] = ["sample", "adc"];

pub fn slots(m: &Model, train: &[Instance], same_both_sides: bool) -> Vec<Slot> {
    let mut out: Vec<Slot> = vec![];
    for inst in train
        .iter()
        .filter(|i| m.lib.of_kind(i.kind).next().is_some())
    {
        match out
            .iter_mut()
            .find(|s| s.kind == inst.kind && same_both_sides)
        {
            Some(slot) => {
                slot.instances.push(inst.name);
                slot.label = format!("{} + {}", slot.label, inst.title);
            }
            None => out.push(Slot {
                label: inst.title.into(),
                kind: inst.kind,
                instances: vec![inst.name],
                include: !EXCLUDED_KINDS.contains(&inst.kind) && inst.kind != "bs",
            }),
        }
    }
    out
}

pub fn options(m: &Model, slot: &Slot) -> Vec<Option<String>> {
    std::iter::once(None)
        .chain(m.lib.of_kind(slot.kind).map(|c| Some(c.name.clone())))
        .collect()
}

pub fn count(m: &Model, slots: &[Slot]) -> usize {
    slots
        .iter()
        .filter(|s| s.include)
        .map(|s| options(m, s).len())
        .product()
}

fn product(slots: &[Slot], choices: &[Vec<Option<String>>]) -> Vec<Parts> {
    let mut combos: Vec<Parts> = vec![vec![]];
    for (slot, opts) in slots.iter().zip(choices).filter(|(s, _)| s.include) {
        combos = combos
            .iter()
            .flat_map(|c| {
                opts.iter().map(move |o| {
                    let mut next = c.clone();
                    if let Some(name) = o {
                        next.extend(slot.instances.iter().map(|&i| (i, name.clone())));
                    }
                    next
                })
            })
            .collect();
    }
    combos
}

pub fn all_combos(m: &Model, slots: &[Slot]) -> Vec<Parts> {
    let choices: Vec<_> = slots.iter().map(|s| options(m, s)).collect();
    product(slots, &choices)
}

pub fn apply(m: &Model, s: &mut State, parts: &Parts) {
    parts
        .iter()
        .for_each(|(inst, name)| s.apply_component(m, inst, name));
}

pub fn evaluate(
    m: &Arc<Model>,
    base: &State,
    objective: &Objective,
    combos: Vec<Parts>,
) -> Vec<Row> {
    let threads = thread::available_parallelism().map_or(2, |n| n.get());
    let chunk = combos.len().div_ceil(threads).max(1);
    let mut rows: Vec<Row> = thread::scope(|s| {
        let handles: Vec<_> = combos
            .chunks(chunk)
            .map(|part| {
                s.spawn(move || {
                    part.iter()
                        .map(|parts| {
                            let mut t = base.clone();
                            apply(m, &mut t, parts);
                            let k = metrics(&Eval::new(&t.effective(m)));
                            Row {
                                parts: parts.clone(),
                                cost: objective.cost(&k),
                                metrics: Some(k),
                            }
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap_or_default())
            .collect()
    });
    rows.sort_by(|a, b| a.cost.total_cmp(&b.cost));
    rows
}

pub fn nearest(
    m: &Model,
    slot: &Slot,
    vars: &[DesignVar],
    optimum: &[f64],
    keep: usize,
) -> Vec<Option<String>> {
    let mut scored: Vec<(f64, String)> = m
        .lib
        .of_kind(slot.kind)
        .filter_map(|c| {
            let terms: Vec<f64> = slot
                .instances
                .iter()
                .flat_map(|inst| {
                    c.params.iter().filter_map(move |(k, &x)| {
                        let id = if k.contains('.') {
                            k.clone()
                        } else {
                            format!("{inst}.{k}")
                        };
                        let i = m.index.get(&id)?;
                        let (var, u) = vars.iter().zip(optimum).find(|(v, _)| v.idx == *i)?;
                        Some((var.unit(x) - u).powi(2))
                    })
                })
                .collect();
            (!terms.is_empty()).then(|| (terms.iter().sum::<f64>().sqrt(), c.name.clone()))
        })
        .collect();
    scored.sort_by(|a, b| a.0.total_cmp(&b.0));
    scored
        .into_iter()
        .take(keep)
        .map(|(_, n)| Some(n))
        .collect()
}

pub fn snap(
    m: &Arc<Model>,
    base: &State,
    objective: &Objective,
    slots: &[Slot],
    vars: &[DesignVar],
    optimum: &[f64],
    keep: usize,
) -> Vec<Row> {
    let choices: Vec<_> = slots
        .iter()
        .map(|s| {
            let near = nearest(m, s, vars, optimum, keep);
            if near.is_empty() {
                vec![None]
            } else {
                near
            }
        })
        .collect();
    evaluate(m, base, objective, product(slots, &choices))
}
