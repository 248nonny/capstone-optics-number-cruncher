use super::inputs::Params;
use crate::engine::get::Get;
use crate::physics::{confocal::Bank, glass, radiometry as rad, spectral::qe};
use std::f64::consts::PI;

pub const LAM0: f64 = 550e-9;
pub const BAND: (f64, f64) = (450e-9, 670e-9);
pub const VALID: (f64, f64) = (400e-9, 800e-9);
pub const CENTER: f64 = 560e-9;
pub const N_PX: usize = 1024;
pub const BS_R: f64 = 0.5;
pub const BS_LOSS: f64 = 0.02;
pub const PROXIMITY: f64 = 0.95;
pub const AIRY_V: f64 = PI * 1.22;
const CROSSTALK: f64 = 0.2;
const CROSSTALK_SIGMA_PX: f64 = 4.0 / 7.5;

pub struct Dispersion {
    shape: usize,
    dz: f64,
}

impl Dispersion {
    fn focal_like(&self, l: f64) -> f64 {
        if self.shape == 0 { l } else { 1.0 / glass::power_ratio(self.shape - 1, l, LAM0) }
    }

    pub fn z(&self, l: f64) -> f64 {
        self.dz * (self.focal_like(l) - self.focal_like(BAND.0)) / (self.focal_like(BAND.1) - self.focal_like(BAND.0))
    }

    pub fn slope(&self, l: f64) -> f64 {
        (self.z(l + 1e-9) - self.z(l - 1e-9)) / 2e-9
    }

    pub fn na_ratio(&self, l: f64) -> f64 {
        if self.shape == 0 { 1.0 } else { glass::power_ratio(self.shape - 1, l, LAM0) }
    }
}

pub struct Setup {
    pub p: Params,
    pub disp: Dispersion,
    pub bank: Bank,
    pub lam: Vec<f64>,
    pub sigma_px: f64,
    radiance: f64,
    etendue: f64,
}

impl Setup {
    pub fn new(g: &dyn Get) -> Self {
        let p = Params::read(g);
        let vp0 = [AIRY_V * p.au, AIRY_V * p.au * p.ratio];
        let lam = (0..N_PX).map(|i| CENTER + (i as f64 - (N_PX as f64 - 1.0) / 2.0) * p.disp).collect();
        let sigma_px = ((p.res / 2.355 / p.disp).powi(2) + 1.0 / 12.0 + CROSSTALK * CROSSTALK_SIGMA_PX.powi(2)).sqrt();
        Self {
            disp: Dispersion { shape: p.shape, dz: p.dz },
            bank: Bank::new(vp0, LAM0, 430e-9, 700e-9),
            radiance: rad::radiance_of(rad::flux_at(rad::LED_FLUX_REF, p.current), rad::LED_SIDE),
            etendue: PI * PI / 4.0 * (1.22 * LAM0 * p.au).powi(2),
            lam,
            sigma_px,
            p,
        }
    }

    pub fn na(&self, l: f64) -> f64 {
        self.p.na * self.disp.na_ratio(l)
    }

    pub fn vp(&self, l: f64) -> [f64; 2] {
        [AIRY_V * self.p.au * LAM0 / l, AIRY_V * self.p.au * self.p.ratio * LAM0 / l]
    }

    pub fn strehl(&self, l: f64) -> f64 {
        if !self.p.aberrations {
            return 1.0;
        }
        let sigma = self.p.a * (self.na(l) / 0.1).powi(4) * LAM0 / l;
        (-(2.0 * PI * sigma).powi(2)).exp()
    }

    pub fn p_in(&self, l: f64) -> f64 {
        rad::per_nm(self.radiance, l) * self.etendue * PROXIMITY
    }

    pub fn gain(&self, l: f64, z: f64) -> f64 {
        let na = self.na(l);
        let u = 2.0 * PI * na * na * (z - self.disp.z(l)) / l;
        let s = self.strehl(l);
        self.p.r * (self.p.spec * self.bank.mirror_at(l, 2.0 * u) * s.powi(4) + (1.0 - self.p.spec) * na * na * self.bank.plane_at(l, u) * s.powi(2))
    }

    fn electrons_per_s(&self, i: usize, watts_per_nm: f64) -> f64 {
        let l = self.lam[i];
        watts_per_nm / rad::photon_energy(l) * self.p.disp * 1e9 * self.p.eff * qe().at(l * 1e9)
    }

    pub fn valid(&self, i: usize) -> bool {
        (VALID.0..=VALID.1).contains(&self.lam[i])
    }

    pub fn rate(&self, i: usize, z: f64) -> f64 {
        if !self.valid(i) {
            return 0.0;
        }
        let l = self.lam[i];
        let through = self.p.t * BS_R * self.p.t * (1.0 - BS_R - BS_LOSS);
        self.electrons_per_s(i, self.p_in(l) * through * self.gain(l, z))
    }

    pub fn stray(&self, i: usize) -> f64 {
        if !self.valid(i) {
            return 0.0;
        }
        self.electrons_per_s(i, self.p_in(self.lam[i]) * self.p.stray)
    }

    pub fn total_power(&self) -> f64 {
        (400..800).map(|l| self.p_in(l as f64 * 1e-9)).sum()
    }
}
