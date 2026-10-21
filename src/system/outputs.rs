use super::{inputs, optics::*, readout};
use crate::engine::{get::Get, values::Values, var::VarDef};

const SOFT_P: f64 = 16.0;
const IMAGE_PIXELS: f64 = 2000.0;

pub const BINNING: &[&str] = &["x1", "x2", "x4"];
pub const REGION: &[&str] = &["Full line", "Centre 512"];

pub fn output_defs() -> Vec<VarDef> {
    const UM: f64 = 1e-6;
    const NM: f64 = 1e-9;
    let o = |id: &str, label, unit, scale, min, max| {
        let mut d = VarDef::new(id, label, unit, scale, min, max, 0.0);
        d.output = true;
        d
    };
    vec![
        o("lat.airy", "Airy unit at the sample (550 nm)", "µm", UM, 0.1 * UM, 100.0 * UM).doc("1.22 λ / NA: the size of the diffraction spot the pinhole is measured against."),
        o("lat.mean", "Lateral FWHM, mean over depth", "µm", UM, 0.1 * UM, 200.0 * UM).doc("Width of the confocal spot, with the finite pinholes, averaged over the depth range."),
        o("lat.worst", "Lateral FWHM, worst", "µm", UM, 0.1 * UM, 200.0 * UM).doc("The widest spot anywhere in the depth range (usually the red end, where the NA is lowest)."),
        o("ax.spec_mean", "Axial FWHM, mirror, mean", "µm", UM, 0.1 * UM, 1e4 * UM).doc("Peak width in height for a specular surface."),
        o("ax.spec_worst", "Axial FWHM, mirror, worst", "µm", UM, 0.1 * UM, 1e4 * UM),
        o("ax.diff_mean", "Axial FWHM, diffuse, mean", "µm", UM, 0.1 * UM, 1e4 * UM).doc("Peak width in height for a rough surface (wider than a mirror's)."),
        o("ax.diff_worst", "Axial FWHM, diffuse, worst", "µm", UM, 0.1 * UM, 1e4 * UM),
        o("ax.layers", "Resolvable layers", "", 1.0, 0.1, 1e4).doc("Depth range divided by the worst diffuse axial FWHM."),
        o("ax.peak_min", "Narrowest spectral peak", "nm", NM, 0.1 * NM, 500.0 * NM).doc("Peak width in wavelength; the spectrometer resolution should stay well below it."),
        o("ax.peak_max", "Widest spectral peak", "nm", NM, 0.1 * NM, 500.0 * NM),
        o("pr.mean", "Height noise σ_z, mean (lower is better)", "nm", NM, 0.01 * NM, 1e6 * NM).doc("Cramér–Rao bound on the height error for a known peak shape, averaged over the depth range."),
        o("pr.worst", "Height noise σ_z, worst (lower is better)", "nm", NM, 0.01 * NM, 1e6 * NM),
        o("pr.soft", "Height noise σ_z, smooth worst case", "nm", NM, 0.01 * NM, 1e6 * NM).doc("A 16-norm over the depth range: close to the worst case but smooth, which is what the optimiser minimises."),
        o("pr.snr_mean", "Peak SNR, mean", "", 1.0, 1e-3, 1e5),
        o("pr.snr_worst", "Peak SNR, worst", "", 1.0, 1e-3, 1e5),
        o("ph.p_pin", "Power into pinhole 1 (400-800 nm)", "µW", 1e-6, 1e-9, 1.0).doc("Radiance × (π²/4)(1.22 λ·AU)²: set by the pinhole size in Airy units and the LED, not by NA or the physical pinhole diameter."),
        o("ph.strehl", "Strehl ratio, worst", "", 1.0, 0.0, 1.0),
        o("ph.peak_e", "Signal electrons in the peak, mean", "ke-", 1e3, 1.0, 1e9),
        o("ph.fill", "Brightest pixel, fraction of full well", "", 1.0, 0.0, 1.0).doc("The exposure is chosen to keep this at 0.8 when the sample is bright."),
        o("acq.texp", "Exposure (automatic)", "µs", UM, 0.1 * UM, 1.0).doc("The shorter of the sample period and the time to reach 80 % of full well in the brightest pixel."),
        o("acq.duty", "Exposure / sample period", "", 1.0, 0.0, 1.0),
        o("acq.fread", "Read clock (automatic)", "MHz", 1e6, 0.1e6, 60e6),
        o("acq.bin", "Binning (automatic)", "", 1.0, 0.0, 2.0).pick(BINNING, 0),
        o("acq.roi", "Read-out region (automatic)", "", 1.0, 0.0, 1.0).pick(REGION, 0),
        o("acq.fill", "Sensor fill by band + peak width", "", 1.0, 0.01, 10.0).doc("Pixels needed for the in-focus band plus one peak width on each side, over 1024. Above 1 the peaks at the ends of the range are cut."),
        o("acq.img", "Time per 2000 × 2000 image", "min", 60.0, 1.0, 1e6),
        o("acq.fov", "Field of view at Nyquist steps", "mm", 1e-3, 1e-3, 1e3).doc("2000 steps of half the worst lateral FWHM."),
        o("acq.arate", "Area rate", "mm²/s", 1e-6, 1e-9, 1e3),
    ]
}

pub fn all_defs() -> Vec<VarDef> {
    let mut d = inputs::defs();
    d.extend(output_defs());
    d
}

pub struct Report {
    pub values: Values,
    pub nodes: Vec<readout::Node>,
    pub assessment: readout::Assessment,
    pub flags: Vec<String>,
    pub budget: Vec<(&'static str, f64)>,
}

fn height_weights(nodes: &[readout::Node]) -> Vec<f64> {
    let n = nodes.len();
    let w: Vec<f64> = (0..n)
        .map(|k| {
            let span = if k == 0 {
                nodes[1].z - nodes[0].z
            } else if k == n - 1 {
                nodes[n - 1].z - nodes[n - 2].z
            } else {
                (nodes[k + 1].z - nodes[k - 1].z) / 2.0
            };
            span.abs()
        })
        .collect();
    let total: f64 = w.iter().sum();
    w.iter().map(|x| x / total).collect()
}

fn mean(v: &[f64], w: &[f64]) -> f64 {
    v.iter().zip(w).map(|(a, b)| a * b).sum()
}

fn max(v: &[f64]) -> f64 {
    v.iter().cloned().fold(f64::MIN, f64::max)
}

fn min(v: &[f64]) -> f64 {
    v.iter().cloned().fold(f64::INFINITY, f64::min)
}

pub fn evaluate(inputs: &Values) -> Report {
    let s = Setup::new(inputs);
    let nodes: Vec<readout::Node> = (0..readout::NODES).map(|k| readout::node(&s, k)).collect();
    let a = readout::best(&s, &nodes);
    let w = height_weights(&nodes);
    let col = |f: fn(&readout::Node) -> f64| nodes.iter().map(f).collect::<Vec<_>>();
    let (lat, spec, diff, peak) = (
        col(|n| n.lateral),
        col(|n| n.ax_spec),
        col(|n| n.ax_diff),
        col(|n| n.peak_nm),
    );
    let strehl = col(|n| n.strehl);
    let soft = (a.sigma.iter().map(|x| x.powf(SOFT_P)).sum::<f64>() / a.sigma.len() as f64)
        .powf(1.0 / SOFT_P);
    let pad = max(&peak);
    let px_needed = (BAND.1 - BAND.0 + 2.0 * pad) / s.p.disp;
    let step = 0.5 * max(&lat);
    let period = 1.0 / s.p.rate;
    let r = a.readout;
    let mut values = inputs.clone();
    let mut put = |id: &str, x: f64| values.set(id, x);
    put("lat.airy", 1.22 * LAM0 / s.p.na);
    put("lat.mean", mean(&lat, &w));
    put("lat.worst", max(&lat));
    put("ax.spec_mean", mean(&spec, &w));
    put("ax.spec_worst", max(&spec));
    put("ax.diff_mean", mean(&diff, &w));
    put("ax.diff_worst", max(&diff));
    put("ax.layers", s.p.dz / max(&diff));
    put("ax.peak_min", min(&peak));
    put("ax.peak_max", pad);
    put("pr.mean", mean(&a.sigma, &w));
    put("pr.worst", max(&a.sigma));
    put("pr.soft", soft);
    put("pr.snr_mean", mean(&a.snr, &w));
    put("pr.snr_worst", min(&a.snr));
    put("ph.p_pin", s.total_power());
    put("ph.strehl", min(&strehl));
    put("ph.peak_e", mean(&a.peak_e, &w));
    put("ph.fill", a.fill);
    put("acq.texp", r.texp);
    put("acq.duty", r.texp / period);
    put("acq.fread", r.fread);
    put("acq.bin", r.bin as f64);
    put("acq.roi", if r.roi { 1.0 } else { 0.0 });
    put("acq.fill", px_needed / N_PX as f64);
    put("acq.img", IMAGE_PIXELS * IMAGE_PIXELS * period);
    put("acq.fov", IMAGE_PIXELS * step);
    put("acq.arate", s.p.rate * step * step);
    let flags = flags(&s, &values, &a);
    let budget = budget(&s, &nodes[nodes.len() / 2], a.readout.texp);
    Report {
        values,
        nodes,
        assessment: a,
        flags,
        budget,
    }
}

fn flags(s: &Setup, v: &Values, a: &readout::Assessment) -> Vec<String> {
    let g = |k: &str| v.g(k);
    let mut out = vec![];
    let mut add = |bad: bool, msg: String| {
        if bad {
            out.push(msg)
        }
    };
    add(g("acq.fill") > 1.0, format!("The band plus one peak width on each side needs {:.0} % of the sensor: peaks at the ends of the depth range are cut off", g("acq.fill") * 100.0));
    add(
        !a.feasible,
        "The sample rate is beyond what the epc901 can read out; the fastest read-out is used"
            .into(),
    );
    add(
        g("pr.snr_worst") < 10.0,
        format!(
            "Worst-case peak SNR is {:.1}: photon or read-noise limited",
            g("pr.snr_worst")
        ),
    );
    add(g("ph.strehl") < 0.8, format!("Strehl {:.2} at the worst wavelength: not diffraction limited, resolution numbers are optimistic", g("ph.strehl")));
    add(
        AIRY_V * s.p.au * s.p.ratio.max(1.0) * LAM0 / BAND.0 > 24.0,
        "The pinhole is too large for the response tables; results are unreliable".into(),
    );
    add(
        s.p.na > 0.5,
        "NA above 0.5: the paraxial axial-response normalisation is inaccurate".into(),
    );
    add(s.p.res > g("ax.peak_min") / 4.0, format!("Spectral resolution {:.1} nm is more than a quarter of the narrowest peak ({:.1} nm): the peaks are smeared", s.p.res * 1e9, g("ax.peak_min") * 1e9));
    add(
        g("ax.peak_min") / s.p.disp < 4.0,
        format!(
            "Fewer than 4 pixels across the narrowest peak ({:.1} px)",
            g("ax.peak_min") / s.p.disp
        ),
    );
    add(g("acq.duty") < 0.5, format!("Exposure is cut to {:.0} µs ({:.0} % of the period) to avoid saturating the brightest pixel", g("acq.texp") * 1e6, g("acq.duty") * 100.0));
    out
}

fn budget(s: &Setup, n: &readout::Node, texp: f64) -> Vec<(&'static str, f64)> {
    let l = n.lam;
    vec![
        (
            "Power into pinhole 1 (W/nm): LED radiance × (π²/4)(1.22 λ AU)² × proximity",
            s.p_in(l),
        ),
        ("Collimator and objective, one pass", s.p.t),
        ("Beam-splitter reflection", BS_R),
        (
            "Sample: reflectance × capture × pinhole-2 pass × Strehl (in focus)",
            s.gain(l, n.z),
        ),
        (
            "Objective and collimator on return, beam-splitter transmission",
            s.p.t * (1.0 - BS_R - BS_LOSS),
        ),
        ("Spectrometer efficiency", s.p.eff),
        (
            "Quantum efficiency",
            crate::physics::spectral::qe().at(l * 1e9),
        ),
        (
            "Photons per W/nm per exposure (Δλ_px · t_exp / hν)",
            s.p.disp * 1e9 * texp / crate::physics::radiometry::photon_energy(l),
        ),
    ]
}

pub struct SpectrumPlot {
    pub lam: Vec<f64>,
    pub signal: Vec<f64>,
    pub background: Vec<f64>,
    pub noise: Vec<f64>,
}

pub fn spectrum(inputs: &Values, report: &Report, fraction: f64) -> SpectrumPlot {
    let s = Setup::new(inputs);
    let (texp, z) = (report.assessment.readout.texp, fraction * s.p.dz);
    let profile = crate::physics::spectrometer::blur(
        &(0..N_PX).map(|i| s.rate(i, z)).collect::<Vec<_>>(),
        s.sigma_px,
    );
    let rn = crate::physics::detector::read_noise_at(report.assessment.readout.fread);
    let dark = crate::physics::detector::dark_rate_at(s.p.celsius);
    let signal: Vec<f64> = profile.iter().map(|r| r * texp).collect();
    let background: Vec<f64> = (0..N_PX).map(|i| (s.stray(i) + dark) * texp).collect();
    let noise = (0..N_PX)
        .map(|i| (signal[i] + background[i] + rn * rn).sqrt())
        .collect();
    SpectrumPlot {
        lam: s.lam.clone(),
        signal,
        background,
        noise,
    }
}
