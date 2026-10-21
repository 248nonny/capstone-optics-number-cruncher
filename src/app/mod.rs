mod knobs;
mod optimize;
mod parts_panel;
mod persist;
mod plots;
mod results;
mod sensitivity;
mod sweep;
mod verify;
mod widgets;

use crate::engine::{background::Latest, model::Model, values::Values};
use crate::system::{self, outputs::Report};
use eframe::egui::{self, CentralPanel, ScrollArea, SidePanel, TopBottomPanel};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use widgets::{Action, View};

const SCENARIO: &str = "scenario.toml";

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Results,
    Plots,
    Sweep,
    Sensitivity,
    Optimize,
    Verify,
}

pub struct App {
    model: Arc<Model>,
    inputs: Values,
    version: u64,
    shown: Option<(u64, Report)>,
    worker: Latest<(u64, Report)>,
    changed_at: Instant,
    tab: Tab,
    plot: plots::PlotState,
    sweep: sweep::Sweep,
    sensitivity: sensitivity::Sensitivity,
    optimize: optimize::Optimize,
    parts: parts_panel::PartsPanel,
    verify_page: usize,
    show_parts: bool,
    objective: crate::optimize::problem::Objective,
    status: String,
    dirty: bool,
}

impl App {
    pub fn new(tab: usize, page: usize, demo: bool) -> Self {
        let model = Arc::new(system::model());
        let sweep = sweep::Sweep::new(&model);
        let optimize = optimize::Optimize::new(&model);
        crate::physics::confocal::warm_background();
        let mut parts = parts_panel::PartsPanel::new();
        if demo {
            parts.demo();
        }
        Self {
            inputs: model.defaults(),
            model,
            version: 0,
            shown: None,
            worker: Latest::new(),
            changed_at: Instant::now(),
            tab: [
                Tab::Results,
                Tab::Plots,
                Tab::Sweep,
                Tab::Sensitivity,
                Tab::Optimize,
                Tab::Verify,
            ][tab.min(5)],
            plot: plots::PlotState::new(page),
            sweep,
            sensitivity: sensitivity::Sensitivity::new(),
            optimize,
            parts,
            verify_page: page.min(6),
            show_parts: true,
            objective: Default::default(),
            status: String::new(),
            dirty: true,
        }
    }

    fn apply(&mut self, acts: Vec<Action>) {
        self.dirty |= !acts.is_empty();
        for a in acts {
            match a {
                Action::Set(id, x) => self.inputs.set(&id, x),
                Action::Preset(name) => {
                    let params = self
                        .model
                        .lib
                        .get(&name)
                        .map(|c| c.params.clone())
                        .unwrap_or_default();
                    for (k, x) in params
                        .iter()
                        .filter(|(k, _)| matches!(k.as_str(), "r" | "spec"))
                    {
                        self.inputs.set(&format!("smp.{k}"), *x);
                    }
                }
                Action::Selection(sel) => self.parts.selection = sel,
                Action::Apply(e) => e.apply(&mut self.inputs),
            }
        }
        self.sanitise();
    }

    fn sanitise(&mut self) {
        for (i, d) in self
            .model
            .defs
            .iter()
            .enumerate()
            .filter(|(_, d)| !d.output)
        {
            let x = self.inputs.v[i];
            self.inputs.v[i] = if x.is_finite() {
                x.clamp(d.min, d.max)
            } else {
                d.default
            };
        }
    }

    fn save(&mut self) {
        let text = persist::save(&self.model, &self.inputs, &self.parts.selection)
            .and_then(|t| std::fs::write(SCENARIO, t).map_err(|e| e.to_string()));
        self.status = text.map_or_else(
            |e| format!("Save failed: {e}"),
            |()| format!("Saved {SCENARIO}"),
        );
    }

    fn load(&mut self) {
        let loaded = std::fs::read_to_string(SCENARIO)
            .map_err(|e| e.to_string())
            .and_then(|t| {
                persist::load(&self.model, &mut self.inputs, &mut self.parts.selection, &t)
            });
        self.sanitise();
        self.dirty |= loaded.is_ok();
        self.status = loaded.map_or_else(
            |e| format!("Load failed: {e}"),
            |n| format!("Loaded {n} settings from {SCENARIO}"),
        );
    }

    fn refresh(&mut self, ctx: &egui::Context) {
        self.version += 1;
        self.changed_at = Instant::now();
        let (version, values, ctx) = (self.version, self.inputs.clone(), ctx.clone());
        self.worker.submit(
            move || (version, system::outputs::evaluate(&values)),
            move || ctx.request_repaint(),
        );
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(done) = self.worker.take() {
            self.shown = Some(done);
        }
        if std::mem::take(&mut self.dirty) {
            self.refresh(ctx);
        }
        let behind = self.shown.as_ref().is_none_or(|(v, _)| *v != self.version);
        let stale = behind && self.changed_at.elapsed() > Duration::from_millis(150);
        if behind {
            ctx.request_repaint_after(Duration::from_millis(160));
        }
        let mut acts = vec![];
        let view = View {
            m: &self.model,
            inputs: &self.inputs,
            out: self.shown.as_ref().map(|(_, r)| &r.values),
            stale,
        };
        let flags: Vec<String> = self
            .shown
            .as_ref()
            .map(|(_, r)| r.flags.clone())
            .unwrap_or_default();
        let (mut save, mut load) = (false, false);
        TopBottomPanel::top("top").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.tab, Tab::Results, "Results");
                ui.selectable_value(&mut self.tab, Tab::Plots, "Plots");
                ui.selectable_value(&mut self.tab, Tab::Sweep, "Sweep");
                ui.selectable_value(&mut self.tab, Tab::Sensitivity, "Sensitivity");
                ui.selectable_value(&mut self.tab, Tab::Optimize, "Optimize");
                ui.selectable_value(&mut self.tab, Tab::Verify, "Verify");
                ui.separator();
                ui.checkbox(&mut self.show_parts, "Real components");
                ui.separator();
                save = ui.button("Save scenario").clicked();
                load = ui.button("Load scenario").clicked();
                ui.label(self.status.as_str());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.colored_label(
                        if flags.is_empty() {
                            egui::Color32::LIGHT_GREEN
                        } else {
                            egui::Color32::GOLD
                        },
                        format!("{} warning(s)", flags.len()),
                    );
                    if stale {
                        ui.weak("updating…");
                    }
                });
            });
        });
        SidePanel::left("knobs")
            .default_width(430.0)
            .width_range(0.0..=f32::INFINITY)
            .show(ctx, |ui| {
                ScrollArea::both()
                    .auto_shrink([false, false])
                    .show(ui, |ui| knobs::ui(ui, &view, &mut acts));
            });
        SidePanel::right("parts")
            .default_width(430.0)
            .width_range(0.0..=f32::INFINITY)
            .show_animated(ctx, self.show_parts, |ui| {
                ScrollArea::both()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        self.parts.ui(
                            ui,
                            &self.model,
                            &self.inputs,
                            &self.objective,
                            self.optimize.finished.as_ref(),
                            &mut acts,
                        )
                    });
            });
        CentralPanel::default().show(ctx, |ui| {
            ScrollArea::both()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for w in &flags {
                        ui.colored_label(egui::Color32::GOLD, format!("⚠ {w}"));
                    }
                    match self.tab {
                        Tab::Results => results::ui(ui, &view),
                        Tab::Sweep => self.sweep.ui(ui, &self.model, &self.inputs, self.version),
                        Tab::Sensitivity => self.sensitivity.ui(
                            ui,
                            &self.model,
                            &self.inputs,
                            self.version,
                            &self.objective,
                        ),
                        Tab::Optimize => self.optimize.ui(
                            ui,
                            &self.model,
                            &self.inputs,
                            &mut self.objective,
                            &mut acts,
                        ),
                        Tab::Verify => verify::ui(
                            ui,
                            &mut self.verify_page,
                            &self.inputs,
                            self.shown.as_ref().map(|(_, r)| r),
                        ),
                        Tab::Plots => {
                            if let Some((_, r)) = self.shown.as_ref() {
                                plots::ui(ui, &mut self.plot, &self.inputs, r);
                            }
                        }
                    }
                });
        });
        self.apply(acts);
        if save {
            self.save();
        }
        if load {
            self.load();
        }
    }
}
