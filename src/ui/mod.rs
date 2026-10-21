mod constraints;
mod optimize;
mod plots;
mod sweep;
mod verify;
mod view;
mod widgets;

use crate::engine::{background::Latest, model::Model, state::State};
use crate::model::{
    self,
    outputs::{self, Evaluation},
    persist,
    train::{default_train, Instance, READOUTS},
};
use eframe::egui::{self, CentralPanel, ScrollArea, SidePanel, TopBottomPanel};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use view::{Action, View};

const SCENARIO: &str = "scenario.toml";

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Results,
    Plots,
    Sweep,
    Constraints,
    Optimize,
    Verify,
}

pub struct App {
    model: Arc<Model>,
    state: State,
    train: Vec<Instance>,
    version: u64,
    shown: Option<(u64, Evaluation)>,
    worker: Latest<(u64, Evaluation)>,
    blocked: Vec<Option<String>>,
    tab: Tab,
    plot: usize,
    sweep: sweep::Sweep,
    links: constraints::LinkPicker,
    optimize: optimize::Optimize,
    verify_page: usize,
    status: String,
    open: bool,
    dirty: bool,
    changed_at: Instant,
}

impl App {
    pub fn new(tab: usize, plot: usize, open: bool) -> Self {
        let model = Arc::new(model::build());
        let state = State::new(&model);
        crate::physics::confocal::warm_background();
        let optimize = optimize::Optimize::new(&model, &default_train());
        Self {
            sweep: sweep::Sweep::new(&model),
            blocked: vec![None; model.defs.len()],
            model,
            state,
            train: default_train(),
            version: 0,
            shown: None,
            worker: Latest::new(),
            tab: [
                Tab::Results,
                Tab::Plots,
                Tab::Sweep,
                Tab::Constraints,
                Tab::Optimize,
                Tab::Verify,
            ][tab.min(5)],
            plot,
            links: constraints::LinkPicker::new(),
            optimize,
            verify_page: plot.min(5),
            status: String::new(),
            open,
            dirty: true,
            changed_at: Instant::now(),
        }
    }

    fn apply(&mut self, acts: Vec<Action>) {
        self.dirty |= !acts.is_empty();
        let (m, s) = (self.model.clone(), &mut self.state);
        let status = &mut self.status;
        for a in acts {
            match a {
                Action::Set(id, x) => {
                    if let Some(t) = s.release(&m, &id) {
                        *status = format!("Constraint “{t}” turned off");
                    }
                    s.set(&m, &id, x)
                }
                Action::Comp(inst, name) => s.apply_component(&m, &inst, &name),
                Action::ClearComp(inst) => s.clear_components(&m, &inst),
                Action::Reset(inst) => s.reset(&m, &inst),
                Action::Toggle(k, on) => s.toggle(&m, k, on),
                Action::AddLink(a, b) => s.add_link(&m, &a, &b),
                Action::RemoveLink(k) => s.remove_link(&m, k),
                Action::SetMany(sets) => {
                    let idx: Vec<_> = sets.iter().map(|(id, x)| (m.index[id], *x)).collect();
                    s.set_many(&m, &idx);
                }
                Action::Parts(parts) => parts
                    .iter()
                    .for_each(|(inst, name)| s.apply_component(&m, inst, name)),
            }
        }
    }

    fn refresh(&mut self, ctx: &egui::Context) {
        self.version += 1;
        self.changed_at = Instant::now();
        self.blocked = (0..self.model.defs.len())
            .map(|i| {
                if self.model.defs[i].output {
                    None
                } else {
                    self.state.blocked(&self.model, i)
                }
            })
            .collect();
        let (version, values) = (self.version, self.state.effective(&self.model));
        let extra: Vec<String> = self
            .state
            .conflicts
            .iter()
            .map(|c| format!("Over-constrained: {c}"))
            .collect();
        let ctx = ctx.clone();
        self.worker.submit(
            move || (version, outputs::evaluate(&values, &extra)),
            move || ctx.request_repaint(),
        );
    }

    fn save(&mut self) {
        self.status = match persist::save(&self.model, &self.state)
            .and_then(|t| std::fs::write(SCENARIO, t).map_err(|e| e.to_string()))
        {
            Ok(()) => format!("Saved {SCENARIO}"),
            Err(e) => format!("Save failed: {e}"),
        };
    }

    fn load(&mut self) {
        let loaded = std::fs::read_to_string(SCENARIO)
            .map_err(|e| e.to_string())
            .and_then(|t| persist::load(&self.model, &mut self.state, &t));
        self.dirty |= loaded.is_ok();
        self.status = match loaded {
            Ok(n) => format!("Loaded {n} settings from {SCENARIO}"),
            Err(e) => format!("Load failed: {e}"),
        };
    }
}

fn results(ui: &mut egui::Ui, v: &View, train: &[Instance], acts: &mut Vec<Action>) {
    for i in train.iter().filter(|i| READOUTS.contains(&i.kind)) {
        ui.heading(i.title);
        widgets::grid(ui, v, i.name, acts);
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
        let mut acts = vec![];
        let (mut save, mut load) = (false, false);
        let Self {
            model,
            state,
            train,
            shown,
            blocked,
            tab,
            plot,
            sweep,
            links,
            optimize,
            verify_page,
            status,
            open,
            version,
            changed_at,
            ..
        } = self;
        let behind = shown.as_ref().is_none_or(|(v, _)| v != version);
        let stale = behind && changed_at.elapsed() > Duration::from_millis(150);
        if behind {
            ctx.request_repaint_after(Duration::from_millis(160));
        }
        let view = View {
            m: model,
            s: state,
            blocked,
            out: shown.as_ref().map(|(_, e)| &e.values),
            stale,
        };
        let warnings: Vec<String> = shown
            .as_ref()
            .map(|(_, e)| e.flags.clone())
            .unwrap_or_default();
        TopBottomPanel::top("top").show(ctx, |ui| {
            ui.horizontal(|ui| {
                for (t, name) in [
                    (Tab::Results, "Results"),
                    (Tab::Plots, "Plots"),
                    (Tab::Sweep, "Sweep"),
                    (Tab::Constraints, "Constraints"),
                    (Tab::Optimize, "Optimize"),
                    (Tab::Verify, "Verify"),
                ] {
                    ui.selectable_value(tab, t, name);
                }
                ui.separator();
                save = ui.button("Save scenario").clicked();
                load = ui.button("Load scenario").clicked();
                ui.label(status.as_str());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let color = if warnings.is_empty() {
                        egui::Color32::LIGHT_GREEN
                    } else {
                        egui::Color32::GOLD
                    };
                    ui.colored_label(color, format!("{} warning(s)", warnings.len()));
                    if stale {
                        ui.weak("updating…");
                    }
                });
            });
        });
        SidePanel::left("inputs")
            .default_width(480.0)
            .min_width(440.0)
            .show(ctx, |ui| {
                ScrollArea::both()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for i in train.iter().filter(|i| !READOUTS.contains(&i.kind)) {
                            widgets::section(ui, &view, i, *open, &mut acts);
                        }
                    });
            });
        CentralPanel::default().show(ctx, |ui| {
            ScrollArea::both()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for w in &warnings {
                        ui.colored_label(egui::Color32::GOLD, format!("⚠ {w}"));
                    }
                    match *tab {
                        Tab::Results => results(ui, &view, train, &mut acts),
                        Tab::Plots => {
                            ui.horizontal(|ui| {
                                for (k, name) in plots::KINDS.iter().enumerate() {
                                    ui.selectable_value(plot, k, *name);
                                }
                            });
                            if let Some((_, e)) = shown.as_ref() {
                                plots::draw(ui, *plot, &e.values, &e.signal);
                            }
                        }
                        Tab::Sweep => sweep.ui(ui, model, state, *version),
                        Tab::Constraints => constraints::ui(ui, &view, links, &mut acts),
                        Tab::Optimize => optimize.ui(ui, model, &view, &mut acts),
                        Tab::Verify => {
                            verify::ui(ui, verify_page, model, shown.as_ref().map(|(_, e)| e))
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
