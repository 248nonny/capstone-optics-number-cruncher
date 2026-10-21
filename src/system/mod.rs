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
