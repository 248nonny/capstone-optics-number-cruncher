use super::{
    optimize::Finished,
    widgets::{fmt, Action},
};
use crate::engine::{background::Latest, get::Get, model::Model, values::Values};
use crate::optimize::{
    discrete::{self, Row, SLOTS},
    problem::Objective,
};
use crate::parts::{effective, Effective, Selection};
use crate::system::inputs::SHAPES;
use eframe::egui::{self, ComboBox, RichText, Ui};
use std::sync::Arc;

pub struct PartsPanel {
    pub selection: Selection,
    stop_match: bool,
    include: [bool; 4],
    search: Latest<(String, Vec<Row>)>,
    table: (String, Vec<Row>),
}

fn picker(ui: &mut Ui, label: &str, kind: &str, slot: &mut Option<String>, m: &Model) {
    ui.label(label);
    ComboBox::from_id_salt(format!("part-{kind}"))
        .width(250.0)
        .selected_text(slot.as_deref().map_or("(use the knob values)", |s| s))
        .show_ui(ui, |ui| {
            ui.selectable_value(slot, None, "(use the knob values)");
            for c in m.lib.of_kind(kind) {
                ui.selectable_value(slot, Some(c.name.clone()), &c.name)
                    .on_hover_text(&c.note);
            }
        });
    ui.end_row();
}

fn describe(e: &Effective) -> Vec<(String, String)> {
    let mut rows = vec![];
    let mut add = |name: &str, v: Option<String>| {
        v.into_iter().for_each(|v| rows.push((name.to_string(), v)))
    };
    add("NA", e.na.map(|x| format!("{x:.3}")));
    add("Pinhole size", e.au.map(|x| format!("{x:.2} AU")));
    add(
        "Depth range (450-670 nm)",
        e.dz.map(|x| format!("{} µm", fmt(x * 1e6))),
    );
    add(
        "Dispersion shape",
        e.shape.map(|s| SHAPES[s.min(SHAPES.len() - 1)].to_string()),
    );
    add("Throughput, one pass", e.t.map(|x| format!("{x:.2}")));
    add(
        "Wavefront error at NA 0.1",
        e.a.map(|x| format!("{x:.3} waves")),
    );
    add("Spectrometer efficiency", e.eff.map(|x| format!("{x:.2}")));
    add(
        "Magnification (pinhole to sample)",
        e.magnification.map(|x| format!("{x:.2}×")),
    );
    add(
        "Physical pinhole",
        e.pinhole_d.map(|x| format!("{} µm", fmt(x * 1e6))),
    );
    add(
        "Stop diameter",
        e.stop_d.map(|x| format!("{} mm", fmt(x * 1e3))),
    );
    rows
}

fn parts_label(s: &Selection) -> String {
    let short = |n: &Option<String>| {
        n.as_deref().map_or("–".to_string(), |n| {
            n.split(" (").next().unwrap_or(n).to_string()
        })
    };
    format!(
        "{} | {} | {} | {}",
        short(&s.pinhole),
        short(&s.lens),
        short(&s.objective),
        short(&s.grating)
    )
}

impl PartsPanel {
    pub fn new() -> Self {
        Self {
            selection: Selection::default(),
            stop_match: true,
            include: [true, true, true, false],
            search: Latest::new(),
            table: Default::default(),
        }
    }

    pub fn demo(&mut self) {
        self.selection = Selection {
            pinhole: Some("Thorlabs P25D (25 um pinhole)".into()),
            lens: Some("Thorlabs AC254-050-A (achromat f=50)".into()),
            objective: Some("Newport SFPX040 (N-SF11 PCX, uncoated)".into()),
            grating: Some("Thorlabs GT25-03 (300/mm transmission)".into()),
            stop: None,
        };
    }

    pub fn ui(
        &mut self,
        ui: &mut Ui,
        m: &Arc<Model>,
        inputs: &Values,
        objective: &Objective,
        optimum: Option<&Finished>,
        acts: &mut Vec<Action>,
    ) {
        ui.heading("Real components");
        ui.weak("Pick catalog parts to see the knob values they would give. Apply copies them to the knobs, which stay editable. Slots left on 'use the knob values' are not overridden.");
        egui::Grid::new("parts-pick").num_columns(2).show(ui, |ui| {
            picker(
                ui,
                "Pinhole (both sides)",
                "pin",
                &mut self.selection.pinhole,
                m,
            );
            picker(
                ui,
                "Collimator (both sides)",
                "lens",
                &mut self.selection.lens,
                m,
            );
            picker(ui, "Objective", "obj", &mut self.selection.objective, m);
            picker(ui, "Grating", "spec", &mut self.selection.grating, m);
        });
        ui.checkbox(&mut self.stop_match, "Stop diameter follows the NA knob");
        if !self.stop_match {
            let mut mm = self.selection.stop.unwrap_or(0.0025) * 1e3;
            ui.add(
                egui::DragValue::new(&mut mm)
                    .speed(0.05)
                    .range(0.2..=50.0)
                    .suffix(" mm stop"),
            );
            self.selection.stop = Some(mm * 1e-3);
        } else {
            self.selection.stop = None;
        }
        let target = inputs.g("opt.na");
        let e = effective(&m.lib, &self.selection, target);
        egui::Grid::new("effective").striped(true).show(ui, |ui| {
            for (name, value) in describe(&e) {
                ui.label(name);
                ui.label(value);
                ui.end_row();
            }
        });
        e.notes.iter().for_each(|n| {
            ui.colored_label(egui::Color32::GOLD, format!("⚠ {n}"));
        });
        if ui.button("Apply to the knobs").clicked() {
            acts.push(Action::Apply(e));
        }
        ui.separator();
        egui::CollapsingHeader::new("Search combinations").show(ui, |ui| {
            self.search_ui(ui, m, inputs, objective, optimum, acts)
        });
    }

    fn search_ui(
        &mut self,
        ui: &mut Ui,
        m: &Arc<Model>,
        inputs: &Values,
        objective: &Objective,
        optimum: Option<&Finished>,
        acts: &mut Vec<Action>,
    ) {
        for (k, (label, kind)) in SLOTS.iter().enumerate() {
            ui.checkbox(
                &mut self.include[k],
                format!("{label} ({} options)", m.lib.of_kind(kind).count()),
            );
        }
        let n = discrete::count(&m.lib, self.include);
        ui.label(format!("{n} combinations; the others are kept as they are. Ranked by the optimiser's cost at the current knob values."));
        let busy = self.search.busy();
        let ctx = ui.ctx().clone();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!busy && n > 0, egui::Button::new("Run search"))
                .clicked()
            {
                let (model, base, obj, include, sel) = (
                    m.clone(),
                    inputs.clone(),
                    objective.clone(),
                    self.include,
                    self.selection.clone(),
                );
                let c = ctx.clone();
                self.search.submit(
                    move || {
                        (
                            "Ranked combinations".into(),
                            discrete::evaluate_all(
                                &discrete::Ctx {
                                    lib: &model.lib,
                                    base: &base,
                                    objective: &obj,
                                },
                                discrete::all(&model.lib, include, &sel),
                            ),
                        )
                    },
                    move || c.request_repaint(),
                );
            }
            if let Some(f) = optimum {
                if ui
                    .add_enabled(
                        !busy,
                        egui::Button::new("Snap the optimiser result to parts"),
                    )
                    .clicked()
                {
                    let (model, base, obj, include, sel) = (
                        m.clone(),
                        inputs.clone(),
                        objective.clone(),
                        self.include,
                        self.selection.clone(),
                    );
                    let (vars, best) = (f.problem.vars.clone(), f.best.clone());
                    let mut base = base;
                    for (d, &u) in vars.iter().zip(&best) {
                        base.v[d.idx] = d.value(u);
                    }
                    let c = ctx.clone();
                    self.search.submit(
                        move || {
                            (
                                "Parts nearest to the optimiser result".into(),
                                discrete::snap(
                                    &discrete::Ctx {
                                        lib: &model.lib,
                                        base: &base,
                                        objective: &obj,
                                    },
                                    include,
                                    &sel,
                                    &vars,
                                    &best,
                                    8,
                                ),
                            )
                        },
                        move || c.request_repaint(),
                    );
                }
            }
            if busy {
                ui.weak("Searching…");
            }
        });
        if let Some(done) = self.search.take() {
            self.table = done;
        }
        if self.table.1.is_empty() {
            return;
        }
        ui.label(RichText::new(&self.table.0).strong());
        egui::Grid::new("combo-table").striped(true).show(ui, |ui| {
            for h in [
                "#",
                "Cost",
                "Lateral (µm)",
                "Noise (nm)",
                "Parts: pinhole | collimator | objective | grating",
                "",
            ] {
                ui.label(RichText::new(h).weak());
            }
            ui.end_row();
            for (k, r) in self.table.1.iter().take(40).enumerate() {
                ui.label((k + 1).to_string());
                ui.label(fmt(r.cost));
                ui.label(
                    r.metrics
                        .as_ref()
                        .map_or("–".into(), |x| fmt(x.lateral * 1e6)),
                );
                ui.label(
                    r.metrics
                        .as_ref()
                        .map_or("–".into(), |x| fmt(x.height * 1e9)),
                );
                ui.label(parts_label(&r.selection));
                if ui.small_button("Apply").clicked() {
                    acts.push(Action::Selection(r.selection.clone()));
                    acts.push(Action::Apply(r.effective.clone()));
                }
                ui.end_row();
            }
        });
    }
}
