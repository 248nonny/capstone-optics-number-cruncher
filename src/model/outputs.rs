use super::flags;
use crate::engine::{formula::Refs, get::Get, values::Values};
use crate::physics::{
    aberration, confocal, detector, dispersion as disp, geometry, glass, montecarlo,
    radiometry as rad,
    signal::{self, Signal},
    spectrometer as spec,
};
use std::cell::OnceCell;

const AIRY: Refs = &[("Airy disk", "https://en.wikipedia.org/wiki/Airy_disk")];
const CONFOCAL: Refs = &[
    (
        "Pinhole effect in confocal microscopes (Leica)",
        "https://www.leica-microsystems.com/science-lab/pinhole-effect-in-confocal-microscopes/",
    ),
    (
        "Confocal microscopy",
        "https://en.wikipedia.org/wiki/Confocal_microscopy",
    ),
];
const ETENDUE: Refs = &[("Etendue", "https://en.wikipedia.org/wiki/Etendue")];
const EPC: Refs = &[(
    "epc901 datasheet",
    "https://www.espros.com/photonics/epc901/",
)];
const CRB: Refs = &[(
    "Cramer-Rao bound",
    "https://en.wikipedia.org/wiki/Cram%C3%A9r%E2%80%93Rao_bound",
)];
const STREHL: Refs = &[("Strehl ratio", "https://en.wikipedia.org/wiki/Strehl_ratio")];

pub struct Eval<'a> {
    pub v: &'a Values,
    sig: OnceCell<Signal>,
    lat: OnceCell<f64>,
    mc: OnceCell<f64>,
}

impl<'a> Eval<'a> {
    pub fn new(v: &'a Values) -> Self {
        Self {
            v,
            sig: OnceCell::new(),
            lat: OnceCell::new(),
            mc: OnceCell::new(),
        }
    }

    pub fn sig(&self) -> &Signal {
        self.sig.get_or_init(|| signal::compute(self.v))
    }

    pub fn lat(&self) -> f64 {
        *self.lat.get_or_init(|| {
            confocal::lateral_fwhm(
                self.v.g("sys.lam0"),
                geometry::nas(self.v),
                geometry::pinhole_radii(self.v),
            )
        })
    }

    pub fn mc(&self) -> f64 {
        *self.mc.get_or_init(|| {
            let s = self.sig();
            montecarlo::centroid_sigma(&s.sig, &s.var, &s.lam)
        })
    }

    pub fn into_signal(self) -> Signal {
        self.sig
            .into_inner()
            .unwrap_or_else(|| signal::compute(self.v))
    }
}

pub struct Output {
    pub id: &'static str,
    pub eq: &'static str,
    pub refs: Refs,
    pub f: fn(&Eval) -> f64,
}

const fn o(id: &'static str, eq: &'static str, refs: Refs, f: fn(&Eval) -> f64) -> Output {
    Output { id, eq, refs, f }
}

fn spectrum_bytes(v: &Values) -> f64 {
    if v.g("link.onboard") > 0.5 {
        8.0
    } else {
        v.g("link.bytes") * spec::N_PX as f64 / 2f64.powf(v.g("det.bin"))
    }
}

pub const OUTPUTS: &[Output] = &[
    o(
        "obj.na_ill",
        "NA_ill = min(NA, D_coll / 2 f_obj)",
        &[],
        |e| geometry::na_ill(e.v),
    ),
    o(
        "obj.na_det",
        "NA_det = min(NA, D_dcoll / 2 f_obj)",
        &[],
        |e| geometry::na_det(e.v),
    ),
    o(
        "led.flux",
        "Φ_v = Φ_ref (I / 350 mA)^0.82 (fit to the datasheet flux-current curve)",
        &[],
        |e| rad::flux_lm(e.v),
    ),
    o(
        "led.pe",
        "Φ_e = Φ_v / (683 <V>)",
        &[(
            "Luminous efficacy",
            "https://en.wikipedia.org/wiki/Luminous_efficacy",
        )],
        |e| rad::flux_w(e.v),
    ),
    o(
        "led.rad",
        "L = Φ_e / (π A)",
        &[(
            "Lambertian reflectance",
            "https://en.wikipedia.org/wiki/Lambertian_reflectance",
        )],
        |e| rad::radiance(e.v),
    ),
    o(
        "obj.lsa",
        "Meridional trace: n sin i = n' sin i'",
        &[
            (
                "Spherical aberration",
                "https://en.wikipedia.org/wiki/Spherical_aberration",
            ),
            ("Newport SFPX040", "https://www.newport.com/p/SFPX040"),
        ],
        |e| {
            let g = e.v.g("obj.glass") as usize;
            if g >= 5 {
                0.0
            } else {
                aberration::pcx_marginal_lsa(
                    e.v.g("obj.f"),
                    glass::index(g, e.v.g("obj.lamf")),
                    glass::index(g, e.v.g("sys.lam0")),
                    e.v.g("obj.ct"),
                    e.v.g("obj.na"),
                )
            }
        },
    ),
    o("spec.span", "span = 1024 Δλ", &[], |e| {
        spec::N_PX as f64 * e.v.g("spec.dlam")
    }),
    o(
        "spec.res",
        "σ² = (w/2)²/4 [disk] + 1/12 [pixel] + (λ/mN)²/2.355² + (aberration/2.355)²",
        &[(
            "Grating handbook",
            "https://www.newport.com/n/diffraction-grating-physics",
        )],
        |e| 2.355 * spec::resolution_sigma_px(e.v) * e.v.g("spec.dlam"),
    ),
    o("spec.slit", "w = d f_cam / f_col / pitch", &[], |e| {
        spec::slit_px(e.v)
    }),
    o("det.nclk", "1027 / 515 / 259 clocks", EPC, |e| {
        detector::clocks(e.v)
    }),
    o("det.tread", "t = N / f_read", EPC, |e| {
        detector::clocks(e.v) / e.v.g("det.fread")
    }),
    o("det.noise", "SNR table (datasheet table 7)", EPC, |e| {
        detector::read_noise_e(e.v)
    }),
    o("det.fw", "FW = swing / CG", EPC, |e| {
        detector::full_well_e(e.v)
    }),
    o("acq.step", "step = sf × FWHM", &[], |e| {
        e.v.g("acq.sf") * e.lat()
    }),
    o("acq.arate", "area rate = rate × step²", &[], |e| {
        e.v.g("acq.rate") * (e.v.g("acq.sf") * e.lat()).powi(2)
    }),
    o("acq.timg", "t = nx ny / rate", &[], |e| {
        e.v.g("acq.nx") * e.v.g("acq.ny") / e.v.g("acq.rate")
    }),
    o("link.need", "bytes × pixels × rate", &[], |e| {
        spectrum_bytes(e.v) * e.v.g("acq.rate")
    }),
    o("link.max", "rate / bytes per spectrum", &[], |e| {
        e.v.g("link.rate") / spectrum_bytes(e.v)
    }),
    o("lat.au", "AU = 1.22 λ / NA", AIRY, |e| {
        1.22 * e.v.g("sys.lam0") / geometry::na_ill(e.v)
    }),
    o("lat.wide", "FWHM = 0.51 λ / NA", AIRY, |e| {
        0.51 * e.v.g("sys.lam0") / geometry::na_ill(e.v)
    }),
    o(
        "lat.fwhm",
        "I(r) = [|h|² ⊗ P₁] [|h|² ⊗ P₂]",
        CONFOCAL,
        |e| e.lat(),
    ),
    o(
        "ax.slope",
        "dz/dλ from f(λ) of objective and collimators",
        &[(
            "Axial chromatic aberration",
            "https://en.wikipedia.org/wiki/Chromatic_aberration",
        )],
        |e| disp::slope(e.v, e.v.g("sys.lam0")).abs(),
    ),
    o("ax.range", "z(λ_max) − z(λ_min)", &[], |e| {
        let half = 0.5 * spec::N_PX as f64 * e.v.g("spec.dlam");
        (disp::z_eff(e.v, e.v.g("spec.lam_c") + half)
            - disp::z_eff(e.v, e.v.g("spec.lam_c") - half))
        .abs()
    }),
    o(
        "ax.ideal",
        "FWHM = 0.886 λ / NA² (paraxial mirror)",
        CONFOCAL,
        |e| e.sig().fwhm_ideal,
    ),
    o(
        "ax.mirror",
        "I = ∫ K(ρ) |U(ρ, 2z)|² d²ρ",
        CONFOCAL,
        |e| e.sig().fwhm_mirror,
    ),
    o(
        "ax.plane",
        "I = ∫ D₁(ρ, z) D₂(ρ, z) d²ρ",
        CONFOCAL,
        |e| e.sig().fwhm_plane,
    ),
    o(
        "ax.peak",
        "Half-maximum width of the modelled spectrum",
        &[],
        |e| e.sig().peak_fwhm,
    ),
    o("ax.lam_peak", "λ where z_eff(λ) = z_sample", &[], |e| {
        e.sig().lam_peak
    }),
    o("ph.p_pin", "P = η_prox L A π NA_pin²", ETENDUE, |e| {
        e.sig().p_pin
    }),
    o(
        "ph.eta",
        "η = 1 − J₀²(v) − J₁²(v) in the point-source limit",
        AIRY,
        |e| e.sig().eta_p2,
    ),
    o(
        "ph.strehl",
        "S = exp(−(2π σ)²); the confocal peak scales as S² (illumination and detection)",
        STREHL,
        |e| e.sig().strehl,
    ),
    o("ph.peak_e", "Σ over pixels of QE × photons", EPC, |e| {
        e.sig().peak_e
    }),
    o("ph.pix_max", "signal + dark + stray", EPC, |e| {
        e.sig().pix_max
    }),
    o("ph.snr", "S / √(S + B + read² + ADC²)", EPC, |e| {
        e.sig().snr
    }),
    o("ph.sat", "pixel electrons / full well", EPC, |e| {
        e.sig().sat
    }),
    o(
        "ph.snr_min",
        "min over λ of S / √(S + B + read²)",
        EPC,
        |e| e.sig().snr_min,
    ),
    o(
        "ph.usable",
        "fraction of the band with SNR ≥ 10",
        &[],
        |e| e.sig().usable,
    ),
    o(
        "pr.sigma_z",
        "σ² ≥ 1 / Σ (∂μ/∂z)² / var",
        CRB,
        |e| e.sig().sigma_z,
    ),
    o("pr.sigma_lam", "σ_λ = σ_z |dλ/dz|", &[], |e| {
        e.sig().sigma_lam
    }),
    o("pr.mc_lam", "std of centroid(noisy spectrum)", CRB, |e| {
        e.mc()
    }),
    o("pr.mc_z", "σ_z = σ_λ |dz/dλ|", &[], |e| {
        e.mc() * disp::slope(e.v, e.sig().lam_peak).abs()
    }),
];

pub fn lookup(id: &str) -> Option<&'static Output> {
    OUTPUTS.iter().find(|o| o.id == id)
}

pub struct Evaluation {
    pub values: Values,
    pub signal: Signal,
    pub flags: Vec<String>,
}

pub fn evaluate(inputs: &Values, extra_flags: &[String]) -> Evaluation {
    let mut values = inputs.clone();
    let e = Eval::new(inputs);
    for o in OUTPUTS {
        values.set(o.id, (o.f)(&e));
    }
    let signal = e.into_signal();
    let flags = flags::flags(&values, &signal, extra_flags);
    Evaluation {
        values,
        signal,
        flags,
    }
}

pub fn evaluate_ids(inputs: &Values, ids: &[&str]) -> Vec<f64> {
    let e = Eval::new(inputs);
    ids.iter()
        .map(|id| lookup(id).map_or_else(|| inputs.g(id), |o| (o.f)(&e)))
        .collect()
}
