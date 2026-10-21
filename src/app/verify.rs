use super::widgets::fmt;
use crate::checks;
use crate::engine::values::Values;
use crate::physics::{detector, glass, montecarlo, radiometry as rad, spectral};
use crate::system::{
    inputs::SHAPES,
    optics::{Setup, BAND, N_PX},
    outputs::Report,
    readout,
};
use eframe::egui::{self, Color32, RichText, Ui};
use egui_plot::{Legend, Line, Plot, PlotPoints, Points};

pub const PAGES: [&str; 7] = [
    "Self-checks",
    "LED source",
    "Detector",
    "Dispersion",
    "Spectrometer",
    "Photon budget",
    "Realism check",
];

struct Series {
    name: &'static str,
    pts: Vec<[f64; 2]>,
    markers: bool,
}

fn line(name: &'static str, pts: Vec<[f64; 2]>) -> Series {
    Series {
        name,
        pts,
        markers: false,
    }
}

fn dots(name: &'static str, pts: Vec<[f64; 2]>) -> Series {
    Series {
        name,
        pts,
        markers: true,
    }
}

fn plot(ui: &mut Ui, id: &str, x: &str, y: &str, series: Vec<Series>) {
    Plot::new(id)
        .allow_scroll(false)
        .height(300.0)
        .legend(Legend::default())
        .x_axis_label(x)
        .y_axis_label(y)
        .show(ui, |p| {
            for s in series {
                if s.markers {
                    p.points(Points::new(s.name, PlotPoints::from(s.pts)).radius(3.0_f32));
                } else {
                    p.line(Line::new(s.name, PlotPoints::from(s.pts)));
                }
            }
        });
}

fn sweep(lo: f64, hi: f64, n: usize, f: impl Fn(f64) -> f64) -> Vec<[f64; 2]> {
    (0..n)
        .map(|k| lo + (hi - lo) * k as f64 / (n - 1) as f64)
        .map(|x| [x, f(x)])
        .collect()
}

fn curve_points(c: &crate::engine::curve::Curve, scale: f64) -> Vec<[f64; 2]> {
    c.0.iter().map(|&(x, y)| [x, y * scale]).collect()
}

fn table(ui: &mut Ui, id: &str, rows: Vec<(String, String)>) {
    egui::Grid::new(id).striped(true).show(ui, |ui| {
        for (a, b) in rows {
            ui.label(a);
            ui.label(b);
            ui.end_row();
        }
    });
}

fn self_checks(ui: &mut Ui) {
    ui.weak("Each row recomputes a known result from the model's own functions. Green means the code agrees with the literature, datasheet or an exact identity within the tolerance.");
    egui::Grid::new("checks").striped(true).show(ui, |ui| {
        for h in ["Check", "Computed", "Expected", "Error", "", "Source"] {
            ui.label(RichText::new(h).weak());
        }
        ui.end_row();
        for c in checks::checks() {
            ui.label(c.name);
            ui.label(format!("{} {}", fmt(c.computed), c.unit));
            ui.label(format!("{} {}", fmt(c.expected), c.unit));
            ui.label(format!(
                "{:.1} % (tolerance {:.1} %)",
                c.error() * 100.0,
                c.tol * 100.0
            ));
            ui.colored_label(
                if c.ok() {
                    Color32::LIGHT_GREEN
                } else {
                    Color32::LIGHT_RED
                },
                if c.ok() { "✔" } else { "⚠" },
            );
            ui.hyperlink_to(c.source.0, c.source.1);
            ui.end_row();
        }
    });
}

fn led(ui: &mut Ui, s: &Setup) {
    let (l, vl) = (spectral::led(), spectral::vlambda());
    ui.weak("The LED curve was traced from the datasheet's relative-spectral-emission figure (4000 K, black curve) by pixel analysis; the dots are the traced points. Compare with the datasheet: peak near 455 nm, dip near 480 nm, broad maximum near 620 nm.");
    plot(
        ui,
        "v-led",
        "wavelength (nm)",
        "relative",
        vec![
            line(
                "LED emission (relative)",
                sweep(400.0, 800.0, 201, |x| l.at(x)),
            ),
            dots("traced points", curve_points(l, 1.0)),
            line("photopic V(λ)", sweep(400.0, 800.0, 201, |x| vl.at(x))),
            line(
                "LED × V(λ)",
                sweep(400.0, 800.0, 201, |x| l.at(x) * vl.at(x)),
            ),
        ],
    );
    let lumens = rad::flux_at(rad::LED_FLUX_REF, s.p.current);
    let watts = rad::watts(lumens);
    table(
        ui,
        "led-numbers",
        vec![
            (
                "Luminous flux at this drive current".into(),
                format!("{} lm", fmt(lumens)),
            ),
            (
                "Radiant flux (flux / 683 <V>)".into(),
                format!("{} W", fmt(watts)),
            ),
            (
                "Luminous efficacy of radiation".into(),
                format!("{} lm/W", fmt(lumens / watts)),
            ),
            (
                "Radiance (flux / π A, 1.1 × 1.1 mm)".into(),
                format!(
                    "{} W/(mm² sr)",
                    fmt(rad::radiance_of(lumens, rad::LED_SIDE) / 1e6)
                ),
            ),
        ],
    );
}

fn detector_page(ui: &mut Ui, r: &Report) {
    let qe = spectral::qe();
    ui.weak("QE traced from datasheet figure 6 (dots). Read noise and full well are interpolated in the read clock between the datasheet's 1, 10 and 54 MHz entries (dots: 700 µV at 1 MHz from the electrical table, 1.9 mV and 1.8 mV derived from the SNR table). Below 1 MHz (the datasheet's slowest entry) the values are held at their 1 MHz level, which is why the curves are flat there. The operating point chosen automatically (the read clock needed for the spectra per second you asked for) is marked.");
    plot(
        ui,
        "v-qe",
        "wavelength (nm)",
        "quantum efficiency (%)",
        vec![
            line("QE", sweep(400.0, 1000.0, 121, |x| 100.0 * qe.at(x))),
            dots("traced points", curve_points(qe, 100.0)),
        ],
    );
    let at = r.assessment.readout.fread / 1e6;
    let cg = detector::CONVERSION_GAIN;
    let table_pts = |k: f64| {
        detector::NOISE_V
            .iter()
            .map(|&(f, x)| [f / 1e6, k * x / cg])
            .collect::<Vec<_>>()
    };
    plot(
        ui,
        "v-noise",
        "read clock (MHz)",
        "read noise (e-)",
        vec![
            line(
                "interpolated",
                sweep(0.1, 54.0, 270, |x| detector::read_noise_at(x * 1e6)),
            ),
            dots("datasheet", table_pts(1.0)),
            dots(
                "operating point",
                vec![[at, detector::read_noise_at(at * 1e6)]],
            ),
        ],
    );
    plot(
        ui,
        "v-fw",
        "read clock (MHz)",
        "usable full well (ke-)",
        vec![
            line(
                "interpolated",
                sweep(0.1, 54.0, 270, |x| detector::full_well_at(x * 1e6) / 1e3),
            ),
            dots(
                "datasheet swing",
                detector::SWING_V
                    .iter()
                    .map(|&(f, x)| [f / 1e6, x / cg / 1e3])
                    .collect(),
            ),
        ],
    );
}

fn dispersion(ui: &mut Ui, s: &Setup) {
    ui.weak(format!("Dispersion shape: {}. The refractive shapes use the Sellmeier equation of the named glass (it reproduces the catalogue index and Abbe number, see Self-checks); the normalised curve is scaled to the depth range.", SHAPES[s.p.shape.min(SHAPES.len() - 1)]));
    if (1..=5).contains(&s.p.shape) {
        plot(
            ui,
            "v-index",
            "wavelength (nm)",
            "refractive index",
            vec![line(
                "n(λ)",
                sweep(400.0, 800.0, 101, |x| glass::index(s.p.shape - 1, x * 1e-9)),
            )],
        );
    }
    plot(
        ui,
        "v-z",
        "wavelength (nm)",
        "focus height (µm)",
        vec![line(
            "z(λ)",
            sweep(400.0, 800.0, 101, |x| s.disp.z(x * 1e-9) * 1e6),
        )],
    );
    plot(
        ui,
        "v-nalam",
        "wavelength (nm)",
        "NA",
        vec![line(
            "NA(λ) with a fixed stop",
            sweep(400.0, 800.0, 101, |x| s.na(x * 1e-9)),
        )],
    );
}

fn spectrometer_page(ui: &mut Ui, s: &Setup, r: &Report) {
    ui.weak("The sensor is centred on 560 nm. The in-focus band (450-670 nm) plus one peak width on each side has to fit on the 1024 pixels or the peaks at the ends of the depth range are cut.");
    plot(
        ui,
        "v-px",
        "pixel",
        "wavelength (nm)",
        vec![
            line(
                "λ(pixel)",
                s.lam
                    .iter()
                    .enumerate()
                    .map(|(i, l)| [i as f64, l * 1e9])
                    .collect(),
            ),
            line(
                "band start",
                vec![[0.0, BAND.0 * 1e9], [N_PX as f64, BAND.0 * 1e9]],
            ),
            line(
                "band end",
                vec![[0.0, BAND.1 * 1e9], [N_PX as f64, BAND.1 * 1e9]],
            ),
        ],
    );
    let sigma_res = s.p.res / 2.355 / s.p.disp;
    let pixel = (1.0f64 / 12.0).sqrt();
    let total = s.sigma_px;
    table(
        ui,
        "res-terms",
        vec![
            (
                "Spectrometer blur (resolution knob)".into(),
                format!("σ = {} px", fmt(sigma_res)),
            ),
            ("Pixel width".into(), format!("σ = {} px", fmt(pixel))),
            (
                "Pixel crosstalk (20 %, 4 µm)".into(),
                format!(
                    "σ = {} px",
                    fmt((total * total - sigma_res * sigma_res - pixel * pixel)
                        .max(0.0)
                        .sqrt())
                ),
            ),
            (
                "Total (quadrature)".into(),
                format!(
                    "σ = {} px = FWHM {} nm",
                    fmt(total),
                    fmt(2.355 * total * s.p.disp * 1e9)
                ),
            ),
            (
                "Narrowest / widest spectral peak".into(),
                format!(
                    "{} / {} nm",
                    fmt(r
                        .nodes
                        .iter()
                        .map(|n| n.peak_nm)
                        .fold(f64::INFINITY, f64::min)
                        * 1e9),
                    fmt(r.nodes.iter().map(|n| n.peak_nm).fold(0.0, f64::max) * 1e9)
                ),
            ),
        ],
    );
}

fn budget(ui: &mut Ui, r: &Report) {
    ui.weak("Power at the middle of the band, in focus, stage by stage. Multiply the factors by hand to check the electron count. It is the unblurred peak pixel at the exposure the tool chose.");
    egui::Grid::new("budget").striped(true).show(ui, |ui| {
        for h in ["Stage", "Factor", "Running total"] {
            ui.label(RichText::new(h).weak());
        }
        ui.end_row();
        let mut total = 1.0;
        for (k, (name, factor)) in r.budget.iter().enumerate() {
            total = if k == 0 { *factor } else { total * factor };
            ui.label(*name);
            ui.label(fmt(*factor));
            ui.label(if k + 1 == r.budget.len() {
                format!("{} e- per pixel per exposure", fmt(total))
            } else {
                format!("{} W/nm", fmt(total))
            });
            ui.end_row();
        }
    });
    let mid = &r.nodes[r.nodes.len() / 2];
    let peak = mid.rate.iter().cloned().fold(0.0, f64::max) * r.assessment.readout.texp;
    ui.label(format!(
        "Modelled peak pixel at this height (after spectrometer blur): {} e-",
        fmt(peak)
    ));
}

fn realism(ui: &mut Ui, inputs: &Values, r: &Report) {
    ui.weak("The Cramér–Rao bound assumes the peak shape, amplitude and background are known. A simple thresholded centroid, run on 120 noisy synthetic spectra at the middle of the depth range, shows what a basic estimator achieves.");
    let s = Setup::new(inputs);
    let n = readout::node(&s, r.nodes.len() / 2);
    let a = &r.assessment.readout;
    let (rn, dark) = (
        detector::read_noise_at(a.fread),
        detector::dark_rate_at(s.p.celsius),
    );
    let sig: Vec<f64> = n.rate.iter().map(|x| x * a.texp).collect();
    let var: Vec<f64> = sig
        .iter()
        .zip(&n.stray)
        .map(|(s, b)| s + (b + dark) * a.texp + rn * rn)
        .collect();
    let lam: Vec<f64> = (0..sig.len()).map(|i| s.lam[n.first + i]).collect();
    let sigma_lam = montecarlo::centroid_sigma(&sig, &var, &lam);
    let sigma_z = sigma_lam * s.disp.slope(n.lam).abs();
    let crb = readout::assess(
        &s,
        std::slice::from_ref(&n),
        readout::Readout {
            bin: 0,
            roi: false,
            fread: a.fread,
            texp: a.texp,
        },
    )
    .sigma[0];
    table(
        ui,
        "mc",
        vec![
            (
                "Height".into(),
                format!("{} µm ({} nm in focus)", fmt(n.z * 1e6), fmt(n.lam * 1e9)),
            ),
            (
                "Cramér–Rao σ_z (no binning)".into(),
                format!("{} nm", fmt(crb * 1e9)),
            ),
            (
                "Thresholded-centroid scatter".into(),
                format!("{} nm", fmt(sigma_z * 1e9)),
            ),
            ("Ratio".into(), fmt(sigma_z / crb)),
        ],
    );
}

pub fn ui(ui: &mut Ui, page: &mut usize, inputs: &Values, r: Option<&Report>) {
    ui.horizontal_wrapped(|ui| {
        for (k, name) in PAGES.iter().enumerate() {
            ui.selectable_value(page, k, *name);
        }
    });
    let Some(r) = r else {
        ui.label("Waiting for the first evaluation…");
        return;
    };
    let s = Setup::new(inputs);
    match *page {
        0 => self_checks(ui),
        1 => led(ui, &s),
        2 => detector_page(ui, r),
        3 => dispersion(ui, &s),
        4 => spectrometer_page(ui, &s, r),
        5 => budget(ui, r),
        _ => realism(ui, inputs, r),
    }
}
