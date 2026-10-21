use crate::engine::{ctx::Ctx, get::Get};
use crate::physics::{confocal, geometry, signal::Signal, special::airy};
use eframe::egui::Ui;
use egui_plot::{Legend, Line, Plot, PlotPoints};
use std::{f64::consts::PI, rc::Rc};

pub const KINDS: [&str; 5] = ["Detector spectrum", "Axial response", "Focus vs wavelength", "Lateral PSF", "On-focus SNR"];

pub type Series = (String, Vec<[f64; 2]>);

pub fn show(ui: &mut Ui, id: &str, x_label: &str, y_label: &str, series: Vec<Series>) {
    Plot::new(id).legend(Legend::default()).x_axis_label(x_label).y_axis_label(y_label).height(ui.available_height().max(300.0)).show(ui, |p| {
        for (name, pts) in series {
            p.line(Line::new(name, PlotPoints::from(pts)));
        }
    });
}

fn xy(xs: impl Iterator<Item = f64>, f: impl Fn(f64) -> f64) -> Vec<[f64; 2]> {
    xs.map(|x| [x, f(x)]).collect()
}

fn band(s: &Signal, f: impl Fn(usize) -> f64) -> Vec<[f64; 2]> {
    (0..s.lam.len()).map(|i| [s.lam[i] * 1e9, f(i)]).collect()
}

fn spectrum(s: &Signal) -> (&'static str, &'static str, Vec<Series>) {
    ("wavelength (nm)", "electrons / pixel / exposure", vec![
        ("signal".into(), band(s, |i| s.sig[i])),
        ("background".into(), band(s, |i| s.bg[i])),
        ("noise sigma".into(), band(s, |i| s.var[i].sqrt())),
    ])
}

fn axial(c: &Ctx, s: &Signal) -> (&'static str, &'static str, Vec<Series>) {
    let ax = confocal::axial(s.vp[0], s.vp[1]);
    let lam = c.g("sys.lam0");
    let na = c.g("obj.na_ill").min(c.g("obj.na_det"));
    let to_um = lam / (2.0 * PI * na * na) * 1e6;
    let zs = || (-60..=60).map(|k| k as f64 * 0.5);
    ("height from focus (um)", "relative response", vec![
        ("mirror".into(), xy(zs(), |u| ax.mirror_at(2.0 * u) / ax.mirror[0])),
        ("diffuse plane".into(), xy(zs(), |u| ax.plane_at(u) / ax.plane[0])),
    ].into_iter().map(|(n, p): (String, Vec<[f64; 2]>)| (n, p.into_iter().map(|[u, y]| [u * to_um, y]).collect())).collect())
}

fn focus(s: &Signal) -> (&'static str, &'static str, Vec<Series>) {
    ("wavelength (nm)", "focus position z (um)", vec![("z_eff".into(), band(s, |i| s.z_eff[i] * 1e6))])
}

fn snr(s: &Signal) -> (&'static str, &'static str, Vec<Series>) {
    ("wavelength (nm)", "peak SNR if in focus at this wavelength", vec![
        ("SNR".into(), band(s, |i| s.snr_curve[i])),
        ("threshold".into(), band(s, |_| crate::physics::signal::SNR_TARGET)),
    ])
}

fn lateral(c: &Ctx) -> (&'static str, &'static str, Vec<Series>) {
    let (lam, na, radius) = (c.g("sys.lam0"), geometry::nas(c), geometry::pinhole_radii(c));
    let step = confocal::lateral_step(lam, na);
    let p = confocal::lateral_profile(lam, na, radius, step, 200);
    let sym = |f: &dyn Fn(usize) -> f64| (-199i32..200).map(|k| [k as f64 * step * 1e6, f(k.unsigned_abs() as usize)]).collect();
    ("lateral position (um)", "relative intensity", vec![
        ("confocal, finite pinholes".into(), sym(&|k| p[k] / p[0])),
        ("wide-field Airy".into(), sym(&|k| airy(2.0 * PI * na[0] * k as f64 * step / lam))),
        ("confocal, point pinholes".into(), sym(&|k| airy(2.0 * PI * na[0] * k as f64 * step / lam).powi(2))),
    ])
}

pub fn draw(ui: &mut Ui, kind: usize, c: &Ctx, s: &Rc<Signal>) {
    let (x, y, series) = match kind {
        0 => spectrum(s),
        1 => axial(c, s),
        2 => focus(s),
        3 => lateral(c),
        _ => snr(s),
    };
    show(ui, KINDS[kind], x, y, series);
}
