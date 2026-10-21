use super::spectral::{led, vlambda};
use std::sync::OnceLock;

pub const H: f64 = 6.62607015e-34;
pub const C: f64 = 2.99792458e8;
pub const LUMEN_PER_WATT: f64 = 683.0;
pub const FLUX_EXPONENT: f64 = 0.82;

pub fn photon_energy(lam: f64) -> f64 {
    H * C / lam
}

pub const LED_FLUX_REF: f64 = 122.0;
pub const LED_SIDE: f64 = 1.1e-3;

pub fn flux_at(flux_ref: f64, current: f64) -> f64 {
    flux_ref * (current / 0.35).powf(FLUX_EXPONENT)
}

fn mean_v() -> f64 {
    static V: OnceLock<f64> = OnceLock::new();
    *V.get_or_init(compute_mean_v)
}

fn led_norm() -> f64 {
    static N: OnceLock<f64> = OnceLock::new();
    *N.get_or_init(|| led().integral(400.0, 800.0, 400))
}

fn compute_mean_v() -> f64 {
    let (l, v) = (led(), vlambda());
    let n = 400;
    let at = |k: usize| 400.0 + (k as f64 + 0.5) * 400.0 / n as f64;
    let num: f64 = (0..n).map(|k| l.at(at(k)) * v.at(at(k))).sum();
    num / (0..n).map(|k| l.at(at(k))).sum::<f64>()
}

pub fn watts(lumens: f64) -> f64 {
    lumens / (LUMEN_PER_WATT * mean_v())
}

pub fn radiance_of(lumens: f64, side: f64) -> f64 {
    watts(lumens) / (std::f64::consts::PI * side * side)
}

pub fn per_nm(radiance: f64, lam: f64) -> f64 {
    radiance * led().at(lam * 1e9) / led_norm()
}
