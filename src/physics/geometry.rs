use super::dispersion::f_obj;
use crate::engine::get::Get;
use std::f64::consts::PI;

pub fn na_ill(g: &dyn Get) -> f64 {
    g.g("obj.na")
        .min(g.g("coll.d") / (2.0 * f_obj(g, g.g("sys.lam0"))))
}

pub fn na_det(g: &dyn Get) -> f64 {
    g.g("obj.na")
        .min(g.g("dcoll.d") / (2.0 * f_obj(g, g.g("sys.lam0"))))
}

pub fn nas(g: &dyn Get) -> [f64; 2] {
    [na_ill(g), na_det(g)]
}

pub fn pinhole_radii(g: &dyn Get) -> [f64; 2] {
    let f_o = f_obj(g, g.g("sys.lam0"));
    [("pin1", "coll"), ("pin2", "dcoll")]
        .map(|(p, l)| 0.5 * g.g(&format!("{p}.d")) * f_o / g.g(&format!("{l}.f")))
}

pub fn normalised_radii(g: &dyn Get) -> [f64; 2] {
    let (na, r) = (nas(g), pinhole_radii(g));
    [0, 1].map(|i| 2.0 * PI * na[i] * r[i] / g.g("sys.lam0"))
}
