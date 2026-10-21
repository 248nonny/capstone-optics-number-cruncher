const SELLMEIER: [([f64; 3], [f64; 3]); 5] = [
    (
        [1.73759695, 0.313747346, 1.89878101],
        [0.013188707, 0.0623068142, 155.23629],
    ),
    (
        [1.03961212, 0.231792344, 1.01046945],
        [0.00600069867, 0.0200179144, 103.560653],
    ),
    (
        [1.34533359, 0.209073176, 0.937357162],
        [0.00997743871, 0.0470450767, 111.886764],
    ),
    (
        [1.77931763, 0.338149866, 2.08734474],
        [0.0133714182, 0.0617533621, 174.01759],
    ),
    (
        [0.6961663, 0.4079426, 0.8974794],
        [0.00467914826, 0.0135120631, 97.9340025],
    ),
];

pub fn index(glass: usize, lam: f64) -> f64 {
    let l2 = (lam * 1e6).powi(2);
    let (b, c) = SELLMEIER[glass.min(4)];
    (1.0 + (0..3).map(|i| b[i] * l2 / (l2 - c[i])).sum::<f64>()).sqrt()
}

pub fn power_ratio(glass: usize, lam: f64, lam0: f64) -> f64 {
    if glass >= 5 {
        lam / lam0
    } else {
        (index(glass, lam) - 1.0) / (index(glass, lam0) - 1.0)
    }
}

pub fn focal(f0: f64, glass: usize, lam0: f64, lam: f64) -> f64 {
    f0 / power_ratio(glass, lam, lam0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn abbe(glass: usize) -> f64 {
        (index(glass, 587.56e-9) - 1.0) / (index(glass, 486.13e-9) - index(glass, 656.27e-9))
    }

    #[test]
    fn catalog_nd_and_vd() {
        let cat = [
            (0, 1.7847, 25.68),
            (1, 1.5168, 64.17),
            (2, 1.6200, 36.37),
            (3, 1.8052, 25.36),
            (4, 1.4585, 67.8),
        ];
        for (g, nd, vd) in cat {
            assert!(
                (index(g, 587.56e-9) - nd).abs() < 2e-4,
                "{} nd {}",
                g,
                index(g, 587.56e-9)
            );
            assert!((abbe(g) - vd).abs() < 0.3, "{} vd {}", g, abbe(g));
        }
    }

    #[test]
    fn nsf11_singlet_range() {
        let f = |l| focal(12.7e-3, 0, 587.6e-9, l);
        assert!((f(650e-9) - f(450e-9) - 0.666e-3).abs() < 0.01e-3);
    }
}
