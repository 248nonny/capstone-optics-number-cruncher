use super::train::Instance;
use crate::engine::var::VarDef;

const NM: f64 = 1e-9;
const UM: f64 = 1e-6;
const MM: f64 = 1e-3;
const DEG: f64 = std::f64::consts::PI / 180.0;
const ONE: f64 = 1.0;

fn v(
    l: &'static str,
    label: &'static str,
    unit: &'static str,
    s: f64,
    min: f64,
    max: f64,
    d: f64,
) -> VarDef {
    VarDef::new(l, label, unit, s, min, max, d)
}

fn locals(kind: &str) -> Vec<VarDef> {
    match kind {
        "sys" => vec![v("lam0", "Reference wavelength", "nm", NM, 400.0 * NM, 800.0 * NM, 550.0 * NM).doc("Wavelength used for all scalar (non-spectral) formulas")],
        "led" => vec![
            v("i", "Drive current", "mA", MM, 10.0 * MM, 1.0, 0.35).doc("Datasheet point: 350 mA, 85 degC junction").log().design(),
            v("flux_ref", "Luminous flux at 350 mA", "lm", ONE, 10.0, 300.0, 122.0).doc("GW VJLPL1.CM 4000 K typical"),
            v("side", "Emitting area side", "mm", MM, 0.2 * MM, 3.0 * MM, 1.1 * MM).doc("Light-emitting surface 1.1 x 1.1 mm (dimension drawing)"),
            v("prox", "Pinhole proximity coupling", "", ONE, 0.1, 1.0, 0.95).doc("Fraction lost because the fragile pinhole cannot touch the LED"),
            v("flux", "Luminous flux at drive current", "lm", ONE, 1.0, 1e3, 0.0),
            v("pe", "Radiant flux", "W", ONE, 1e-3, 10.0, 0.0).doc("Radiant flux from luminous flux via the photopic curve"),
            v("rad", "Radiance", "W/(mm2 sr)", 1e6, 1e3, 1e7, 0.0).doc("Lambertian radiance L = flux / (pi A)"),
        ],
        "pin" => vec![
            v("d", "Diameter", "um", UM, 1.0 * UM, 500.0 * UM, 25.0 * UM).doc("Physical pinhole diameter").log().design(),
            v("au", "Size in Airy units", "AU", ONE, 0.05, 40.0, 0.0).doc("Pinhole diameter divided by the Airy-disc diameter in the pinhole plane").log(),
        ],
        "lens" => vec![
            VarDef::new("on", "Part model", "", ONE, 0.0, 1.0, 0.0).pick(&["Real part", "Ideal (ignore losses and aberrations)"], 0).doc("Ideal parts have no loss, stray light, chromatic drift or wavefront error; their imperfection parameters are hidden"),
            v("f", "Focal length", "mm", MM, 5.0 * MM, 500.0 * MM, 50.0 * MM).doc("At the reference wavelength").log().design(),
            v("d", "Clear aperture", "mm", MM, 2.0 * MM, 60.0 * MM, 25.4 * MM),
            v("na", "Acceptance NA", "", ONE, 0.005, 0.95, 0.0).doc("NA = D / 2f"),
            v("m", "Relay magnification", "x", ONE, 0.02, 20.0, 0.0).doc("Pinhole-to-sample magnification f_obj / f").log(),
            v("dfrac", "Focal drift per 100 nm", "%", 1e-2, -0.1, 0.1, 0.0005).doc("Relative focal-length change per 100 nm; 0 for an OAP mirror").imperfect(0.0),
            v("t", "Transmission", "", ONE, 0.1, 1.0, 0.95).imperfect(1.0),
        ],
        "bs" => vec![
            VarDef::new("on", "Part model", "", ONE, 0.0, 1.0, 0.0).pick(&["Real part", "Ideal (ignore losses and aberrations)"], 0).doc("Ideal parts have no loss, stray light, chromatic drift or wavefront error; their imperfection parameters are hidden"),
            v("r", "Reflectance", "", ONE, 0.05, 0.95, 0.5).doc("Illumination reflects, detection transmits").design(),
            v("loss", "Absorption / scatter", "", ONE, 0.0, 0.3, 0.02).imperfect(0.0),
            v("ghost", "Stray light reaching detector", "", ONE, 1e-9, 1e-2, 1e-6).doc("Fraction of the power entering pinhole 1 that reaches the spectrometer unfocused, after pinhole rejection").log().imperfect(0.0),
        ],
        "obj" => vec![
            VarDef::new("on", "Part model", "", ONE, 0.0, 1.0, 0.0).pick(&["Real part", "Ideal (ignore losses and aberrations)"], 0).doc("Ideal parts have no loss, stray light, chromatic drift or wavefront error; their imperfection parameters are hidden"),
            v("f", "Focal length", "mm", MM, 2.0 * MM, 100.0 * MM, 12.7 * MM).doc("At the design wavelength below").log(),
            v("lamf", "Design wavelength", "nm", NM, 400.0 * NM, 800.0 * NM, 587.6 * NM),
            VarDef::new("glass", "Dispersion model", "", ONE, 0.0, 5.0, 0.0).pick(crate::physics::glass::NAMES, 0).doc("Singlet glass (Sellmeier) or diffractive f ~ 1/lambda"),
            v("d", "Stop diameter", "mm", MM, 0.2 * MM, 50.0 * MM, 2.54 * MM).doc("Limits the NA: a plano-convex singlet needs stopping down").design(),
            v("na", "NA", "", ONE, 0.005, 0.95, 0.0).doc("NA = D / 2f (Abbe sine condition)"),
            v("na_ill", "Illumination NA", "", ONE, 0.005, 0.95, 0.0).doc("Smaller of the stop and what the collimator fills"),
            v("na_det", "Detection NA", "", ONE, 0.005, 0.95, 0.0).doc("Smaller of the stop and what the de-collimator collects"),
            v("ct", "Centre thickness", "mm", MM, 0.5 * MM, 20.0 * MM, 4.0 * MM).doc("Used by the singlet spherical-aberration trace; the SFPX040 drawing gives 4 mm").hidden_if_ideal(),
            v("rms", "RMS wavefront error", "waves", ONE, 0.0, 1.0, 0.0).doc("Maréchal: Strehl = exp(-(2 pi sigma)^2)").imperfect(0.0),
            v("lsa", "Marginal longitudinal SA", "um", UM, 0.0, 1e-2, 0.0).doc("Meridional ray trace of a plano-convex singlet, curved side first").hidden_if_ideal(),
            v("t", "Transmission", "", ONE, 0.1, 1.0, 0.86).doc("Uncoated N-SF11 is about 0.86").imperfect(1.0),
        ],
        "sample" => vec![
            v("z", "Height from focus at reference wavelength", "um", UM, -2000.0 * UM, 2000.0 * UM, 0.0).doc("Axial position of the surface"),
            v("r", "Total reflectance", "", ONE, 0.01, 1.0, 0.6),
            v("spec", "Specular fraction", "", ONE, 0.0, 1.0, 0.5).doc("Remainder is diffuse (Lambertian)"),
            v("tilt", "Surface tilt", "deg", DEG, 0.0, 30.0 * DEG, 0.0).doc("Specular return is lost beyond asin(NA)"),
        ],
        "spec" => vec![
            VarDef::new("on", "Part model", "", ONE, 0.0, 1.0, 0.0).pick(&["Real part", "Ideal (ignore losses and aberrations)"], 0).doc("Ideal parts have no loss, stray light, chromatic drift or wavefront error; their imperfection parameters are hidden"),
            v("lam_c", "Centre wavelength", "nm", NM, 400.0 * NM, 800.0 * NM, 550.0 * NM),
            v("dlam", "Dispersion per pixel", "nm/px", NM, 0.02 * NM, 2.0 * NM, 200.0 * NM / 1024.0).doc("Wavelength per pixel; the default spreads 450-650 nm over the 1024 pixels"),
            v("lines", "Grating lines", "/mm", 1e3, 100e3, 2400e3, 300e3),
            v("order", "Diffraction order", "", ONE, 1.0, 3.0, 1.0),
            v("alpha", "Incidence angle", "deg", DEG, 0.0, 60.0 * DEG, 10.0 * DEG),
            v("f_col", "Collimator focal length", "mm", MM, 10.0 * MM, 300.0 * MM, 100.0 * MM).design(),
            v("f_cam", "Camera focal length", "mm", MM, 10.0 * MM, 500.0 * MM, 128.0 * MM).doc("dx/dlambda = f m / (d cos beta)").design(),
            v("width", "Grating width", "mm", MM, 5.0 * MM, 100.0 * MM, 25.0 * MM).doc("The beam from pinhole 2 fills 2 f_col NA of it; that sets the diffraction-limited resolution lambda / (m N)"),
            v("eta", "Peak grating efficiency", "", ONE, 0.05, 1.0, 0.7).imperfect(1.0),
            v("blaze", "Blaze wavelength", "nm", NM, 400.0 * NM, 800.0 * NM, 550.0 * NM).hidden_if_ideal(),
            v("eta_w", "Efficiency bandwidth", "nm", NM, 20.0 * NM, 600.0 * NM, 200.0 * NM).doc("Gaussian efficiency envelope").imperfect(1e-3),
            v("aber", "Aberration blur (FWHM)", "px", ONE, 0.0, 10.0, 1.0).imperfect(0.0),
            v("t", "Optics transmission", "", ONE, 0.1, 1.0, 0.85).imperfect(1.0),
            v("span", "Spectral span", "nm", NM, 10.0 * NM, 1000.0 * NM, 0.0).doc("1024 pixels x dispersion"),
            v("res", "Optical resolution (FWHM)", "nm", NM, 0.01 * NM, 50.0 * NM, 0.0).doc("Slit image + pixel + diffraction + aberration in quadrature"),
            v("slit", "Pinhole 2 image width", "px", ONE, 0.1, 50.0, 0.0).doc("Needs at least 2 px for Nyquist sampling"),
        ],
        "adc" => vec![
            VarDef::new("on", "ADC", "", ONE, 0.0, 1.0, 0.0).pick(&["Off (continuous)", "On"], 0).doc("Off means no quantization and no rate limit"),
            v("bits", "Resolution", "bit", ONE, 8.0, 18.0, 12.0).when_on(),
            v("fs", "Sample rate", "MS/s", 1e6, 0.1e6, 50e6, 10e6).doc("Becomes the pixel clock when linked to the detector read clock").log().when_on(),
            v("range", "Input range", "V", ONE, 0.5, 5.0, 2.0).when_on(),
            v("noise", "Noise", "LSB", ONE, 0.0, 5.0, 0.5).doc("RMS, added to quantization noise").when_on(),
            v("tconv", "Conversion time", "ns", 1e-9, 0.0, 1e-6, 0.0).doc("Serial ADCs must clock data out inside T - t_conv").when_on(),
            VarDef::new("iface", "Interface", "", ONE, 0.0, 1.0, 0.0).pick(&["Parallel (PIO)", "SPI"], 0).when_on(),
            v("sclk_max", "Max serial clock", "MHz", 1e6, 1e6, 150e6, 66.6e6).doc("Limit of the ADC (ADS8860: 66.6 MHz); a Pico 2 PIO state machine can clock SPI at up to half its 150 MHz system clock").when_on(),
        ],
        "det" => vec![
            VarDef::new("mode", "Output mode", "", ONE, 0.0, 1.0, 0.0).pick(&["Differential", "Single-ended"], 0).doc("Differential: 2 Vpp, 700 uV noise; single-ended: 1.6 Vpp, 500 uV"),
            v("cg", "Conversion gain", "uV/e-", 1e-6, 3e-6, 8e-6, 5e-6).doc("Datasheet 3-8, typical 5"),
            v("dark", "Dark drift", "V/s", ONE, 0.01, 5.0, 0.8).doc("Datasheet typical, doubles every 8 K"),
            v("prnu", "Residual pixel gain non-uniformity", "%", 1e-2, 0.0, 0.1, 0.005).doc("Datasheet PRNU is 2 % typical (6 % max); a flat-field correction leaves a fraction of it"),
            v("temp", "Temperature", "C", ONE, -20.0, 70.0, 25.0),
            v("fread", "Read clock", "MHz", 1e6, 0.1e6, 54e6, 5e6).doc("Noise and swing interpolated from datasheet table 7").log(),
            VarDef::new("bin", "Binning", "", ONE, 0.0, 2.0, 0.0).pick(&["x1 (1024 px)", "x2 (512 px)", "x4 (256 px)"], 0),
            v("nclk", "Read clocks per spectrum", "", ONE, 1.0, 2000.0, 0.0).doc("1027 / 515 / 259 clock pulses"),
            v("tread", "Read-out time", "us", UM, 1e-6, 1e-1, 0.0).doc("clocks / read clock"),
            v("noise", "Read noise", "e-", ONE, 1.0, 1e4, 0.0).doc("From datasheet SNR table, interpolated in read clock"),
            v("fw", "Usable full well", "ke-", 1e3, 1e3, 1e6, 0.0).doc("Output swing / conversion gain"),
        ],
        "acq" => vec![
            v("rate", "Spectra per second", "kHz", 1e3, 1.0, 1e5, 1e3).doc("Requested sample rate").log(),
            VarDef::new("overlap", "Read-out", "", ONE, 0.0, 1.0, 0.0).pick(&["Overlapped with exposure", "Sequential"], 0).doc("The frame store lets the next exposure run during read-out"),
            v("texp", "Exposure time", "us", UM, 1e-7, 1.0, 0.0).doc("Period minus overhead (and read-out if sequential); NaN if not positive").log(),
            v("sf", "Scan step / lateral FWHM", "", ONE, 0.1, 2.0, 0.5).doc("0.5 is Nyquist"),
            v("nx", "Image width", "px", ONE, 2.0, 5000.0, 200.0),
            v("ny", "Image height", "px", ONE, 1.0, 5000.0, 200.0),
            v("step", "Scan step", "um", UM, 1e-8, 1e-3, 0.0),
            v("arate", "Area rate", "mm2/s", 1e-6, 1e-9, 1e3, 0.0).doc("rate x step^2"),
            v("timg", "Time per image", "s", ONE, 1e-3, 1e5, 0.0),
        ],
        "link" => vec![
            v("rate", "Link throughput", "MB/s", 1e6, 0.05e6, 100e6, 1e6).doc("Pico 2 full-speed USB is roughly 1 MB/s in practice"),
            v("bytes", "Bytes per pixel", "B", ONE, 1.0, 4.0, 2.0),
            VarDef::new("onboard", "On-device processing", "", ONE, 0.0, 1.0, 0.0).pick(&["Send raw spectra", "Send peak only"], 0),
            v("need", "Required throughput", "MB/s", 1e6, 1e3, 1e10, 0.0).doc("Flagged, never clamps the sample rate"),
            v("max", "Spectra per second the link carries", "kHz", 1e3, 1e-3, 1e4, 0.0),
        ],
        "lat" => vec![
            v("au", "Airy unit (illumination)", "um", UM, 0.1 * UM, 100.0 * UM, 0.0).doc("1.22 lambda / NA"),
            v("wide", "Wide-field FWHM", "um", UM, 0.1 * UM, 100.0 * UM, 0.0).doc("0.51 lambda / NA"),
            v("fwhm", "Confocal FWHM (finite pinholes)", "um", UM, 0.1 * UM, 100.0 * UM, 0.0).doc("Product of the two pinhole-blurred PSFs"),
        ],
        "ax" => vec![
            v("slope", "Chromatic slope dz/dlambda", "um/nm", 1e3, 1.0, 1e5, 0.0).doc("At the reference wavelength"),
            v("range", "Axial range over spectrometer span", "um", UM, 1.0 * UM, 1e-2, 0.0),
            v("ideal", "Mirror FWHM, ideal point pinhole", "um", UM, 0.1 * UM, 1e-2, 0.0).doc("0.886 lambda / NA^2"),
            v("mirror", "Mirror FWHM (finite pinholes)", "um", UM, 0.1 * UM, 1e-2, 0.0).doc("Numeric incoherent pinhole integral"),
            v("plane", "Diffuse plane FWHM (finite pinholes)", "um", UM, 0.1 * UM, 1e-2, 0.0).doc("Both paths defocused by z, integrated over the plane"),
            v("peak", "Peak width in wavelength", "nm", NM, 0.01 * NM, 500.0 * NM, 0.0).doc("Measured on the modelled spectrum"),
            v("lam_peak", "Peak wavelength", "nm", NM, 300.0 * NM, 900.0 * NM, 0.0),
        ],
        "ph" => vec![
            v("p_pin", "Power into pinhole 1", "uW", 1e-6, 1e-9, 1.0, 0.0).doc("Etendue limited: L A pi NA^2"),
            v("eta", "Pinhole 2 transmission at focus", "", ONE, 1e-4, 1.0, 0.0).doc("Mirror at best focus"),
            v("strehl", "Strehl ratio", "", ONE, 0.0, 1.0, 0.0),
            v("peak_e", "Signal electrons per exposure (whole spectrum)", "ke-", 1e3, 1.0, 1e9, 0.0).doc("Per exposure"),
            v("pix_max", "Brightest pixel", "ke-", 1e3, 1.0, 1e9, 0.0).doc("Per exposure, signal plus background"),
            v("snr", "Peak pixel SNR", "", ONE, 1e-3, 1e5, 0.0).doc("Signal over shot + read + dark + ADC noise"),
            v("sat", "Fraction of full well", "", ONE, 1e-6, 100.0, 0.0),
            v("snr_min", "Worst on-focus SNR across the band", "", ONE, 1e-3, 1e5, 0.0).doc("Peak SNR if the surface sat at each wavelength's focus; the LED spectrum dips near 480 nm"),
            v("usable", "Band fraction with SNR above 10", "", ONE, 0.0, 1.0, 0.0).doc("Edges (10 % each side) excluded"),
        ],
        "pr" => vec![
            v("sigma_z", "Height noise σ_z (Cramér–Rao, lower is better)", "nm", NM, 1e-3 * NM, 1e-3, 0.0).doc("Smallest height error any unbiased peak locator could reach; lower is better. Lower bound for any unbiased peak locator"),
            v("sigma_lam", "Peak-position noise σ_λ (lower is better)", "pm", 1e-12, 1e-6 * NM, 1e-6, 0.0),
            v("mc_lam", "Centroid scatter (Monte Carlo)", "pm", 1e-12, 1e-6 * NM, 1e-6, 0.0).doc("120 noisy spectra through a thresholded centroid estimator"),
            v("mc_z", "Centroid height scatter (Monte Carlo)", "nm", NM, 1e-3 * NM, 1e-3, 0.0).doc("What a simple centroid achieves, versus the Cramer-Rao bound"),
        ],
        _ => vec![],
    }
}

pub fn defs(train: &[Instance]) -> Vec<VarDef> {
    train
        .iter()
        .flat_map(|i| {
            locals(i.kind).into_iter().map(move |mut d| {
                d.id = format!("{}.{}", i.name, d.id);
                d
            })
        })
        .collect()
}
