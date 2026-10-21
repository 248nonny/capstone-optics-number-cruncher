use crate::engine::{model::Model, values::Values, var::Spacing};
use crate::system::outputs::{evaluate, Report};
use std::sync::Arc;

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
    pub fits: bool,
}

impl Default for Objective {
    fn default() -> Self {
        Self { w_lateral: 1.0, w_height: 1.0, lim_lateral: 6e-6, lim_height: 500e-9, penalty: 100.0, strehl: true, fits: true }
    }
}

#[derive(Clone)]
pub struct Metrics {
    pub lateral: f64,
    pub height: f64,
    pub strehl: f64,
    pub fill: f64,
}

pub fn metrics(r: &Report) -> Metrics {
    use crate::engine::get::Get;
    let g = |k: &str| r.values.g(k);
    Metrics { lateral: g("lat.worst"), height: g("pr.soft"), strehl: g("ph.strehl"), fill: g("acq.fill") }
}

impl Objective {
    pub fn cost(&self, k: &Metrics) -> f64 {
        let over = |x: f64, lim: f64| (x / lim - 1.0).max(0.0).powi(2);
        let hard = [(self.strehl, (0.8 - k.strehl).max(0.0).powi(2) * 100.0), (self.fits, over(k.fill, 1.0))];
        let hard: f64 = hard.iter().filter(|(on, _)| *on).map(|(_, v)| v).sum();
        let total = self.w_lateral * k.lateral / self.lim_lateral
            + self.w_height * k.height / self.lim_height
            + self.penalty * (over(k.lateral, self.lim_lateral) + over(k.height, self.lim_height) + hard);
        if total.is_finite() { total } else { INFEASIBLE }
    }
}

#[derive(Clone)]
pub struct Problem {
    pub model: Arc<Model>,
    pub base: Values,
    pub vars: Vec<DesignVar>,
    pub objective: Objective,
}

impl Problem {
    pub fn values_at_unit(&self, u: &[f64]) -> Values {
        let mut v = self.base.clone();
        self.vars.iter().zip(u).for_each(|(d, &x)| v.v[d.idx] = d.value(x));
        v
    }

    pub fn evaluate(&self, u: &[f64]) -> (f64, Option<Metrics>) {
        let k = metrics(&evaluate(&self.values_at_unit(u)));
        (self.objective.cost(&k), Some(k))
    }

    pub fn cost(&self, u: &[f64]) -> f64 {
        self.evaluate(u).0
    }

    pub fn start(&self) -> Vec<f64> {
        self.vars.iter().map(|v| v.unit(self.base.v[v.idx])).collect()
    }

    pub fn values_at(&self, u: &[f64]) -> Vec<(String, f64)> {
        self.vars.iter().zip(u).map(|(v, &x)| (self.model.defs[v.idx].id.clone(), v.value(x))).collect()
    }
}
