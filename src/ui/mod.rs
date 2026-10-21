mod plots;
mod sweep;
mod widgets;

use crate::engine::ctx::{Ctx, Model};
use crate::model::{self, flags::flags, persist, train::{default_train, Instance, READOUTS}};
use eframe::egui::{self, CentralPanel, ScrollArea, SidePanel, TopBottomPanel};
use widgets::Action;

const SCENARIO: &str = "scenario.toml";

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Results,
    Plots,
    Sweep,
}

pub struct App {
    model: Model,
    train: Vec<Instance>,
    tab: Tab,
    plot: usize,
    sweep: sweep::Sweep,
    version: u64,
    status: String,
    open: bool,
}

impl App {
    pub fn new(tab: usize, plot: usize, open: bool) -> Self {
        let model = model::build();
        let tab = [Tab::Results, Tab::Plots, Tab::Sweep][tab.min(2)];
        Self { sweep: sweep::Sweep::new(&model), model, train: default_train(), tab, plot, version: 0, status: String::new(), open }
    }

    fn apply(&mut self, acts: Vec<Action>) {
        self.version += !acts.is_empty() as u64;
        for a in acts {
            match a {
                Action::Set(id, x) => self.model.set_value(&id, x),
                Action::Src(id, s) => self.model.set_source(&id, s),
                Action::Comp(inst, name) => model::apply_component(&mut self.model, &inst, &name),
                Action::Reset(inst) => self.model.reset(&inst),
            }
        }
    }

    fn save(&mut self) {
        self.status = match persist::save(&self.model).and_then(|t| std::fs::write(SCENARIO, t).map_err(|e| e.to_string())) {
            Ok(()) => format!("Saved {SCENARIO}"),
            Err(e) => format!("Save failed: {e}"),
        };
    }

    fn load(&mut self) {
        let loaded = std::fs::read_to_string(SCENARIO).map_err(|e| e.to_string()).and_then(|t| persist::load(&mut self.model, &t));
        self.version += loaded.is_ok() as u64;
        self.status = match loaded {
            Ok(n) => format!("Loaded {n} settings from {SCENARIO}"),
            Err(e) => format!("Load failed: {e}"),
        };
    }
}

fn results(ui: &mut egui::Ui, model: &Model, train: &[Instance], c: &Ctx, acts: &mut Vec<Action>) {
    for i in train.iter().filter(|i| READOUTS.contains(&i.kind)) {
        ui.heading(i.title);
        widgets::grid(ui, model, c, i.name, acts);
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut acts = vec![];
        let (mut save, mut load) = (false, false);
        let Self { model, train, tab, plot, sweep, version, status, open } = self;
        let c = Ctx::new(model);
        let warnings = flags(&c);
        TopBottomPanel::top("top").show(ctx, |ui| {
            ui.horizontal(|ui| {
                for (t, name) in [(Tab::Results, "Results"), (Tab::Plots, "Plots"), (Tab::Sweep, "Sweep")] {
                    ui.selectable_value(tab, t, name);
                }
                ui.separator();
                save = ui.button("Save scenario").clicked();
                load = ui.button("Load scenario").clicked();
                ui.label(status.as_str());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let color = if warnings.is_empty() { egui::Color32::LIGHT_GREEN } else { egui::Color32::GOLD };
                    ui.colored_label(color, format!("{} warning(s)", warnings.len()));
                });
            });
        });
        SidePanel::left("inputs").default_width(480.0).min_width(440.0).show(ctx, |ui| {
            ScrollArea::vertical().show(ui, |ui| {
                for i in train.iter().filter(|i| !READOUTS.contains(&i.kind)) {
                    widgets::section(ui, model, &c, i, *open, &mut acts);
                }
            });
        });
        CentralPanel::default().show(ctx, |ui| {
            for w in &warnings {
                ui.colored_label(egui::Color32::GOLD, format!("⚠ {w}"));
            }
            match *tab {
                Tab::Results => {
                    ScrollArea::vertical().show(ui, |ui| results(ui, model, train, &c, &mut acts));
                }
                Tab::Plots => {
                    ui.horizontal(|ui| {
                        for (k, name) in plots::KINDS.iter().enumerate() {
                            ui.selectable_value(plot, k, *name);
                        }
                    });
                    let sig = c.memo("signal", || crate::physics::signal::compute(&c));
                    plots::draw(ui, *plot, &c, &sig);
                }
                Tab::Sweep => sweep.ui(ui, model, *version),
            }
        });
        drop(c);
        let _ = version;
        self.apply(acts);
        if save {
            self.save();
        }
        if load {
            self.load();
        }
    }
}
