use super::widgets::{fmt, Action};
use crate::engine::{model::Model, values::Values};
use crate::optimize::{
    methods::Method,
    problem::{DesignVar, Metrics, Objective, Problem},
    runner::{self, Run},
};
use eframe::egui::{self, RichText, Ui};
use egui_plot::{Line, Plot, PlotPoints};
use std::sync::Arc;

const DEFAULT_ON: [&str; 3] = ["opt.na", "opt.au", "spc.disp"];

struct VarRow {
    var: DesignVar,
    on: bool,
}

pub struct Finished {
    pub problem: Problem,
    pub best: Vec<f64>,
    start: Option<Metrics>,
    end: Option<Metrics>,
}

pub struct Optimize {
    rows: Vec<VarRow>,
    method: Method,
    budget: usize,
    run: Option<(Run, Problem)>,
    pub finished: Option<Finished>,
}

impl Optimize {
    pub fn new(m: &Model) -> Self {
        let rows = m
            .defs
            .iter()
            .enumerate()
            .filter(|(_, d)| d.design)
            .map(|(i, d)| VarRow {
                var: DesignVar::new(m, i),
                on: DEFAULT_ON.contains(&d.id.as_str()),
            })
            .collect();
        Self {
            rows,
            method: Method::GradientThenNelderMead,
            budget: 40,
            run: None,
            finished: None,
        }
    }

    fn objective_ui(ui: &mut Ui, o: &mut Objective, rate: f64) {
        ui.heading("Objective");
        ui.weak(format!("The sensor speed is fixed at {} Hz (set it in the Acquisition group). Lateral precision is the worst-case confocal lateral FWHM; height precision is the smooth worst case of the Cramér–Rao height noise over the depth range.", fmt(rate)));
        egui::Grid::new("objective").num_columns(3).show(ui, |ui| {
            for h in ["", "weight", "limit"] {
                ui.label(RichText::new(h).weak());
            }
            ui.end_row();
            ui.label("Lateral FWHM (worst)");
            ui.add(
                egui::DragValue::new(&mut o.w_lateral)
                    .speed(0.05)
                    .range(0.0..=100.0),
            );
            let mut l = o.lim_lateral * 1e6;
            ui.add(
                egui::DragValue::new(&mut l)
                    .speed(0.1)
                    .range(0.1..=1000.0)
                    .suffix(" µm"),
            );
            o.lim_lateral = l * 1e-6;
            ui.end_row();
            ui.label("Height noise (smooth worst)");
            ui.add(
                egui::DragValue::new(&mut o.w_height)
                    .speed(0.05)
                    .range(0.0..=100.0),
            );
            let mut h = o.lim_height * 1e9;
            ui.add(
                egui::DragValue::new(&mut h)
                    .speed(5.0)
                    .range(1.0..=100000.0)
                    .suffix(" nm"),
            );
            o.lim_height = h * 1e-9;
            ui.end_row();
        });
        ui.horizontal(|ui| {
            ui.label(
                "Cost = Σ weight · value / limit, plus a steep penalty past a limit. Penalty:",
            );
            ui.add(
                egui::DragValue::new(&mut o.penalty)
                    .speed(1.0)
                    .range(0.0..=10000.0),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Hard constraints:");
            ui.checkbox(&mut o.strehl, "Strehl ≥ 0.8");
            ui.checkbox(&mut o.fits, "band fits on the sensor");
        });
    }

    pub fn ui(
        &mut self,
        ui: &mut Ui,
        m: &Arc<Model>,
        base: &Values,
        objective: &mut Objective,
        acts: &mut Vec<Action>,
    ) {
        Self::objective_ui(ui, objective, crate::engine::get::Get::g(base, "acq.rate"));
        ui.separator();
        ui.heading("Design variables");
        egui::Grid::new("design-vars")
            .num_columns(4)
            .striped(true)
            .show(ui, |ui| {
                for h in ["", "Variable", "Lower bound", "Upper bound"] {
                    ui.label(RichText::new(h).weak());
                }
                ui.end_row();
                for r in &mut self.rows {
                    let d = &m.defs[r.var.idx];
                    ui.checkbox(&mut r.on, "");
                    ui.label(format!(
                        "{} (now {} {})",
                        d.label,
                        fmt(base.v[r.var.idx] / d.scale),
                        d.unit
                    ));
                    for bound in [&mut r.var.lo, &mut r.var.hi] {
                        let mut x = *bound / d.scale;
                        let speed = 0.01 * x.abs().max(1e-3);
                        ui.add_enabled(r.on, egui::DragValue::new(&mut x).speed(speed));
                        *bound = x * d.scale;
                    }
                    ui.end_row();
                }
            });
        ui.weak("The depth range, LED current, sample, spectral resolution, efficiency, rate and temperature are requirements or hardware facts and are never changed by the optimiser.");
        ui.horizontal(|ui| {
            ui.label("Method:");
            ui.radio_value(&mut self.method, Method::NelderMead, "Nelder–Mead");
            ui.radio_value(&mut self.method, Method::Gradient, "Gradient descent");
            ui.radio_value(
                &mut self.method,
                Method::GradientThenNelderMead,
                "Gradient, Nelder–Mead fallback",
            );
            ui.add(egui::Slider::new(&mut self.budget, 5..=300).text("iterations"));
        });
        let running = self.run.as_ref().is_some_and(|(r, _)| !r.snapshot().done);
        ui.horizontal(|ui| {
            if ui.add_enabled(!running, egui::Button::new("Run")).clicked() {
                let vars: Vec<DesignVar> = self
                    .rows
                    .iter()
                    .filter(|r| r.on)
                    .map(|r| r.var.clone())
                    .collect();
                if !vars.is_empty() {
                    let problem = Problem {
                        model: m.clone(),
                        base: base.clone(),
                        vars,
                        objective: objective.clone(),
                    };
                    let ctx = ui.ctx().clone();
                    self.finished = None;
                    self.run = Some((
                        runner::spawn(problem.clone(), self.method, self.budget, move || {
                            ctx.request_repaint()
                        }),
                        problem,
                    ));
                }
            }
            if ui.add_enabled(running, egui::Button::new("Stop")).clicked() {
                if let Some((r, _)) = &self.run {
                    r.stop.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            }
        });
        self.progress(ui, acts);
    }

    fn progress(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let Some((run, problem)) = &self.run else {
            return;
        };
        let p = run.snapshot();
        if p.done && self.finished.is_none() {
            let start = problem.evaluate(&problem.start()).1;
            self.finished = Some(Finished {
                problem: problem.clone(),
                best: p.best.clone(),
                start,
                end: problem.evaluate(&p.best).1,
            });
        }
        ui.heading("Progress");
        ui.label(format!(
            "{} evaluations, cost {} {}",
            p.evals,
            fmt(p.cost),
            if p.done {
                format!("({})", p.message)
            } else {
                "(running…)".into()
            }
        ));
        Plot::new("opt-history")
            .allow_scroll(false)
            .height(180.0)
            .x_axis_label("iteration")
            .y_axis_label("cost")
            .show(ui, |pl| {
                pl.line(Line::new(
                    "cost",
                    PlotPoints::from(
                        p.history
                            .iter()
                            .enumerate()
                            .map(|(i, &c)| [i as f64, c])
                            .collect::<Vec<_>>(),
                    ),
                ));
            });
        let Some(f) = &self.finished else { return };
        egui::Grid::new("opt-result").num_columns(3).show(ui, |ui| {
            for h in ["", "Before", "After"] {
                ui.label(RichText::new(h).weak());
            }
            ui.end_row();
            for (label, get) in [
                (
                    "Lateral FWHM, worst (µm)",
                    (|k: &Metrics| k.lateral * 1e6) as fn(&Metrics) -> f64,
                ),
                ("Height noise, smooth worst (nm)", |k: &Metrics| {
                    k.height * 1e9
                }),
                ("Strehl, worst", |k: &Metrics| k.strehl),
                ("Band fill", |k: &Metrics| k.fill),
            ] {
                ui.label(label);
                ui.label(f.start.as_ref().map_or("–".into(), |k| fmt(get(k))));
                ui.label(f.end.as_ref().map_or("–".into(), |k| fmt(get(k))));
                ui.end_row();
            }
        });
        for (id, x) in f.problem.values_at(&f.best) {
            let d = f.problem.model.def(&id);
            ui.label(format!("{}: {} {}", d.label, fmt(x / d.scale), d.unit));
        }
        if ui.button("Apply to the knobs").clicked() {
            f.problem
                .values_at(&f.best)
                .into_iter()
                .for_each(|(id, x)| acts.push(Action::Set(id, x)));
        }
    }
}
