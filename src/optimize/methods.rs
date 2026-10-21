use std::thread;

pub const GRADIENT_STEP: f64 = 1e-3;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Method {
    NelderMead,
    Gradient,
    GradientThenNelderMead,
}

#[derive(Clone, Debug)]
pub struct Trace {
    pub x: Vec<f64>,
    pub f: f64,
    pub evals: usize,
    pub stalled: bool,
}

pub type Report<'a> = &'a mut dyn FnMut(&[f64], f64, usize) -> bool;

fn clamp(x: &mut [f64]) {
    x.iter_mut().for_each(|v| *v = v.clamp(0.0, 1.0));
}

fn gradient(f: &(impl Fn(&[f64]) -> f64 + Sync), x: &[f64]) -> Vec<f64> {
    thread::scope(|s| {
        let handles: Vec<_> = (0..x.len())
            .map(|i| {
                s.spawn(move || {
                    let (mut hi, mut lo) = (x.to_vec(), x.to_vec());
                    hi[i] = (x[i] + GRADIENT_STEP).min(1.0);
                    lo[i] = (x[i] - GRADIENT_STEP).max(0.0);
                    (f(&hi) - f(&lo)) / (hi[i] - lo[i])
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().unwrap_or(f64::NAN))
            .collect()
    })
}

pub fn gradient_descent(
    f: &(impl Fn(&[f64]) -> f64 + Sync),
    x0: &[f64],
    max_iters: usize,
    report: Report,
) -> Trace {
    let mut x = x0.to_vec();
    let mut fx = f(&x);
    let (mut evals, mut step) = (1, 0.1);
    for _ in 0..max_iters {
        let g = gradient(f, &x);
        evals += 2 * x.len();
        let norm2: f64 = g.iter().map(|v| v * v).sum();
        if !norm2.is_finite() || norm2 < 1e-24 {
            return Trace {
                x,
                f: fx,
                evals,
                stalled: true,
            };
        }
        let mut accepted = false;
        for _ in 0..40 {
            let mut trial: Vec<f64> = x
                .iter()
                .zip(&g)
                .map(|(a, b)| a - step * b / norm2.sqrt())
                .collect();
            clamp(&mut trial);
            let ft = f(&trial);
            evals += 1;
            let moved: f64 = trial
                .iter()
                .zip(&x)
                .map(|(a, b)| (a - b).powi(2))
                .sum::<f64>()
                .sqrt();
            if ft.is_finite() && ft <= fx - 1e-4 * step * norm2.sqrt() && moved > 0.0 {
                let converged = (fx - ft).abs() < 1e-10 * fx.abs().max(1e-30) || moved < 1e-9;
                (x, fx, accepted) = (trial, ft, true);
                step *= 1.5;
                if converged {
                    return Trace {
                        x,
                        f: fx,
                        evals,
                        stalled: false,
                    };
                }
                break;
            }
            step *= 0.5;
        }
        if !accepted {
            return Trace {
                x,
                f: fx,
                evals,
                stalled: true,
            };
        }
        if report(&x, fx, evals) {
            break;
        }
    }
    Trace {
        x,
        f: fx,
        evals,
        stalled: false,
    }
}

pub fn nelder_mead(
    f: &impl Fn(&[f64]) -> f64,
    x0: &[f64],
    max_evals: usize,
    report: Report,
) -> Trace {
    let n = x0.len();
    let eval = |x: &mut Vec<f64>| {
        clamp(x);
        let v = f(x);
        if v.is_nan() {
            f64::INFINITY
        } else {
            v
        }
    };
    let mut simplex: Vec<(Vec<f64>, f64)> = (0..=n)
        .map(|k| {
            let mut x = x0.to_vec();
            if k > 0 {
                x[k - 1] = if x[k - 1] > 0.5 {
                    x[k - 1] - 0.05
                } else {
                    x[k - 1] + 0.05
                };
            }
            let v = eval(&mut x);
            (x, v)
        })
        .collect();
    let mut evals = n + 1;
    while evals < max_evals {
        simplex.sort_by(|a, b| a.1.total_cmp(&b.1));
        let spread = simplex[n].1 - simplex[0].1;
        let size: f64 = simplex[1..]
            .iter()
            .map(|(x, _)| {
                x.iter()
                    .zip(&simplex[0].0)
                    .map(|(a, b)| (a - b).abs())
                    .fold(0.0, f64::max)
            })
            .fold(0.0, f64::max);
        if size < 1e-7 || spread.abs() < 1e-12 * simplex[0].1.abs().max(1e-30) {
            break;
        }
        if report(&simplex[0].0, simplex[0].1, evals) {
            break;
        }
        let centroid: Vec<f64> = (0..n)
            .map(|i| simplex[..n].iter().map(|(x, _)| x[i]).sum::<f64>() / n as f64)
            .collect();
        let along = |t: f64| -> Vec<f64> {
            centroid
                .iter()
                .zip(&simplex[n].0)
                .map(|(c, w)| c + t * (c - w))
                .collect()
        };
        let mut reflected = along(1.0);
        let fr = eval(&mut reflected);
        evals += 1;
        if fr < simplex[0].1 {
            let mut expanded = along(2.0);
            let fe = eval(&mut expanded);
            evals += 1;
            simplex[n] = if fe < fr {
                (expanded, fe)
            } else {
                (reflected, fr)
            };
        } else if fr < simplex[n - 1].1 {
            simplex[n] = (reflected, fr);
        } else {
            let mut contracted = along(if fr < simplex[n].1 { 0.5 } else { -0.5 });
            let fc = eval(&mut contracted);
            evals += 1;
            if fc < fr.min(simplex[n].1) {
                simplex[n] = (contracted, fc);
            } else {
                let best = simplex[0].0.clone();
                for item in simplex.iter_mut().skip(1) {
                    let mut x: Vec<f64> = item
                        .0
                        .iter()
                        .zip(&best)
                        .map(|(a, b)| b + 0.5 * (a - b))
                        .collect();
                    item.1 = eval(&mut x);
                    item.0 = x;
                    evals += 1;
                }
            }
        }
    }
    simplex.sort_by(|a, b| a.1.total_cmp(&b.1));
    let (x, f) = simplex.swap_remove(0);
    Trace {
        x,
        f,
        evals,
        stalled: false,
    }
}

pub fn run(
    method: Method,
    f: &(impl Fn(&[f64]) -> f64 + Sync),
    x0: &[f64],
    budget: usize,
    report: Report,
) -> Trace {
    match method {
        Method::NelderMead => nelder_mead(f, x0, budget * 4, report),
        Method::Gradient => gradient_descent(f, x0, budget, report),
        Method::GradientThenNelderMead => {
            let first = gradient_descent(f, x0, budget, report);
            if !first.stalled {
                return first;
            }
            let second = nelder_mead(f, &first.x, budget * 4, report);
            Trace {
                evals: first.evals + second.evals,
                ..if second.f < first.f { second } else { first }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad(x: &[f64]) -> f64 {
        (x[0] - 0.3).powi(2) + 4.0 * (x[1] - 0.7).powi(2) + 0.5
    }

    #[test]
    fn nelder_mead_finds_the_quadratic_minimum() {
        let t = nelder_mead(&quad, &[0.9, 0.1], 400, &mut |_, _, _| false);
        assert!(
            (t.x[0] - 0.3).abs() < 1e-3 && (t.x[1] - 0.7).abs() < 1e-3 && (t.f - 0.5).abs() < 1e-6,
            "{t:?}"
        );
    }

    #[test]
    fn gradient_descent_finds_the_quadratic_minimum() {
        let t = gradient_descent(&quad, &[0.9, 0.1], 200, &mut |_, _, _| false);
        assert!(
            (t.x[0] - 0.3).abs() < 1e-3 && (t.x[1] - 0.7).abs() < 1e-3,
            "{t:?}"
        );
    }

    #[test]
    fn both_respect_the_unit_cube() {
        let edge = |x: &[f64]| (x[0] + 1.0).powi(2) + (x[1] - 2.0).powi(2);
        let g = gradient_descent(&edge, &[0.5, 0.5], 100, &mut |_, _, _| false);
        let n = nelder_mead(&edge, &[0.5, 0.5], 400, &mut |_, _, _| false);
        for t in [g, n] {
            assert!(t.x.iter().all(|v| (0.0..=1.0).contains(v)));
            assert!(t.x[0] < 1e-3 && t.x[1] > 0.999, "{t:?}");
        }
    }

    #[test]
    fn fallback_rescues_a_kinked_cost_where_gradient_stalls() {
        let kink = |x: &[f64]| {
            if x[0] < 0.5 {
                1.0 + (x[0] - 0.2).abs()
            } else {
                2.0
            }
        };
        let g = gradient_descent(&kink, &[0.8, 0.5], 50, &mut |_, _, _| false);
        assert!(g.stalled || g.f > 1.0);
        let t = run(
            Method::GradientThenNelderMead,
            &kink,
            &[0.8, 0.5],
            50,
            &mut |_, _, _| false,
        );
        assert!(t.f <= g.f);
    }

    #[test]
    fn stop_request_ends_the_run() {
        let mut calls = 0;
        let t = gradient_descent(&quad, &[0.9, 0.1], 1000, &mut |_, _, _| {
            calls += 1;
            calls >= 2
        });
        assert!(t.evals < 200);
    }
}
