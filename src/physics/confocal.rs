use super::{halfmax::crossing, special::j0};
use std::{cell::RefCell, collections::HashMap, f64::consts::PI, rc::Rc, sync::OnceLock};

const V_STEP: f64 = 0.2;
const V_N: usize = 161;
const U_STEP: f64 = 0.5;
const U_N: usize = 81;
const PUPIL_N: usize = 200;
const TOTAL: f64 = 4.0 * PI;

pub struct Table(Vec<f64>);

fn build() -> Table {
    let rho: Vec<f64> = (0..PUPIL_N).map(|k| (k as f64 + 0.5) / PUPIL_N as f64).collect();
    let phase: Vec<Vec<(f64, f64)>> =
        (0..U_N).map(|j| rho.iter().map(|r| (j as f64 * U_STEP * r * r / 2.0).sin_cos()).collect()).collect();
    let mut t = vec![0.0; V_N * U_N];
    for i in 0..V_N {
        let bes: Vec<f64> = rho.iter().map(|r| j0(i as f64 * V_STEP * r) * r / PUPIL_N as f64).collect();
        for j in 0..U_N {
            let (re, im) = phase[j].iter().zip(&bes).fold((0.0, 0.0), |(a, b), (&(s, c), &w)| (a + c * w, b + s * w));
            t[i * U_N + j] = 4.0 * (re * re + im * im);
        }
    }
    Table(t)
}

pub fn table() -> &'static Table {
    static T: OnceLock<Table> = OnceLock::new();
    T.get_or_init(build)
}

impl Table {
    pub fn at(&self, v: f64, u: f64) -> f64 {
        let (fv, fu) = (v / V_STEP, u.abs() / U_STEP);
        if fv >= (V_N - 1) as f64 {
            return 0.0;
        }
        let tail = if fu >= (U_N - 1) as f64 { ((U_N - 1) as f64 / fu).powi(2) } else { 1.0 };
        let fu = fu.min((U_N - 1) as f64 - 1e-9);
        let (i, j) = (fv as usize, fu as usize);
        let (a, b) = (fv - i as f64, fu - j as f64);
        let g = |i: usize, j: usize| self.0[i * U_N + j];
        tail * ((1.0 - a) * ((1.0 - b) * g(i, j) + b * g(i, j + 1)) + a * ((1.0 - b) * g(i + 1, j) + b * g(i + 1, j + 1)))
    }

    pub fn disk(&self, vp: f64, rho: f64, u: f64) -> f64 {
        const NP: usize = 16;
        let nr = (vp / 0.6).ceil().clamp(10.0, 60.0) as usize;
        let mut sum = 0.0;
        for a in 0..nr {
            let r = vp * (a as f64 + 0.5) / nr as f64;
            let w = r * vp / nr as f64 * PI / NP as f64 * 2.0;
            for b in 0..NP {
                let phi = PI * (b as f64 + 0.5) / NP as f64;
                sum += w * self.at((rho * rho + r * r - 2.0 * rho * r * phi.cos()).max(0.0).sqrt(), u);
            }
        }
        sum
    }
}

pub fn overlap(r1: f64, r2: f64, d: f64) -> f64 {
    if d >= r1 + r2 {
        return 0.0;
    }
    if d <= (r1 - r2).abs() {
        return PI * r1.min(r2).powi(2);
    }
    let c1 = ((d * d + r1 * r1 - r2 * r2) / (2.0 * d * r1)).clamp(-1.0, 1.0);
    let c2 = ((d * d + r2 * r2 - r1 * r1) / (2.0 * d * r2)).clamp(-1.0, 1.0);
    let k = (-d + r1 + r2) * (d + r1 - r2) * (d - r1 + r2) * (d + r1 + r2);
    r1 * r1 * c1.acos() + r2 * r2 * c2.acos() - 0.5 * k.max(0.0).sqrt()
}

pub struct Axial {
    pub mirror: Vec<f64>,
    pub plane: Vec<f64>,
}

fn simpson_like(n: usize, h: f64, f: impl Fn(f64) -> f64) -> f64 {
    (0..n).map(|k| f((k as f64 + 0.5) * h) * h).sum()
}

fn compute_axial(vp1: f64, vp2: f64) -> Axial {
    let t = table();
    let a1 = PI * vp1 * vp1;
    let reach = vp1 + vp2;
    let n = (reach / 0.1).ceil().max(8.0) as usize;
    let mirror = (0..U_N)
        .map(|j| {
            let u = j as f64 * U_STEP;
            simpson_like(n, reach / n as f64, |r| overlap(vp1, vp2, r) * t.at(r, u) * 2.0 * PI * r) / (a1 * TOTAL)
        })
        .collect();
    let rho_n = V_N - 1;
    let plane = (0..U_N)
        .map(|j| {
            let u = j as f64 * U_STEP;
            simpson_like(rho_n, V_STEP, |r| t.disk(vp1, r, u) * t.disk(vp2, r, u) * 2.0 * PI * r) / (a1 * TOTAL * TOTAL)
        })
        .collect();
    Axial { mirror, plane }
}

thread_local! {
    static CACHE: RefCell<HashMap<(i32, i32), Rc<Axial>>> = RefCell::new(HashMap::new());
}

pub fn axial(vp1: f64, vp2: f64) -> Rc<Axial> {
    let key = ((vp1.max(0.05) * 20.0).round() as i32, (vp2.max(0.05) * 20.0).round() as i32);
    CACHE.with(|c| {
        c.borrow_mut()
            .entry(key)
            .or_insert_with(|| Rc::new(compute_axial(key.0 as f64 / 20.0, key.1 as f64 / 20.0)))
            .clone()
    })
}

impl Axial {
    fn interp(curve: &[f64], u: f64) -> f64 {
        let f = u.abs() / U_STEP;
        let last = (curve.len() - 1) as f64;
        if f >= last {
            return curve[curve.len() - 1] * (last / f).powi(2);
        }
        let (i, b) = (f as usize, f.fract());
        curve[i] * (1.0 - b) + curve[i + 1] * b
    }

    pub fn mirror_at(&self, u_defocus: f64) -> f64 {
        Self::interp(&self.mirror, u_defocus)
    }

    pub fn plane_at(&self, u_defocus: f64) -> f64 {
        Self::interp(&self.plane, u_defocus)
    }

    fn fwhm(curve: &[f64]) -> f64 {
        2.0 * U_STEP * crossing(curve.iter().copied())
    }

    pub fn mirror_fwhm_u(&self) -> f64 {
        Self::fwhm(&self.mirror)
    }

    pub fn plane_fwhm_u(&self) -> f64 {
        Self::fwhm(&self.plane)
    }
}

pub fn lateral_profile(lam: f64, na: [f64; 2], radius: [f64; 2], step: f64, n: usize) -> Vec<f64> {
    let t = table();
    let v = |i: usize, r: f64| 2.0 * PI * na[i] * r / lam;
    let vp = [v(0, radius[0]), v(1, radius[1])];
    (0..n).map(|k| t.disk(vp[0], v(0, k as f64 * step), 0.0) * t.disk(vp[1], v(1, k as f64 * step), 0.0)).collect()
}

pub fn lateral_step(lam: f64, na: [f64; 2]) -> f64 {
    0.02 * lam / na[0].min(na[1])
}

pub fn lateral_fwhm(lam: f64, na: [f64; 2], radius: [f64; 2]) -> f64 {
    let step = lateral_step(lam, na);
    let p = lateral_profile(lam, na, radius, step, 400);
    2.0 * step * crossing(p.into_iter())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_pinhole_mirror_fwhm_is_0886() {
        let a = axial(0.05, 0.05);
        let z_norm = a.mirror_fwhm_u() / 2.0;
        assert!((z_norm / (2.0 * PI) - 0.886).abs() < 0.03, "{}", z_norm / (2.0 * PI));
    }

    #[test]
    fn plane_is_wider_than_mirror() {
        let a = axial(0.05, 0.05);
        let r = a.plane_fwhm_u() / (a.mirror_fwhm_u() / 2.0);
        assert!(r > 1.2 && r < 1.6, "{r}");
    }

    #[test]
    fn encircled_energy_at_first_zero() {
        let a = axial(1e-3 + 0.05, 3.8317);
        let eta = a.mirror[0] * 1.0;
        assert!((eta - 0.838).abs() < 0.02, "{eta}");
    }

    #[test]
    fn lateral_widefield_and_confocal() {
        let c = lateral_fwhm(550e-9, [0.1, 0.1], [1e-9, 1e-9]) / (550e-9 / 0.1);
        assert!((c - 0.37).abs() < 0.02, "{c}");
    }
}
