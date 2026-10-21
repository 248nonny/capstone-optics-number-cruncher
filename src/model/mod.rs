pub mod flags;
pub mod formulas;
pub mod library;
pub mod outputs;
pub mod params;
pub mod persist;
pub mod train;
pub mod verify;

use crate::engine::model::Model;

pub fn build() -> Model {
    let defs = params::defs(&train::default_train())
        .into_iter()
        .map(|mut d| {
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
        assert!(
            (s.values.g("obj.na")
                - s.values.g("obj.d")
                    / (2.0 * crate::physics::dispersion::f_obj(&s.values, s.values.g("sys.lam0"))))
            .abs()
                < 1e-9
        );
        assert!((s.values.g("obj.na") - 0.1).abs() < 0.002);
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
        let f = crate::physics::dispersion::f_obj(&s.values, s.values.g("sys.lam0"));
        s.set(&m, "obj.na", 0.2);
        assert!((s.values.g("obj.d") - 0.4 * f).abs() < 1e-6);
        s.set(&m, "obj.d", 1.27e-3);
        assert!((s.values.g("obj.na") - 1.27e-3 / (2.0 * f)).abs() < 1e-9);
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
        assert!(
            (s.values.g("pin1.au") - 2.0 * au_before).abs() < 0.02,
            "{} {}",
            s.values.g("pin1.au"),
            au_before
        );
        assert!(s.blocked(&m, m.index["obj.f"]).is_none());
    }

    #[test]
    fn every_ordinary_slider_is_draggable_in_the_default_state() {
        let (m, s) = setup();
        for (i, d) in m
            .defs
            .iter()
            .enumerate()
            .filter(|(_, d)| !d.output && !d.discrete())
        {
            assert!(
                s.blocked(&m, i).is_none(),
                "{} {:?}",
                d.id,
                s.blocked(&m, i)
            );
        }
    }

    #[test]
    fn a_pinhole_component_blocks_exactly_the_pinhole_sliders() {
        let (m, mut s) = setup();
        s.apply_component(&m, "pin1", "Thorlabs P50D (50 um pinhole)");
        let blocked: Vec<&str> = m
            .defs
            .iter()
            .enumerate()
            .filter(|(i, d)| !d.output && s.blocked(&m, *i).is_some())
            .map(|(_, d)| d.id.as_str())
            .collect();
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
        assert_eq!(
            s.release(&m, "det.fread").as_deref(),
            Some("Read clock from sample rate")
        );
        assert!(s.setter(&m, "det.fread").is_none());
        s.set(&m, "det.fread", 3e6);
        assert_eq!(s.values.g("det.fread"), 3e6);
        s.set(&m, "acq.rate", 2e3);
        assert_eq!(s.values.g("det.fread"), 3e6);
    }

    #[test]
    fn adc_component_overrides_the_read_clock_constraint() {
        let (m, mut s) = setup();
        s.apply_component(
            &m,
            "adc",
            "LTC2225 (12-bit 10 MS/s parallel) + differential epc901",
        );
        assert_eq!(s.values.g("det.fread"), 10e6);
        assert_eq!(s.values.g("adc.bits"), 12.0);
        assert!(s.conflicts.is_empty(), "{:?}", s.conflicts);
        assert!(outputs::evaluate(&s.values, &[]).values.g("det.tread") < 110e-6);
    }

    #[test]
    fn ads8860_flags_the_spi_clock_and_uses_single_ended() {
        let (m, mut s) = setup();
        s.apply_component(
            &m,
            "adc",
            "ADS8860 (16-bit 1 MS/s SPI) + single-ended epc901",
        );
        let e = outputs::evaluate(&s.values, &[]);
        assert_eq!(e.values.g("det.mode"), 1.0);
        assert!(!e.flags.iter().any(|f| f.contains("SPI needs")));
        s.set(&m, "adc.fs", 1.2e6);
        assert!(outputs::evaluate(&s.values, &[])
            .flags
            .iter()
            .any(|f| f.contains("SPI needs")));
    }

    #[test]
    fn rates_are_flagged_not_capped() {
        let (m, mut s) = setup();
        s.set(&m, "acq.rate", 2e6);
        assert!(s.values.g("acq.texp").is_nan());
        let e = outputs::evaluate(&s.values, &[]);
        assert!(e
            .flags
            .iter()
            .any(|f| f.contains("Exposure is not positive")));
        assert!(e.values.g("link.need") > 1e8);
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
        assert!(rows
            .windows(2)
            .all(|w| w[1][0] >= w[0][0] * 0.99 && w[1][1] >= w[0][1] * 0.99));
    }

    #[test]
    fn full_evaluation_time_budget() {
        let (_, s) = setup();
        crate::physics::confocal::warm();
        let ms = (0..10)
            .map(|_| {
                let t = std::time::Instant::now();
                outputs::evaluate(&s.values, &[]);
                t.elapsed().as_secs_f64() * 1e3
            })
            .fold(f64::MAX, f64::min);
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
            let _: Vec<_> = (0..m.defs.len())
                .map(|i| {
                    if m.defs[i].output {
                        None
                    } else {
                        s.blocked(&m, i)
                    }
                })
                .collect();
        }
        let ms = t.elapsed().as_secs_f64() * 1e3 / n as f64;
        println!("drag step (settle + blocked flags): {ms:.2} ms");
        assert!(ms < 8.0, "{ms}");
    }
}

#[cfg(test)]
mod dispersion {
    use super::*;
    use crate::engine::{get::Get, state::State};

    #[test]
    fn default_dispersion_matches_the_grating_and_spreads_450_to_650() {
        let m = build();
        let s = State::new(&m);
        assert!((s.values.g("spec.dlam") * 1024.0 * 1e9 - 200.0).abs() < 0.5);
        assert!(s.conflicts.is_empty());
        assert!(outputs::evaluate(&s.values, &[])
            .flags
            .iter()
            .all(|f| !f.contains("grating")));
    }

    #[test]
    fn dragging_dispersion_moves_the_camera_focal_length_and_the_grating_stays_consistent() {
        let m = build();
        let mut s = State::new(&m);
        let f0 = s.values.g("spec.f_cam");
        s.set(&m, "spec.dlam", 0.4e-9);
        assert!(
            (s.values.g("spec.f_cam") - f0 * 0.1953125 / 0.4).abs() < 1e-4,
            "{}",
            s.values.g("spec.f_cam")
        );
        assert_eq!(s.values.g("spec.lines"), 300e3);
        s.set(&m, "spec.lines", 600e3);
        assert!(s.values.g("spec.dlam") < 0.4e-9);
    }

    #[test]
    fn large_dispersion_has_no_nans_and_flags_the_band() {
        let m = build();
        for dl in [0.5e-9, 0.8e-9, 1.0e-9, 1.5e-9, 2.0e-9] {
            let mut s = State::new(&m);
            s.set(&m, "spec.dlam", dl);
            let e = outputs::evaluate(&s.values, &[]);
            let nan: Vec<&str> = m
                .defs
                .iter()
                .filter(|d| d.output && e.values.v[m.index[&d.id]].is_nan())
                .map(|d| d.id.as_str())
                .collect();
            assert!(nan.is_empty(), "{dl} {nan:?}");
            assert!(
                e.flags.iter().any(|f| f.contains("Spectrometer covers")),
                "{dl}"
            );
        }
    }
}

#[cfg(test)]
mod ideal_parts {
    use super::*;
    use crate::engine::{get::Get, state::State};

    #[test]
    fn an_ideal_objective_has_no_loss_or_aberration_and_hides_its_imperfection_rows() {
        let m = build();
        let mut s = State::new(&m);
        let real = outputs::evaluate(&s.effective(&m), &[]);
        assert!(real.values.g("ph.strehl") < 0.99 && real.values.g("obj.t") < 0.9);
        s.set(&m, "obj.on", 1.0);
        let ideal = outputs::evaluate(&s.effective(&m), &[]);
        assert_eq!(ideal.values.g("obj.t"), 1.0);
        assert_eq!(ideal.values.g("ph.strehl"), 1.0);
        assert!(ideal.values.g("ph.peak_e") > real.values.g("ph.peak_e"));
        assert_eq!(s.values.g("obj.t"), 0.86);
        for id in ["obj.t", "obj.rms", "obj.ct"] {
            assert!(m.hidden(&s.values, m.index[id]), "{id}");
        }
        assert!(!m.hidden(&s.values, m.index["obj.f"]));
    }

    #[test]
    fn adc_rows_appear_only_when_the_adc_is_on() {
        let m = build();
        let mut s = State::new(&m);
        assert!(m.hidden(&s.values, m.index["adc.bits"]));
        s.set(&m, "adc.on", 1.0);
        assert!(!m.hidden(&s.values, m.index["adc.bits"]));
    }

    #[test]
    fn ideal_everything_beats_real_parts() {
        let m = build();
        let mut s = State::new(&m);
        let before = outputs::evaluate_ids(&s.effective(&m), &["pr.sigma_z"])[0];
        for inst in ["coll", "dcoll", "bs", "obj", "spec"] {
            s.set(&m, &format!("{inst}.on"), 1.0);
        }
        assert!(outputs::evaluate_ids(&s.effective(&m), &["pr.sigma_z"])[0] < before);
    }
}

#[cfg(test)]
mod parameter_audit {
    use super::*;
    use crate::engine::state::State;

    #[test]
    fn every_default_and_derived_input_lies_inside_its_slider_range() {
        let m = build();
        let s = State::new(&m);
        for (i, d) in m.defs.iter().enumerate().filter(|(_, d)| !d.output) {
            let x = s.values.v[i];
            assert!(
                x.is_finite() && x >= d.min * 0.999 && x <= d.max * 1.001,
                "{}: {} not in {}..{}",
                d.id,
                x,
                d.min,
                d.max
            );
            assert!(d.scale > 0.0 && d.min < d.max, "{}", d.id);
        }
    }

    #[test]
    fn every_output_stays_inside_its_display_range_in_the_default_state() {
        let m = build();
        let e = outputs::evaluate(&State::new(&m).values, &[]);
        let bad: Vec<String> = m
            .defs
            .iter()
            .filter(|d| d.output)
            .map(|d| (d, e.values.v[m.index[&d.id]]))
            .filter(|(d, x)| !(x.is_finite() && *x >= d.min && *x <= d.max))
            .map(|(d, x)| format!("{}: {x} not in {}..{}", d.id, d.min, d.max))
            .collect();
        assert!(bad.is_empty(), "{bad:?}");
    }
}

#[cfg(test)]
mod review_fixes {
    use super::*;
    use crate::engine::{get::Get, state::State};
    use crate::physics::confocal;

    #[test]
    fn binning_halves_the_pixel_count_and_still_finds_the_peak() {
        let m = build();
        let mut s = State::new(&m);
        let before = outputs::evaluate(&s.values, &[]);
        s.set(&m, "det.bin", 1.0);
        let after = outputs::evaluate(&s.values, &[]);
        assert_eq!(before.signal.lam.len(), 1024);
        assert_eq!(after.signal.lam.len(), 512);
        assert!((after.values.g("ax.lam_peak") - before.values.g("ax.lam_peak")).abs() < 2e-9);
        assert!((after.values.g("ax.peak") / before.values.g("ax.peak") - 1.0).abs() < 0.05);
        assert!(after.values.g("pr.sigma_z").is_finite());
    }

    #[test]
    fn the_link_requirement_is_in_bytes_per_second() {
        let m = build();
        let e = outputs::evaluate(&State::new(&m).values, &[]);
        assert!((e.values.g("link.need") - 2.048e6).abs() < 1.0);
        assert!(e.flags.iter().any(|f| f.contains("2.05 MB/s")));
    }

    #[test]
    fn the_pinhole_shape_bank_reproduces_the_single_wavelength_table_at_the_reference() {
        let bank = confocal::Bank::new([3.6, 3.6], 550e-9, 450e-9, 650e-9);
        let direct = confocal::axial(3.6, 3.6);
        for u in [0.0, 3.0, 10.0] {
            let blended = bank.mirror_at(550e-9, u);
            assert!(
                (blended / direct.mirror_at(u) - 1.0).abs() < 0.06,
                "{u} {blended} {}",
                direct.mirror_at(u)
            );
        }
        assert!(bank.mirror_at(450e-9, 0.0) > bank.mirror_at(650e-9, 0.0));
    }

    #[test]
    fn a_specular_mirror_loses_more_to_aberration_than_a_diffuse_plane() {
        let m = build();
        let mut s = State::new(&m);
        s.set(&m, "sample.spec", 1.0);
        s.release(&m, "obj.rms");
        s.set(&m, "obj.rms", 0.0);
        let clean = outputs::evaluate(&s.values, &[]).values.g("ph.peak_e");
        s.set(&m, "obj.rms", 0.05);
        let specular = outputs::evaluate(&s.values, &[]).values.g("ph.peak_e") / clean;
        s.set(&m, "sample.spec", 0.0);
        s.set(&m, "obj.rms", 0.0);
        let clean = outputs::evaluate(&s.values, &[]).values.g("ph.peak_e");
        s.set(&m, "obj.rms", 0.05);
        let diffuse = outputs::evaluate(&s.values, &[]).values.g("ph.peak_e") / clean;
        assert!(
            specular < diffuse
                && (diffuse / specular
                    - (-(2.0 * std::f64::consts::PI * 0.05).powi(2))
                        .exp()
                        .powi(2)
                        .recip())
                .abs()
                    < 0.1 * diffuse / specular
        );
    }
}
