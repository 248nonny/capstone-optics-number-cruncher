type Map = fn(f64) -> f64;

pub fn bisect(lo: f64, hi: f64, log: bool, mut f: impl FnMut(f64) -> f64) -> Option<f64> {
    let (to, from): (Map, Map) = if log {
        (f64::ln, f64::exp)
    } else {
        (|x| x, |x| x)
    };
    let (mut a, mut b) = (to(lo), to(hi));
    let (fa, fb) = (f(from(a)), f(from(b)));
    if !(fa.is_finite() && fb.is_finite()) || fa * fb > 0.0 {
        return None;
    }
    let up = fb > fa;
    for _ in 0..80 {
        let m = 0.5 * (a + b);
        if (f(from(m)) > 0.0) == up {
            b = m;
        } else {
            a = m;
        }
    }
    Some(from(0.5 * (a + b)))
}
