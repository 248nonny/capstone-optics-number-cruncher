use super::{halfmax::crossing, special::j0};
use std::{
    f64::consts::PI,
    sync::{Arc, OnceLock},
};

const V_STEP: f64 = 0.2;
const V_STEP_RHO: f64 = V_STEP;
const V_N: usize = 161;
const U_STEP: f64 = 0.5;
const U_N: usize = 81;
const PUPIL_N: usize = 200;
const TOTAL: f64 = 4.0 * PI;

pub struct Table(Vec<f64>);

fn build() -> Table {
    let rho: Vec<f64> = (0..PUPIL_N)
        .map(|k| (k as f64 + 0.5) / PUPIL_N as f64)
        .collect();
    let phase: Vec<Vec<(f64, f64)>> = (0..U_N)
        .map(|j| {
            rho.iter()
                .map(|r| (j as f64 * U_STEP * r * r / 2.0).sin_cos())
                .collect()
        })
        .collect();
    let mut t = vec![0.0; V_N * U_N];
    for i in 0..V_N {
        let bes: Vec<f64> = rho
            .iter()
            .map(|r| j0(i as f64 * V_STEP * r) * r / PUPIL_N as f64)
            .collect();
        for j in 0..U_N {
            let (re, im) = phase[j]
                .iter()
                .zip(&bes)
                .fold((0.0, 0.0), |(a, b), (&(s, c), &w)| (a + c * w, b + s * w));
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
        let tail = if fu >= (U_N - 1) as f64 {
            ((U_N - 1) as f64 / fu).powi(2)
        } else {
            1.0
        };
        let fu = fu.min((U_N - 1) as f64 - 1e-9);
        let (i, j) = (fv as usize, fu as usize);
        let (a, b) = (fv - i as f64, fu - j as f64);
        let g = |i: usize, j: usize| self.0[i * U_N + j];
        tail * ((1.0 - a) * ((1.0 - b) * g(i, j) + b * g(i, j + 1))
            + a * ((1.0 - b) * g(i + 1, j) + b * g(i + 1, j + 1)))
    }
}

const VP_STEP: f64 = 0.25;
const VP_N: usize = 129;

pub struct DiskTable(Vec<f64>);

fn build_disk(vp: f64) -> DiskTable {
    let t = table();
    if vp == 0.0 {
        return DiskTable((0..V_N * U_N).map(|k| t.0[k]).collect());
    }
    const NP: usize = 16;
    let nr = (vp / 0.6).ceil().clamp(10.0, 60.0) as usize;
    let rings: Vec<(f64, f64)> = (0..nr)
        .map(|a| {
            (
                (a as f64 + 0.5) * vp / nr as f64,
                (a as f64 + 0.5) / nr as f64,
            )
        })
        .collect();
    let mut out = vec![0.0; V_N * U_N];
    for i in 0..V_N {
        let rho = i as f64 * V_STEP_RHO;
        for j in 0..U_N {
            let u = j as f64 * U_STEP;
            let mut sum = 0.0;
            for &(r, w) in &rings {
                for b in 0..NP {
                    let phi = PI * (b as f64 + 0.5) / NP as f64;
                    sum += w * t.at(
                        (rho * rho + r * r - 2.0 * rho * r * phi.cos())
                            .max(0.0)
                            .sqrt(),
                        u,
                    );
                }
            }
            out[i * U_N + j] = 2.0 * sum / (nr * NP) as f64;
        }
    }
    DiskTable(out)
}

fn slots() -> &'static Vec<OnceLock<Arc<DiskTable>>> {
    static S: OnceLock<Vec<OnceLock<Arc<DiskTable>>>> = OnceLock::new();
    S.get_or_init(|| (0..VP_N).map(|_| OnceLock::new()).collect())
}

fn disk_table(k: usize) -> Arc<DiskTable> {
    slots()[k]
        .get_or_init(|| Arc::new(build_disk(k as f64 * VP_STEP)))
        .clone()
}

pub fn warm() {
    let threads = std::thread::available_parallelism().map_or(2, |n| n.get());
    std::thread::scope(|s| {
        for t in 0..threads {
            s.spawn(move || (t..VP_N).step_by(threads).for_each(|k| drop(disk_table(k))));
        }
    });
}

pub fn warm_background() {
    std::thread::spawn(warm);
}

pub struct Disk {
    lo: Arc<DiskTable>,
    hi: Arc<DiskTable>,
    w: f64,
}

fn bilinear(t: &[f64], rho: f64, u: f64) -> f64 {
    let (fv, fu) = (rho / V_STEP, u.abs() / U_STEP);
    if fv >= (V_N - 1) as f64 {
        return 0.0;
    }
    let tail = if fu >= (U_N - 1) as f64 {
        ((U_N - 1) as f64 / fu).powi(2)
    } else {
        1.0
    };
    let fu = fu.min((U_N - 1) as f64 - 1e-9);
    let (i, j) = (fv as usize, fu as usize);
    let (a, b) = (fv - i as f64, fu - j as f64);
    let g = |i: usize, j: usize| t[i * U_N + j];
    tail * ((1.0 - a) * ((1.0 - b) * g(i, j) + b * g(i, j + 1))
        + a * ((1.0 - b) * g(i + 1, j) + b * g(i + 1, j + 1)))
}

pub fn disk(vp: f64) -> Disk {
    let x = (vp / VP_STEP).clamp(0.0, (VP_N - 1) as f64 - 1e-9);
    let k = x as usize;
    Disk {
        lo: disk_table(k),
        hi: disk_table(k + 1),
        w: x - k as f64,
    }
}

impl Disk {
    pub fn avg(&self, rho: f64, u: f64) -> f64 {
        (1.0 - self.w) * bilinear(&self.lo.0, rho, u) + self.w * bilinear(&self.hi.0, rho, u)
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

pub fn axial(vp1: f64, vp2: f64) -> Axial {
    let t = table();
    let (vp1, vp2) = (vp1.max(0.05), vp2.max(0.05));
    let (a1, a2) = (PI * vp1 * vp1, PI * vp2 * vp2);
    let reach = vp1 + vp2;
    let n = (reach / 0.1).ceil().max(8.0) as usize;
    let h = reach / n as f64;
    let kernel: Vec<(f64, f64)> = (0..n)
        .map(|k| ((k as f64 + 0.5) * h, 0.0))
        .map(|(r, _)| (r, overlap(vp1, vp2, r) * 2.0 * PI * r * h))
        .collect();
    let mirror = (0..U_N)
        .map(|j| {
            kernel
                .iter()
                .map(|&(r, w)| w * t.at(r, j as f64 * U_STEP))
                .sum::<f64>()
                / (a1 * TOTAL)
        })
        .collect();
    let (d1, d2) = (disk(vp1), disk(vp2));
    let plane = (0..U_N)
        .map(|j| {
            let u = j as f64 * U_STEP;
            (0..V_N - 1)
                .map(|i| {
                    let r = (i as f64 + 0.5) * V_STEP;
                    d1.avg(r, u) * d2.avg(r, u) * 2.0 * PI * r * V_STEP
                })
                .sum::<f64>()
                * a2
                / (TOTAL * TOTAL)
        })
        .collect();
    Axial { mirror, plane }
}

impl Axial {
    fn interp(curve: &[f64], u: f64) -> f64 {
        let f = u.abs() / U_STEP;
        let last = curve.len() - 1;
        if f >= last as f64 {
            let tail: f64 = (last - 9..=last)
                .map(|i| curve[i] * (i * i) as f64)
                .sum::<f64>()
                / 10.0;
            return tail / (f * f);
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

pub struct Bank {
    lams: Vec<f64>,
    axials: Vec<Axial>,
}

const BANK_NODES: usize = 5;

impl Bank {
    pub fn new(vp0: [f64; 2], lam0: f64, lo: f64, hi: f64) -> Self {
        let nodes = if hi > lo { BANK_NODES } else { 1 };
        let lams: Vec<f64> = (0..nodes)
            .map(|k| lo + (hi - lo) * k as f64 / (nodes - 1).max(1) as f64)
            .collect();
        let axials = lams
            .iter()
            .map(|&l| axial(vp0[0] * lam0 / l, vp0[1] * lam0 / l))
            .collect();
        Self { lams, axials }
    }

    fn blend(&self, lam: f64, f: impl Fn(&Axial) -> f64) -> f64 {
        let k = self
            .lams
            .partition_point(|&l| l < lam)
            .clamp(1, self.lams.len().max(2) - 1);
        if self.lams.len() == 1 {
            return f(&self.axials[0]);
        }
        let (l0, l1) = (self.lams[k - 1], self.lams[k]);
        let w = ((lam - l0) / (l1 - l0)).clamp(0.0, 1.0);
        (1.0 - w) * f(&self.axials[k - 1]) + w * f(&self.axials[k])
    }

    pub fn mirror_at(&self, lam: f64, u: f64) -> f64 {
        self.blend(lam, |a| a.mirror_at(u))
    }

    pub fn plane_at(&self, lam: f64, u: f64) -> f64 {
        self.blend(lam, |a| a.plane_at(u))
    }

    pub fn mirror_fwhm_u(&self, lam: f64) -> f64 {
        self.blend(lam, |a| a.mirror_fwhm_u())
    }

    pub fn plane_fwhm_u(&self, lam: f64) -> f64 {
        self.blend(lam, |a| a.plane_fwhm_u())
    }
}

pub fn lateral_profile(lam: f64, na: [f64; 2], radius: [f64; 2], step: f64, n: usize) -> Vec<f64> {
    let v = |i: usize, r: f64| 2.0 * PI * na[i] * r / lam;
    let (d1, d2) = (disk(v(0, radius[0])), disk(v(1, radius[1])));
    (0..n)
        .map(|k| d1.avg(v(0, k as f64 * step), 0.0) * d2.avg(v(1, k as f64 * step), 0.0))
        .collect()
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
        assert!(
            (z_norm / (2.0 * PI) - 0.886).abs() < 0.03,
            "{}",
            z_norm / (2.0 * PI)
        );
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
