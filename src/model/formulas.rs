use crate::engine::{
    formula::{Formula, Refs},
    get::Get,
    values::Values,
};
use crate::physics::{aberration, detector, geometry, glass, spectrometer as spec};

const ABBE: Refs = &[(
    "Abbe sine condition",
    "https://en.wikipedia.org/wiki/Abbe_sine_condition",
)];
const AIRY: Refs = &[("Airy disk", "https://en.wikipedia.org/wiki/Airy_disk")];
const EPC: Refs = &[(
    "epc901 datasheet",
    "https://www.espros.com/photonics/epc901/",
)];
const GRATING: Refs = &[(
    "Diffraction grating",
    "https://en.wikipedia.org/wiki/Diffraction_grating",
)];
const SA: Refs = &[
    (
        "Spherical aberration",
        "https://en.wikipedia.org/wiki/Spherical_aberration",
    ),
    ("Strehl ratio", "https://en.wikipedia.org/wiki/Strehl_ratio"),
];

fn focal(v: &Values, id: &str) -> f64 {
    if id == "obj.f" {
        crate::physics::dispersion::f_obj(v, v.g("sys.lam0"))
    } else {
        v.g(id)
    }
}

fn na_formula(inst: &str, title: &str) -> Formula {
    let ids = ["na", "d", "f"].map(|l| format!("{inst}.{l}"));
    Formula::new(&format!("na-{inst}"), title, &ids.each_ref().map(String::as_str), 3, "NA = D / (2 f(λ)), f taken at the working wavelength", |v, n| v.g(&n[1]) / (2.0 * focal(v, &n[2])))
        .always()
        .refs(ABBE)
        .desc("Numerical aperture from aperture diameter and focal length (valid for a lens that obeys the Abbe sine condition). Whichever of the three you do not touch gives way: NA first, then the diameter.")
}

fn magnification_formula(lens: &str, title: &str) -> Formula {
    let ids = [format!("{lens}.m"), format!("{lens}.f")];
    Formula::new(&format!("m-{lens}"), title, &[&ids[0], &ids[1], "obj.f"], 3, "m = f_obj(λ) / f", |v, n| focal(v, &n[2]) / v.g(&n[1]))
        .always()
        .desc("Magnification with which the pinhole is imaged into the sample: objective focal length over this lens's focal length.")
}

fn airy_formula(pin: &str, lens: &str, title: &str, det: bool) -> Formula {
    let ids = [format!("{pin}.au"), format!("{pin}.d")];
    let vars = [
        ids[0].as_str(),
        ids[1].as_str(),
        "obj.na",
        "obj.f",
        "coll.f",
        "coll.d",
        "dcoll.f",
        "dcoll.d",
        "obj.glass",
        "obj.lamf",
        "sys.lam0",
    ];
    let lens = lens.to_string();
    Formula::new(&format!("airy-{pin}"), title, &vars, 2, "AU = d NA_pin / (1.22 lambda)", move |v, n| pin_au(v, &n[1], &lens, det))
        .always()
        .refs(AIRY)
        .desc("Pinhole diameter in units of the Airy-disc diameter 1.22 lambda / NA, evaluated in the pinhole plane. Drag either the size or the Airy units; the other follows.")
}

fn pin_au(v: &Values, d_id: &str, lens: &str, det: bool) -> f64 {
    let na = if det {
        geometry::na_det(v)
    } else {
        geometry::na_ill(v)
    };
    let f_o = crate::physics::dispersion::f_obj(v, v.g("sys.lam0"));
    v.g(d_id) * na * f_o / (1.22 * v.g("sys.lam0") * v.g(&format!("{lens}.f")))
}

pub fn formulas() -> Vec<Formula> {
    vec![
        na_formula("obj", "Objective NA"),
        na_formula("coll", "Collimator NA"),
        na_formula("dcoll", "De-collimator NA"),
        magnification_formula("coll", "Collimator relay magnification"),
        magnification_formula("dcoll", "De-collimator relay magnification"),
        airy_formula("pin1", "coll", "Pinhole 1 size in Airy units", false),
        airy_formula("pin2", "dcoll", "Pinhole 2 size in Airy units", true),
        Formula::new("same-pinholes", "Identical pinholes", &["pin2.d", "pin1.d"], 2, "d₂ = d₁", |v, _| v.g("pin1.d"))
            .desc("Detection pinhole has the same diameter as the source pinhole. Dragging either one moves the other."),
        Formula::new("read-clock", "Read clock from sample rate", &["det.fread", "det.bin", "acq.rate"], 1, "f_read = 1.1 · N_clk · rate", |v, _| 1.1 * detector::clocks(v) * v.g("acq.rate"))
            .refs(EPC)
            .desc("Slowest (quietest) epc901 read clock that still reads a spectrum within one sample period, with 10 % margin. Turn off to set the clock by hand."),
        Formula::new("exposure", "Exposure from sample rate", &["acq.texp", "acq.rate", "acq.overlap", "det.fread", "det.bin"], 1, "t_exp = 1/rate − t_overhead (− t_read if sequential)", |v, _| {
            let t = 1.0 / v.g("acq.rate") - detector::overhead_s() - if v.g("acq.overlap") > 0.5 { detector::clocks(v) / v.g("det.fread") } else { 0.0 };
            if t > 0.0 { t } else { f64::NAN }
        })
        .refs(EPC)
        .desc("Exposure fills the sample period minus the frame-shift overhead, and minus the read-out time when read-out cannot overlap exposure. NaN when not positive."),
        Formula::new("sa-rms", "Wavefront error from singlet spherical aberration", &["obj.rms", "obj.f", "obj.glass", "obj.ct", "obj.na", "sys.lam0"], 1, "σ = 0.0745 · (LSA · NA² / 4) / λ", |v, _| {
            let g = v.g("obj.glass") as usize;
            if g >= 5 {
                return 0.0;
            }
            let lsa = aberration::pcx_marginal_lsa(v.g("obj.f"), glass::index(g, v.g("obj.lamf")), glass::index(g, v.g("sys.lam0")), v.g("obj.ct"), v.g("obj.na"));
            aberration::sa_rms_waves(lsa, v.g("obj.na"), v.g("sys.lam0"))
        })
        .refs(SA)
        .desc("RMS wavefront error of a plano-convex singlet from a meridional ray trace (curved side toward the collimated beam). Turn off to enter the wavefront error by hand, for example from Zemax."),
        Formula::new("grating-dispersion", "Spectrometer dispersion", &["spec.dlam", "spec.f_cam", "spec.lines", "spec.order", "spec.alpha", "spec.lam_c"], 3, "Δλ/px = d cos β · p / (m f_cam),  m λ = d (sin α + sin β)", |v, _| spec::grating_dlam(v))
            .always()
            .refs(GRATING)
            .desc("Wavelength per pixel follows from the grating and the camera focal length. Drag the dispersion and the camera focal length moves; drag the camera focal length or the line density and the dispersion follows."),
    ]
}
