use crate::engine::values::Values;
use crate::physics::{
    aberration, confocal, detector, glass, halfmax, radiometry, special, spectrometer,
};
use crate::system;
use std::f64::consts::PI;

pub struct Check {
    pub name: &'static str,
    pub unit: &'static str,
    pub computed: f64,
    pub expected: f64,
    pub tol: f64,
    pub source: (&'static str, &'static str),
}

impl Check {
    pub fn error(&self) -> f64 {
        (self.computed - self.expected).abs() / self.expected.abs()
    }

    pub fn ok(&self) -> bool {
        self.error() <= self.tol
    }
}

type Source = (&'static str, &'static str);
const WILSON: Source = (
    "Confocal point-pinhole limits (arXiv 1404.2408)",
    "https://arxiv.org/pdf/1404.2408",
);
const SCHOTT: Source = (
    "Schott glass catalogue",
    "https://www.schott.com/en-gb/products/optical-glass",
);
const EPC: Source = (
    "epc901 datasheet",
    "https://www.espros.com/photonics/epc901/",
);
const NEWPORT: Source = (
    "Newport SFPX040 listing",
    "https://www.newport.com/p/SFPX040",
);
const BORN: Source = ("Airy disk", "https://en.wikipedia.org/wiki/Airy_disk");
const LED: Source = ("OSRAM GW VJLPL1.CM datasheet", "https://look.ams-osram.com");
const STREHL: Source = (
    "Maréchal criterion",
    "https://en.wikipedia.org/wiki/Strehl_ratio",
);
const ETENDUE: Source = ("Etendue", "https://en.wikipedia.org/wiki/Etendue");

fn check(
    name: &'static str,
    unit: &'static str,
    computed: f64,
    expected: f64,
    tol: f64,
    source: Source,
) -> Check {
    Check {
        name,
        unit,
        computed,
        expected,
        tol,
        source,
    }
}

fn point_object_fwhm_u() -> f64 {
    let t = confocal::table();
    let curve: Vec<f64> = (0..81).map(|k| t.at(0.0, k as f64 * 0.5).powi(2)).collect();
    halfmax::crossing(curve.into_iter())
}

fn airy_fwhm() -> f64 {
    let curve: Vec<f64> = (0..400).map(|k| special::airy(k as f64 * 0.01)).collect();
    2.0 * 0.01 * halfmax::crossing(curve.into_iter()) / (2.0 * PI)
}

fn snr_db(fread: f64) -> f64 {
    20.0 * (detector::full_well_at(fread) / detector::read_noise_at(fread)).log10()
}

fn pinhole_power(na: f64) -> f64 {
    let mut v: Values = system::model().defaults();
    v.set("opt.na", na);
    system::outputs::evaluate(&v).values.v[v.index["ph.p_pin"]]
}

pub fn checks() -> Vec<Check> {
    let point = confocal::axial(0.05, 0.05);
    let glasses = [
        ("N-SF11 refractive index n_d", 1.7847),
        ("N-BK7 refractive index n_d", 1.5168),
        ("F2 refractive index n_d", 1.6200),
        ("N-SF6HT refractive index n_d", 1.8052),
        ("Fused silica refractive index n_d", 1.4585),
    ];
    let mut out = vec![
        check("Airy wide-field intensity FWHM", "λ/NA", airy_fwhm(), 0.514, 0.01, BORN),
        check("Encircled energy inside the first Airy zero", "", confocal::axial(0.05, 3.8317).mirror[0], 0.838, 0.01, BORN),
        check("Confocal lateral FWHM, point pinholes", "λ/NA", confocal::lateral_fwhm(1.0, [1.0, 1.0], [1e-9, 1e-9]), 0.37, 0.04, WILSON),
        check("Mirror axial FWHM, point pinholes (= 2 × 0.443)", "λ/NA²", point.mirror_fwhm_u() / 2.0 / (2.0 * PI), 0.886, 0.03, ("Sinc² mirror response", "https://my.ece.utah.edu/~blair/T/ece6460/papers/confocal_intro.pdf")),
        check("Point-object axial FWHM, point pinholes, sinc⁴(u/4) (= 2 × 0.64)", "λ/NA²", point_object_fwhm_u() / (2.0 * PI), 1.28, 0.02, WILSON),
        check("Diffuse-plane axial FWHM vs point object (model value; no literature figure, should be slightly wider)", "λ/NA²", point.plane_fwhm_u() / (2.0 * PI), point_object_fwhm_u() / (2.0 * PI), 0.15, WILSON),
    ];
    for (g, (name, nd)) in glasses.into_iter().enumerate() {
        out.push(check(
            name,
            "",
            glass::index(g, 587.56e-9),
            nd,
            2e-4,
            SCHOTT,
        ));
    }
    let (f0, vd) = (12.7e-3, 25.68);
    let shift =
        glass::focal(f0, 0, 587.56e-9, 656.27e-9) - glass::focal(f0, 0, 587.56e-9, 486.13e-9);
    out.push(check(
        "N-SF11 singlet focal shift F–C versus the f/V_d rule",
        "mm",
        shift * 1e3,
        f0 / vd * 1e3,
        0.03,
        SCHOTT,
    ));
    out.push(check(
        "SFPX040 back focal length from the ray trace (R = f (n_d - 1), CT = 4 mm)",
        "mm",
        aberration::pcx_back_focus(12.7e-3, 1.7847, 1.7847, 4e-3, 1e-5) * 1e3,
        10.46,
        0.01,
        NEWPORT,
    ));
    out.push(check(
        "LED radiant flux / electrical power at 350 mA (2.8 V)",
        "",
        radiometry::watts(radiometry::LED_FLUX_REF) / (0.35 * 2.8),
        0.40,
        0.15,
        LED,
    ));
    out.push(check(
        "Maréchal: Strehl for λ/14 RMS wavefront error",
        "",
        (-(2.0 * PI / 14.0).powi(2)).exp(),
        0.80,
        0.03,
        STREHL,
    ));
    for (name, f, db) in [
        ("epc901 SNR at 1 MHz read clock", 1e6, 70.0),
        ("epc901 SNR at 10 MHz read clock", 1e7, 58.0),
        ("epc901 SNR at 54 MHz read clock", 5.4e7, 49.0),
    ] {
        out.push(check(name, "dB", snr_db(f), db, 0.03, EPC));
    }
    out.push(check(
        "epc901 read-out time, 515 clocks at 54 MHz",
        "µs",
        515.0 / 54e6 * 1e6,
        9.54,
        0.01,
        EPC,
    ));
    out.push(check(
        "300 lines/mm, f = 128 mm: dispersion for 200 nm over 1024 px",
        "nm/px",
        spectrometer::grating_dlam_for(300e3, 1.0, 10f64.to_radians(), 550e-9, 0.128) * 1e9,
        200.0 / 1024.0,
        0.005,
        (
            "Diffraction grating",
            "https://en.wikipedia.org/wiki/Diffraction_grating",
        ),
    ));
    out.push(check(
        "Power into pinhole 1 at fixed Airy-unit size: NA 0.3 / NA 0.1",
        "",
        pinhole_power(0.3) / pinhole_power(0.1),
        1.0,
        1e-6,
        ETENDUE,
    ));
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_self_check_passes() {
        for c in super::checks() {
            assert!(
                c.ok(),
                "{}: computed {} expected {} (error {:.3})",
                c.name,
                c.computed,
                c.expected,
                c.error()
            );
        }
    }
}
