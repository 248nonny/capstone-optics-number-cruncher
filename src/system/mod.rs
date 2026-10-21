pub mod inputs;
pub mod optics;
pub mod outputs;
pub mod readout;

use crate::engine::model::Model;

pub fn model() -> Model {
    Model::new(outputs::all_defs(), crate::parts::library())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::get::Get;

    #[test]
    fn the_windowed_fisher_information_matches_the_full_spectrum() {
        confocal_warm();
        let m = model();
        let s = optics::Setup::new(&m.defaults());
        let a = {
            let nodes: Vec<_> = (0..readout::NODES).map(|k| readout::node(&s, k)).collect();
            readout::best(&s, &nodes)
        };
        let r = a.readout;
        for k in [0, 2, 4, 6, 8] {
            let windowed = readout::node(&s, k);
            let full = readout::node_with(&s, k, 1e9);
            let sw = readout::assess(&s, &[windowed], r).sigma[0];
            let sf = readout::assess(&s, &[full], r).sigma[0];
            assert!(
                (sw / sf - 1.0).abs() < 0.02,
                "node {k}: windowed {sw} full {sf}"
            );
        }
    }

    fn confocal_warm() {
        crate::physics::confocal::warm();
    }

    #[test]
    fn a_full_evaluation_is_fast_once_the_tables_are_warm() {
        confocal_warm();
        let m = model();
        let v = m.defaults();
        let ms = (0..5)
            .map(|_| {
                let t = std::time::Instant::now();
                outputs::evaluate(&v);
                t.elapsed().as_secs_f64() * 1e3
            })
            .fold(f64::MAX, f64::min);
        println!("system evaluation {ms:.2} ms");
        assert!(ms < 40.0, "{ms}");
    }

    fn eval(set: &[(&str, f64)]) -> outputs::Report {
        confocal_warm();
        let m = model();
        let mut v = m.defaults();
        set.iter().for_each(|(k, x)| v.set(k, *x));
        outputs::evaluate(&v)
    }

    #[test]
    fn worst_case_is_never_better_than_the_mean_and_the_smooth_worst_lies_between() {
        let r = eval(&[]);
        let g = |k: &str| r.values.g(k);
        assert!(
            g("lat.worst") >= g("lat.mean")
                && g("ax.diff_worst") >= g("ax.diff_mean")
                && g("ax.spec_worst") >= g("ax.spec_mean")
        );
        assert!(g("pr.worst") >= g("pr.mean") && g("pr.snr_worst") <= g("pr.snr_mean"));
        assert!(g("pr.soft") <= g("pr.worst") + 1e-12 && g("pr.soft") >= g("pr.mean"));
    }

    #[test]
    fn every_dispersion_shape_spans_exactly_the_requested_depth_range() {
        for shape in 0..inputs::SHAPES.len() {
            let r = eval(&[("opt.shape", shape as f64)]);
            let (first, last) = (&r.nodes[0], &r.nodes[readout::NODES - 1]);
            assert!(
                first.z.abs() < 1e-12 && (last.z - 700e-6).abs() < 1e-9,
                "shape {shape}: {} {}",
                first.z,
                last.z
            );
            assert!(r.values.g("pr.worst").is_finite(), "shape {shape}");
        }
    }

    #[test]
    fn the_extremes_of_every_knob_give_finite_outputs() {
        let m = model();
        for d in m.defs.iter().filter(|d| !d.output) {
            let values: Vec<f64> = if d.discrete() {
                (0..d.choices.len()).map(|k| k as f64).collect()
            } else {
                vec![d.min, d.max]
            };
            for x in values {
                let r = eval(&[(d.id.as_str(), x)]);
                for o in m.defs.iter().filter(|o| o.output) {
                    let y = r.values.g(&o.id);
                    assert!(y.is_finite(), "{} = {x}: {} is {y}", d.id, o.id);
                }
            }
        }
    }

    #[test]
    fn the_exposure_never_saturates_even_for_a_bright_mirror() {
        let r = eval(&[
            ("smp.r", 1.0),
            ("smp.spec", 1.0),
            ("src.i", 1.0),
            ("opt.au", 3.0),
        ]);
        assert!(
            r.values.g("ph.fill") <= 0.8 + 1e-9,
            "{}",
            r.values.g("ph.fill")
        );
        assert!(r.values.g("acq.duty") < 0.95);
        assert!(r.flags.iter().any(|f| f.contains("avoid saturating")));
    }

    #[test]
    fn more_light_and_a_cooler_sensor_never_hurt_and_a_faster_rate_never_helps() {
        let base = eval(&[]).values.g("pr.soft");
        assert!(eval(&[("src.i", 0.7)]).values.g("pr.soft") < base);
        assert!(eval(&[("acq.cool", 0.0)]).values.g("pr.soft") <= base * 1.0001);
        assert!(eval(&[("acq.rate", 5000.0)]).values.g("pr.soft") > base);
    }

    #[test]
    fn a_rate_beyond_the_sensor_still_gives_an_answer_and_raises_no_panic() {
        let r = eval(&[("acq.rate", 5e4)]);
        assert!(r.values.g("acq.fread") <= 54e6 && r.values.g("pr.worst").is_finite());
    }

    #[test]
    fn a_band_that_does_not_fit_is_flagged_and_hurts_the_end_nodes() {
        let tight = eval(&[("spc.disp", 0.1e-9)]);
        assert!(
            tight.values.g("acq.fill") > 1.0
                && tight.flags.iter().any(|f| f.contains("of the sensor"))
        );
        let ok = eval(&[("spc.disp", 0.5e-9)]);
        assert!(tight.values.g("pr.worst") > ok.values.g("pr.worst"));
    }

    #[test]
    fn the_aberration_tick_removes_the_strehl_penalty() {
        assert_eq!(eval(&[("opt.ab", 0.0)]).values.g("ph.strehl"), 1.0);
        assert!(eval(&[("opt.na", 0.4)]).values.g("ph.strehl") < 0.5);
    }

    #[test]
    #[ignore]
    fn print_default() {
        let m = model();
        let t = std::time::Instant::now();
        let r = outputs::evaluate(&m.defaults());
        println!("evaluate {:.2} ms", t.elapsed().as_secs_f64() * 1e3);
        for d in m.defs.iter().filter(|d| d.output) {
            println!(
                "{:<12} {:>12.4} {}",
                d.id,
                r.values.g(&d.id) / d.scale,
                d.unit
            );
        }
        println!("flags {:?}", r.flags);
        for n in &r.nodes {
            println!(
                "node {:.0} nm z {:.0} um lat {:.2} spec {:.1} diff {:.1} peak {:.1} nm",
                n.lam * 1e9,
                n.z * 1e6,
                n.lateral * 1e6,
                n.ax_spec * 1e6,
                n.ax_diff * 1e6,
                n.peak_nm * 1e9
            );
        }
        println!(
            "sigma {:?}",
            r.assessment
                .sigma
                .iter()
                .map(|x| (x * 1e9).round())
                .collect::<Vec<_>>()
        );
        println!(
            "snr {:?}",
            r.assessment
                .snr
                .iter()
                .map(|x| x.round())
                .collect::<Vec<_>>()
        );
    }
}
