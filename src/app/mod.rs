mod knobs;
mod plots;
mod results;
mod widgets;

use crate::engine::{background::Latest, model::Model, values::Values};
use crate::system::{self, outputs::Report};
use eframe::egui::{self, CentralPanel, ScrollArea, SidePanel, TopBottomPanel};
use std::{sync::Arc, time::{Duration, Instant}};
use widgets::{Action, View};

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Results,
    Plots,
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
    dirty: bool,
}

impl App {
    pub fn new(tab: usize, page: usize, _open: bool) -> Self {
        let model = Arc::new(system::model());
        crate::physics::confocal::warm_background();
        Self { inputs: model.defaults(), model, version: 0, shown: None, worker: Latest::new(), changed_at: Instant::now(), tab: [Tab::Results, Tab::Plots][tab.min(1)], plot: plots::PlotState::new(page), dirty: true }
    }

    fn apply(&mut self, acts: Vec<Action>) {
        self.dirty |= !acts.is_empty();
        for a in acts {
            match a {
                Action::Set(id, x) => self.inputs.set(&id, x),
                Action::Preset(name) => {
                    let params = self.model.lib.get(&name).map(|c| c.params.clone()).unwrap_or_default();
                    params.iter().for_each(|(k, x)| self.inputs.set(&format!("smp.{k}"), *x));
                }
                _ => {}
            }
        }
    }

    fn refresh(&mut self, ctx: &egui::Context) {
        self.version += 1;
        self.changed_at = Instant::now();
        let (version, values, ctx) = (self.version, self.inputs.clone(), ctx.clone());
        self.worker.submit(move || (version, system::outputs::evaluate(&values)), move || ctx.request_repaint());
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
        let view = View { m: &self.model, inputs: &self.inputs, out: self.shown.as_ref().map(|(_, r)| &r.values), stale };
        let flags: Vec<String> = self.shown.as_ref().map(|(_, r)| r.flags.clone()).unwrap_or_default();
        TopBottomPanel::top("top").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.tab, Tab::Results, "Results");
                ui.selectable_value(&mut self.tab, Tab::Plots, "Plots");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.colored_label(if flags.is_empty() { egui::Color32::LIGHT_GREEN } else { egui::Color32::GOLD }, format!("{} warning(s)", flags.len()));
                    if stale {
                        ui.weak("updating…");
                    }
                });
            });
        });
        SidePanel::left("knobs").default_width(430.0).min_width(380.0).show(ctx, |ui| {
            ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| knobs::ui(ui, &view, &mut acts));
        });
        CentralPanel::default().show(ctx, |ui| {
            ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
                for w in &flags {
                    ui.colored_label(egui::Color32::GOLD, format!("⚠ {w}"));
                }
                match self.tab {
                    Tab::Results => results::ui(ui, &view),
                    Tab::Plots => {
                        if let Some((_, r)) = self.shown.as_ref() {
                            plots::ui(ui, &mut self.plot, &self.inputs, r);
                        }
                    }
                }
            });
        });
        self.apply(acts);
    }
}
