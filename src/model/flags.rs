use crate::engine::{ctx::Ctx, get::Get};
use super::rules::sig;
use crate::physics::spectrometer as spec;

fn check(ok: bool, msg: impl FnOnce() -> String) -> Option<String> {
    (!ok).then(msg)
}

pub fn flags(c: &Ctx) -> Vec<String> {
    let g = |k: &str| c.g(k);
    let period = 1.0 / g("acq.rate");
    let sclk = g("adc.bits") / (1.0 / g("adc.fs") - g("adc.tconv"));
    let adc = g("adc.on") > 0.5;
    let spi = adc && g("adc.iface") > 0.5;
    let au = [g("pin1.au"), g("pin2.au")];
    let grating = spec::grating_dlam(c);
    let vp = sig(c).vp;
    let checks = [
        check(g("acq.texp").is_finite(), || "Exposure is not positive: rate too high for the overhead and read-out".into()),
        check(g("det.tread") <= period, || format!("Read-out takes {:.0} us, longer than the {:.0} us period", g("det.tread") * 1e6, period * 1e6)),
        check(!adc || (g("adc.fs") - g("det.fread")).abs() < 1.0, || "ADC sample rate differs from the detector read clock".into()),
        check(!spi || sclk.is_nan() || sclk <= g("adc.sclk_max"), || format!("SPI needs a {:.0} MHz clock, above the {:.0} MHz limit", sclk / 1e6, g("adc.sclk_max") / 1e6)),
        check(g("link.need").is_nan() || g("link.need") * 1e6 <= g("link.rate"), || format!("Link needs {:.2} MB/s but carries {:.2} MB/s", g("link.need"), g("link.rate") / 1e6)),
        check(g("ph.sat") <= 1.0, || format!("Pixels saturate ({:.1}x full well)", g("ph.sat"))),
        check(g("ph.snr").is_nan() || g("ph.snr") >= 10.0, || format!("Peak SNR is only {:.1}: photon or read-noise limited", g("ph.snr"))),
        check(g("ph.strehl") >= 0.8, || format!("Strehl {:.2}: not diffraction limited, resolution numbers are optimistic", g("ph.strehl"))),
        check(vp.iter().all(|&v| v <= 24.0), || format!("Pinhole radii of {:.1} and {:.1} (normalised) exceed the response tables; resolution and axial numbers are unreliable", vp[0], vp[1])),
        check(g("obj.na_ill") <= 0.5, || "NA above 0.5: the paraxial axial-response normalisation is inaccurate".into()),
        check(g("spec.slit") >= 2.0, || format!("Pinhole 2 image is {:.1} px wide: spectrum undersampled", g("spec.slit"))),
        check(au.iter().all(|a| (0.5..=2.0).contains(a)), || format!("Pinholes are {:.2} and {:.2} Airy units: far from the matched 1 AU", au[0], au[1])),
        check((g("spec.dlam") / grating - 1.0).abs() < 0.1, || format!("Dispersion {:.3} nm/px differs from the grating-derived {:.3} nm/px", g("spec.dlam") * 1e9, grating * 1e9)),
    ];
    let errors = c.errors.borrow().iter().chain(&c.m.lib.errors).cloned().collect::<Vec<_>>();
    checks.into_iter().flatten().chain(errors).collect()
}
