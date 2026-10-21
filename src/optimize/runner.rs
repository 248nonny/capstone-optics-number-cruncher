use super::{
    methods::{self, Method},
    problem::Problem,
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

#[derive(Default, Clone)]
pub struct Progress {
    pub history: Vec<f64>,
    pub best: Vec<f64>,
    pub cost: f64,
    pub evals: usize,
    pub done: bool,
    pub message: String,
}

pub struct Run {
    pub progress: Arc<Mutex<Progress>>,
    pub stop: Arc<AtomicBool>,
}

impl Run {
    pub fn snapshot(&self) -> Progress {
        self.progress.lock().map(|p| p.clone()).unwrap_or_default()
    }
}

pub fn spawn(
    problem: Problem,
    method: Method,
    budget: usize,
    repaint: impl Fn() + Send + 'static,
) -> Run {
    let progress = Arc::new(Mutex::new(Progress {
        cost: f64::NAN,
        ..Default::default()
    }));
    let stop = Arc::new(AtomicBool::new(false));
    let (p, s) = (progress.clone(), stop.clone());
    std::thread::spawn(move || {
        let f = |u: &[f64]| problem.cost(u);
        let x0 = problem.start();
        let mut report = |x: &[f64], cost: f64, evals: usize| {
            if let Ok(mut g) = p.lock() {
                g.history.push(cost);
                (g.best, g.cost, g.evals) = (x.to_vec(), cost, evals);
            }
            repaint();
            s.load(Ordering::Relaxed)
        };
        let trace = methods::run(method, &f, &x0, budget, &mut report);
        if let Ok(mut g) = p.lock() {
            g.history.push(trace.f);
            (g.best, g.cost, g.evals, g.done) = (trace.x, trace.f, trace.evals, true);
            g.message = if s.load(Ordering::Relaxed) {
                "Stopped".into()
            } else if trace.stalled {
                "Stalled".into()
            } else {
                "Converged".into()
            };
        }
        repaint();
    });
    Run { progress, stop }
}
