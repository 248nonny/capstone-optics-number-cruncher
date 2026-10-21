use super::glass;
use crate::engine::get::Get;

pub fn f_obj(g: &dyn Get, lam: f64) -> f64 {
    glass::focal(g.g("obj.f"), g.g("obj.glass") as usize, g.g("obj.lamf"), lam)
}

fn f_lens(g: &dyn Get, lens: &str, lam: f64) -> f64 {
    g.g(&format!("{lens}.f")) * (1.0 + g.g(&format!("{lens}.dfrac")) * (lam - g.g("sys.lam0")) / 100e-9)
}

fn z_path(g: &dyn Get, lens: &str, lam: f64) -> f64 {
    let vergence = 1.0 / f_lens(g, lens, lam) - 1.0 / f_lens(g, lens, g.g("sys.lam0")) + 1.0 / f_obj(g, lam);
    1.0 / vergence
}

pub fn z_eff(g: &dyn Get, lam: f64) -> f64 {
    let z = |l| 0.5 * (z_path(g, "coll", l) + z_path(g, "dcoll", l));
    z(lam) - z(g.g("sys.lam0"))
}

pub fn slope(g: &dyn Get, lam: f64) -> f64 {
    let d = 1e-9;
    (z_eff(g, lam + d) - z_eff(g, lam - d)) / (2.0 * d)
}
