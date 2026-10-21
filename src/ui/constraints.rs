use super::view::{Action, View};
use eframe::egui::{self, ComboBox, RichText, Ui};

pub struct LinkPicker {
    a: String,
    b: String,
}

impl LinkPicker {
    pub fn new() -> Self {
        Self { a: "dcoll.f".into(), b: "coll.f".into() }
    }
}

fn status(v: &View, k: usize) -> Option<&'static str> {
    let f = &v.m.formulas[k];
    if !v.s.active[k] {
        return None;
    }
    if v.s.conflicts.contains(&f.title) {
        return Some("over-constrained");
    }
    (f.free == 1 && v.s.locked[v.m.index[&f.vars[0]]].is_some()).then_some("overridden by a component")
}

fn formula(ui: &mut Ui, v: &View, k: usize, acts: &mut Vec<Action>) {
    let f = &v.m.formulas[k];
    ui.horizontal(|ui| {
        let mut on = v.s.active[k];
        if ui.add_enabled(!f.always, egui::Checkbox::new(&mut on, "")).changed() {
            acts.push(Action::Toggle(k, on));
        }
        egui::CollapsingHeader::new(RichText::new(&f.title).strong()).id_salt(&f.id).show(ui, |ui| {
            ui.label(RichText::new(&f.eq).monospace().size(15.0));
            ui.label(&f.desc);
            let free: Vec<_> = f.vars.iter().take(f.free).map(|x| v.m.def(x).label).collect();
            ui.weak(format!("Gives way, in order: {}", free.join(" → ")));
            for (name, url) in f.refs {
                ui.hyperlink_to(*name, *url);
            }
        });
        if let Some(s) = status(v, k) {
            ui.colored_label(egui::Color32::GOLD, format!("⚠ {s}"));
        }
    });
}

pub fn ui(ui: &mut Ui, v: &View, picker: &mut LinkPicker, acts: &mut Vec<Action>) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.heading("Always on");
        ui.weak("Definitions that keep the sliders physically consistent. Dragging any variable moves the next one in its give-way order.");
        for k in (0..v.m.formulas.len()).filter(|&k| v.m.formulas[k].always) {
            formula(ui, v, k, acts);
        }
        ui.heading("Design constraints");
        ui.weak("Tick to impose. A constraint that sets one variable locks that slider until it is unticked or a component overrides it.");
        for k in (0..v.m.formulas.len()).filter(|&k| !v.m.formulas[k].always) {
            formula(ui, v, k, acts);
        }
        ui.heading("Custom links");
        ui.weak("Make any two parameters equal (for example the same lens on both sides). Dragging either moves the other.");
        for (k, (a, b)) in v.s.links.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(format!("{} = {}", v.m.def(a).label.to_string() + " (" + a + ")", v.m.def(b).label.to_string() + " (" + b + ")"));
                if ui.small_button("Remove").clicked() {
                    acts.push(Action::RemoveLink(k));
                }
            });
        }
        ui.horizontal(|ui| {
            for (id, tag) in [(&mut picker.a, "a"), (&mut picker.b, "b")] {
                ComboBox::from_id_salt(format!("link-{tag}")).width(220.0).selected_text(format!("{} ({id})", v.m.def(id).label)).show_ui(ui, |ui| {
                    for d in v.m.defs.iter().filter(|d| !d.output && !d.discrete()) {
                        ui.selectable_value(id, d.id.clone(), format!("{} ({})", d.label, d.id));
                    }
                });
                if tag == "a" {
                    ui.label("=");
                }
            }
            if ui.button("Add link").clicked() {
                acts.push(Action::AddLink(picker.a.clone(), picker.b.clone()));
            }
        });
        if !v.s.conflicts.is_empty() {
            ui.heading("Conflicts");
            for c in &v.s.conflicts {
                ui.colored_label(egui::Color32::GOLD, format!("⚠ {c}: no free variable left to satisfy it"));
            }
        }
    });
}
