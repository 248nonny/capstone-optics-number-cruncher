use super::widgets::fmt;
use crate::engine::{get::Get, model::Model, values::Values};
use crate::model::{outputs::Evaluation, verify};
use crate::physics::{detector, dispersion as disp, glass, spectral, spectrometer as spec};
use eframe::egui::{self, Color32, RichText, Ui};
use egui_plot::{Legend, Line, Plot, PlotPoints, Points};

pub const PAGES: [&str; 6] = [
    "Self-checks",
    "LED source",
    "Detector",
    "Glass and focus",
    "Spectrometer",
    "Photon budget",
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

fn checks(ui: &mut Ui, m: &Model) {
    ui.weak("Each row recomputes a known result from the model's own functions. Green means the code agrees with the literature or datasheet value within the tolerance.");
    egui::Grid::new("checks").striped(true).show(ui, |ui| {
        for h in ["Check", "Computed", "Expected", "Error", "", "Source"] {
            ui.label(RichText::new(h).weak());
        }
        ui.end_row();
        for c in verify::checks(m) {
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

fn led(ui: &mut Ui, v: &Values) {
    let (l, vl) = (spectral::led(), spectral::vlambda());
    ui.weak("The LED curve was traced from the datasheet's relative-spectral-emission figure (4000 K, black curve) by pixel analysis; the dots are the traced points. Compare with the datasheet figure: peak near 455 nm, dip near 480 nm, broad maximum near 620 nm.");
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
    let (flux, pe, rad) = (v.g("led.flux"), v.g("led.pe"), v.g("led.rad"));
    egui::Grid::new("led-numbers").striped(true).show(ui, |ui| {
        for (name, value) in [
            ("Luminous flux", format!("{} lm", fmt(flux))),
            ("Radiant flux (flux / 683 <V>)", format!("{} W", fmt(pe))),
            (
                "Luminous efficacy of radiation",
                format!("{} lm/W", fmt(flux / pe)),
            ),
            (
                "Radiance (flux / π A)",
                format!("{} W/(mm² sr)", fmt(rad / 1e6)),
            ),
            (
                "Emitting area",
                format!("{} mm²", fmt((v.g("led.side") * 1e3).powi(2))),
            ),
        ] {
            ui.label(name);
            ui.label(value);
            ui.end_row();
        }
    });
}

fn detector_page(ui: &mut Ui, v: &Values) {
    let qe = spectral::qe();
    ui.weak("QE traced from datasheet figure 6 (dots). Read noise and full well are interpolated in the read clock between the datasheet's 1, 10 and 54 MHz table entries (dots are the table values: 700 uV at 1 MHz from the electrical table, 1.9 mV and 1.8 mV derived from the SNR table).");
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
    let at = |f: f64, mode: f64, get: fn(&Values) -> f64| {
        let mut t = v.clone();
        t.set("det.fread", f);
        t.set("det.mode", mode);
        get(&t)
    };
    let mhz = |lo: f64, hi: f64, mode: f64, get: fn(&Values) -> f64| {
        sweep(lo, hi, 60, |x| at(x * 1e6, mode, get))
    };
    let cg = v.g("det.cg");
    let table = |pts: &[(f64, f64)], k: f64| {
        pts.iter()
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
                "differential",
                mhz(1.0, 54.0, 0.0, |t| detector::read_noise_e(t)),
            ),
            line(
                "single-ended",
                mhz(1.0, 54.0, 1.0, |t| detector::read_noise_e(t)),
            ),
            dots("datasheet (differential)", table(&detector::NOISE_V, 1.0)),
            dots(
                "datasheet (single-ended, scaled 0.5/0.7)",
                table(&detector::NOISE_V, detector::SE_NOISE),
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
                "differential",
                mhz(1.0, 54.0, 0.0, |t| detector::full_well_e(t) / 1e3),
            ),
            line(
                "single-ended",
                mhz(1.0, 54.0, 1.0, |t| detector::full_well_e(t) / 1e3),
            ),
            dots(
                "datasheet swing (differential)",
                table(&detector::SWING_V, 1e-3),
            ),
        ],
    );
}

fn glass_page(ui: &mut Ui, v: &Values) {
    let g = v.g("obj.glass") as usize;
    ui.weak(format!("Objective dispersion model: {}. Sellmeier coefficients reproduce each glass's catalogue n_d and Abbe number (see Self-checks).", glass::NAMES[g.min(5)]));
    if g < 5 {
        plot(
            ui,
            "v-index",
            "wavelength (nm)",
            "refractive index",
            vec![line(
                "n(λ)",
                sweep(400.0, 800.0, 101, |x| glass::index(g, x * 1e-9)),
            )],
        );
    }
    plot(
        ui,
        "v-f",
        "wavelength (nm)",
        "objective focal length (mm)",
        vec![line(
            "f(λ)",
            sweep(400.0, 800.0, 101, |x| disp::f_obj(v, x * 1e-9) * 1e3),
        )],
    );
    let lam0 = v.g("sys.lam0");
    ui.label(format!(
        "Chromatic slope at {} nm: {} µm per nm; focal shift 450 → 650 nm: {} µm",
        fmt(lam0 * 1e9),
        fmt(disp::slope(v, lam0).abs() * 1e-3),
        fmt((disp::f_obj(v, 650e-9) - disp::f_obj(v, 450e-9)).abs() * 1e6)
    ));
}

fn spectrometer_page(ui: &mut Ui, v: &Values, e: &Evaluation) {
    ui.weak("Pixel-to-wavelength map, grating efficiency, and the four terms that add in quadrature to the optical resolution.");
    plot(
        ui,
        "v-px",
        "pixel",
        "wavelength (nm)",
        vec![line(
            "λ(pixel)",
            e.signal
                .lam
                .iter()
                .enumerate()
                .map(|(i, l)| [i as f64, l * 1e9])
                .collect(),
        )],
    );
    plot(
        ui,
        "v-eta",
        "wavelength (nm)",
        "efficiency",
        vec![
            line(
                "grating efficiency",
                sweep(400.0, 800.0, 101, |x| spec::grating_eff(v, x * 1e-9)),
            ),
            line(
                "detector QE",
                sweep(400.0, 800.0, 101, |x| spectral::qe().at(x)),
            ),
        ],
    );
    let terms = spec::resolution_terms(v);
    egui::Grid::new("res-terms").striped(true).show(ui, |ui| {
        for (name, s) in terms {
            ui.label(name);
            ui.label(format!(
                "σ = {} px = {} nm",
                fmt(s),
                fmt(s * v.g("spec.dlam") * 1e9)
            ));
            ui.end_row();
        }
        ui.label(RichText::new("Total (quadrature), FWHM").strong());
        ui.label(format!(
            "{} nm",
            fmt(2.355 * spec::resolution_sigma_px(v) * v.g("spec.dlam") * 1e9)
        ));
        ui.end_row();
    });
}

fn budget(ui: &mut Ui, e: &Evaluation) {
    ui.weak("Power at the reference wavelength, in focus, stage by stage. Multiply the factors by hand to check the final electron count; it is the unblurred peak pixel, so the modelled peak (which includes spectrometer blur) is slightly lower.");
    egui::Grid::new("budget").striped(true).show(ui, |ui| {
        for h in ["Stage", "Factor", "Running total"] {
            ui.label(RichText::new(h).weak());
        }
        ui.end_row();
        let mut total = 1.0;
        for (k, (name, factor)) in e.signal.budget.iter().enumerate() {
            total = if k == 0 { *factor } else { total * factor };
            ui.label(*name);
            ui.label(fmt(*factor));
            ui.label(if k + 1 == e.signal.budget.len() {
                format!("{} e- per pixel per exposure", fmt(total))
            } else {
                format!("{} W/nm", fmt(total))
            });
            ui.end_row();
        }
    });
    ui.label(format!(
        "Modelled peak pixel (after spectrometer blur): {} e-",
        fmt(e.signal.sig.iter().cloned().fold(0.0, f64::max))
    ));
}

pub fn ui(ui: &mut Ui, page: &mut usize, m: &Model, e: Option<&Evaluation>) {
    ui.horizontal_wrapped(|ui| {
        for (k, name) in PAGES.iter().enumerate() {
            ui.selectable_value(page, k, *name);
        }
    });
    match (*page, e) {
        (0, _) => checks(ui, m),
        (_, None) => {
            ui.label("Waiting for the first evaluation…");
        }
        (1, Some(e)) => led(ui, &e.values),
        (2, Some(e)) => detector_page(ui, &e.values),
        (3, Some(e)) => glass_page(ui, &e.values),
        (4, Some(e)) => spectrometer_page(ui, &e.values, e),
        (_, Some(e)) => budget(ui, e),
    }
}
