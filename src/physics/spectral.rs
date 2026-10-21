use crate::engine::curve::Curve;
use std::sync::OnceLock;

macro_rules! curve {
    ($name:ident, $file:literal) => {
        pub fn $name() -> &'static Curve {
            static C: OnceLock<Curve> = OnceLock::new();
            C.get_or_init(|| Curve::parse(include_str!(concat!("../../data/", $file))))
        }
    };
}

curve!(led, "led_4000k.csv");
curve!(qe, "qe_epc901.csv");
curve!(vlambda, "vlambda.csv");
