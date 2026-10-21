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
        assert!(rows.iter().any(|r| r.selection == Selection::default()));
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
