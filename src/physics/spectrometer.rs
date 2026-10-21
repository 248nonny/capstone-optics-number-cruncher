pub const PITCH: f64 = 7.5e-6;

pub fn grating_dlam_for(lines: f64, order: f64, alpha: f64, lam_c: f64, f_cam: f64) -> f64 {
    let d = 1.0 / lines;
    let sin_b = order * lam_c / d - alpha.sin();
    d * (1.0 - sin_b * sin_b).max(0.0).sqrt() * PITCH / (order * f_cam)
}

pub fn blur(v: &[f64], sigma: f64) -> Vec<f64> {
    let r = (4.0 * sigma).ceil().max(1.0) as isize;
    let k: Vec<f64> = (-r..=r)
        .map(|d| (-0.5 * (d as f64 / sigma).powi(2)).exp())
        .collect();
    (0..v.len() as isize)
        .map(|i| {
            let used = (-r..=r).filter(|d| (0..v.len() as isize).contains(&(i + d)));
            let (num, norm) = used.fold((0.0, 0.0), |(n, w), d| {
                (
                    n + v[(i + d) as usize] * k[(d + r) as usize],
                    w + k[(d + r) as usize],
                )
            });
            num / norm
        })
        .collect()
}
