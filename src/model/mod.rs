pub mod flags;
pub mod library;
pub mod persist;
pub mod params;
pub mod rules;
pub mod train;

use crate::engine::{ctx::Model, source::Source};

pub fn build() -> Model {
    let train = train::default_train();
    let defs = params::defs(&train);
    let rules = rules::rules();
    let derived = ["obj.rms", "acq.texp", "det.fread"];
    let mut defaults: Vec<(String, Source)> = rules
        .iter()
        .enumerate()
        .filter(|(_, r)| defs.iter().any(|d| d.id == r.out() && (d.output || derived.contains(&d.id.as_str()))))
        .map(|(i, r)| (r.out().to_string(), Source::Rule(i)))
        .collect();
    defaults.push(("pin2.d".into(), Source::Link("pin1.d".into())));
    Model::new(defs, rules, library::library(), defaults)
}

pub fn apply_component(m: &mut Model, instance: &str, name: &str) {
    let Some(comp) = m.lib.get(name) else { return };
    let ids: Vec<String> = comp.params.keys().map(|k| if k.contains('.') { k.clone() } else { format!("{instance}.{k}") }).collect();
    for id in ids.into_iter().filter(|id| m.index.contains_key(id)) {
        m.sources.insert(id, Source::Component(name.into()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{ctx::Ctx, get::Get};

    #[test]
    fn default_scenario_matches_plan() {
        let m = build();
        let c = Ctx::new(&m);
        let um = |k: &str| c.g(k) * 1e6;
        println!("pin1.au {} pin2.au {}", c.g("pin1.au"), c.g("pin2.au"));
        println!("lat au {} wide {} fwhm {}", um("lat.au"), um("lat.wide"), um("lat.fwhm"));
        println!("ax slope {} range {} ideal {} mirror {} plane {}", c.g("ax.slope") * 1e-3 * 1e9 / 1e-6, um("ax.range"), um("ax.ideal"), um("ax.mirror"), um("ax.plane"));
        println!("peak nm {} lam {}", c.g("ax.peak") * 1e9, c.g("ax.lam_peak") * 1e9);
        println!("p_pin uW {} eta {} strehl {} peak_e {} pix_max {} snr {} sat {}", c.g("ph.p_pin") * 1e6, c.g("ph.eta"), c.g("ph.strehl"), c.g("ph.peak_e"), c.g("ph.pix_max"), c.g("ph.snr"), c.g("ph.sat"));
        println!("sigma_z nm {} sigma_lam pm {}", c.g("pr.sigma_z") * 1e9, c.g("pr.sigma_lam") * 1e12);
        println!("texp us {} tread us {} link need {} max {}", c.g("acq.texp") * 1e6, c.g("det.tread") * 1e6, c.g("link.need"), c.g("link.max"));
        println!("flags {:?} errors {:?}", flags::flags(&c), c.errors.borrow());
        assert!((c.g("obj.na") - 0.1).abs() < 1e-9);
        assert!((c.g("pin1.au") - 0.946).abs() < 0.01);
        assert!(c.g("pr.sigma_z").is_finite());
    }
}

#[cfg(test)]
mod scenario_tests {
    use super::*;
    use crate::engine::{ctx::Ctx, get::Get};

    fn value(m: &Model, k: &str) -> f64 {
        Ctx::new(m).g(k)
    }

    #[test]
    fn ltc2225_component_sets_adc_and_detector() {
        let mut m = build();
        apply_component(&mut m, "adc", "LTC2225 (12-bit 10 MS/s parallel) + differential epc901");
        assert_eq!(value(&m, "adc.bits"), 12.0);
        assert_eq!(value(&m, "det.fread"), 10e6);
        assert_eq!(value(&m, "det.mode"), 0.0);
        assert!(value(&m, "det.tread") < 110e-6);
    }

    #[test]
    fn ads8860_flags_spi_clock_and_uses_single_ended() {
        let mut m = build();
        apply_component(&mut m, "adc", "ADS8860 (16-bit 1 MS/s SPI) + single-ended epc901");
        let c = Ctx::new(&m);
        assert_eq!(c.g("det.mode"), 1.0);
        assert!(flags::flags(&c).iter().any(|f| f.contains("SPI needs")));
    }

    #[test]
    fn matching_pinhole_by_inverting_airy_rule() {
        let mut m = build();
        let r = m.rules_of("pin1.au").find(|&r| m.rules[r].out() == "pin1.au").unwrap();
        m.set_value("pin1.au", 0.5);
        m.set_source("pin1.d", crate::engine::source::Source::Rule(r));
        assert!((value(&m, "pin1.d") - 13.21e-6).abs() < 0.1e-6);
        assert!((value(&m, "pin2.d") - value(&m, "pin1.d")).abs() < 1e-12);
    }

    #[test]
    fn grating_derived_dispersion_close_to_fixed_span() {
        let mut m = build();
        let r = m.rules_of("spec.dlam").find(|&r| m.rules[r].out() == "spec.dlam").unwrap();
        m.set_source("spec.dlam", crate::engine::source::Source::Rule(r));
        assert!((value(&m, "spec.dlam") * 1e9 - 0.208).abs() < 0.01);
    }

    #[test]
    fn rates_are_flagged_not_capped() {
        let mut m = build();
        m.set_value("acq.rate", 1e6);
        let c = Ctx::new(&m);
        assert!(c.g("acq.texp").is_nan());
        assert!(flags::flags(&c).iter().any(|f| f.contains("Exposure is not positive")));
        assert!(c.g("link.need") > 100.0);
    }

    #[test]
    fn dependent_na_slider_inverts_into_stop_diameter() {
        let mut m = build();
        m.set_value("obj.na", 0.2);
        m.set_source("obj.d", crate::engine::source::Source::Rule(0));
        assert!((value(&m, "obj.d") - 5.08e-3).abs() < 1e-6);
    }

    #[test]
    fn sampling_overrides_matches_sweep_semantics() {
        let m = build();
        let c = Ctx::with(&m, [("pin1.d".to_string(), 10e-6)].into());
        assert!(c.g("lat.fwhm") < Ctx::new(&m).g("lat.fwhm"));
    }
}

#[cfg(test)]
mod perf {
    use super::*;
    use crate::engine::{ctx::Ctx, get::Get};

    #[test]
    fn full_evaluation_is_fast() {
        let m = build();
        let t = std::time::Instant::now();
        let n = 20;
        for _ in 0..n {
            let c = Ctx::new(&m);
            m.defs.iter().for_each(|d| {
                c.g(&d.id);
            });
            flags::flags(&c);
        }
        let ms = t.elapsed().as_secs_f64() * 1e3 / n as f64;
        println!("full evaluation {ms:.2} ms");
        assert!(ms < 50.0, "{ms}");
    }
}

#[cfg(test)]
mod pinhole_sweep {
    use super::*;
    use crate::engine::{ctx::Ctx, get::Get};

    #[test]
    fn outputs_stay_finite_and_monotone_over_the_slider_range() {
        let m = build();
        let rows: Vec<[f64; 4]> = [5e-6, 12e-6, 25e-6, 50e-6, 100e-6]
            .iter()
            .map(|&d| {
                let c = Ctx::with(&m, [("pin1.d".to_string(), d)].into());
                [c.g("lat.fwhm") * 1e6, c.g("ax.mirror") * 1e6, c.g("ph.eta"), c.g("ph.snr")]
            })
            .collect();
        rows.iter().for_each(|r| println!("{r:?}"));
        assert!(rows.iter().flatten().all(|v| v.is_finite()));
        assert!(rows.windows(2).all(|w| w[1][0] >= w[0][0] * 0.99));
        assert!(rows.windows(2).all(|w| w[1][1] >= w[0][1] * 0.99));
    }

    #[test]
    fn default_read_clock_follows_rate() {
        let m = build();
        let c = Ctx::new(&m);
        println!("fread {} noise {} snr {} sigma_z {} mc_z {}", c.g("det.fread"), c.g("det.noise"), c.g("ph.snr"), c.g("pr.sigma_z") * 1e9, c.g("pr.mc_z") * 1e9);
        assert!((c.g("det.fread") - 1.1 * 1027.0 * 1e3).abs() < 1.0);
        assert!(c.g("det.noise") < 160.0);
    }
}
