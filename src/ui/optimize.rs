use super::{view::{Action, View}, widgets::fmt};
use crate::engine::{background::Latest, model::Model, state::State};
use crate::model::train::Instance;
use crate::optimize::{discrete::{self, Parts, Row, Slot}, methods::Method, problem::{DesignVar, Metrics, Objective, Problem}, runner::{self, Run}};
use eframe::egui::{self, RichText, Ui};
use egui_plot::{Line, Plot, PlotPoints};
use std::sync::Arc;

const DEFAULT_ON: [&str; 4] = ["pin1.d", "obj.d", "coll.f", "dcoll.f"];

#[derive(PartialEq, Clone, Copy)]
enum Mode {
    Continuous,
    Discrete,
}

struct VarRow {
    var: DesignVar,
    on: bool,
}

struct Finished {
    problem: Problem,
    best: Vec<f64>,
    start: Option<Metrics>,
    end: Option<Metrics>,
}

pub struct Optimize {
    objective: Objective,
    mode: Mode,
    rows: Vec<VarRow>,
    method: Method,
    budget: usize,
    run: Option<(Run, Problem)>,
    finished: Option<Finished>,
    error: String,
    slots: Vec<Slot>,
    same_both: bool,
    train: Vec<(&'static str, &'static str, &'static str)>,
    search: Latest<Vec<Row>>,
    table: Vec<Row>,
    table_title: String,
}

fn parts_label(parts: &Parts) -> String {
    if parts.is_empty() {
        return "current parts".into();
    }
    parts.iter().map(|(i, n)| format!("{i}: {}", n.split(" (").next().unwrap_or(n))).collect::<Vec<_>>().join(", ")
}

impl Optimize {
    pub fn new(m: &Model, train: &[Instance]) -> Self {
        let rows = m.defs.iter().enumerate().filter(|(_, d)| d.design).map(|(i, d)| VarRow { var: DesignVar::new(m, i), on: DEFAULT_ON.contains(&d.id.as_str()) }).collect();
        Self {
            objective: Objective::default(),
            mode: Mode::Continuous,
            rows,
            method: Method::GradientThenNelderMead,
            budget: 40,
            run: None,
            finished: None,
            error: String::new(),
            slots: discrete::slots(m, train, true),
            same_both: true,
            train: train.iter().map(|i| (i.name, i.kind, i.title)).collect(),
            search: Latest::new(),
            table: vec![],
            table_title: String::new(),
        }
    }

    fn instances(&self) -> Vec<Instance> {
        self.train.iter().map(|&(name, kind, title)| Instance { name, kind, title }).collect()
    }

    fn problem(&self, m: &Arc<Model>, s: &State) -> Problem {
        let vars = self.rows.iter().filter(|r| r.on).map(|r| r.var.clone()).collect();
        Problem { model: m.clone(), base: s.clone(), vars, objective: self.objective.clone() }
    }

    fn objective_ui(&mut self, ui: &mut Ui, v: &View) {
        let o = &mut self.objective;
        ui.heading("Objective");
        ui.weak(format!("Sensor speed is fixed at {} Hz (set it on the Acquisition panel). Lateral precision is the confocal lateral FWHM; height precision is the Cramér–Rao σ_z.", fmt(v.s.values.v[v.m.index["acq.rate"]])));
        egui::Grid::new("objective").num_columns(4).show(ui, |ui| {
            ui.label("");
            ui.label("weight");
            ui.label("limit");
            ui.end_row();
            ui.label("Lateral FWHM");
            ui.add(egui::DragValue::new(&mut o.w_lateral).speed(0.05).range(0.0..=100.0));
            let mut l = o.lim_lateral * 1e6;
            ui.add(egui::DragValue::new(&mut l).speed(0.1).range(0.1..=1000.0).suffix(" um"));
            o.lim_lateral = l * 1e-6;
            ui.end_row();
            ui.label("Height precision");
            ui.add(egui::DragValue::new(&mut o.w_height).speed(0.05).range(0.0..=100.0));
            let mut h = o.lim_height * 1e9;
            ui.add(egui::DragValue::new(&mut h).speed(1.0).range(1.0..=100000.0).suffix(" nm"));
            o.lim_height = h * 1e-9;
            ui.end_row();
        });
        ui.horizontal(|ui| {
            ui.label("Cost = Σ weight · value / limit, plus a steep penalty once a value passes its limit. Penalty:");
            ui.add(egui::DragValue::new(&mut o.penalty).speed(1.0).range(0.0..=10000.0));
        });
        ui.horizontal(|ui| {
            ui.label("Hard constraints:");
            ui.checkbox(&mut o.strehl, "Strehl ≥ 0.8");
            ui.checkbox(&mut o.saturation, "no saturation");
            ui.checkbox(&mut o.tables, "pinholes inside the response tables");
        });
    }

    fn continuous(&mut self, ui: &mut Ui, m: &Arc<Model>, v: &View, acts: &mut Vec<Action>) {
        ui.heading("Design variables");
        egui::Grid::new("design-vars").num_columns(5).striped(true).show(ui, |ui| {
            for h in ["", "Variable", "Now", "Lower bound", "Upper bound"] {
                ui.label(RichText::new(h).weak());
            }
            ui.end_row();
            for r in &mut self.rows {
                let d = &m.defs[r.var.idx];
                let blocked = v.blocked[r.var.idx].is_some();
                if blocked {
                    r.on = false;
                }
                ui.add_enabled(!blocked, egui::Checkbox::new(&mut r.on, ""));
                ui.label(format!("{} ({})", d.label, d.id));
                ui.label(format!("{} {}", fmt(v.s.values.v[r.var.idx] / d.scale), d.unit));
                for bound in [&mut r.var.lo, &mut r.var.hi] {
                    let mut x = *bound / d.scale;
                    let speed = 0.01 * x.abs().max(1e-3);
                    ui.add_enabled(r.on, egui::DragValue::new(&mut x).speed(speed));
                    *bound = x * d.scale;
                }
                ui.end_row();
            }
        });
        ui.horizontal(|ui| {
            ui.label("Method:");
            ui.radio_value(&mut self.method, Method::NelderMead, "Nelder–Mead");
            ui.radio_value(&mut self.method, Method::Gradient, "Gradient descent");
            ui.radio_value(&mut self.method, Method::GradientThenNelderMead, "Gradient, Nelder–Mead fallback");
            ui.add(egui::Slider::new(&mut self.budget, 5..=300).text("iterations"));
        });
        let running = self.run.as_ref().is_some_and(|(r, _)| !r.snapshot().done);
        ui.horizontal(|ui| {
            if ui.add_enabled(!running, egui::Button::new("Run")).clicked() {
                let problem = self.problem(m, v.s);
                match problem.validate() {
                    Err(e) => self.error = e,
                    Ok(()) if problem.vars.is_empty() => self.error = "Tick at least one design variable.".into(),
                    Ok(()) => {
                        self.error.clear();
                        self.finished = None;
                        let ctx = ui.ctx().clone();
                        self.run = Some((runner::spawn(problem.clone(), self.method, self.budget, move || ctx.request_repaint()), problem));
                    }
                }
            }
            if ui.add_enabled(running, egui::Button::new("Stop")).clicked() {
                if let Some((r, _)) = &self.run {
                    r.stop.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            }
        });
        if !self.error.is_empty() {
            ui.colored_label(egui::Color32::GOLD, format!("⚠ {}", self.error));
        }
        self.progress(ui, acts);
    }

    fn progress(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let Some((run, problem)) = &self.run else { return };
        let p = run.snapshot();
        if p.done && self.finished.is_none() {
            let start = problem.evaluate(&problem.start()).1;
            self.finished = Some(Finished { problem: problem.clone(), best: p.best.clone(), start, end: problem.evaluate(&p.best).1 });
        }
        ui.heading("Progress");
        ui.label(format!("{} evaluations, cost {} {}", p.evals, fmt(p.cost), if p.done { format!("({})", p.message) } else { "(running…)".into() }));
        Plot::new("opt-history").height(180.0).x_axis_label("iteration").y_axis_label("cost").show(ui, |pl| {
            pl.line(Line::new("cost", PlotPoints::from(p.history.iter().enumerate().map(|(i, &c)| [i as f64, c]).collect::<Vec<_>>())));
        });
        let Some(f) = &self.finished else { return };
        egui::Grid::new("opt-result").num_columns(3).show(ui, |ui| {
            for (label, get) in [("Lateral FWHM", (|k: &Metrics| k.lateral * 1e6) as fn(&Metrics) -> f64), ("Height precision (nm)", |k: &Metrics| k.height * 1e9)] {
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
        if ui.button("Apply to the design").clicked() {
            acts.push(Action::SetMany(f.problem.values_at(&f.best)));
        }
    }

    fn discrete(&mut self, ui: &mut Ui, m: &Arc<Model>, v: &View, acts: &mut Vec<Action>) {
        ui.heading("Component search");
        if ui.checkbox(&mut self.same_both, "Same part on both sides (pinholes, collimators)").changed() {
            self.slots = discrete::slots(m, &self.instances(), self.same_both);
        }
        for s in &mut self.slots {
            ui.checkbox(&mut s.include, format!("{} ({} options + keep current)", s.label, m.lib.of_kind(s.kind).count()));
        }
        let n = discrete::count(m, &self.slots);
        ui.label(format!("{n} combinations"));
        let (busy, ctx) = (self.search.busy(), ui.ctx().clone());
        ui.horizontal(|ui| {
            if ui.add_enabled(!busy && n > 0, egui::Button::new("Run search")).clicked() {
                let (model, base, obj, combos) = (m.clone(), v.s.clone(), self.objective.clone(), discrete::all_combos(m, &self.slots));
                self.table_title = "Ranked combinations".into();
                self.search.submit(move || discrete::evaluate(&model, &base, &obj, combos), move || ctx.request_repaint());
            }
            if let Some(f) = &self.finished {
                if ui.add_enabled(!busy, egui::Button::new("Snap the continuous optimum to catalog parts")).clicked() {
                    let (model, obj, slots) = (m.clone(), self.objective.clone(), self.slots.clone());
                    let (vars, best, mut base) = (f.problem.vars.clone(), f.best.clone(), v.s.clone());
                    let sets: Vec<_> = f.problem.vars.iter().zip(&best).map(|(d, &u)| (d.idx, d.value(u))).collect();
                    base.set_many(m, &sets);
                    let ctx = ui.ctx().clone();
                    self.table_title = "Nearest catalog parts to the continuous optimum".into();
                    self.search.submit(move || discrete::snap(&model, &base, &obj, &slots, &vars, &best, 3), move || ctx.request_repaint());
                }
            }
            if busy {
                ui.weak("Searching…");
            }
        });
        if let Some(rows) = self.search.take() {
            self.table = rows;
        }
        if self.table.is_empty() {
            return;
        }
        ui.heading(&self.table_title);
        egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
            egui::Grid::new("combo-table").striped(true).show(ui, |ui| {
                for h in ["#", "Cost", "Lateral", "Height", "Parts", ""] {
                    ui.label(RichText::new(h).weak());
                }
                ui.end_row();
                for (k, r) in self.table.iter().take(60).enumerate() {
                    ui.label((k + 1).to_string());
                    ui.label(fmt(r.cost));
                    ui.label(r.metrics.as_ref().map_or("–".into(), |x| format!("{} um", fmt(x.lateral * 1e6))));
                    ui.label(r.metrics.as_ref().map_or("–".into(), |x| format!("{} nm", fmt(x.height * 1e9))));
                    ui.label(parts_label(&r.parts));
                    if ui.small_button("Apply").clicked() {
                        acts.push(Action::Parts(r.parts.iter().map(|(i, n)| (i.to_string(), n.clone())).collect()));
                    }
                    ui.end_row();
                }
            });
        });
    }

    pub fn ui(&mut self, ui: &mut Ui, m: &Arc<Model>, v: &View, acts: &mut Vec<Action>) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            self.objective_ui(ui, v);
            ui.separator();
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.mode, Mode::Continuous, "Continuous parameters");
                ui.selectable_value(&mut self.mode, Mode::Discrete, "Discrete components");
            });
            match self.mode {
                Mode::Continuous => self.continuous(ui, m, v, acts),
                Mode::Discrete => self.discrete(ui, m, v, acts),
            }
        });
    }
}
