use crate::engine::{background::Latest, get::Get, model::Model, values::Values};
use crate::optimize::problem::{metrics, Objective};
use crate::system::outputs::evaluate;
use eframe::egui::{self, RichText, Ui};
use std::{sync::Arc, thread};

const STEP: f64 = 0.1;
const OUTPUTS: [(&str, &str); 4] = [
    ("lat.worst", "Lateral FWHM (worst)"),
    ("ax.diff_worst", "Axial FWHM, diffuse (worst)"),
    ("pr.soft", "Height noise (smooth worst)"),
    ("pr.snr_worst", "SNR (worst)"),
];

pub struct Row {
    id: String,
    label: &'static str,
    changes: Vec<f64>,
    cost: f64,
}

pub struct Sensitivity {
    latest: Latest<(String, Vec<Row>)>,
    submitted: String,
    shown: Vec<Row>,
}

impl Sensitivity {
    pub fn new() -> Self {
        Self {
            latest: Latest::new(),
            submitted: String::new(),
            shown: vec![],
        }
    }

    fn job(
        m: Arc<Model>,
        base: Values,
        key: String,
        objective: Objective,
    ) -> impl FnOnce() -> (String, Vec<Row>) + Send + 'static {
        move || {
            let knobs: Vec<usize> = (0..m.defs.len())
                .filter(|&i| !m.defs[i].output && !m.defs[i].discrete())
                .collect();
            let at = |i: usize, f: f64| {
                let d = &m.defs[i];
                let mut v = base.clone();
                v.v[i] = (base.v[i] * f).clamp(d.min, d.max);
                evaluate(&v)
            };
            let mid = evaluate(&base);
            let mut rows: Vec<Row> = thread::scope(|s| {
                let handles: Vec<_> = knobs
                    .iter()
                    .map(|&i| {
                        let (m, objective, mid, at) = (&m, &objective, &mid, &at);
                        s.spawn(move || {
                            let (up, down) = (at(i, 1.0 + STEP), at(i, 1.0 - STEP));
                            let change = |id: &str| {
                                (up.values.g(id) - down.values.g(id)) / (2.0 * mid.values.g(id))
                                    * 100.0
                            };
                            let cost = (objective.cost(&metrics(&up))
                                - objective.cost(&metrics(&down)))
                                / (2.0 * objective.cost(&metrics(mid)))
                                * 100.0;
                            Row {
                                id: m.defs[i].id.clone(),
                                label: m.defs[i].label,
                                changes: OUTPUTS.iter().map(|(id, _)| change(id)).collect(),
                                cost,
                            }
                        })
                    })
                    .collect();
                handles.into_iter().filter_map(|h| h.join().ok()).collect()
            });
            rows.sort_by(|a, b| b.cost.abs().total_cmp(&a.cost.abs()));
            (key, rows)
        }
    }

    pub fn ui(
        &mut self,
        ui: &mut Ui,
        m: &Arc<Model>,
        base: &Values,
        version: u64,
        objective: &Objective,
    ) {
        ui.weak("Percent change of each output for a +10 % change of each knob (central difference, other knobs held). Knobs are ranked by their effect on the optimiser's cost; the choice knobs (dispersion shape, aberrations on/off, temperature) are not listed.");
        let key = format!("{version}");
        if self.submitted != key {
            self.submitted = key.clone();
            let ctx = ui.ctx().clone();
            self.latest.submit(
                Self::job(m.clone(), base.clone(), key, objective.clone()),
                move || ctx.request_repaint(),
            );
        }
        if let Some((_, rows)) = self.latest.take() {
            self.shown = rows;
        }
        if self.latest.busy() {
            ui.weak("Computing…");
        }
        egui::Grid::new("sens").striped(true).show(ui, |ui| {
            ui.label(RichText::new("Knob").weak());
            for (_, name) in OUTPUTS {
                ui.label(RichText::new(name).weak());
            }
            ui.label(RichText::new("Optimiser cost").weak());
            ui.end_row();
            for r in &self.shown {
                ui.label(r.label).on_hover_text(&r.id);
                let max = r
                    .changes
                    .iter()
                    .chain([&r.cost])
                    .map(|c| c.abs())
                    .fold(0.0, f64::max)
                    .max(1e-9);
                for c in r.changes.iter().chain([&r.cost]) {
                    let strength = (c.abs() / max).sqrt();
                    ui.colored_label(
                        egui::Color32::from_rgb(
                            (120.0 + 135.0 * strength) as u8,
                            (120.0 + 80.0 * (1.0 - strength)) as u8,
                            120,
                        ),
                        format!("{c:+.1} %"),
                    );
                }
                ui.end_row();
            }
        });
    }
}
