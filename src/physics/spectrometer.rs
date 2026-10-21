use super::{dispersion::f_obj, geometry, radiometry::photon_energy};
use crate::engine::get::Get;

pub const N_PX: usize = 1024;
pub const PITCH: f64 = 7.5e-6;
const CROSSTALK: f64 = 0.2;
const CROSSTALK_SIGMA: f64 = 4e-6;

pub fn grating_dlam(g: &dyn Get) -> f64 {
    let (d, m) = (1.0 / g.g("spec.lines"), g.g("spec.order"));
    let sin_b = m * g.g("spec.lam_c") / d - g.g("spec.alpha").sin();
    d * (1.0 - sin_b * sin_b).max(0.0).sqrt() * PITCH / (m * g.g("spec.f_cam"))
}

pub fn lam_px(g: &dyn Get, i: usize) -> f64 {
    g.g("spec.lam_c") + (i as f64 - (N_PX as f64 - 1.0) / 2.0) * g.g("spec.dlam")
}

pub fn beam_width(g: &dyn Get) -> f64 {
    let na_pin2 = geometry::na_det(g) * f_obj(g, g.g("sys.lam0")) / g.g("dcoll.f");
    g.g("spec.width").min(2.0 * g.g("spec.f_col") * na_pin2)
}

pub fn slit_px(g: &dyn Get) -> f64 {
    g.g("pin2.d") * g.g("spec.f_cam") / g.g("spec.f_col") / PITCH
}

pub fn resolution_terms(g: &dyn Get) -> [(&'static str, f64); 4] {
    let illuminated = beam_width(g) / g.g("spec.alpha").cos();
    let diff_px = g.g("spec.lam_c")
        / (g.g("spec.order") * g.g("spec.lines") * illuminated)
        / g.g("spec.dlam");
    [
        ("Pinhole 2 image (projected disk)", slit_px(g) / 4.0),
        (
            "Pixel width and charge crosstalk",
            (1.0 / 12.0 + CROSSTALK * (CROSSTALK_SIGMA / PITCH).powi(2)).sqrt(),
        ),
        ("Grating diffraction limit", diff_px / 2.355),
        ("Optical aberration", g.g("spec.aber") / 2.355),
    ]
}

pub fn resolution_sigma_px(g: &dyn Get) -> f64 {
    resolution_terms(g)
        .iter()
        .map(|t| t.1 * t.1)
        .sum::<f64>()
        .sqrt()
}

pub fn grating_eff(g: &dyn Get, lam: f64) -> f64 {
    g.g("spec.eta") * (-((lam - g.g("spec.blaze")) / g.g("spec.eta_w")).powi(2)).exp()
}

pub fn blur(v: &[f64], sigma: f64) -> Vec<f64> {
    let r = (4.0 * sigma).ceil().max(1.0) as isize;
    let k: Vec<f64> = (-r..=r)
        .map(|d| (-0.5 * (d as f64 / sigma).powi(2)).exp())
        .collect();
    (0..v.len() as isize)
        .map(|i| {
            let used = (-r..=r).filter(|d| (0..v.len() as isize).contains(&(i + d)));
            let (num, norm) = used.fold((0.0, 0.0), |(n, w), d| {
                (
                    n + v[(i + d) as usize] * k[(d + r) as usize],
                    w + k[(d + r) as usize],
                )
            });
            num / norm
        })
        .collect()
}

pub fn rebin(v: &[f64], b: usize) -> Vec<f64> {
    v.chunks(b)
        .map(|c| c.iter().sum::<f64>() / c.len() as f64)
        .collect()
}

pub fn photons_per_nm_to_electrons(w_per_nm: f64, lam: f64, dlam_nm: f64, t: f64, qe: f64) -> f64 {
    w_per_nm / photon_energy(lam) * dlam_nm * t * qe
}
