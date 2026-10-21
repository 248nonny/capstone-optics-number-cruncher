pub mod library;

use crate::engine::{
    library::{Component, Library},
    values::Values,
};
use crate::physics::{aberration, glass};
use crate::system::optics::{BAND, CENTER, LAM0};

pub const SPECTROMETER_OPTICS: f64 = 0.85;

#[derive(Clone, Default, PartialEq)]
pub struct Selection {
    pub pinhole: Option<String>,
    pub lens: Option<String>,
    pub objective: Option<String>,
    pub grating: Option<String>,
    pub stop: Option<f64>,
}

#[derive(Clone, Default, Debug)]
pub struct Effective {
    pub na: Option<f64>,
    pub au: Option<f64>,
    pub dz: Option<f64>,
    pub shape: Option<usize>,
    pub t: Option<f64>,
    pub a: Option<f64>,
    pub eff: Option<f64>,
    pub magnification: Option<f64>,
    pub pinhole_d: Option<f64>,
    pub stop_d: Option<f64>,
    pub notes: Vec<String>,
}

fn param(c: &Component, key: &str) -> Option<f64> {
    c.params.get(key).copied()
}

fn glass_of(c: &Component) -> usize {
    param(c, "glass").unwrap_or(0.0) as usize
}

fn focal(c: &Component, lam: f64) -> f64 {
    glass::focal(
        param(c, "f").unwrap_or(f64::NAN),
        glass_of(c),
        param(c, "lamf").unwrap_or(LAM0),
        lam,
    )
}

pub fn effective(lib: &Library, sel: &Selection, target_na: f64) -> Effective {
    let part = |name: &Option<String>| name.as_deref().and_then(|n| lib.get(n));
    let (pin, lens, obj, grating) = (
        part(&sel.pinhole),
        part(&sel.lens),
        part(&sel.objective),
        part(&sel.grating),
    );
    let mut e = Effective::default();
    let f0 = obj.map(|o| focal(o, LAM0));
    if let (Some(o), Some(f0)) = (obj, f0) {
        let stop = sel
            .stop
            .unwrap_or(2.0 * target_na * f0)
            .min(param(o, "ca").unwrap_or(f64::INFINITY));
        let clip = lens
            .and_then(|l| param(l, "d"))
            .map_or(f64::INFINITY, |d| d / (2.0 * f0));
        let na = (stop / (2.0 * f0)).min(clip);
        if stop / (2.0 * f0) < 0.999 * target_na && sel.stop.is_none() {
            e.notes.push(format!(
                "The objective's clear aperture limits NA to {:.2}",
                stop / (2.0 * f0)
            ));
        }
        if clip < stop / (2.0 * f0) {
            e.notes
                .push(format!("The collimator's aperture limits NA to {na:.2}"));
        }
        let g = glass_of(o);
        let (f_lo, f_hi) = (focal(o, BAND.0), focal(o, BAND.1));
        (e.na, e.stop_d, e.shape, e.dz) =
            (Some(na), Some(stop), Some(g + 1), Some((f_hi - f_lo).abs()));
        if g < 5 {
            let n0 = glass::index(g, param(o, "lamf").unwrap_or(LAM0));
            let lsa = aberration::pcx_marginal_lsa(
                param(o, "f").unwrap_or(f64::NAN),
                n0,
                glass::index(g, LAM0),
                param(o, "ct").unwrap_or(4e-3),
                na,
            );
            e.a = Some(aberration::sa_rms_waves(lsa, na, LAM0) / (na / 0.1).powi(4));
        } else {
            (e.shape, e.a) = (Some(6), Some(0.0));
        }
        if let Some(l) = lens {
            e.magnification = Some(f0 / param(l, "f").unwrap_or(f64::NAN));
        }
    }
    e.t = lens
        .zip(obj)
        .and_then(|(l, o)| Some(param(l, "t")? * param(o, "t")?));
    if let (Some(p), Some(l), Some(na), Some(f_o)) = (pin, lens, e.na, f0) {
        let (d, f_c) = (
            param(p, "d").unwrap_or(f64::NAN),
            param(l, "f").unwrap_or(f64::NAN),
        );
        e.au = Some(d * na * f_o / f_c / (1.22 * LAM0));
        e.pinhole_d = Some(d);
    }
    if let Some(g) = grating {
        let envelope = (-((CENTER - param(g, "blaze").unwrap_or(CENTER))
            / param(g, "eta_w").unwrap_or(1.0))
        .powi(2))
        .exp();
        e.eff = Some(param(g, "eta").unwrap_or(1.0) * envelope * SPECTROMETER_OPTICS);
    }
    e
}

impl Effective {
    pub fn apply(&self, v: &mut Values) {
        let set =
            |v: &mut Values, id: &str, x: Option<f64>| x.into_iter().for_each(|x| v.set(id, x));
        set(v, "opt.na", self.na);
        set(v, "opt.au", self.au);
        set(v, "opt.dz", self.dz);
        set(v, "opt.shape", self.shape.map(|s| s as f64));
        set(v, "opt.t", self.t);
        set(v, "opt.a", self.a.map(|a| a.max(0.001)));
        set(v, "opt.ab", self.a.map(|a| if a > 0.0 { 1.0 } else { 0.0 }));
        set(v, "spc.eff", self.eff);
    }
}

pub use library::library;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::get::Get;
    use crate::system;

    #[test]
    fn missing_parts_leave_the_knobs_alone() {
        let lib = library();
        let e = effective(
            &lib,
            &Selection {
                pinhole: Some("Thorlabs P25D (25 um pinhole)".into()),
                ..Default::default()
            },
            0.1,
        );
        assert!(e.na.is_none() && e.au.is_none() && e.t.is_none() && e.dz.is_none());
        let mut v = system::model().defaults();
        let before = v.g("opt.na");
        e.apply(&mut v);
        assert_eq!(v.g("opt.na"), before);
    }

    #[test]
    fn the_sfpx040_with_an_achromat_and_a_pinhole_gives_the_documented_knobs() {
        let lib = library();
        let sel = Selection {
            pinhole: Some("Thorlabs P25D (25 um pinhole)".into()),
            lens: Some("Thorlabs AC254-050-A (achromat f=50)".into()),
            objective: Some("Newport SFPX040 (N-SF11 PCX, uncoated)".into()),
            ..Default::default()
        };
        let e = effective(&lib, &sel, 0.1);
        let close = |a: f64, b: f64, tol: f64| assert!((a / b - 1.0).abs() < tol, "{a} vs {b}");
        close(e.na.unwrap(), 0.1, 1e-9);
        let f_o = crate::physics::glass::focal(12.7e-3, 0, 587.6e-9, 550e-9);
        close(
            e.au.unwrap(),
            25e-6 * 0.1 * f_o / 0.05 / (1.22 * 550e-9),
            1e-9,
        );
        close(e.t.unwrap(), 0.97 * 0.86, 1e-9);
        close(e.dz.unwrap(), 0.7e-3, 0.06);
        close(e.a.unwrap(), 0.0232, 0.05);
        close(e.magnification.unwrap(), f_o / 0.05, 1e-9);
    }
}
