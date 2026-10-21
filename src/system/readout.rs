use super::optics::{Setup, BAND, CENTER, N_PX};
use crate::physics::{confocal::lateral_fwhm, detector, spectrometer::blur};
use std::f64::consts::PI;

pub const NODES: usize = 9;
const FILL: f64 = 0.8;
const PRNU: f64 = 0.005;
const WINDOW: f64 = 2.5;

pub struct Node {
    pub lam: f64,
    pub z: f64,
    pub na: f64,
    pub strehl: f64,
    pub lateral: f64,
    pub ax_spec: f64,
    pub ax_diff: f64,
    pub peak_nm: f64,
    pub first: usize,
    pub rate: Vec<f64>,
    pub drate: Vec<f64>,
    pub stray: Vec<f64>,
}

fn centre_pixel(s: &Setup, lam: f64) -> f64 {
    (lam - CENTER) / s.p.disp + (N_PX as f64 - 1.0) / 2.0
}

fn profile(s: &Setup, ext: std::ops::Range<usize>, z: f64) -> Vec<f64> {
    blur(&ext.map(|i| s.rate(i, z)).collect::<Vec<_>>(), s.sigma_px)
}

pub fn node(s: &Setup, k: usize) -> Node {
    node_with(s, k, WINDOW)
}

pub fn node_with(s: &Setup, k: usize, window: f64) -> Node {
    let lam = BAND.0 + (BAND.1 - BAND.0) * k as f64 / (NODES - 1) as f64;
    let (na, z) = (s.na(lam), s.disp.z(lam));
    let vp = s.vp(lam);
    let radius = |v: f64| v * lam / (2.0 * PI * na);
    let lateral = lateral_fwhm(lam, [na, na], [radius(vp[0]), radius(vp[1])]);
    let to_z = lam / (2.0 * PI * na * na);
    let (ax_spec, ax_diff) = (s.bank.mirror_fwhm_u(lam) * to_z / 2.0, s.bank.plane_fwhm_u(lam) * to_z);
    let peak_nm = ax_spec.max(ax_diff) / s.disp.slope(lam).abs();
    let width_px = peak_nm.hypot(s.p.res) / s.p.disp;
    let half = (window * width_px).ceil().max(10.0) as i64;
    let (margin, centre) = ((4.0 * s.sigma_px).ceil() as i64 + 1, centre_pixel(s, lam).round() as i64);
    let clip = |i: i64| i.clamp(0, N_PX as i64 - 1) as usize;
    let (first, last) = (clip(centre - half), clip(centre + half));
    let empty = centre + half < 0 || centre - half >= N_PX as i64;
    let ext = clip(first as i64 - margin)..clip(last as i64 + margin) + 1;
    let (skip, len) = (first - ext.start, if empty { 0 } else { last - first + 1 });
    let h = 0.02 * ax_spec;
    let cut = |v: Vec<f64>| v[skip..skip + len].to_vec();
    let (up, down) = (profile(s, ext.clone(), z + h), profile(s, ext.clone(), z - h));
    let rate = cut(profile(s, ext.clone(), z));
    let drate = (0..len).map(|i| (up[skip + i] - down[skip + i]) / (2.0 * h)).collect();
    Node { lam, z, na, strehl: s.strehl(lam), lateral, ax_spec, ax_diff, peak_nm, first, rate, drate, stray: (first..first + len).map(|i| s.stray(i)).collect() }
}

#[derive(Clone, Copy)]
pub struct Readout {
    pub bin: usize,
    pub roi: bool,
    pub clocks: f64,
    pub fread: f64,
    pub texp: f64,
}

pub struct Assessment {
    pub readout: Readout,
    pub feasible: bool,
    pub sigma: Vec<f64>,
    pub snr: Vec<f64>,
    pub peak_e: Vec<f64>,
    pub fill: f64,
}

fn candidates(rate: f64) -> Vec<(usize, bool, f64, f64)> {
    let mut all = vec![];
    for roi in [false, true] {
        for bin in 0..3 {
            let clocks = if roi { detector::CLOCKS_ROI[bin] } else { detector::CLOCKS_FULL[bin] };
            all.push((bin, roi, clocks, 1.1 * clocks * rate));
        }
    }
    all
}

pub fn assess(s: &Setup, nodes: &[Node], bin: usize, roi: bool, clocks: f64, fread: f64) -> Assessment {
    let dark = detector::dark_rate_at(s.p.celsius);
    let peak_rate = nodes.iter().flat_map(|n| n.rate.iter().zip(&n.stray).map(|(a, b)| a + b)).fold(0.0, f64::max) + dark;
    let period = 1.0 / s.p.rate - detector::overhead_s();
    let full_well = detector::full_well_at(fread);
    let texp = period.min(FILL * full_well / peak_rate.max(1e-30));
    let (rn, b) = (detector::read_noise_at(fread), 1usize << bin);
    let (r0, r1) = if roi { (N_PX / 4, 3 * N_PX / 4) } else { (0, N_PX) };
    let (mut sigma, mut snr, mut peak_e) = (vec![], vec![], vec![]);
    for n in nodes {
        let lo = n.first.max(r0).next_multiple_of(b).max(r0);
        let hi = (n.first + n.rate.len()).min(r1);
        let (mut fisher, mut best, mut signal) = (0.0, (0.0, 1.0), 0.0);
        let mut i = lo;
        while i + b <= hi {
            let range = (i - n.first)..(i - n.first + b);
            let mean = |v: &[f64]| v[range.clone()].iter().sum::<f64>() / b as f64;
            let (m, dm, bg) = (mean(&n.rate) * texp, mean(&n.drate) * texp, (mean(&n.stray) + dark) * texp);
            let var = (m + bg + (PRNU * m).powi(2)) / b as f64 + rn * rn;
            fisher += dm * dm / var;
            signal += m * b as f64;
            if m > best.0 {
                best = (m, var);
            }
            i += b;
        }
        sigma.push(if fisher > 0.0 { 1.0 / fisher.sqrt() } else { f64::INFINITY });
        snr.push(best.0 / best.1.sqrt());
        peak_e.push(signal);
    }
    Assessment { readout: Readout { bin, roi, clocks, fread, texp }, feasible: fread <= detector::MAX_READ_CLOCK, sigma, snr, peak_e, fill: (texp * peak_rate / full_well).min(1.0) }
}

fn worst(a: &Assessment) -> f64 {
    a.sigma.iter().cloned().fold(0.0, f64::max)
}

pub fn best(s: &Setup, nodes: &[Node]) -> Assessment {
    let feasible: Vec<_> = candidates(s.p.rate).into_iter().filter(|c| c.3 <= detector::MAX_READ_CLOCK).collect();
    let pool = if feasible.is_empty() { vec![(2, true, detector::CLOCKS_ROI[2], detector::MAX_READ_CLOCK)] } else { feasible };
    pool.into_iter()
        .map(|(bin, roi, clocks, fread)| assess(s, nodes, bin, roi, clocks, fread))
        .min_by(|a, b| worst(a).total_cmp(&worst(b)))
        .expect("at least one read-out candidate")
}
