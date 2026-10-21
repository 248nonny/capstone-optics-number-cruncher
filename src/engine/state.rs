use super::{formula::Formula, model::Model, solve::bisect, values::Values, var::Spacing};
use std::sync::Arc;

type Eval = Arc<dyn Fn(&Values) -> f64 + Send + Sync>;

struct Constraint {
    name: String,
    vars: Vec<usize>,
    free: usize,
    f: Eval,
}

#[derive(Clone)]
pub struct State {
    pub values: Values,
    pub locked: Vec<Option<String>>,
    pub active: Vec<bool>,
    pub links: Vec<(String, String)>,
    pub conflicts: Vec<String>,
}

fn same(a: f64, b: f64) -> bool {
    (a.is_nan() && b.is_nan()) || (a - b).abs() <= 1e-9 * a.abs().max(b.abs())
}

impl State {
    pub fn new(m: &Model) -> Self {
        let mut s = Self {
            values: m.defaults(),
            locked: vec![None; m.defs.len()],
            active: m.formulas.iter().map(|f| f.always || f.default_on).collect(),
            links: vec![],
            conflicts: vec![],
        };
        s.settle(m, &[]);
        s
    }

    fn constraints(&self, m: &Model) -> Vec<Constraint> {
        let formula = |f: &Formula| Constraint { name: f.title.clone(), vars: f.vars.iter().map(|v| m.index[v]).collect(), free: f.free, f: f.f.clone() };
        let links = self.links.iter().map(|(a, b)| {
            let ib = m.index[b];
            Constraint { name: format!("{a} = {b}"), vars: vec![m.index[a], ib], free: 2, f: Arc::new(move |v: &Values| v.v[ib]) }
        });
        m.formulas.iter().zip(&self.active).filter(|(_, &on)| on).map(|(f, _)| formula(f)).chain(links).collect()
    }

    pub fn settle(&mut self, m: &Model, pinned: &[usize]) {
        let cons = self.constraints(m);
        let mut moved: Vec<usize> = vec![];
        for _ in 0..32 {
            let (mut changed, mut bad) = (false, vec![]);
            for c in &cons {
                if c.free == 1 && self.locked[c.vars[0]].is_some() {
                    continue;
                }
                let want = (c.f)(&self.values);
                if same(self.values.v[c.vars[0]], want) {
                    continue;
                }
                let usable = |k: usize| !pinned.contains(&c.vars[k]) && self.locked[c.vars[k]].is_none() && !m.defs[c.vars[k]].discrete();
                let mover = (0..c.free).find(|&k| usable(k) && !moved.contains(&c.vars[k])).or_else(|| (0..c.free).find(|&k| usable(k)));
                let Some(k) = mover else {
                    bad.push(c.name.clone());
                    continue;
                };
                let var = c.vars[k];
                let new = if k == 0 { Some(want) } else { self.solve_for(m, c, var) };
                match new {
                    Some(x) => {
                        self.values.v[var] = x;
                        moved.push(var);
                        changed = true;
                    }
                    None => bad.push(c.name.clone()),
                }
            }
            if !changed {
                self.conflicts = bad;
                return;
            }
        }
        self.conflicts = cons.iter().filter(|c| !(c.free == 1 && self.locked[c.vars[0]].is_some()) && !same(self.values.v[c.vars[0]], (c.f)(&self.values))).map(|c| c.name.clone()).collect();
    }

    fn solve_for(&self, m: &Model, c: &Constraint, var: usize) -> Option<f64> {
        let d = &m.defs[var];
        let target = self.values.v[c.vars[0]];
        let mut trial = self.values.clone();
        bisect(d.min, d.max, d.spacing == Spacing::Log, |x| {
            trial.v[var] = x;
            target - (c.f)(&trial)
        })
    }

    pub fn set(&mut self, m: &Model, id: &str, x: f64) {
        let i = m.index[id];
        self.values.v[i] = x;
        self.settle(m, &[i]);
    }

    pub fn setter(&self, m: &Model, id: &str) -> Option<usize> {
        m.formulas.iter().zip(&self.active).position(|(f, &on)| on && f.free == 1 && f.vars[0] == id)
    }

    pub fn release(&mut self, m: &Model, id: &str) -> Option<String> {
        let k = self.setter(m, id)?;
        self.active[k] = false;
        Some(m.formulas[k].title.clone())
    }

    pub fn set_many(&mut self, m: &Model, sets: &[(usize, f64)]) -> bool {
        let before = self.conflicts.len();
        sets.iter().for_each(|&(i, x)| self.values.v[i] = x);
        let pinned: Vec<usize> = sets.iter().map(|s| s.0).collect();
        self.settle(m, &pinned);
        self.conflicts.len() <= before
    }

    pub fn blocked(&self, m: &Model, i: usize) -> Option<String> {
        if let Some(c) = &self.locked[i] {
            return Some(format!("Set by component {c}"));
        }
        let id = &m.defs[i].id;
        let before = self.conflicts.len();
        let mut trial = self.clone();
        trial.release(m, id);
        let x = trial.values.v[i];
        trial.set(m, id, if x == 0.0 { 1e-6 } else { x * 1.001 });
        (trial.conflicts.len() > before).then(|| format!("Moving this would violate: {}", trial.conflicts.join(", ")))
    }

    pub fn apply_component(&mut self, m: &Model, instance: &str, name: &str) {
        let Some(comp) = m.lib.get(name) else { return };
        let mut pinned = vec![];
        for (k, &x) in &comp.params {
            let id = if k.contains('.') { k.clone() } else { format!("{instance}.{k}") };
            if let Some(&i) = m.index.get(&id) {
                self.values.v[i] = x;
                self.locked[i] = Some(name.to_string());
                pinned.push(i);
            }
        }
        self.settle(m, &pinned);
    }

    pub fn clear_components(&mut self, m: &Model, instance: &str) {
        let prefix = format!("{instance}.");
        for (d, l) in m.defs.iter().zip(self.locked.iter_mut()).filter(|(d, _)| d.id.starts_with(&prefix)) {
            let _ = d;
            *l = None;
        }
        self.settle(m, &[]);
    }

    pub fn toggle(&mut self, m: &Model, formula: usize, on: bool) {
        self.active[formula] = on;
        self.settle(m, &[]);
    }

    pub fn reset(&mut self, m: &Model, prefix: &str) {
        let prefix = format!("{prefix}.");
        for (i, d) in m.defs.iter().enumerate().filter(|(_, d)| d.id.starts_with(&prefix)) {
            self.values.v[i] = d.default;
            self.locked[i] = None;
        }
        self.settle(m, &[]);
    }

    pub fn add_link(&mut self, m: &Model, a: &str, b: &str) {
        if a != b && !self.links.iter().any(|l| l.0 == a && l.1 == b) {
            self.links.push((a.into(), b.into()));
            self.settle(m, &[]);
        }
    }

    pub fn remove_link(&mut self, m: &Model, k: usize) {
        self.links.remove(k);
        self.settle(m, &[]);
    }

    pub fn component_of(&self, m: &Model, instance: &str) -> Option<&str> {
        let prefix = format!("{instance}.");
        m.defs.iter().zip(&self.locked).find_map(|(d, l)| if d.id.starts_with(&prefix) { l.as_deref() } else { None })
    }
}
