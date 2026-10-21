use crate::engine::{ctx::Ctx, get::Get, rule::{Refs, Rule}};
use crate::physics::{aberration, confocal, detector, geometry, dispersion as disp, glass, radiometry as rad, signal, spectrometer as spec};
use std::rc::Rc;

const AIRY: Refs = &[("Airy disk", "https://en.wikipedia.org/wiki/Airy_disk")];
const CONFOCAL: Refs = &[
    ("Pinhole effect in confocal microscopes (Leica)", "https://www.leica-microsystems.com/science-lab/pinhole-effect-in-confocal-microscopes/"),
    ("Confocal microscopy", "https://en.wikipedia.org/wiki/Confocal_microscopy"),
];
const ABBE: Refs = &[("Abbe sine condition", "https://en.wikipedia.org/wiki/Abbe_sine_condition")];
const STREHL: Refs = &[("Strehl ratio", "https://en.wikipedia.org/wiki/Strehl_ratio")];
const CRB: Refs = &[("Cramer-Rao bound", "https://en.wikipedia.org/wiki/Cram%C3%A9r%E2%80%93Rao_bound")];
const ETENDUE: Refs = &[("Etendue", "https://en.wikipedia.org/wiki/Etendue")];
const EPC: Refs = &[("epc901 datasheet", "https://www.espros.com/photonics/epc901/")];

pub fn sig(c: &Ctx) -> Rc<signal::Signal> {
    c.memo("signal", || signal::compute(c))
}

fn lateral(c: &Ctx) -> Rc<f64> {
    c.memo("lateral", || confocal::lateral_fwhm(c.g("sys.lam0"), geometry::nas(c), geometry::pinhole_radii(c)))
}

fn na_rule(inst: &str) -> Rule {
    let ids = ["na", "d", "f"].map(|l| format!("{inst}.{l}"));
    Rule::per(&ids.each_ref().map(String::as_str), "NA = D / (2 f)", ABBE, |c, v| c.g(&v[1]) / (2.0 * c.g(&v[2])))
}

fn lens_rules(lens: &str) -> Vec<Rule> {
    let ids = ["m", "f"].map(|l| format!("{lens}.{l}"));
    let magnification = Rule::per(&[&ids[0], "obj.f", &ids[1]], "m = f_obj / f", &[], |c, v| c.g(&v[1]) / c.g(&v[2]));
    vec![na_rule(lens), magnification]
}

fn pin_rule(pin: &str, lens: &str, na: &str) -> Rule {
    let ids = [format!("{pin}.au"), format!("{pin}.d"), format!("{lens}.f")];
    let vars = [ids[0].as_str(), ids[1].as_str(), na, "obj.f", ids[2].as_str(), "sys.lam0"];
    Rule::per(&vars, "AU = d NA (f_obj / f) / (1.22 lambda)", AIRY, |c, v| c.g(&v[1]) * c.g(&v[2]) * c.g(&v[3]) / (1.22 * c.g(&v[5]) * c.g(&v[4])))
}

fn aperture_rule(out_id: &str, lens: &str) -> Rule {
    let d = format!("{lens}.d");
    Rule::per(&[out_id, "obj.na", &d, "obj.f"], "NA_ill = min(NA, D_coll / (2 f_obj))", &[], |c, v| c.g(&v[1]).min(c.g(&v[2]) / (2.0 * c.g(&v[3]))))
}

fn out(var: &str, eq: &'static str, refs: Refs, f: impl Fn(&Ctx) -> f64 + 'static) -> Rule {
    Rule::new(&[var], eq, refs, f)
}

pub fn rules() -> Vec<Rule> {
    let mut r = vec![na_rule("obj")];
    r.extend(["coll", "dcoll"].into_iter().flat_map(lens_rules));
    r.extend([aperture_rule("obj.na_ill", "coll"), aperture_rule("obj.na_det", "dcoll"), pin_rule("pin1", "coll", "obj.na_ill"), pin_rule("pin2", "dcoll", "obj.na_det")]);
    r.extend([
        Rule::new(&["led.flux", "led.flux_ref", "led.i", "led.droop"], "Phi_v = Phi_ref (I / 350 mA)^droop", &[], |c| rad::flux_lm(c)),
        Rule::new(&["led.pe", "led.flux_ref", "led.i", "led.droop"], "Phi_e = Phi_v / (683 <V>)", &[("Luminous efficacy", "https://en.wikipedia.org/wiki/Luminous_efficacy")], |c| rad::flux_w(c)),
        Rule::new(&["led.rad", "led.flux_ref", "led.i", "led.droop", "led.side"], "L = Phi_e / (pi A)", &[("Lambertian reflectance", "https://en.wikipedia.org/wiki/Lambertian_reflectance")], |c| rad::radiance(c)),
        Rule::new(&["obj.lsa", "obj.f", "obj.glass", "obj.ct", "obj.na", "sys.lam0"], "Meridional trace: n sin i = n' sin i'", &[("Spherical aberration", "https://en.wikipedia.org/wiki/Spherical_aberration"), ("Newport SFPX040", "https://www.newport.com/p/SFPX040")], |c| {
            let g = c.g("obj.glass") as usize;
            if g >= 5 { 0.0 } else { aberration::pcx_marginal_lsa(c.g("obj.f"), glass::index(g, c.g("sys.lam0")), c.g("obj.ct"), c.g("obj.na")) }
        }),
        Rule::new(&["obj.rms", "obj.lsa", "obj.na", "sys.lam0"], "sigma = 0.0745 W040 / lambda, W040 = LSA NA^2 / 4", STREHL, |c| aberration::sa_rms_waves(c.g("obj.lsa"), c.g("obj.na"), c.g("sys.lam0"))),
        Rule::new(&["spec.dlam", "spec.lines", "spec.order", "spec.alpha", "spec.f_cam", "spec.lam_c"], "m lambda = d (sin a + sin b); dlambda/px = d cos b p / (m f)", &[("Diffraction grating", "https://en.wikipedia.org/wiki/Diffraction_grating")], |c| spec::grating_dlam(c)),
        Rule::new(&["spec.span", "spec.dlam"], "span = 1024 dlambda", &[], |c| spec::N_PX as f64 * c.g("spec.dlam")),
        Rule::new(&["spec.res", "spec.dlam", "pin2.d", "spec.f_cam", "spec.f_col", "spec.lines", "spec.order", "spec.beam", "spec.aber"], "FWHM^2 = slit^2 + pixel^2 + (lambda / mN)^2 + aberration^2", &[("Grating handbook", "https://www.newport.com/n/diffraction-grating-physics")], |c| 2.355 * spec::resolution_sigma_px(c) * c.g("spec.dlam")),
        Rule::new(&["spec.slit", "pin2.d", "spec.f_cam", "spec.f_col"], "w = d f_cam / f_col / pitch", &[], |c| spec::slit_px(c)),
        Rule::new(&["det.nclk", "det.bin"], "1027 / 515 / 259 clocks", EPC, |c| detector::clocks(c)),
        Rule::new(&["det.fread", "det.nclk", "acq.rate"], "f_read = 1.1 N_clk rate (slowest, quietest clock that fits the period)", EPC, |c| 1.1 * c.g("det.nclk") * c.g("acq.rate")),
        Rule::new(&["det.tread", "det.nclk", "det.fread"], "t = N / f_read", EPC, |c| c.g("det.nclk") / c.g("det.fread")),
        Rule::new(&["det.noise", "det.fread", "det.mode", "det.cg"], "SNR table (datasheet table 7)", EPC, |c| detector::read_noise_e(c)),
        Rule::new(&["det.fw", "det.fread", "det.mode", "det.cg"], "FW = swing / CG", EPC, |c| detector::full_well_e(c)),
        Rule::new(&["acq.texp", "acq.rate", "acq.overlap", "det.tread"], "t_exp = 1/rate - overhead (- t_read if sequential)", EPC, |c| {
            let t = 1.0 / c.g("acq.rate") - detector::overhead_s() - if c.g("acq.overlap") > 0.5 { c.g("det.tread") } else { 0.0 };
            if t > 0.0 { t } else { f64::NAN }
        }),
        Rule::new(&["acq.step", "lat.fwhm", "acq.sf"], "step = sf x FWHM", &[], |c| c.g("acq.sf") * c.g("lat.fwhm")),
        Rule::new(&["acq.arate", "acq.rate", "acq.step"], "area rate = rate x step^2", &[], |c| c.g("acq.rate") * c.g("acq.step").powi(2)),
        Rule::new(&["acq.timg", "acq.nx", "acq.ny", "acq.rate"], "t = nx ny / rate", &[], |c| c.g("acq.nx") * c.g("acq.ny") / c.g("acq.rate")),
        Rule::new(&["link.need", "link.bytes", "link.onboard", "det.bin", "acq.rate"], "bytes x pixels x rate", &[], |c| link_bytes(c) * c.g("acq.rate") / 1e6),
        Rule::new(&["link.max", "link.rate", "link.bytes", "link.onboard", "det.bin"], "rate / bytes per spectrum", &[], |c| c.g("link.rate") / link_bytes(c)),
        out("lat.au", "AU = 1.22 lambda / NA", AIRY, |c| 1.22 * c.g("sys.lam0") / c.g("obj.na_ill")),
        out("lat.wide", "FWHM = 0.51 lambda / NA", AIRY, |c| 0.51 * c.g("sys.lam0") / c.g("obj.na_ill")),
        out("lat.fwhm", "I(r) = [|h|^2 (x) P1][|h|^2 (x) P2]", CONFOCAL, |c| *lateral(c)),
        out("ax.slope", "dz/dlambda from f(lambda) of objective and collimators", &[("Axial chromatic aberration", "https://en.wikipedia.org/wiki/Chromatic_aberration")], |c| disp::slope(c, c.g("sys.lam0")).abs()),
        out("ax.range", "z(lambda_max) - z(lambda_min)", &[], |c| {
            let half = 0.5 * c.g("spec.span");
            (disp::z_eff(c, c.g("spec.lam_c") + half) - disp::z_eff(c, c.g("spec.lam_c") - half)).abs()
        }),
        out("ax.ideal", "FWHM = 0.886 lambda / NA^2 (paraxial mirror)", CONFOCAL, |c| sig(c).fwhm_ideal),
        out("ax.mirror", "I = int K(rho) |U(rho, 2z)|^2 d2rho", CONFOCAL, |c| sig(c).fwhm_mirror),
        out("ax.plane", "I = int D1(rho, z) D2(rho, z) d2rho", CONFOCAL, |c| sig(c).fwhm_plane),
        out("ax.peak", "Half-maximum width of the modelled spectrum", &[], |c| sig(c).peak_fwhm),
        out("ax.lam_peak", "lambda where z_eff(lambda) = z_sample", &[], |c| sig(c).lam_peak),
        out("ph.p_pin", "P = eta_prox L A pi NA_pin^2", ETENDUE, |c| sig(c).p_pin),
        out("ph.eta", "eta = 1 - J0^2(v) - J1^2(v) in the point-source limit", AIRY, |c| sig(c).eta_p2),
        out("ph.strehl", "S = exp(-(2 pi sigma)^2)", STREHL, |c| sig(c).strehl),
        out("ph.peak_e", "sum over pixels of QE x photons", EPC, |c| sig(c).peak_e),
        out("ph.pix_max", "signal + dark + stray", EPC, |c| sig(c).pix_max),
        out("ph.snr", "S / sqrt(S + B + read^2 + ADC^2)", EPC, |c| sig(c).snr),
        out("ph.sat", "pixel electrons / full well", EPC, |c| sig(c).sat),
        out("pr.sigma_z", "sigma^2 >= 1 / sum (dmu/dz)^2 / var", CRB, |c| sig(c).sigma_z),
        out("ph.snr_min", "min over wavelength of S / sqrt(S + B + read^2)", EPC, |c| sig(c).snr_min),
        out("ph.usable", "fraction of the band with SNR >= 10", &[], |c| sig(c).usable),
        out("pr.mc_lam", "std of centroid(noisy spectrum)", CRB, |c| sig(c).mc_lam),
        out("pr.mc_z", "sigma_z = sigma_lambda |dz/dlambda|", &[], |c| sig(c).mc_lam * disp::slope(c, sig(c).lam_peak).abs()),
        out("pr.sigma_lam", "sigma_lambda = sigma_z |dlambda/dz|", &[], |c| sig(c).sigma_lam),
    ]);
    r
}

fn link_bytes(c: &Ctx) -> f64 {
    if c.g("link.onboard") > 0.5 { 8.0 } else { c.g("link.bytes") * spec::N_PX as f64 / 2f64.powf(c.g("det.bin")) }
}
