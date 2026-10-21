pub fn crossing(mut v: impl Iterator<Item = f64>) -> f64 {
    let Some(peak) = v.next() else {
        return f64::NAN;
    };
    let half = peak / 2.0;
    let mut prev = peak;
    for (i, y) in v.enumerate() {
        if y < half {
            return i as f64 + (prev - half) / (prev - y).max(1e-300);
        }
        prev = y;
    }
    f64::NAN
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triangle_width() {
        let v = [0.0, 1.0, 2.0, 3.0, 4.0, 3.0, 2.0, 1.0, 0.0];
        assert!((crossing(v[4..].iter().copied()) - 2.0).abs() < 1e-12);
    }
}
