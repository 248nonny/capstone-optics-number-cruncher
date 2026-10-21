use crate::model::train::{Instance, READOUTS};
use crate::engine::{ctx::{Ctx, Model}, get::Get, source::Source, var::{Spacing, VarDef}};
use eframe::egui::{self, ComboBox, RichText, Slider, SliderClamping, Ui};

pub enum Action {
    Set(String, f64),
    Src(String, Source),
    Comp(String, String),
    Reset(String),
}

pub fn fmt(x: f64) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    let mag = if x == 0.0 { 0.0 } else { x.abs().log10().floor() };
    if !(-3.0..5.0).contains(&mag) { format!("{x:.3e}") } else { format!("{:.*}", (3.0 - mag).clamp(0.0, 6.0) as usize, x) }
}

fn doc_ui(ui: &mut Ui, m: &Model, d: &VarDef) {
    ui.label(RichText::new(&d.id).weak());
    ui.label(match &m.sources[&d.id] {
        Source::Manual(_) => "Source: manual".to_string(),
        Source::Link(o) => format!("Source: linked to {o}"),
        Source::Rule(r) => format!("Source: rule {}", m.rules[*r].eq),
        Source::Component(c) => format!("Source: component {c}"),
    });
    if !d.doc.is_empty() {
        ui.label(d.doc);
    }
    for r in m.rules_of(&d.id).map(|r| &m.rules[r]).filter(|r| !r.eq.is_empty()) {
        ui.label(RichText::new(r.eq).italics());
    }
}

fn source_menu(ui: &mut Ui, m: &Model, d: &VarDef, acts: &mut Vec<Action>, current: f64) {
    let src = &m.sources[&d.id];
    ui.menu_button("⚙", |ui| {
        if ui.selectable_label(matches!(src, Source::Manual(_)), "Manual").clicked() {
            acts.push(Action::Set(d.id.clone(), current));
            ui.close();
        }
        if !d.discrete() {
            for r in m.rules_of(&d.id).filter(|&r| !m.rules[r].eq.is_empty()) {
                if ui.selectable_label(*src == Source::Rule(r), format!("Rule: {}", m.rules[r].eq)).clicked() {
                    acts.push(Action::Src(d.id.clone(), Source::Rule(r)));
                    ui.close();
                }
            }
            ui.menu_button("Link to…", |ui| {
                egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                    for o in m.defs.iter().filter(|o| o.id != d.id && o.unit == d.unit && !o.discrete()) {
                        if ui.selectable_label(*src == Source::Link(o.id.clone()), format!("{} ({})", o.label, o.id)).clicked() {
                            acts.push(Action::Src(d.id.clone(), Source::Link(o.id.clone())));
                            ui.close();
                        }
                    }
                });
            });
        }
        for c in m.lib.comps.iter().filter(|c| c.params.contains_key(&d.id) || c.params.contains_key(d.local())) {
            if ui.selectable_label(*src == Source::Component(c.name.clone()), format!("Component: {}", c.name)).clicked() {
                acts.push(Action::Src(d.id.clone(), Source::Component(c.name.clone())));
                ui.close();
            }
        }
    });
    let refs: Vec<_> = m.rules_of(&d.id).flat_map(|r| m.rules[r].refs.iter()).collect();
    if !refs.is_empty() {
        ui.menu_button("?", |ui| {
            for (name, url) in refs {
                ui.hyperlink_to(*name, *url);
            }
        });
    }
}

pub fn param(ui: &mut Ui, m: &Model, c: &Ctx, d: &VarDef, acts: &mut Vec<Action>) {
    let val = c.g(&d.id);
    ui.label(d.label).on_hover_ui(|ui| doc_ui(ui, m, d));
    if d.discrete() {
        let mut i = val as usize;
        ComboBox::from_id_salt(&d.id).selected_text(d.choices[i.min(d.choices.len() - 1)]).show_ui(ui, |ui| {
            for (k, name) in d.choices.iter().enumerate() {
                if ui.selectable_value(&mut i, k, *name).clicked() {
                    acts.push(Action::Set(d.id.clone(), k as f64));
                }
            }
        });
    } else if READOUTS.contains(&d.prefix()) {
        let color = if val.is_nan() { egui::Color32::LIGHT_RED } else { ui.visuals().strong_text_color() };
        ui.label(RichText::new(format!("{} {}", fmt(val / d.scale), d.unit)).color(color));
    } else {
        let mut x = if val.is_nan() { d.default / d.scale } else { val / d.scale };
        let manual = matches!(m.sources[&d.id], Source::Manual(_));
        ui.horizontal(|ui| {
            let slider = Slider::new(&mut x, d.min / d.scale..=d.max / d.scale)
                .logarithmic(d.spacing == Spacing::Log)
                .clamping(SliderClamping::Never)
                .suffix(format!(" {}", d.unit));
            if ui.add_enabled(manual, slider).changed() {
                acts.push(Action::Set(d.id.clone(), x * d.scale));
            }
            if val.is_nan() {
                ui.colored_label(egui::Color32::LIGHT_RED, "NaN");
            }
        });
    }
    ui.horizontal(|ui| source_menu(ui, m, d, acts, val));
    ui.end_row();
}

pub fn grid(ui: &mut Ui, m: &Model, c: &Ctx, name: &str, acts: &mut Vec<Action>) {
    egui::Grid::new(format!("{name}-grid")).num_columns(3).striped(true).show(ui, |ui| {
        for d in m.defs.iter().filter(|d| d.prefix() == name) {
            param(ui, m, c, d, acts);
        }
    });
}

pub fn section(ui: &mut Ui, m: &Model, c: &Ctx, inst: &Instance, open: bool, acts: &mut Vec<Action>) {
    let (name, kind) = (inst.name, inst.kind);
    let active = m.defs.iter().filter(|d| d.prefix() == name).find_map(|d| match &m.sources[&d.id] {
        Source::Component(c) => Some(c.as_str()),
        _ => None,
    });
    let header = egui::CollapsingHeader::new(inst.title).id_salt(name).default_open(open).show(ui, |ui| {
        ui.horizontal(|ui| {
            let comps: Vec<_> = m.lib.of_kind(kind).collect();
            if !comps.is_empty() {
                ComboBox::from_id_salt(format!("{name}-comp")).selected_text(active.unwrap_or("Component…")).show_ui(ui, |ui| {
                    for comp in comps {
                        if ui.selectable_label(active == Some(&comp.name), &comp.name).on_hover_text(&comp.note).clicked() {
                            acts.push(Action::Comp(name.into(), comp.name.clone()));
                        }
                    }
                });
            }
            if ui.small_button("Reset").clicked() {
                acts.push(Action::Reset(name.into()));
            }
        });
        grid(ui, m, c, name, acts);
    });
    header.header_response.on_hover_ui(|ui| {
        let prefix = format!("{name}.");
        for r in m.rules.iter().filter(|r| r.out().starts_with(&prefix) && !r.eq.is_empty()) {
            ui.label(RichText::new(r.eq).italics());
            ui.weak(r.refs.iter().map(|(n, _)| *n).collect::<Vec<_>>().join(", "));
        }
    });
}
