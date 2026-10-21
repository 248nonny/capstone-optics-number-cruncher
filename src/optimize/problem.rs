use crate::engine::{model::Model, state::State, var::Spacing};
use crate::model::outputs::Eval;

pub const INFEASIBLE: f64 = 1e30;

#[derive(Clone)]
pub struct DesignVar {
    pub idx: usize,
    pub lo: f64,
    pub hi: f64,
    pub log: bool,
}

impl DesignVar {
    pub fn new(m: &Model, idx: usize) -> Self {
        let d = &m.defs[idx];
        Self { idx, lo: d.min, hi: d.max, log: d.spacing == Spacing::Log }
    }

    pub fn value(&self, u: f64) -> f64 {
        if self.log { self.lo * (self.hi / self.lo).powf(u) } else { self.lo + (self.hi - self.lo) * u }
    }

    pub fn unit(&self, x: f64) -> f64 {
        let u = if self.log { (x / self.lo).ln() / (self.hi / self.lo).ln() } else { (x - self.lo) / (self.hi - self.lo) };
        u.clamp(0.0, 1.0)
    }
}

#[derive(Clone)]
pub struct Objective {
    pub w_lateral: f64,
    pub w_height: f64,
    pub lim_lateral: f64,
    pub lim_height: f64,
    pub penalty: f64,
    pub strehl: bool,
    pub saturation: bool,
    pub tables: bool,
}

impl Default for Objective {
    fn default() -> Self {
        Self { w_lateral: 1.0, w_height: 1.0, lim_lateral: 5e-6, lim_height: 300e-9, penalty: 100.0, strehl: true, saturation: true, tables: true }
    }
}

#[derive(Clone)]
pub struct Metrics {
    pub lateral: f64,
    pub height: f64,
    pub strehl: f64,
    pub saturation: f64,
    pub tables: f64,
}

pub fn metrics(e: &Eval) -> Metrics {
    let s = e.sig();
    Metrics { lateral: e.lat(), height: s.sigma_z, strehl: s.strehl, saturation: s.sat, tables: s.vp[0].max(s.vp[1]) }
}

impl Objective {
    pub fn cost(&self, k: &Metrics) -> f64 {
        let over = |x: f64, lim: f64| (x / lim - 1.0).max(0.0).powi(2);
        let hard = [(self.strehl, (0.8 - k.strehl).max(0.0).powi(2) * 100.0), (self.saturation, over(k.saturation, 1.0)), (self.tables, over(k.tables, 24.0))];
        let hard: f64 = hard.iter().filter(|(on, _)| *on).map(|(_, v)| v).sum();
        let total = self.w_lateral * k.lateral / self.lim_lateral + self.w_height * k.height / self.lim_height
            + self.penalty * (over(k.lateral, self.lim_lateral) + over(k.height, self.lim_height) + hard);
        if total.is_finite() { total } else { INFEASIBLE }
    }
}

#[derive(Clone)]
pub struct Problem {
    pub model: std::sync::Arc<Model>,
    pub base: State,
    pub vars: Vec<DesignVar>,
    pub objective: Objective,
}

impl Problem {
    pub fn state_at(&self, u: &[f64]) -> Option<State> {
        let mut t = self.base.clone();
        let sets: Vec<(usize, f64)> = self.vars.iter().zip(u).map(|(v, &x)| (v.idx, v.value(x))).collect();
        t.set_many(&self.model, &sets).then_some(t)
    }

    pub fn evaluate(&self, u: &[f64]) -> (f64, Option<Metrics>) {
        let Some(t) = self.state_at(u) else { return (INFEASIBLE, None) };
        let k = metrics(&Eval::new(&t.values));
        (self.objective.cost(&k), Some(k))
    }

    pub fn cost(&self, u: &[f64]) -> f64 {
        self.evaluate(u).0
    }

    pub fn start(&self) -> Vec<f64> {
        self.vars.iter().map(|v| v.unit(self.base.values.v[v.idx])).collect()
    }

    pub fn validate(&self) -> Result<(), String> {
        let nudged: Vec<f64> = self.start().iter().enumerate().map(|(k, u)| if *u > 0.5 { u - 0.03 * (k + 1) as f64 } else { u + 0.03 * (k + 1) as f64 }).collect();
        let mut t = self.base.clone();
        let sets: Vec<(usize, f64)> = self.vars.iter().zip(&nudged).map(|(v, &u)| (v.idx, v.value(u))).collect();
        if t.set_many(&self.model, &sets) {
            return Ok(());
        }
        Err(format!("These variables cannot all be free at once; {} constrains them. Untick one.", t.conflicts.join(", ")))
    }

    pub fn values_at(&self, u: &[f64]) -> Vec<(String, f64)> {
        self.vars.iter().zip(u).map(|(v, &x)| (self.model.defs[v.idx].id.clone(), v.value(x))).collect()
    }
}

