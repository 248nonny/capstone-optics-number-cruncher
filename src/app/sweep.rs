use crate::engine::{background::Latest, model::Model, values::Values, var::Spacing};
use crate::system::outputs::evaluate;
use eframe::egui::{self, ComboBox, Ui};
use egui_plot::{Legend, Line, Plot, PlotPoints};
use std::sync::Arc;

type Series = (String, Vec<[f64; 2]>);

pub struct Sweep {
    x: String,
    lo: f64,
    hi: f64,
    n: usize,
    ys: Vec<String>,
    latest: Latest<(String, Vec<Series>)>,
    submitted: String,
    shown: (String, Vec<Series>),
}

impl Sweep {
    pub fn new(m: &Model) -> Self {
        let d = m.def("opt.au");
        Self {
            x: d.id.clone(),
            lo: 0.2,
            hi: 3.0,
            n: 30,
            ys: vec![
                "lat.worst".into(),
                "ax.diff_worst".into(),
                "pr.soft".into(),
                "pr.snr_worst".into(),
            ],
            latest: Latest::new(),
            submitted: String::new(),
            shown: Default::default(),
        }
    }

    fn job(
        &self,
        m: Arc<Model>,
        base: Values,
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
            let rows: Vec<Vec<f64>> = xs
                .iter()
                .map(|&v| {
                    let mut t = base.clone();
                    t.set(&x, v * d.scale);
                    let r = evaluate(&t);
                    ys.iter()
                        .map(|y| crate::engine::get::Get::g(&r.values, y))
                        .collect()
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

    pub fn ui(&mut self, ui: &mut Ui, m: &Arc<Model>, base: &Values, version: u64) {
        ui.horizontal_wrapped(|ui| {
            ui.label("X:");
            ComboBox::from_id_salt("sweep-x")
                .width(280.0)
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
                            self.lo = d.min.max(d.default * 0.25) / d.scale;
                            self.hi = d.max.min(d.default * 4.0) / d.scale;
                        }
                    }
                });
            let unit = m.def(&self.x).unit;
            ui.add(
                egui::DragValue::new(&mut self.lo)
                    .speed(0.05)
                    .suffix(format!(" {unit}")),
            );
            ui.label("to");
            ui.add(
                egui::DragValue::new(&mut self.hi)
                    .speed(0.05)
                    .suffix(format!(" {unit}")),
            );
            ui.add(egui::Slider::new(&mut self.n, 5..=100).text("points"));
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
                .submit(self.job(m.clone(), base.clone(), key), move || {
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
        for (name, pts) in &self.shown.1 {
            Plot::new(format!("sweep-{name}"))
                .allow_scroll(false)
                .height(260.0)
                .x_axis_label(format!("{} ({unit})", m.def(&self.x).label))
                .legend(Legend::default())
                .show(ui, |p| {
                    p.line(Line::new(name.clone(), PlotPoints::from(pts.clone())));
                });
        }
    }
}
