use crate::engine::{
    get::Get,
    model::Model,
    values::Values,
    var::{Spacing, VarDef},
};
use eframe::egui::{self, ComboBox, RichText, Slider, SliderClamping, Ui};

pub enum Action {
    Set(String, f64),
    Preset(String),
    Selection(crate::parts::Selection),
    Apply(crate::parts::Effective),
}

pub struct View<'a> {
    pub m: &'a Model,
    pub inputs: &'a Values,
    pub out: Option<&'a Values>,
    pub stale: bool,
}

pub fn fmt(x: f64) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x.is_infinite() {
        return "inf".into();
    }
    let mag = if x == 0.0 {
        0.0
    } else {
        x.abs().log10().floor()
    };
    if !(-3.0..5.0).contains(&mag) {
        format!("{x:.3e}")
    } else {
        format!("{:.*}", (3.0 - mag).clamp(0.0, 6.0) as usize, x)
    }
}

fn current(v: &View, d: &VarDef) -> f64 {
    if d.output {
        v.out.map_or(f64::NAN, |o| o.g(&d.id))
    } else {
        v.inputs.g(&d.id)
    }
}

fn show(d: &VarDef, x: f64) -> String {
    if d.discrete() {
        d.choices[(x as usize).min(d.choices.len() - 1)].to_string()
    } else {
        format!("{} {}", fmt(x / d.scale), d.unit)
    }
}

fn tooltip(ui: &mut Ui, v: &View, d: &VarDef) {
    if !d.doc.is_empty() {
        ui.label(d.doc);
    }
    if !d.related.is_empty() {
        ui.separator();
        ui.label(RichText::new("Related").weak());
        for id in d.related {
            let r = v.m.def(id);
            ui.label(format!("{}: {}", r.label, show(r, current(v, r))));
        }
    }
    if !d.refs.is_empty() {
        ui.separator();
        for (name, url) in d.refs {
            ui.hyperlink_to(*name, *url);
        }
    }
}

pub fn row(ui: &mut Ui, v: &View, d: &VarDef, acts: &mut Vec<Action>) {
    let label = ui.label(d.label);
    label.on_hover_ui(|ui| tooltip(ui, v, d));
    let x = current(v, d);
    if d.output {
        let color = if x.is_nan() {
            egui::Color32::LIGHT_RED
        } else if v.stale {
            ui.visuals().weak_text_color()
        } else {
            ui.visuals().strong_text_color()
        };
        ui.label(RichText::new(show(d, x)).color(color));
    } else if d.discrete() {
        let mut k = (x as usize).min(d.choices.len() - 1);
        ComboBox::from_id_salt(&d.id)
            .selected_text(d.choices[k])
            .show_ui(ui, |ui| {
                for (n, name) in d.choices.iter().enumerate() {
                    if ui.selectable_value(&mut k, n, *name).clicked() {
                        acts.push(Action::Set(d.id.clone(), n as f64));
                    }
                }
            });
    } else {
        let mut y = x / d.scale;
        let slider = Slider::new(&mut y, d.min / d.scale..=d.max / d.scale)
            .logarithmic(d.spacing == Spacing::Log)
            .clamping(SliderClamping::Never)
            .suffix(format!(" {}", d.unit));
        if ui.add(slider).changed() {
            acts.push(Action::Set(d.id.clone(), y * d.scale));
        }
    }
    if !d.refs.is_empty() || !d.related.is_empty() {
        ui.menu_button("?", |ui| {
            tooltip(ui, v, d);
        });
    } else {
        ui.label("");
    }
    ui.end_row();
}

pub fn grid(ui: &mut Ui, v: &View, ids: &[&VarDef], name: &str, acts: &mut Vec<Action>) {
    egui::Grid::new(format!("{name}-grid"))
        .num_columns(3)
        .striped(true)
        .show(ui, |ui| {
            for d in ids
                .iter()
                .filter(|d| !v.m.hidden(v.inputs, v.m.index[&d.id]))
            {
                row(ui, v, d, acts);
            }
        });
}
