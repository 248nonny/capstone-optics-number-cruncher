pub mod flags;
pub mod formulas;
pub mod library;
pub mod outputs;
pub mod params;
pub mod persist;
pub mod train;

use crate::engine::model::Model;

pub fn build() -> Model {
    let defs = params::defs(&train::default_train()).into_iter().map(|mut d| {
        d.output = outputs::lookup(&d.id).is_some();
        d
    });
    Model::new(defs.collect(), formulas::formulas(), library::library())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{get::Get, state::State};

    #[test]
    fn every_output_has_a_definition_and_the_reverse() {
        let m = build();
        for o in outputs::OUTPUTS {
            assert!(m.index.contains_key(o.id), "{}", o.id);
        }
        for d in m.defs.iter().filter(|d| d.output) {
            assert!(outputs::lookup(&d.id).is_some(), "{}", d.id);
        }
    }

    #[test]
    fn initial_state_is_consistent() {
        let m = build();
        let s = State::new(&m);
        assert!(s.conflicts.is_empty(), "{:?}", s.conflicts);
        assert!((s.values.g("obj.na") - 0.1).abs() < 1e-9);
        assert!((s.values.g("pin1.au") - 0.946).abs() < 0.01);
        assert_eq!(s.values.g("pin2.d"), s.values.g("pin1.d"));
        assert!((s.values.g("det.fread") - 1.1 * 1027.0 * 1e3).abs() < 1.0);
    }

    #[test]
    fn default_scenario_matches_plan() {
        let m = build();
        let e = outputs::evaluate(&State::new(&m).values, &[]);
        let v = &e.values;
        assert!((v.g("lat.fwhm") * 1e6 - 4.77).abs() < 0.1);
        assert!((v.g("ax.mirror") * 1e6 - 65.5).abs() < 2.0);
        assert!(v.g("pr.sigma_z").is_finite());
    }

    fn setup() -> (Model, State) {
        let m = build();
        let s = State::new(&m);
        (m, s)
    }

    #[test]
    fn dragging_na_moves_the_stop_and_dragging_the_stop_moves_na() {
        let (m, mut s) = setup();
        s.set(&m, "obj.na", 0.2);
        assert!((s.values.g("obj.d") - 5.08e-3).abs() < 1e-6);
        s.set(&m, "obj.d", 1.27e-3);
        assert!((s.values.g("obj.na") - 0.05).abs() < 1e-9);
        assert_eq!(s.values.g("obj.f"), 12.7e-3);
    }

    #[test]
    fn airy_units_and_pinhole_size_are_both_draggable_and_pinholes_stay_identical() {
        let (m, mut s) = setup();
        let (au0, d0) = (s.values.g("pin1.au"), s.values.g("pin1.d"));
        s.set(&m, "pin1.au", 0.5);
        assert!((s.values.g("pin1.d") - d0 * 0.5 / au0).abs() < 1e-9);
        assert_eq!(s.values.g("pin2.d"), s.values.g("pin1.d"));
        assert!((s.values.g("pin2.au") - 0.5).abs() < 1e-6);
        s.set(&m, "pin1.d", 40e-6);
        assert!((s.values.g("pin1.au") - au0 * 40.0 / 25.0).abs() < 0.01);
    }

    #[test]
    fn component_locks_pinhole_size_but_airy_units_follow_other_parts() {
        let (m, mut s) = setup();
        s.apply_component(&m, "pin1", "Thorlabs P50D (50 um pinhole)");
        assert_eq!(s.values.g("pin1.d"), 50e-6);
        assert!(s.blocked(&m, m.index["pin1.d"]).is_some());
        assert!(s.blocked(&m, m.index["pin1.au"]).is_some());
        assert!(s.blocked(&m, m.index["pin2.d"]).is_some());
        let au_before = s.values.g("pin1.au");
        s.set(&m, "obj.d", 5.08e-3);
        assert!((s.values.g("pin1.au") - 2.0 * au_before).abs() < 0.02, "{} {}", s.values.g("pin1.au"), au_before);
        assert!(s.blocked(&m, m.index["obj.f"]).is_none());
    }

    #[test]
    fn every_ordinary_slider_is_draggable_in_the_default_state() {
        let (m, s) = setup();
        for (i, d) in m.defs.iter().enumerate().filter(|(_, d)| !d.output && !d.discrete()) {
            assert!(s.blocked(&m, i).is_none(), "{} {:?}", d.id, s.blocked(&m, i));
        }
    }

    #[test]
    fn a_pinhole_component_blocks_exactly_the_pinhole_sliders() {
        let (m, mut s) = setup();
        s.apply_component(&m, "pin1", "Thorlabs P50D (50 um pinhole)");
        let blocked: Vec<&str> = m.defs.iter().enumerate().filter(|(i, d)| !d.output && s.blocked(&m, *i).is_some()).map(|(_, d)| d.id.as_str()).collect();
        assert_eq!(blocked, ["pin1.d", "pin1.au", "pin2.d", "pin2.au"]);
    }

    #[test]
    fn dragging_the_second_pinhole_in_airy_units_moves_the_first_pinhole_too() {
        let (m, mut s) = setup();
        s.set(&m, "pin2.au", 0.5);
        assert!((s.values.g("pin2.au") - 0.5).abs() < 1e-6);
        assert!((s.values.g("pin1.au") - 0.5).abs() < 1e-3);
        assert!((s.values.g("pin2.d") - s.values.g("pin1.d")).abs() < 1e-12);
        assert!(s.conflicts.is_empty(), "{:?}", s.conflicts);
    }

    #[test]
    fn dragging_a_constraint_set_variable_turns_its_constraint_off() {
        let (m, mut s) = setup();
        let fread = m.index["det.fread"];
        assert!(s.blocked(&m, fread).is_none());
        assert_eq!(s.release(&m, "det.fread").as_deref(), Some("Read clock from sample rate"));
        assert!(s.setter(&m, "det.fread").is_none());
        s.set(&m, "det.fread", 3e6);
        assert_eq!(s.values.g("det.fread"), 3e6);
        s.set(&m, "acq.rate", 2e3);
        assert_eq!(s.values.g("det.fread"), 3e6);
    }

    #[test]
    fn adc_component_overrides_the_read_clock_constraint() {
        let (m, mut s) = setup();
        s.apply_component(&m, "adc", "LTC2225 (12-bit 10 MS/s parallel) + differential epc901");
        assert_eq!(s.values.g("det.fread"), 10e6);
        assert_eq!(s.values.g("adc.bits"), 12.0);
        assert!(s.conflicts.is_empty(), "{:?}", s.conflicts);
        assert!(outputs::evaluate(&s.values, &[]).values.g("det.tread") < 110e-6);
    }

    #[test]
    fn ads8860_flags_the_spi_clock_and_uses_single_ended() {
        let (m, mut s) = setup();
        s.apply_component(&m, "adc", "ADS8860 (16-bit 1 MS/s SPI) + single-ended epc901");
        let e = outputs::evaluate(&s.values, &[]);
        assert_eq!(e.values.g("det.mode"), 1.0);
        assert!(e.flags.iter().any(|f| f.contains("SPI needs")));
    }

    #[test]
    fn rates_are_flagged_not_capped() {
        let (m, mut s) = setup();
        s.set(&m, "acq.rate", 1e6);
        assert!(s.values.g("acq.texp").is_nan());
        let e = outputs::evaluate(&s.values, &[]);
        assert!(e.flags.iter().any(|f| f.contains("Exposure is not positive")));
        assert!(e.values.g("link.need") > 100.0);
    }

    #[test]
    fn custom_links_couple_any_two_parameters_in_both_directions() {
        let (m, mut s) = setup();
        s.add_link(&m, "dcoll.f", "coll.f");
        s.set(&m, "coll.f", 75e-3);
        assert!((s.values.g("dcoll.f") - 75e-3).abs() < 1e-12);
        s.set(&m, "dcoll.f", 40e-3);
        assert!((s.values.g("coll.f") - 40e-3).abs() < 1e-12);
    }

    #[test]
    fn locked_parts_make_a_conflict_instead_of_moving() {
        let (m, mut s) = setup();
        s.apply_component(&m, "coll", "Thorlabs AC254-050-A (achromat f=50)");
        s.apply_component(&m, "dcoll", "Thorlabs AC127-019-A (achromat f=19)");
        s.add_link(&m, "dcoll.f", "coll.f");
        assert!(!s.conflicts.is_empty());
    }

    #[test]
    fn sweep_semantics_follow_constraints() {
        let (m, s) = setup();
        let mut t = s.clone();
        t.set(&m, "pin1.d", 10e-6);
        let small = outputs::evaluate_ids(&t.values, &["lat.fwhm"])[0];
        assert!(small < outputs::evaluate_ids(&s.values, &["lat.fwhm"])[0]);
        assert_eq!(t.values.g("pin2.d"), 10e-6);
    }

    #[test]
    fn pinhole_sweep_stays_finite_and_monotone() {
        let (m, s) = setup();
        let rows: Vec<[f64; 3]> = [5e-6, 12e-6, 25e-6, 50e-6, 100e-6]
            .iter()
            .map(|&d| {
                let mut t = s.clone();
                t.set(&m, "pin1.d", d);
                let o = outputs::evaluate_ids(&t.values, &["lat.fwhm", "ax.mirror", "ph.snr"]);
                [o[0], o[1], o[2]]
            })
            .collect();
        assert!(rows.iter().flatten().all(|v| v.is_finite()));
        assert!(rows.windows(2).all(|w| w[1][0] >= w[0][0] * 0.99 && w[1][1] >= w[0][1] * 0.99));
    }

    #[test]
    fn full_evaluation_time_budget() {
        let (_, s) = setup();
        let t = std::time::Instant::now();
        (0..20).for_each(|_| {
            outputs::evaluate(&s.values, &[]);
        });
        let ms = t.elapsed().as_secs_f64() * 1e3 / 20.0;
        println!("full evaluation {ms:.2} ms");
        assert!(ms < 25.0, "{ms}");
    }

    #[test]
    fn changing_pinhole_geometry_is_cheap_and_smooth() {
        let (m, s) = setup();
        confocal_warm();
        let mut last = 0.0;
        let t = std::time::Instant::now();
        let vals: Vec<f64> = (0..20)
            .map(|k| {
                let mut u = s.clone();
                u.set(&m, "obj.d", 2.54e-3 * (1.0 + 0.004 * k as f64));
                outputs::evaluate_ids(&u.values, &["ax.mirror"])[0]
            })
            .collect();
        let ms = t.elapsed().as_secs_f64() * 1e3 / 20.0;
        println!("evaluate after geometry change: {ms:.2} ms");
        assert!(vals.windows(2).all(|w| w[1] < w[0]), "{vals:?}");
        last += ms;
        assert!(last < 20.0, "{last}");
    }

    fn confocal_warm() {
        crate::physics::confocal::warm();
    }
}

#[cfg(test)]
mod interaction_cost {
    use super::*;
    use crate::engine::state::State;

    #[test]
    fn a_slider_drag_step_costs_well_under_a_frame() {
        let m = build();
        let mut s = State::new(&m);
        let t = std::time::Instant::now();
        let n = 50;
        for k in 0..n {
            s.set(&m, "obj.d", 2.54e-3 * (1.0 + 0.001 * k as f64));
            let _: Vec<_> = (0..m.defs.len()).map(|i| if m.defs[i].output { None } else { s.blocked(&m, i) }).collect();
        }
        let ms = t.elapsed().as_secs_f64() * 1e3 / n as f64;
        println!("drag step (settle + blocked flags): {ms:.2} ms");
        assert!(ms < 8.0, "{ms}");
    }
}
