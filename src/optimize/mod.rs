pub mod discrete;
pub mod methods;
pub mod problem;
pub mod runner;

#[cfg(test)]
mod tests {
    use super::{discrete, methods::Method, problem::{DesignVar, Objective, Problem}, runner};
    use crate::engine::{get::Get, state::State};
    use crate::model::{build, train::default_train};
    use std::sync::Arc;

    fn problem(ids: &[&str]) -> Problem {
        let model = Arc::new(build());
        let base = State::new(&model);
        let vars = ids.iter().map(|id| DesignVar::new(&model, model.index[*id])).collect();
        Problem { model, base, vars, objective: Objective::default() }
    }

    #[test]
    fn gradient_descent_lowers_the_real_cost_inside_the_bounds() {
        let p = problem(&["pin1.d", "obj.d", "coll.f"]);
        p.validate().unwrap();
        let start = p.start();
        let f = |u: &[f64]| p.cost(u);
        let t = super::methods::run(Method::GradientThenNelderMead, &f, &start, 30, &mut |_, _, _| false);
        assert!(t.f < p.cost(&start), "{} vs {}", t.f, p.cost(&start));
        assert!(t.x.iter().all(|u| (0.0..=1.0).contains(u)));
    }

    #[test]
    fn nelder_mead_also_improves_the_real_cost() {
        let p = problem(&["pin1.d", "obj.d"]);
        let start = p.start();
        let t = super::methods::run(Method::NelderMead, &|u| p.cost(u), &start, 20, &mut |_, _, _| false);
        assert!(t.f < p.cost(&start));
    }

    #[test]
    fn two_variables_tied_by_a_constraint_are_rejected_before_running() {
        let p = problem(&["pin1.d", "pin2.d"]);
        let e = p.validate().unwrap_err();
        assert!(e.contains("Identical pinholes"), "{e}");
        assert!(problem(&["obj.na", "obj.d", "obj.f"]).validate().unwrap_err().contains("Objective NA"));
        assert!(problem(&["obj.na", "obj.f"]).validate().is_ok());
    }

    #[test]
    fn independent_variables_are_set_together_without_being_moved_by_formulas() {
        let p = problem(&["obj.d", "coll.f"]);
        let t = p.state_at(&[0.5, 0.5]).unwrap();
        assert!((t.values.g("obj.d") - p.vars[0].value(0.5)).abs() < 1e-12);
        assert!((t.values.g("coll.f") - p.vars[1].value(0.5)).abs() < 1e-12);
    }

    #[test]
    fn combination_count_matches_the_product_and_snaps_to_nearby_parts() {
        let m = Arc::new(build());
        let slots = discrete::slots(&m, &default_train(), true);
        let n = discrete::count(&m, &slots);
        let expected: usize = slots.iter().filter(|s| s.include).map(|s| 1 + m.lib.of_kind(s.kind).count()).product();
        assert_eq!(n, expected);
        assert_eq!(discrete::all_combos(&m, &slots).len(), n);
        let separate = discrete::count(&m, &discrete::slots(&m, &default_train(), false));
        assert!(separate > n);
        let base = State::new(&m);
        let rows = discrete::evaluate(&m, &base, &Objective::default(), discrete::all_combos(&m, &slots));
        assert_eq!(rows.len(), n);
        assert!(rows.windows(2).all(|w| w[0].cost <= w[1].cost));
        let vars = vec![DesignVar::new(&m, m.index["pin1.d"])];
        let near = discrete::nearest(&m, &slots.iter().find(|s| s.kind == "pin").unwrap().clone(), &vars, &[vars[0].unit(26e-6)], 3);
        assert_eq!(near[0].as_deref(), Some("Thorlabs P25D (25 um pinhole)"));
    }

    #[test]
    fn background_run_reports_progress_and_finishes() {
        let p = problem(&["pin1.d", "obj.d"]);
        let run = runner::spawn(p, Method::GradientThenNelderMead, 8, || {});
        let t = std::time::Instant::now();
        while !run.snapshot().done && t.elapsed().as_secs() < 60 {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let s = run.snapshot();
        assert!(s.done && !s.history.is_empty() && s.cost.is_finite());
    }
}

#[cfg(test)]
mod demo {
    use super::*;
    use crate::engine::state::State;
    use crate::model::{build, train::default_train};
    use std::sync::Arc;

    #[test]
    #[ignore]
    fn print_a_real_optimisation() {
        let model = Arc::new(build());
        let base = State::new(&model);
        let ids = ["pin1.d", "obj.d", "coll.f", "dcoll.f"];
        let vars = ids.iter().map(|id| problem::DesignVar::new(&model, model.index[*id])).collect();
        let p = problem::Problem { model: model.clone(), base: base.clone(), vars, objective: problem::Objective::default() };
        let x0 = p.start();
        let (c0, k0) = p.evaluate(&x0);
        let t0 = std::time::Instant::now();
        let t = methods::run(methods::Method::GradientThenNelderMead, &|u| p.cost(u), &x0, 60, &mut |_, _, _| false);
        let (c1, k1) = p.evaluate(&t.x);
        let (k0, k1) = (k0.unwrap(), k1.unwrap());
        println!("time {:.2}s evals {} stalled {}", t0.elapsed().as_secs_f64(), t.evals, t.stalled);
        println!("cost {c0:.3} -> {c1:.3}; lateral {:.2} -> {:.2} um; sigma_z {:.0} -> {:.0} nm; strehl {:.2} -> {:.2}", k0.lateral * 1e6, k1.lateral * 1e6, k0.height * 1e9, k1.height * 1e9, k0.strehl, k1.strehl);
        for (id, x) in p.values_at(&t.x) {
            println!("  {id} = {x:.5e}");
        }
        let slots = discrete::slots(&model, &default_train(), true);
        let rows = discrete::evaluate(&model, &base, &p.objective, discrete::all_combos(&model, &slots));
        println!("{} combinations; best:", rows.len());
        for r in rows.iter().take(3) {
            println!("  cost {:.2} {:?}", r.cost, r.parts.iter().map(|(i, n)| format!("{i}:{n}")).collect::<Vec<_>>());
        }
    }
}
