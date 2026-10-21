pub mod discrete;
pub mod methods;
pub mod problem;
pub mod runner;

#[cfg(test)]
mod tests {
    use super::{
        discrete,
        methods::Method,
        problem::{DesignVar, Objective, Problem},
        runner,
    };
    use crate::parts::Selection;
    use crate::system;
    use std::sync::Arc;

    fn problem(ids: &[&str]) -> Problem {
        crate::physics::confocal::warm();
        let model = Arc::new(system::model());
        let vars = ids
            .iter()
            .map(|id| DesignVar::new(&model, model.index[*id]))
            .collect();
        Problem {
            base: model.defaults(),
            model,
            vars,
            objective: Objective::default(),
        }
    }

    #[test]
    fn gradient_descent_lowers_the_real_cost_inside_the_bounds() {
        let p = problem(&["opt.na", "opt.au", "spc.disp"]);
        let start = p.start();
        let t = super::methods::run(
            Method::GradientThenNelderMead,
            &|u| p.cost(u),
            &start,
            25,
            &mut |_, _, _| false,
        );
        assert!(t.f < p.cost(&start), "{} vs {}", t.f, p.cost(&start));
        assert!(t.x.iter().all(|u| (0.0..=1.0).contains(u)));
    }

    #[test]
    fn nelder_mead_also_improves_the_real_cost() {
        let p = problem(&["opt.au", "spc.disp"]);
        let start = p.start();
        let t = super::methods::run(
            Method::NelderMead,
            &|u| p.cost(u),
            &start,
            15,
            &mut |_, _, _| false,
        );
        assert!(t.f < p.cost(&start));
    }

    #[test]
    fn a_background_run_reports_progress_and_finishes() {
        let p = problem(&["opt.au", "spc.disp"]);
        let run = runner::spawn(p, Method::GradientThenNelderMead, 6, || {});
        let t = std::time::Instant::now();
        while !run.snapshot().done && t.elapsed().as_secs() < 60 {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let s = run.snapshot();
        assert!(s.done && !s.history.is_empty() && s.cost.is_finite());
    }

    #[test]
    fn the_part_search_counts_ranks_and_applies() {
        let lib = crate::parts::library();
        let include = [true, true, true, false];
        let n = discrete::count(&lib, include);
        let all = discrete::all(&lib, include, &Selection::default());
        assert_eq!(all.len(), n);
        let model = system::model();
        let rows = discrete::evaluate_all(
            &discrete::Ctx {
                lib: &lib,
                base: &model.defaults(),
                objective: &Objective::default(),
            },
            all,
        );
        assert_eq!(rows.len(), n);
        assert!(rows.windows(2).all(|w| w[0].cost <= w[1].cost));
        let with_parts = rows
            .iter()
            .find(|r| {
                r.selection.objective.is_some()
                    && r.selection.lens.is_some()
                    && r.selection.pinhole.is_some()
            })
            .unwrap();
        assert!(with_parts.effective.na.unwrap() > 0.0 && with_parts.effective.au.unwrap() > 0.0);
        assert!(rows.iter().all(|r| r.selection.pinhole.is_some()
            && r.selection.lens.is_some()
            && r.selection.objective.is_some()
            && r.selection.grating.is_none()));
    }

    #[test]
    fn the_part_search_costs_every_part_at_the_required_depth_range() {
        let lib = crate::parts::library();
        let model = system::model();
        let base = model.defaults();
        let all = discrete::all(&lib, [true, true, true, false], &Selection::default());
        let rows = discrete::evaluate_all(
            &discrete::Ctx {
                lib: &lib,
                base: &base,
                objective: &Objective::default(),
            },
            all,
        );
        let ar = rows
            .iter()
            .find(|r| {
                r.selection
                    .objective
                    .as_deref()
                    .is_some_and(|n| n.contains("AR14"))
            })
            .unwrap();
        let plain = rows
            .iter()
            .find(|r| {
                r.selection
                    .objective
                    .as_deref()
                    .is_some_and(|n| n.contains("uncoated"))
                    && r.selection.lens == ar.selection.lens
                    && r.selection.pinhole == ar.selection.pinhole
            })
            .unwrap();
        assert!(ar.cost < plain.cost);
    }

    #[test]
    fn snapping_prefers_parts_near_the_optimum() {
        let lib = crate::parts::library();
        let model = system::model();
        let base = model.defaults();
        let vars = vec![DesignVar::new(&model, model.index["opt.au"])];
        let rows = discrete::snap(
            &discrete::Ctx {
                lib: &lib,
                base: &base,
                objective: &Objective::default(),
            },
            [true, true, true, false],
            &Selection::default(),
            &vars,
            &[vars[0].unit(0.9)],
            6,
        );
        assert!(!rows.is_empty() && rows.len() <= 6);
    }
}

#[cfg(test)]
mod demo {
    use super::*;
    use crate::engine::get::Get;
    use crate::parts::Selection;
    use crate::system;
    use std::sync::Arc;

    #[test]
    #[ignore]
    fn print_a_real_optimisation() {
        crate::physics::confocal::warm();
        let model = Arc::new(system::model());
        let ids = ["opt.na", "opt.au", "spc.disp"];
        let vars = ids
            .iter()
            .map(|id| problem::DesignVar::new(&model, model.index[*id]))
            .collect();
        let p = problem::Problem {
            base: model.defaults(),
            model: model.clone(),
            vars,
            objective: problem::Objective::default(),
        };
        let x0 = p.start();
        let (c0, k0) = p.evaluate(&x0);
        let t0 = std::time::Instant::now();
        let t = methods::run(
            methods::Method::GradientThenNelderMead,
            &|u| p.cost(u),
            &x0,
            60,
            &mut |_, _, _| false,
        );
        let (c1, k1) = p.evaluate(&t.x);
        let (k0, k1) = (k0.unwrap(), k1.unwrap());
        println!(
            "time {:.2}s evals {} stalled {}",
            t0.elapsed().as_secs_f64(),
            t.evals,
            t.stalled
        );
        println!("cost {c0:.3} -> {c1:.3}; lateral {:.2} -> {:.2} um; noise {:.0} -> {:.0} nm; strehl {:.2} -> {:.2}; fill {:.2} -> {:.2}", k0.lateral * 1e6, k1.lateral * 1e6, k0.height * 1e9, k1.height * 1e9, k0.strehl, k1.strehl, k0.fill, k1.fill);
        for (id, x) in p.values_at(&t.x) {
            println!("  {id} = {x:.5e}");
        }
        let lib = crate::parts::library();
        let ctx = discrete::Ctx {
            lib: &lib,
            base: &model.defaults(),
            objective: &problem::Objective::default(),
        };
        let rows = discrete::evaluate_all(
            &ctx,
            discrete::all(&lib, [true, true, true, true], &Selection::default()),
        );
        println!("{} combinations; best:", rows.len());
        for r in rows.iter().take(3) {
            println!(
                "  cost {:.2} na {:?} au {:?} t {:?} {:?}",
                r.cost,
                r.effective.na.map(|x| (x * 1000.0).round() / 1000.0),
                r.effective.au.map(|x| (x * 100.0).round() / 100.0),
                r.effective.t,
                [
                    &r.selection.pinhole,
                    &r.selection.lens,
                    &r.selection.objective,
                    &r.selection.grating
                ]
                .iter()
                .map(|s| s
                    .as_deref()
                    .map(|n| n.split(" (").next().unwrap_or(n).to_string()))
                .collect::<Vec<_>>()
            );
        }
        println!("{}", model.defaults().g("opt.na"));
    }
}
