use super::problem::{metrics, DesignVar, Metrics, Objective};
use crate::engine::{get::Get, library::Library, values::Values};
use crate::parts::{effective, Effective, Selection};
use crate::system::outputs::evaluate;
use std::thread;

pub const SLOTS: [(&str, &str); 4] = [
    ("Pinhole", "pin"),
    ("Collimator", "lens"),
    ("Objective", "obj"),
    ("Grating", "spec"),
];

pub struct Row {
    pub selection: Selection,
    pub effective: Effective,
    pub cost: f64,
    pub metrics: Option<Metrics>,
}

fn slot_mut(sel: &mut Selection, k: usize) -> &mut Option<String> {
    match k {
        0 => &mut sel.pinhole,
        1 => &mut sel.lens,
        2 => &mut sel.objective,
        _ => &mut sel.grating,
    }
}

pub fn options(lib: &Library, kind: &str) -> Vec<Option<String>> {
    lib.of_kind(kind).map(|c| Some(c.name.clone())).collect()
}

pub fn count(lib: &Library, include: [bool; 4]) -> usize {
    SLOTS
        .iter()
        .zip(include)
        .filter(|(_, on)| *on)
        .map(|((_, kind), _)| options(lib, kind).len())
        .product()
}

fn product(base: &Selection, choices: Vec<Option<Vec<Option<String>>>>) -> Vec<Selection> {
    let mut out = vec![base.clone()];
    for (k, opts) in choices.into_iter().enumerate() {
        if let Some(opts) = opts {
            out = out
                .iter()
                .flat_map(|s| {
                    opts.iter().map(move |o| {
                        let mut n = s.clone();
                        *slot_mut(&mut n, k) = o.clone();
                        n
                    })
                })
                .collect();
        }
    }
    out
}

pub fn all(lib: &Library, include: [bool; 4], base: &Selection) -> Vec<Selection> {
    product(
        base,
        SLOTS
            .iter()
            .zip(include)
            .map(|((_, kind), on)| on.then(|| options(lib, kind)))
            .collect(),
    )
}

pub struct Ctx<'a> {
    pub lib: &'a Library,
    pub base: &'a Values,
    pub objective: &'a Objective,
}

pub fn evaluate_all(ctx: &Ctx, selections: Vec<Selection>) -> Vec<Row> {
    let Ctx {
        lib,
        base,
        objective,
    } = *ctx;
    let target = base.g("opt.na");
    let threads = thread::available_parallelism().map_or(2, |n| n.get());
    let chunk = selections.len().div_ceil(threads).max(1);
    let mut rows: Vec<Row> = thread::scope(|s| {
        let handles: Vec<_> = selections
            .chunks(chunk)
            .map(|part| {
                s.spawn(move || {
                    part.iter()
                        .map(|sel| {
                            let e = effective(lib, sel, target);
                            let mut v = base.clone();
                            let required = v.g("opt.dz");
                            e.apply(&mut v);
                            v.set("opt.dz", required);
                            let k = metrics(&evaluate(&v));
                            let miss = e.dz.map_or(0.0, |d| (d / required - 1.0).abs());
                            let cost = objective.cost(&k)
                                + objective.penalty * (miss - 0.15).max(0.0).powi(2);
                            Row {
                                selection: sel.clone(),
                                effective: e,
                                cost,
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

pub fn snap(
    ctx: &Ctx,
    include: [bool; 4],
    current: &Selection,
    vars: &[DesignVar],
    optimum: &[f64],
    keep: usize,
) -> Vec<Row> {
    let Ctx { lib, base, .. } = *ctx;
    let target = base.g("opt.na");
    let distance = |sel: &Selection| {
        let e = effective(lib, sel, target);
        vars.iter()
            .zip(optimum)
            .filter_map(|(v, u)| {
                let id = ["opt.na", "opt.au", "opt.dz", "opt.t", "spc.eff"];
                let value = [e.na, e.au, e.dz, e.t, e.eff]
                    .into_iter()
                    .zip(id)
                    .find(|(_, id)| base.index.get(*id) == Some(&v.idx))?
                    .0?;
                Some((v.unit(value) - u).powi(2))
            })
            .sum::<f64>()
    };
    let mut scored: Vec<(f64, Selection)> = all(lib, include, current)
        .into_iter()
        .map(|s| (distance(&s), s))
        .collect();
    scored.sort_by(|a, b| a.0.total_cmp(&b.0));
    evaluate_all(ctx, scored.into_iter().take(keep).map(|(_, s)| s).collect())
}
