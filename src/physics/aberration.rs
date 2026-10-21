pub fn pcx_back_focus(f: f64, n_design: f64, n: f64, ct: f64, h: f64) -> f64 {
    let r = f * (n_design - 1.0);
    let s = (h / r).min(0.999);
    let (a, b) = (s.asin(), (s / n).asin());
    let sag = r - (r * r - h * h).sqrt();
    let th = a - b;
    let height = h - (ct - sag) * th.tan();
    let so = n * th.sin();
    if so.abs() >= 1.0 {
        return f64::NAN;
    }
    height / so.asin().tan()
}

pub fn pcx_marginal_lsa(f: f64, n_design: f64, n: f64, ct: f64, na: f64) -> f64 {
    pcx_back_focus(f, n_design, n, ct, 1e-3 * f) - pcx_back_focus(f, n_design, n, ct, na * f)
}

pub fn sa_rms_waves(lsa: f64, na: f64, lam: f64) -> f64 {
    lsa * na * na / 4.0 / lam * 0.0745
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sfpx040_back_focal_length_matches_the_newport_drawing() {
        let bfl = pcx_back_focus(12.7e-3, 1.7847, 1.7847, 4e-3, 1e-5);
        assert!((bfl - 10.46e-3).abs() < 0.1e-3, "{bfl}");
    }

    #[test]
    fn marginal_lsa_maps_to_w040_over_four() {
        let (na, lam) = (0.2, 550e-9);
        let w040 = 3.0 * lam;
        let lsa = 4.0 * w040 / (na * na);
        assert!((sa_rms_waves(lsa, na, lam) - 3.0 * 0.0745).abs() < 1e-9);
    }

    #[test]
    fn nsf11_singlet_sa_grows_with_na() {
        let l = |na| pcx_marginal_lsa(12.7e-3, 1.7847, 1.7911, 4e-3, na);
        assert!((l(0.1) - 68.6e-6).abs() < 6e-6, "{}", l(0.1));
        assert!(l(0.3) > 600e-6 && l(0.3) < 700e-6, "{}", l(0.3));
    }
}
