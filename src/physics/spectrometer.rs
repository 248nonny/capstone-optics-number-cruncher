use super::radiometry::photon_energy;
use crate::engine::get::Get;

pub const N_PX: usize = 1024;
pub const PITCH: f64 = 7.5e-6;

pub fn grating_dlam(g: &dyn Get) -> f64 {
    let (d, m) = (1.0 / g.g("spec.lines"), g.g("spec.order"));
    let sin_b = m * g.g("spec.lam_c") / d - g.g("spec.alpha").sin();
    d * (1.0 - sin_b * sin_b).max(0.0).sqrt() * PITCH / (m * g.g("spec.f_cam"))
}

pub fn lam_px(g: &dyn Get, i: usize) -> f64 {
    g.g("spec.lam_c") + (i as f64 - (N_PX as f64 - 1.0) / 2.0) * g.g("spec.dlam")
}

pub fn slit_px(g: &dyn Get) -> f64 {
    g.g("pin2.d") * g.g("spec.f_cam") / g.g("spec.f_col") / PITCH
}

pub fn resolution_sigma_px(g: &dyn Get) -> f64 {
    let dlam = g.g("spec.dlam");
    let diff_px = g.g("spec.lam_c") / (g.g("spec.order") * g.g("spec.lines") * g.g("spec.beam")) / dlam;
    let rect = (slit_px(g).powi(2) + 1.0) / 12.0;
    (rect + (diff_px / 2.355).powi(2) + (g.g("spec.aber") / 2.355).powi(2)).sqrt()
}

pub fn grating_eff(g: &dyn Get, lam: f64) -> f64 {
    g.g("spec.eta") * (-((lam - g.g("spec.blaze")) / g.g("spec.eta_w")).powi(2)).exp()
}

pub fn blur(v: &[f64], sigma: f64) -> Vec<f64> {
    let r = (4.0 * sigma).ceil().max(1.0) as isize;
    let k: Vec<f64> = (-r..=r).map(|d| (-0.5 * (d as f64 / sigma).powi(2)).exp()).collect();
    let norm: f64 = k.iter().sum();
    (0..v.len() as isize)
        .map(|i| (-r..=r).filter_map(|d| v.get((i + d) as usize).map(|x| x * k[(d + r) as usize])).sum::<f64>() / norm)
        .collect()
}

pub fn photons_per_nm_to_electrons(w_per_nm: f64, lam: f64, dlam_nm: f64, t: f64, qe: f64) -> f64 {
    w_per_nm / photon_energy(lam) * dlam_nm * t * qe
}
