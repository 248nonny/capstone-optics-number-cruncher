const TRIALS: usize = 120;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545F4914F6CDD1D) >> 11) as f64 / (1u64 << 53) as f64
    }

    fn normal(&mut self) -> f64 {
        (-2.0 * self.next().max(1e-300).ln()).sqrt()
            * (2.0 * std::f64::consts::PI * self.next()).cos()
    }
}

pub fn centroid(y: &[f64], lam: &[f64]) -> f64 {
    let smooth: Vec<f64> = (0..y.len())
        .map(|i| {
            y[i.saturating_sub(4)..(i + 5).min(y.len())]
                .iter()
                .sum::<f64>()
                / 9.0
        })
        .collect();
    let top = smooth.iter().cloned().fold(f64::MIN, f64::max);
    let w: Vec<f64> = smooth
        .iter()
        .zip(y)
        .map(|(s, &v)| if *s > 0.5 * top { v - 0.5 * top } else { 0.0 })
        .collect();
    let total: f64 = w.iter().sum();
    w.iter().zip(lam).map(|(w, l)| w * l).sum::<f64>() / total
}

pub fn centroid_sigma(mu: &[f64], var: &[f64], lam: &[f64]) -> f64 {
    let mut rng = Rng(0x9E3779B97F4A7C15);
    let est: Vec<f64> = (0..TRIALS)
        .map(|_| {
            let y: Vec<f64> = mu
                .iter()
                .zip(var)
                .map(|(m, v)| m + v.sqrt() * rng.normal())
                .collect();
            centroid(&y, lam)
        })
        .collect();
    let mean = est.iter().sum::<f64>() / TRIALS as f64;
    (est.iter().map(|e| (e - mean).powi(2)).sum::<f64>() / (TRIALS - 1) as f64).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centroid_recovers_gaussian_peak_and_noise_scales() {
        let lam: Vec<f64> = (0..400).map(|i| i as f64).collect();
        let mu: Vec<f64> = lam
            .iter()
            .map(|l| 1000.0 * (-0.5 * ((l - 200.3) / 10.0).powi(2)).exp())
            .collect();
        assert!((centroid(&mu, &lam) - 200.3).abs() < 0.05);
        let quiet = centroid_sigma(&mu, &vec![100.0; 400], &lam);
        let noisy = centroid_sigma(&mu, &vec![10000.0; 400], &lam);
        assert!(noisy > 5.0 * quiet && quiet > 0.0);
    }
}
