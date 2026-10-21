use crate::engine::get::Get;

pub const NOISE_V: [(f64, f64); 3] = [(1e6, 0.7e-3), (1e7, 1.9e-3), (5.4e7, 1.8e-3)];
pub const SWING_V: [(f64, f64); 3] = [(1e6, 2.0), (1e7, 1.5), (5.4e7, 0.5)];
pub const SE_NOISE: f64 = 0.5 / 0.7;
pub const SE_SWING: f64 = 0.8;
const CLOCKS: [f64; 3] = [1027.0, 515.0, 259.0];
const OVERHEAD_CLK: f64 = 32.0;
const OSC_HZ: f64 = 36e6;

fn log_interp(pts: &[(f64, f64)], x: f64) -> f64 {
    let x = x.clamp(pts[0].0, pts[pts.len() - 1].0).ln();
    let i = pts
        .windows(2)
        .position(|w| x <= w[1].0.ln())
        .unwrap_or(pts.len() - 2);
    let (a, b) = (pts[i], pts[i + 1]);
    a.1 + (b.1 - a.1) * (x - a.0.ln()) / (b.0.ln() - a.0.ln())
}

fn single_ended(g: &dyn Get) -> bool {
    g.g("det.mode") > 0.5
}

pub const CONVERSION_GAIN: f64 = 5e-6;
pub const DARK_V_PER_S: f64 = 0.8;
pub const MAX_READ_CLOCK: f64 = 54e6;

pub fn read_noise_at(fread: f64) -> f64 {
    log_interp(&NOISE_V, fread) / CONVERSION_GAIN
}

pub fn full_well_at(fread: f64) -> f64 {
    log_interp(&SWING_V, fread) / CONVERSION_GAIN
}

pub fn dark_rate_at(celsius: f64) -> f64 {
    DARK_V_PER_S / CONVERSION_GAIN * 2f64.powf((celsius - 25.0) / 8.0)
}

pub fn read_noise_e(g: &dyn Get) -> f64 {
    let k = if single_ended(g) { SE_NOISE } else { 1.0 };
    k * log_interp(&NOISE_V, g.g("det.fread")) / g.g("det.cg")
}

pub fn swing_v(g: &dyn Get) -> f64 {
    let k = if single_ended(g) { SE_SWING } else { 1.0 };
    k * log_interp(&SWING_V, g.g("det.fread"))
}

pub fn full_well_e(g: &dyn Get) -> f64 {
    let k = if single_ended(g) { SE_SWING } else { 1.0 };
    k * log_interp(&SWING_V, g.g("det.fread")) / g.g("det.cg")
}

pub fn dark_e_per_s(g: &dyn Get) -> f64 {
    g.g("det.dark") / g.g("det.cg") * 2f64.powf((g.g("det.temp") - 25.0) / 8.0)
}

pub fn lsb_e(g: &dyn Get) -> f64 {
    g.g("adc.range") / 2f64.powf(g.g("adc.bits")) / g.g("det.cg")
}

pub fn digital_noise_e(g: &dyn Get) -> f64 {
    if g.g("adc.on") < 0.5 {
        0.0
    } else {
        lsb_e(g) * (1.0 / 12.0 + g.g("adc.noise").powi(2)).sqrt()
    }
}

pub fn clocks(g: &dyn Get) -> f64 {
    CLOCKS[g.g("det.bin") as usize]
}

pub const CLOCKS_FULL: [f64; 3] = CLOCKS;
pub const CLOCKS_ROI: [f64; 3] = [515.0, 259.0, 131.0];

pub fn overhead_s() -> f64 {
    OVERHEAD_CLK / OSC_HZ
}
