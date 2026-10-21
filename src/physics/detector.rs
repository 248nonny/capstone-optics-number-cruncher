pub const NOISE_V: [(f64, f64); 3] = [(1e6, 0.7e-3), (1e7, 1.9e-3), (5.4e7, 1.8e-3)];
pub const SWING_V: [(f64, f64); 3] = [(1e6, 2.0), (1e7, 1.5), (5.4e7, 0.5)];
pub const CONVERSION_GAIN: f64 = 5e-6;
pub const DARK_V_PER_S: f64 = 0.8;
pub const MAX_READ_CLOCK: f64 = 54e6;
pub const CLOCKS_FULL: [f64; 3] = [1027.0, 515.0, 259.0];
pub const CLOCKS_ROI: [f64; 3] = [515.0, 259.0, 131.0];
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

pub fn read_noise_at(fread: f64) -> f64 {
    log_interp(&NOISE_V, fread) / CONVERSION_GAIN
}

pub fn full_well_at(fread: f64) -> f64 {
    log_interp(&SWING_V, fread) / CONVERSION_GAIN
}

pub fn dark_rate_at(celsius: f64) -> f64 {
    DARK_V_PER_S / CONVERSION_GAIN * 2f64.powf((celsius - 25.0) / 8.0)
}

pub fn overhead_s() -> f64 {
    OVERHEAD_CLK / OSC_HZ
}
