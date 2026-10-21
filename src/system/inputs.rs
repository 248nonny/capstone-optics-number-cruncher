use crate::engine::{get::Get, var::VarDef};

pub const SHAPES: &[&str] = &[
    "Linear",
    "N-SF11 singlet",
    "N-BK7 singlet",
    "F2 singlet",
    "N-SF6HT singlet",
    "Fused-silica singlet",
    "Diffractive",
];
pub const COOLING: &[&str] = &["Cooled (15 °C)", "Uncooled (35 °C)"];

const NM: f64 = 1e-9;
const UM: f64 = 1e-6;
const MA: f64 = 1e-3;
const AIRY: &[(&str, &str)] = &[("Airy disk", "https://en.wikipedia.org/wiki/Airy_disk")];
const CONFOCAL: &[(&str, &str)] = &[(
    "Confocal microscopy",
    "https://en.wikipedia.org/wiki/Confocal_microscopy",
)];

pub struct Params {
    pub current: f64,
    pub na: f64,
    pub au: f64,
    pub ratio: f64,
    pub dz: f64,
    pub shape: usize,
    pub t: f64,
    pub aberrations: bool,
    pub a: f64,
    pub stray: f64,
    pub r: f64,
    pub spec: f64,
    pub disp: f64,
    pub res: f64,
    pub eff: f64,
    pub rate: f64,
    pub celsius: f64,
}

impl Params {
    pub fn read(g: &dyn Get) -> Self {
        Self {
            current: g.g("src.i"),
            na: g.g("opt.na"),
            au: g.g("opt.au"),
            ratio: g.g("opt.ratio"),
            dz: g.g("opt.dz"),
            shape: g.g("opt.shape") as usize,
            t: g.g("opt.t"),
            aberrations: g.g("opt.ab") > 0.5,
            a: g.g("opt.a"),
            stray: g.g("opt.stray"),
            r: g.g("smp.r"),
            spec: g.g("smp.spec"),
            disp: g.g("spc.disp"),
            res: g.g("spc.res"),
            eff: g.g("spc.eff"),
            rate: g.g("acq.rate"),
            celsius: if g.g("acq.cool") > 0.5 { 35.0 } else { 15.0 },
        }
    }
}

pub const GROUPS: [(&str, &str); 5] = [
    ("src", "Source"),
    ("opt", "Optical system (collimators, objective, pinholes)"),
    ("smp", "Sample"),
    ("spc", "Spectrometer"),
    ("acq", "Acquisition"),
];

pub fn defs() -> Vec<VarDef> {
    let v = |id: &str, label, unit, scale, min, max, default| {
        VarDef::new(id, label, unit, scale, min, max, default)
    };
    vec![
        v("src.i", "LED drive current", "mA", MA, 50.0 * MA, 1000.0 * MA, 350.0 * MA)
            .log()
            .doc("Source brightness. The datasheet maximum is 1 A; there is no thermal or lifetime model, so the optimiser never changes it.")
            .see(&["acq.rate", "acq.cool"]),
        v("opt.na", "Numerical aperture (sample side)", "", 1.0, 0.02, 0.7, 0.1)
            .log()
            .design()
            .doc("Sets lateral resolution (about 0.37 to 0.9 λ/NA depending on the pinhole), axial resolution (∝ 1/NA²), diffuse light capture (∝ NA²) and aberrations (∝ NA⁴). The light into the pinhole does not depend on it at a fixed pinhole size in Airy units.")
            .see(&["opt.au", "opt.a", "opt.dz"])
            .refs(AIRY),
        v("opt.au", "Pinhole size (Airy units)", "AU", 1.0, 0.1, 6.0, 0.95)
            .log()
            .design()
            .doc("Diameter of the source and detection pinholes in units of the Airy disc (1.22 λ / NA at the pinhole). Small: sharp but dim. Large: bright but less confocal. The physical diameter follows from the magnification, which belongs to the real parts and does not change the performance.")
            .see(&["opt.na", "opt.ratio", "src.i"])
            .refs(CONFOCAL),
        v("opt.ratio", "Detection / source pinhole size", "", 1.0, 0.25, 4.0, 1.0)
            .log()
            .design()
            .doc("Advanced. 1 is a true confocal. A larger detection pinhole collects more light from diffuse samples at the price of axial resolution and background rejection.")
            .see(&["opt.au", "smp.spec"]),
        v("opt.dz", "Chromatic depth range (450-670 nm)", "µm", UM, 100.0 * UM, 3000.0 * UM, 700.0 * UM)
            .doc("A requirement: the height span covered when the in-focus wavelength goes from 450 to 670 nm. Height noise scales with it, so it is never optimised.")
            .see(&["opt.shape", "spc.disp"]),
        VarDef::new("opt.shape", "Dispersion shape", "", 1.0, 0.0, 6.0, 1.0)
            .pick(SHAPES, 1)
            .doc("How focus depends on wavelength. A singlet is nonlinear (finer height steps at the blue end); linear and diffractive are idealised. For the refractive and diffractive shapes the NA also changes with wavelength because the stop is fixed (NA ∝ 1/f(λ)).")
            .see(&["opt.dz", "opt.na"]),
        v("opt.t", "Optical throughput, one pass", "", 1.0, 0.2, 1.0, 0.8)
            .doc("Transmission of the collimator and objective in one direction. The beam splitter (50:50, 2 % loss) is a constant.")
            .see(&["src.i", "spc.eff"]),
        VarDef::new("opt.ab", "Aberrations", "", 1.0, 0.0, 1.0, 1.0)
            .pick(&["Off (ideal)", "On"], 1)
            .doc("Tick to include wavefront error in the objective."),
        v("opt.a", "RMS wavefront error at NA 0.1", "waves", 1.0, 0.001, 0.3, 0.02)
            .log()
            .hide_when("ab", 0.0)
            .doc("Scales as NA⁴ (spherical aberration), so raising NA is not free. 0.02 is the uncorrected singlet; Maréchal: Strehl = exp(-(2π σ)²), applied squared to the diffuse return and to the fourth power to the mirror return.")
            .see(&["opt.na"]),
        v("opt.stray", "Stray light", "", 1.0, 1e-9, 1e-2, 1e-6)
            .log()
            .doc("Fraction of the light entering pinhole 1 that reaches the detector without being focused (back-reflections, scatter). The largest unknown of a real confocal; it adds a background under the peak.")
            .see(&["opt.au", "src.i"]),
        v("smp.r", "Reflectance", "", 1.0, 0.01, 1.0, 0.6).doc("Total fraction of the light the surface returns.").see(&["smp.spec"]),
        v("smp.spec", "Specular fraction", "", 1.0, 0.0, 1.0, 0.5)
            .doc("Mirror-like part of the reflection (needs the surface within the NA acceptance); the rest is diffuse and only the NA² fraction is collected. A pure mirror is bright but saturates easily.")
            .see(&["opt.ratio", "opt.na"]),
        v("spc.disp", "Dispersion", "nm/px", NM, 0.1 * NM, 5.0 * NM, 0.5 * NM)
            .log()
            .design()
            .doc("Wavelength per pixel on the 1024-pixel sensor. Fine: many pixels per peak, more read noise per signal electron. Coarse: fewer pixels, more light per pixel, until the peak spans only a handful of pixels. The sensor is centred on 560 nm; a warning appears if the in-focus band plus the peak width does not fit.")
            .see(&["spc.res", "opt.dz", "acq.rate"]),
        v("spc.res", "Spectral resolution (FWHM)", "nm", NM, 0.2 * NM, 20.0 * NM, 1.0 * NM)
            .log()
            .doc("Blur of the spectrometer. It barely matters while it is below about one fifth of the peak width (narrow peaks, high NA, need a better spectrometer).")
            .see(&["spc.disp", "opt.na"]),
        v("spc.eff", "Spectrometer efficiency", "", 1.0, 0.05, 1.0, 0.5)
            .doc("Grating and optics, excluding the sensor's quantum efficiency (a constant).")
            .see(&["opt.t"]),
        v("acq.rate", "Spectra per second", "kHz", 1e3, 1.0, 5e4, 1000.0)
            .log()
            .doc("Measurement speed (one height per spectrum). Exposure, read clock, binning and read-out region are chosen automatically.")
            .see(&["acq.cool", "src.i"]),
        VarDef::new("acq.cool", "Sensor temperature", "", 1.0, 0.0, 1.0, 1.0)
            .pick(COOLING, 1)
            .doc("Dark current doubles every 8 K, so cooling to 15 °C cuts it by a factor of 5.7.")
            .see(&["acq.rate"]),
    ]
}
