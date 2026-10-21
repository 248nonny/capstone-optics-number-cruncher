pub struct Instance {
    pub name: &'static str,
    pub kind: &'static str,
    pub title: &'static str,
}

pub const fn inst(name: &'static str, kind: &'static str, title: &'static str) -> Instance {
    Instance { name, kind, title }
}

pub const READOUTS: [&str; 4] = ["lat", "ax", "ph", "pr"];

pub fn default_train() -> Vec<Instance> {
    vec![
        inst("sys", "sys", "System"),
        inst("led", "led", "LED source"),
        inst("pin1", "pin", "Pinhole 1 (source)"),
        inst("coll", "lens", "Collimator"),
        inst("bs", "bs", "Beam splitter"),
        inst("obj", "obj", "Objective (dispersive)"),
        inst("sample", "sample", "Sample"),
        inst("dcoll", "lens", "De-collimator"),
        inst("pin2", "pin", "Pinhole 2 (detection)"),
        inst("spec", "spec", "Spectrometer"),
        inst("adc", "adc", "ADC (optional)"),
        inst("det", "det", "Detector (epc901)"),
        inst("acq", "acq", "Acquisition"),
        inst("link", "link", "Host link"),
        inst("lat", "lat", "Lateral resolution"),
        inst("ax", "ax", "Axial response"),
        inst("ph", "ph", "Photon budget"),
        inst("pr", "pr", "Precision"),
    ]
}
