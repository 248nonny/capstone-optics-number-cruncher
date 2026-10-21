use super::dispersion::f_obj;
use crate::engine::get::Get;
use std::f64::consts::PI;

pub fn nas(g: &dyn Get) -> [f64; 2] {
    [g.g("obj.na_ill"), g.g("obj.na_det")]
}

pub fn pinhole_radii(g: &dyn Get) -> [f64; 2] {
    let f_o = f_obj(g, g.g("sys.lam0"));
    [("pin1", "coll"), ("pin2", "dcoll")].map(|(p, l)| 0.5 * g.g(&format!("{p}.d")) * f_o / g.g(&format!("{l}.f")))
}

pub fn normalised_radii(g: &dyn Get) -> [f64; 2] {
    let (na, r) = (nas(g), pinhole_radii(g));
    [0, 1].map(|i| 2.0 * PI * na[i] * r[i] / g.g("sys.lam0"))
}
