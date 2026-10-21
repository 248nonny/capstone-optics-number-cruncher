use super::widgets::{grid, Action, View};
use crate::system::inputs::GROUPS;
use eframe::egui::{self, Ui};

const ADVANCED: [&str; 2] = ["opt.ratio", "opt.stray"];

pub fn ui(ui: &mut Ui, v: &View, acts: &mut Vec<Action>) {
    for (prefix, title) in GROUPS {
        egui::CollapsingHeader::new(title)
            .id_salt(prefix)
            .default_open(true)
            .show(ui, |ui| {
                let defs: Vec<_> =
                    v.m.defs
                        .iter()
                        .filter(|d| d.id.starts_with(&format!("{prefix}.")) && !d.output)
                        .collect();
                let (advanced, main): (Vec<_>, Vec<_>) = defs
                    .into_iter()
                    .partition(|d| ADVANCED.contains(&d.id.as_str()));
                grid(ui, v, &main, prefix, acts);
                if prefix == "smp" {
                    preset(ui, v, acts);
                }
                if !advanced.is_empty() {
                    egui::CollapsingHeader::new("Advanced")
                        .id_salt(format!("{prefix}-adv"))
                        .show(ui, |ui| {
                            grid(ui, v, &advanced, &format!("{prefix}-adv"), acts)
                        });
                }
            });
    }
}

fn preset(ui: &mut Ui, v: &View, acts: &mut Vec<Action>) {
    egui::ComboBox::from_id_salt("sample-preset")
        .selected_text("Sample preset…")
        .show_ui(ui, |ui| {
            for c in v.m.lib.of_kind("sample") {
                if ui.selectable_label(false, &c.name).clicked() {
                    acts.push(Action::Preset(c.name.clone()));
                }
            }
        });
}
