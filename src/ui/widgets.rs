use super::view::{Action, View};
use crate::engine::{get::Get, var::{Spacing, VarDef}};
use crate::model::{outputs, train::{Instance, READOUTS}};
use eframe::egui::{self, ComboBox, RichText, Slider, SliderClamping, Ui};

pub fn fmt(x: f64) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    let mag = if x == 0.0 { 0.0 } else { x.abs().log10().floor() };
    if !(-3.0..5.0).contains(&mag) { format!("{x:.3e}") } else { format!("{:.*}", (3.0 - mag).clamp(0.0, 6.0) as usize, x) }
}

fn doc_ui(ui: &mut Ui, v: &View, d: &VarDef, reason: Option<&str>) {
    ui.label(RichText::new(&d.id).weak());
    if !d.doc.is_empty() {
        ui.label(d.doc);
    }
    if let Some(r) = reason {
        ui.colored_label(egui::Color32::GOLD, r);
    }
    if let Some(k) = v.s.setter(v.m, &d.id) {
        ui.colored_label(egui::Color32::LIGHT_BLUE, format!("Set by the constraint “{}”. Dragging this slider turns that constraint off.", v.m.formulas[k].title));
    }
    for f in v.m.formulas.iter().filter(|f| f.vars.contains(&d.id)) {
        ui.label(RichText::new(format!("{}: {}", f.title, f.eq)).italics());
    }
    if let Some(o) = outputs::lookup(&d.id) {
        ui.label(RichText::new(o.eq).italics());
        for (name, _) in o.refs {
            ui.weak(*name);
        }
    }
}

fn refs_menu(ui: &mut Ui, v: &View, d: &VarDef) {
    let mut refs: Vec<_> = v.m.formulas.iter().filter(|f| f.vars.contains(&d.id)).flat_map(|f| f.refs.iter()).collect();
    refs.extend(outputs::lookup(&d.id).into_iter().flat_map(|o| o.refs.iter()));
    if !refs.is_empty() {
        ui.menu_button("?", |ui| {
            for (name, url) in refs {
                ui.hyperlink_to(*name, *url);
            }
        });
    }
}

fn control(ui: &mut Ui, v: &View, d: &VarDef, i: usize, acts: &mut Vec<Action>) -> Option<String> {
    let val = v.s.values.v[i];
    let reason = v.blocked[i].clone();
    if d.discrete() {
        let mut k = (val as usize).min(d.choices.len() - 1);
        ComboBox::from_id_salt(&d.id).selected_text(d.choices[k]).show_ui(ui, |ui| {
            for (n, name) in d.choices.iter().enumerate() {
                if ui.selectable_value(&mut k, n, *name).clicked() {
                    acts.push(Action::Set(d.id.clone(), n as f64));
                }
            }
        });
        return reason;
    }
    let mut x = if val.is_nan() { d.default / d.scale } else { val / d.scale };
    let slider = Slider::new(&mut x, d.min / d.scale..=d.max / d.scale)
        .logarithmic(d.spacing == Spacing::Log)
        .clamping(SliderClamping::Never)
        .suffix(format!(" {}", d.unit));
    if ui.add_enabled(reason.is_none(), slider).changed() {
        acts.push(Action::Set(d.id.clone(), x * d.scale));
    }
    if let Some(c) = &v.s.locked[i] {
        ui.label(RichText::new(c.split(' ').next().unwrap_or("part")).small().italics());
    }
    reason
}

fn readout(ui: &mut Ui, v: &View, d: &VarDef) {
    let val = v.out.map_or(f64::NAN, |o| o.g(&d.id));
    let color = if val.is_nan() { egui::Color32::LIGHT_RED } else if v.stale { ui.visuals().weak_text_color() } else { ui.visuals().strong_text_color() };
    ui.label(RichText::new(format!("{} {}", fmt(val / d.scale), d.unit)).color(color));
}

pub fn row(ui: &mut Ui, v: &View, d: &VarDef, acts: &mut Vec<Action>) {
    let i = v.m.index[&d.id];
    let readonly = d.output || READOUTS.contains(&d.prefix());
    let reason = v.blocked[i].clone();
    let label = ui.label(d.label);
    if readonly {
        label.on_hover_ui(|ui| doc_ui(ui, v, d, None));
        readout(ui, v, d);
    } else {
        let r = control(ui, v, d, i, acts);
        label.on_hover_ui(|ui| doc_ui(ui, v, d, reason.as_deref().or(r.as_deref())));
    }
    ui.horizontal(|ui| refs_menu(ui, v, d));
    ui.end_row();
}

pub fn grid(ui: &mut Ui, v: &View, name: &str, acts: &mut Vec<Action>) {
    egui::Grid::new(format!("{name}-grid")).num_columns(3).striped(true).show(ui, |ui| {
        for d in v.m.defs.iter().filter(|d| d.prefix() == name) {
            row(ui, v, d, acts);
        }
    });
}

pub fn section(ui: &mut Ui, v: &View, inst: &Instance, open: bool, acts: &mut Vec<Action>) {
    let active = v.s.component_of(v.m, inst.name);
    let header = egui::CollapsingHeader::new(inst.title).id_salt(inst.name).default_open(open).show(ui, |ui| {
        ui.horizontal(|ui| {
            let comps: Vec<_> = v.m.lib.of_kind(inst.kind).collect();
            if !comps.is_empty() {
                ComboBox::from_id_salt(format!("{}-comp", inst.name)).selected_text(active.unwrap_or("Component…")).show_ui(ui, |ui| {
                    for comp in comps {
                        if ui.selectable_label(active == Some(&comp.name), &comp.name).on_hover_text(&comp.note).clicked() {
                            acts.push(Action::Comp(inst.name.into(), comp.name.clone()));
                        }
                    }
                });
                if active.is_some() && ui.small_button("Unlock").clicked() {
                    acts.push(Action::ClearComp(inst.name.into()));
                }
            }
            if ui.small_button("Reset").clicked() {
                acts.push(Action::Reset(inst.name.into()));
            }
        });
        grid(ui, v, inst.name, acts);
    });
    header.header_response.on_hover_ui(|ui| {
        let prefix = format!("{}.", inst.name);
        for f in v.m.formulas.iter().filter(|f| f.vars.iter().any(|x| x.starts_with(&prefix))) {
            ui.label(RichText::new(format!("{}: {}", f.title, f.eq)).italics());
        }
    });
}
