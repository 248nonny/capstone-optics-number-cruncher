use super::{
    confocal, detector, dispersion as disp, geometry, halfmax, radiometry as rad,
    spectrometer as spec,
};
use crate::engine::get::Get;
use std::f64::consts::PI;

pub struct Signal {
    pub lam: Vec<f64>,
    pub sig: Vec<f64>,
    pub bg: Vec<f64>,
    pub z_eff: Vec<f64>,
    pub var: Vec<f64>,
    pub p_pin: f64,
    pub eta_p2: f64,
    pub strehl: f64,
    pub lam_peak: f64,
    pub peak_fwhm: f64,
    pub peak_e: f64,
    pub pix_max: f64,
    pub snr: f64,
    pub sat: f64,
    pub sigma_z: f64,
    pub sigma_lam: f64,
    pub fwhm_ideal: f64,
    pub fwhm_mirror: f64,
    pub fwhm_plane: f64,
    pub vp: [f64; 2],
    pub snr_curve: Vec<f64>,
    pub snr_min: f64,
    pub usable: f64,
    pub budget: Vec<(&'static str, f64)>,
}

pub const SNR_TARGET: f64 = 10.0;
pub const LAMBDA_MIN: f64 = 400e-9;
pub const LAMBDA_MAX: f64 = 800e-9;

fn valid(l: f64) -> bool {
    (LAMBDA_MIN..=LAMBDA_MAX).contains(&l)
}

pub fn compute(g: &dyn Get) -> Signal {
    let lam0 = g.g("sys.lam0");
    let [na_ill, na_det] = geometry::nas(g);
    let (f_o, f_c) = (disp::f_obj(g, lam0), g.g("coll.f"));
    let vp = geometry::normalised_radii(g);
    let ax = confocal::axial(vp[0], vp[1]);
    let na_ax = na_ill.min(na_det);
    let (texp, dlam_nm) = (g.g("acq.texp"), g.g("spec.dlam") * 1e9);
    let na_pin = na_ill * f_o / f_c;
    let a_pin = PI * (g.g("pin1.d") / 2.0).powi(2);
    let (r, spec_frac) = (g.g("sample.r"), g.g("sample.spec"));
    let q = (g.g("sample.tilt") / na_det.asin()).abs();
    let collect_spec = if q >= 1.0 {
        0.0
    } else {
        2.0 / PI * (q.acos() - q * (1.0 - q * q).sqrt())
    };
    let fwd = g.g("coll.t") * g.g("bs.r") * g.g("obj.t");
    let back = g.g("obj.t") * (1.0 - g.g("bs.r") - g.g("bs.loss")) * g.g("dcoll.t");
    let rms = g.g("obj.rms");
    let n = spec::N_PX;
    let lam: Vec<f64> = (0..n).map(|i| spec::lam_px(g, i)).collect();
    let z_eff: Vec<f64> = lam
        .iter()
        .map(|&l| disp::z_eff(g, l.clamp(LAMBDA_MIN, LAMBDA_MAX)))
        .collect();
    let bank = confocal::Bank::new(
        vp,
        lam0,
        lam[0].min(lam[n - 1]).max(LAMBDA_MIN),
        lam[0].max(lam[n - 1]).min(LAMBDA_MAX),
    );
    let radiance = rad::radiance(g);
    let p_in = |l: f64| rad::per_nm(radiance, l) * a_pin * PI * na_pin * na_pin * g.g("led.prox");
    let spectrometer =
        |l: f64| g.g("spec.t") * spec::grating_eff(g, l) * super::spectral::qe().at(l * 1e9);
    let sample_gain = |i: usize, zs: f64| {
        let l = lam[i];
        let na_l = na_ax * f_o / disp::f_obj(g, l);
        let u = 2.0 * PI * na_l * na_l * (zs - z_eff[i]) / l;
        let s = (-(2.0 * PI * rms * lam0 / l).powi(2)).exp();
        r * (spec_frac * collect_spec * bank.mirror_at(l, 2.0 * u) * s.powi(4)
            + (1.0 - spec_frac) * na_det.powi(2) * bank.plane_at(l, u) * s.powi(2))
    };
    let rate = |i: usize, zs: f64| {
        if valid(lam[i]) {
            p_in(lam[i]) * fwd * sample_gain(i, zs) * back * spectrometer(lam[i])
        } else {
            0.0
        }
    };
    let to_e = |i: usize, w_per_nm: f64| {
        if valid(lam[i]) {
            spec::photons_per_nm_to_electrons(w_per_nm, lam[i], dlam_nm, texp, 1.0)
        } else {
            0.0
        }
    };
    let sigma_px = spec::resolution_sigma_px(g);
    let spectrum = |zs: f64| {
        spec::blur(
            &(0..n).map(|i| to_e(i, rate(i, zs))).collect::<Vec<_>>(),
            sigma_px,
        )
    };
    let zs = g.g("sample.z");
    let sig_px = spectrum(zs);
    let dark = detector::dark_e_per_s(g) * texp;
    let bg_px: Vec<f64> = (0..n)
        .map(|i| dark + to_e(i, p_in(lam[i]) * g.g("bs.ghost") * spectrometer(lam[i])))
        .collect();
    let (rn, dn, prnu) = (
        detector::read_noise_e(g),
        detector::digital_noise_e(g),
        g.g("det.prnu"),
    );
    let bin = 1usize << (g.g("det.bin") as usize);
    let variance =
        |sig: f64, bg: f64| (sig + bg + (prnu * sig).powi(2)) / bin as f64 + rn * rn + dn * dn;
    let (sig, bg) = (spec::rebin(&sig_px, bin), spec::rebin(&bg_px, bin));
    let var: Vec<f64> = sig.iter().zip(&bg).map(|(&s, &b)| variance(s, b)).collect();
    let k = (0..sig.len())
        .max_by(|&a, &b| sig[a].total_cmp(&sig[b]))
        .unwrap_or(0);
    let c = |u: f64| lam0 / (2.0 * PI * na_ax * na_ax) * u;
    let (fwhm_mirror, fwhm_plane) = (c(ax.mirror_fwhm_u()) / 2.0, c(ax.plane_fwhm_u()));
    let dz = 0.01 * fwhm_mirror;
    let (up, down) = (
        spec::rebin(&spectrum(zs + dz), bin),
        spec::rebin(&spectrum(zs - dz), bin),
    );
    let fisher: f64 = (0..sig.len())
        .map(|i| ((up[i] - down[i]) / (2.0 * dz)).powi(2) / var[i])
        .sum();
    let sigma_z = 1.0 / fisher.sqrt();
    let (lam_b, z_eff_b) = (spec::rebin(&lam, bin), spec::rebin(&z_eff, bin));
    let slope = disp::slope(g, lam_b[k]).abs();
    let pix_max = (0..n).map(|i| sig_px[i] + bg_px[i]).fold(0.0, f64::max);
    let focused: Vec<f64> = (0..n).map(|i| to_e(i, rate(i, z_eff[i]))).collect();
    let snr_curve: Vec<f64> = spec::rebin(&focused, bin)
        .iter()
        .zip(&bg)
        .map(|(&e, &b)| e / variance(e, b).sqrt())
        .collect();
    let i0 = (0..n)
        .min_by(|&a, &b| (lam[a] - lam0).abs().total_cmp(&(lam[b] - lam0).abs()))
        .unwrap_or(0);
    let photon_scale = dlam_nm * texp / rad::photon_energy(lam[i0]);
    let budget = vec![
        (
            "Power into pinhole 1 (W/nm): η_prox · L · A · π · NA_pin²",
            p_in(lam[i0]),
        ),
        ("Collimator transmission", g.g("coll.t")),
        ("Beam-splitter reflection", g.g("bs.r")),
        ("Objective transmission", g.g("obj.t")),
        (
            "Sample: reflectance × capture × pinhole-2 pass × Strehl (in focus)",
            sample_gain(i0, z_eff[i0]),
        ),
        ("Objective, beam-splitter transmission, de-collimator", back),
        (
            "Spectrometer optics × grating efficiency",
            g.g("spec.t") * spec::grating_eff(g, lam[i0]),
        ),
        (
            "Detector quantum efficiency",
            super::spectral::qe().at(lam[i0] * 1e9),
        ),
        (
            "Photons per W/nm per pixel per exposure (Δλ_px · t_exp / hν)",
            photon_scale,
        ),
    ];
    let edge = snr_curve.len() / 10;
    let inner: Vec<f64> = (edge..snr_curve.len() - edge)
        .filter(|&i| valid(lam_b[i]))
        .map(|i| snr_curve[i])
        .collect();
    Signal {
        p_pin: (400..800).map(|l| p_in(l as f64 * 1e-9)).sum(),
        eta_p2: ax.mirror[0],
        strehl: (-(2.0 * PI * rms).powi(2)).exp(),
        lam_peak: lam_b[k],
        peak_fwhm: halfmax::width(&sig, k) * bin as f64 * dlam_nm * 1e-9,
        peak_e: sig_px.iter().sum(),
        pix_max,
        snr: sig[k] / var[k].sqrt(),
        sat: pix_max / detector::full_well_e(g),
        sigma_z,
        sigma_lam: sigma_z / slope,
        fwhm_ideal: 0.886 * lam0 / (na_ax * na_ax),
        fwhm_mirror,
        fwhm_plane,
        vp,
        snr_min: inner
            .iter()
            .cloned()
            .fold(f64::INFINITY, f64::min)
            .min(f64::MAX),
        usable: inner.iter().filter(|&&s| s >= SNR_TARGET).count() as f64
            / inner.len().max(1) as f64,
        snr_curve,
        budget,
        lam: lam_b,
        sig,
        bg,
        z_eff: z_eff_b,
        var,
    }
}
