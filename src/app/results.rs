use super::widgets::{grid, View};
use eframe::egui::Ui;

const SECTIONS: [(&str, &str); 5] = [
    ("lat", "Lateral resolution"),
    ("ax", "Axial resolution"),
    ("pr", "Noise"),
    ("ph", "Light"),
    ("acq", "Acquisition (automatic)"),
];

pub fn ui(ui: &mut Ui, v: &View) {
    for (prefix, title) in SECTIONS {
        ui.heading(title);
        let defs: Vec<_> =
            v.m.defs
                .iter()
                .filter(|d| d.output && d.id.starts_with(&format!("{prefix}.")))
                .collect();
        grid(ui, v, &defs, prefix, &mut vec![]);
    }
}
