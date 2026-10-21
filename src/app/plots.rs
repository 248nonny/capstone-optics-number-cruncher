use crate::engine::values::Values;
use crate::physics::{confocal, special::airy};
use crate::system::{optics::LAM0, outputs::{self, Report}, readout::Node};
use eframe::egui::{self, Ui};
use egui_plot::{Legend, Line, Plot, PlotPoints, Points};
use std::f64::consts::PI;

pub const KINDS: [&str; 5] = ["Performance vs depth", "Detector spectrum", "Focus and NA", "Axial response", "Lateral spot"];

pub struct PlotState {
    pub kind: usize,
    pub node: usize,
    pub fraction: f64,
}

impl PlotState {
    pub fn new(kind: usize) -> Self {
        Self { kind, node: 4, fraction: 0.5 }
    }
}

struct Series {
    name: &'static str,
    pts: Vec<[f64; 2]>,
    markers: bool,
}

fn line(name: &'static str, pts: Vec<[f64; 2]>) -> Series {
    Series { name, pts, markers: false }
}

fn dots(name: &'static str, pts: Vec<[f64; 2]>) -> Series {
    Series { name, pts, markers: true }
}

fn plot(ui: &mut Ui, id: &str, x: &str, y: &str, height: f32, series: Vec<Series>) {
    Plot::new(id).allow_scroll(false).height(height).legend(Legend::default()).x_axis_label(x).y_axis_label(y).show(ui, |p| {
        for s in series {
            if s.markers {
                p.points(Points::new(s.name, PlotPoints::from(s.pts)).radius(3.0_f32));
            } else {
                p.line(Line::new(s.name, PlotPoints::from(s.pts)));
            }
        }
    });
}

fn over_depth(r: &Report, f: impl Fn(&Node) -> f64) -> Vec<[f64; 2]> {
    r.nodes.iter().map(|n| [n.z * 1e6, f(n)]).collect()
}

fn depth(ui: &mut Ui, r: &Report) {
    let nodes = &r.nodes;
    let a = &r.assessment;
    let h = 230.0;
    plot(ui, "p-lat", "height (µm)", "lateral FWHM (µm)", h, vec![line("confocal", over_depth(r, |n| n.lateral * 1e6)), dots("nodes", over_depth(r, |n| n.lateral * 1e6))]);
    plot(ui, "p-ax", "height (µm)", "axial FWHM (µm)", h, vec![line("mirror", over_depth(r, |n| n.ax_spec * 1e6)), line("diffuse", over_depth(r, |n| n.ax_diff * 1e6))]);
    let series = |v: &[f64], scale: f64| nodes.iter().zip(v).map(|(n, x)| [n.z * 1e6, x * scale]).collect::<Vec<_>>();
    plot(ui, "p-sig", "height (µm)", "height noise σ_z (nm), lower is better", h, vec![line("Cramér–Rao", series(&a.sigma, 1e9)), dots("nodes", series(&a.sigma, 1e9))]);
    plot(ui, "p-snr", "height (µm)", "peak SNR", h, vec![line("SNR", series(&a.snr, 1.0)), line("threshold 10", nodes.iter().map(|n| [n.z * 1e6, 10.0]).collect())]);
}

fn spectrum(ui: &mut Ui, s: &mut PlotState, inputs: &Values, r: &Report) {
    ui.add(egui::Slider::new(&mut s.fraction, 0.0..=1.0).text("surface height (fraction of the depth range)"));
    let sp = outputs::spectrum(inputs, r, s.fraction);
    let nm = |v: &[f64]| sp.lam.iter().zip(v).map(|(l, x)| [l * 1e9, *x]).collect::<Vec<_>>();
    plot(ui, "p-spec", "wavelength (nm)", "electrons per pixel per exposure", 420.0, vec![line("signal", nm(&sp.signal)), line("background", nm(&sp.background)), line("noise σ", nm(&sp.noise))]);
}

fn focus(ui: &mut Ui, r: &Report) {
    plot(ui, "p-z", "wavelength (nm)", "focus height (µm)", 260.0, vec![line("z(λ)", r.nodes.iter().map(|n| [n.lam * 1e9, n.z * 1e6]).collect()), dots("nodes", r.nodes.iter().map(|n| [n.lam * 1e9, n.z * 1e6]).collect())]);
    plot(ui, "p-na", "wavelength (nm)", "NA", 260.0, vec![line("NA(λ)", r.nodes.iter().map(|n| [n.lam * 1e9, n.na]).collect())]);
}

fn node_picker(ui: &mut Ui, s: &mut PlotState, r: &Report) {
    let n = &r.nodes[s.node];
    ui.add(egui::Slider::new(&mut s.node, 0..=r.nodes.len() - 1).text(format!("wavelength node: {:.0} nm, height {:.0} µm", n.lam * 1e9, n.z * 1e6)));
}

fn axial(ui: &mut Ui, s: &mut PlotState, inputs: &Values, r: &Report) {
    node_picker(ui, s, r);
    let n = &r.nodes[s.node];
    let setup = crate::system::optics::Setup::new(inputs);
    let vp = setup.vp(n.lam);
    let ax = confocal::axial(vp[0], vp[1]);
    let to_z = n.lam / (2.0 * PI * n.na * n.na) * 1e6;
    let zs = || (-70..=70).map(|k| k as f64 * 0.5);
    let curve = |f: &dyn Fn(f64) -> f64| zs().map(|u| [u * to_z, f(u)]).collect::<Vec<_>>();
    plot(ui, "p-axial", "height from focus (µm)", "relative response", 420.0, vec![
        line("mirror", curve(&|u| ax.mirror_at(2.0 * u) / ax.mirror[0])),
        line("diffuse plane", curve(&|u| ax.plane_at(u) / ax.plane[0])),
    ]);
}

fn lateral(ui: &mut Ui, s: &mut PlotState, r: &Report, inputs: &Values) {
    node_picker(ui, s, r);
    let n = &r.nodes[s.node];
    let setup = crate::system::optics::Setup::new(inputs);
    let vp = setup.vp(n.lam);
    let radius = |v: f64| v * n.lam / (2.0 * PI * n.na);
    let step = confocal::lateral_step(n.lam, [n.na, n.na]);
    let p = confocal::lateral_profile(n.lam, [n.na, n.na], [radius(vp[0]), radius(vp[1])], step, 200);
    let sym = |f: &dyn Fn(usize) -> f64| (-199i32..200).map(|k| [k as f64 * step * 1e6, f(k.unsigned_abs() as usize)]).collect::<Vec<_>>();
    let a = |k: usize| airy(2.0 * PI * n.na * k as f64 * step / n.lam);
    plot(ui, "p-lateral", "lateral position (µm)", "relative intensity", 420.0, vec![
        line("confocal, finite pinholes", sym(&|k| p[k] / p[0])),
        line("wide-field Airy", sym(&a)),
        line("confocal, point pinholes", sym(&|k| a(k).powi(2))),
    ]);
}

pub fn ui(ui: &mut Ui, s: &mut PlotState, inputs: &Values, r: &Report) {
    ui.horizontal_wrapped(|ui| {
        for (k, name) in KINDS.iter().enumerate() {
            ui.selectable_value(&mut s.kind, k, *name);
        }
    });
    let _ = LAM0;
    match s.kind {
        0 => depth(ui, r),
        1 => spectrum(ui, s, inputs, r),
        2 => focus(ui, r),
        3 => axial(ui, s, inputs, r),
        _ => lateral(ui, s, r, inputs),
    }
}
