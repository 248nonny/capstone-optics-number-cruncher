use super::plots::Series;
use crate::engine::{background::Latest, model::Model, state::State, var::Spacing};
use crate::model::outputs;
use eframe::egui::{self, ComboBox, Ui};
use std::sync::Arc;

pub struct Sweep {
    pub x: String,
    pub lo: f64,
    pub hi: f64,
    pub n: usize,
    pub ys: Vec<String>,
    latest: Latest<(String, Vec<Series>)>,
    submitted: String,
    shown: (String, Vec<Series>),
}

impl Sweep {
    pub fn new(m: &Model) -> Self {
        let d = m.def("pin1.d");
        Self {
            x: d.id.clone(),
            lo: 5e-6 / d.scale,
            hi: 100e-6 / d.scale,
            n: 40,
            ys: vec!["lat.fwhm".into(), "ph.snr".into(), "pr.sigma_z".into()],
            latest: Latest::new(),
            submitted: String::new(),
            shown: Default::default(),
        }
    }

    fn job(
        &self,
        m: Arc<Model>,
        s: State,
        key: String,
    ) -> impl FnOnce() -> (String, Vec<Series>) + Send + 'static {
        let (x, lo, hi, n, ys) = (self.x.clone(), self.lo, self.hi, self.n, self.ys.clone());
        move || {
            let d = m.def(&x);
            let log = d.spacing == Spacing::Log && lo > 0.0;
            let xs: Vec<f64> = (0..n)
                .map(|k| {
                    let t = k as f64 / (n - 1) as f64;
                    if log {
                        lo * (hi / lo).powf(t)
                    } else {
                        lo + (hi - lo) * t
                    }
                })
                .collect();
            let ids: Vec<&str> = ys.iter().map(String::as_str).collect();
            let rows: Vec<Vec<f64>> = xs
                .iter()
                .map(|&v| {
                    let mut t = s.clone();
                    t.set(&m, &x, v * d.scale);
                    outputs::evaluate_ids(&t.effective(&m), &ids)
                })
                .collect();
            let series = ys
                .iter()
                .enumerate()
                .map(|(k, y)| {
                    let yd = m.def(y);
                    (
                        format!("{} ({})", yd.label, yd.unit),
                        xs.iter()
                            .zip(&rows)
                            .map(|(&x, r)| [x, r[k] / yd.scale])
                            .collect(),
                    )
                })
                .collect();
            (key, series)
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, m: &Arc<Model>, s: &State, version: u64) {
        ui.horizontal_wrapped(|ui| {
            ui.label("X:");
            ComboBox::from_id_salt("sweep-x")
                .width(260.0)
                .selected_text(format!("{} ({})", m.def(&self.x).label, self.x))
                .show_ui(ui, |ui| {
                    for d in m.defs.iter().filter(|d| !d.discrete() && !d.output) {
                        if ui
                            .selectable_value(
                                &mut self.x,
                                d.id.clone(),
                                format!("{} ({})", d.label, d.id),
                            )
                            .clicked()
                        {
                            self.lo = d.min.max(d.default * 0.2) / d.scale;
                            self.hi = d.max.min(d.default * 5.0) / d.scale;
                        }
                    }
                });
            let unit = m.def(&self.x).unit;
            ui.add(
                egui::DragValue::new(&mut self.lo)
                    .speed(0.1)
                    .suffix(format!(" {unit}")),
            );
            ui.label("to");
            ui.add(
                egui::DragValue::new(&mut self.hi)
                    .speed(0.1)
                    .suffix(format!(" {unit}")),
            );
            ui.add(egui::Slider::new(&mut self.n, 5..=200).text("points"));
        });
        egui::CollapsingHeader::new(format!("Outputs to plot ({})", self.ys.len())).show(
            ui,
            |ui| {
                ui.horizontal_wrapped(|ui| {
                    for d in m.defs.iter().filter(|d| d.output && !d.discrete()) {
                        let mut on = self.ys.contains(&d.id);
                        if ui.checkbox(&mut on, d.label).changed() {
                            self.ys.retain(|y| *y != d.id);
                            if on {
                                self.ys.push(d.id.clone());
                            }
                        }
                    }
                });
            },
        );
        let key = format!(
            "{version}|{}|{}|{}|{}|{:?}",
            self.x, self.lo, self.hi, self.n, self.ys
        );
        if self.submitted != key && self.lo != self.hi {
            self.submitted = key.clone();
            let ctx = ui.ctx().clone();
            self.latest
                .submit(self.job(m.clone(), s.clone(), key), move || {
                    ctx.request_repaint()
                });
        }
        if let Some(done) = self.latest.take() {
            self.shown = done;
        }
        if self.latest.busy() {
            ui.weak("Computing…");
        }
        let unit = m.def(&self.x).unit;
        {
            for (name, pts) in &self.shown.1 {
                egui_plot::Plot::new(format!("sweep-{name}"))
                    .allow_scroll(false)
                    .height(260.0)
                    .x_axis_label(format!("{} ({unit})", m.def(&self.x).label))
                    .legend(egui_plot::Legend::default())
                    .show(ui, |p| {
                        p.line(egui_plot::Line::new(
                            name.clone(),
                            egui_plot::PlotPoints::from(pts.clone()),
                        ));
                    });
            }
        }
    }
}
