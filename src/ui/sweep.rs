use super::plots::Series;
use crate::engine::{ctx::{Ctx, Model}, get::Get, var::Spacing};
use eframe::egui::{self, ComboBox, Ui};
use std::collections::HashMap;

pub struct Sweep {
    pub x: String,
    pub lo: f64,
    pub hi: f64,
    pub n: usize,
    pub ys: Vec<String>,
    cache: Option<(u64, String, Vec<Series>)>,
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
            cache: None,
        }
    }

    fn run(&self, m: &Model) -> Vec<Series> {
        let d = m.def(&self.x);
        let log = d.spacing == Spacing::Log && self.lo > 0.0;
        let xs: Vec<f64> = (0..self.n)
            .map(|k| {
                let t = k as f64 / (self.n - 1) as f64;
                if log { self.lo * (self.hi / self.lo).powf(t) } else { self.lo + (self.hi - self.lo) * t }
            })
            .collect();
        let rows: Vec<Ctx> = xs.iter().map(|&x| Ctx::with(m, HashMap::from([(self.x.clone(), x * d.scale)]))).collect();
        self.ys
            .iter()
            .map(|y| {
                let yd = m.def(y);
                (format!("{} ({})", yd.label, yd.unit), xs.iter().zip(&rows).map(|(&x, c)| [x, c.g(y) / yd.scale]).collect())
            })
            .collect()
    }

    pub fn ui(&mut self, ui: &mut Ui, m: &Model, version: u64) {
        let key = format!("{}{}{}{}{:?}", self.x, self.lo, self.hi, self.n, self.ys);
        ui.horizontal_wrapped(|ui| {
            ui.label("X:");
            ComboBox::from_id_salt("sweep-x").width(260.0).selected_text(format!("{} ({})", m.def(&self.x).label, self.x)).show_ui(ui, |ui| {
                for d in m.defs.iter().filter(|d| !d.discrete() && !d.output) {
                    if ui.selectable_value(&mut self.x, d.id.clone(), format!("{} ({})", d.label, d.id)).clicked() {
                        self.lo = d.min.max(d.default * 0.2) / d.scale;
                        self.hi = d.max.min(d.default * 5.0) / d.scale;
                    }
                }
            });
            let unit = m.def(&self.x).unit;
            ui.add(egui::DragValue::new(&mut self.lo).speed(0.1).suffix(format!(" {unit}")));
            ui.label("to");
            ui.add(egui::DragValue::new(&mut self.hi).speed(0.1).suffix(format!(" {unit}")));
            ui.add(egui::Slider::new(&mut self.n, 5..=200).text("points"));
        });
        egui::CollapsingHeader::new(format!("Outputs to plot ({})", self.ys.len())).show(ui, |ui| {
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
        });
        if self.cache.as_ref().is_none_or(|(v, k, _)| *v != version || *k != key) {
            self.cache = Some((version, key, if self.n > 1 && self.lo != self.hi { self.run(m) } else { vec![] }));
        }
        let unit = m.def(&self.x).unit;
        let series = self.cache.as_ref().map(|c| c.2.clone()).unwrap_or_default();
        let per = (ui.available_height() / series.len().max(1) as f32).max(180.0);
        egui::ScrollArea::vertical().show(ui, |ui| {
            for s in series {
                let id = format!("sweep-{}", s.0);
                egui_plot::Plot::new(id).height(per).x_axis_label(format!("{} ({unit})", m.def(&self.x).label)).legend(egui_plot::Legend::default()).show(ui, |p| {
                    p.line(egui_plot::Line::new(s.0, egui_plot::PlotPoints::from(s.1)));
                });
            }
        });
    }
}
