# Chromatic Confocal Profiler — Number Cruncher: Plan

Status: **draft for review, no code written yet.** Items marked `[verify]` are equations or numbers I am giving from memory or a rough read of a datasheet plot. They get checked against a primary source before they go into a tooltip.

---

## 0. Decisions already made

| Topic | Decision |
|---|---|
| Stack | Nix flake + Rust + egui. Crates: `eframe`/`egui` (bump to current), `egui_plot` (matching version), `serde`, `toml`, `rustfft` (later), `rand` (Monte Carlo, later) |
| Train | LED → pinhole 1 → collimator → BS → objective → sample → objective → BS → de-collimator → pinhole 2 (default; slit selectable) → spectrometer (grating + optics) → epc901. The train is a default, and extra elements (condenser, slit, QWP/polarizer, ...) can be added. |
| Detector | epc901 fixed (1024 × 7.5 µm × 120 µm). |
| LED | GW VJLPL1.CM (OSLON Pure 1414), **4000 K** variant. Pinhole 1 sits near the LED with no condenser by default; a proximity coupling factor `η_prox` (default 0.95) covers the small unavoidable gap. A condenser/focusing lens is an optional extra element. |
| ADC | Included as an **optional** part of the spectrometer. With the ADC switched off the detector output is treated as a continuous voltage. Host is a Raspberry Pi Pico (serial SPI or parallel via PIO). |
| Collimators | Refractive or OAP, selectable per element. |
| Sample | Selectable presets (mirror, machined metal, PCB/solder, matte, custom). |
| Confocal axial response | Analytic first, numeric added later as a cross-check. |
| Aberrations | The user doesn't know the Zemax export format yet. Design the import around plain CSV/text tables, and add direct ZMX/AGF parsing and a "Zemax-lite" paraxial/real ray tracer in the refinement stage. |
| Structure | Phase 1 = shortest path to MVP. Phase 2 = refinement and extras. |

---

## 1. Physics model

Notation: λ wavelength; NA numerical aperture; `f_c`, `f_o`, `f_d` focal lengths of collimator, objective and de-collimator; `D_p1`, `D_p2` pinhole diameters; `n` refractive index at the sample (air, 1).

### 1.1 Optical-train bookkeeping (couplings the user wants synchronized)

| Relation | Equation | Notes |
|---|---|---|
| Objective NA / pupil / focal length | `NA = D_pupil / (2 f_o)` | Exact for an objective that obeys the Abbe sine condition (`h = f sinθ`). Use `NA = n sinθ` when the pupil is given as a half-angle. |
| Pupil fill | `D_beam = 2 f_c NA_pin`, `NA_ill = min(NA_o, D_beam / (2 f_o))`, `η_pupil = min(1, (D_pupil/D_beam)²)` | `NA_pin` is the cone the collimator accepts from the pinhole. Overfill (`D_beam > D_pupil`) clips power; underfill keeps all power but lowers `NA_ill` and so resolution. |
| λ dependence | `NA(λ) = D / (2 f_o(λ))`, `m(λ) = f_o(λ)/f_c`, `AU(λ) = 1.22 λ / NA(λ)` | A dispersive objective has `f_o(λ)`; for the hybrid diffractive model (`f ∝ 1/λ`) it swings about 44 % over 450–650 nm. The scalar variables (`NA`, `f_o`, `m`) are defined at `λ_ref`; everything downstream is a spectral curve derived from `f_o(λ)`. |
| Relay magnification (pinhole → sample) | `m_1 = f_o / f_c`, `m_2 = f_o / f_d` | Illumination and detection use different collimating focal lengths. Keep them separate. |
| NA at a pinhole plane | `NA_pin,i = NA_o · m_i` | |
| Detector-side image of pinhole 2 | `w_img = D_p2 · f_cam / f_scol` | Spectrometer re-imaging, set by the spectrometer's collimator/camera focal lengths. |

Coupled groups get "pick the dependent variable". The group {`f`, `D`, `NA`} can have any one as dependent. Same for {`f_c`, `f_o`, `m`}.

**Dragging a slider in a coupled group.** One variable per group is the *dependent* one (a dropdown on the group header, default chosen per group). Its slider is shown locked (greyed, still readable). The other sliders are free and the dependent one moves in sync. To drive the dependent one instead, change the dropdown; the previous dependent becomes free and keeps its current value. Dragging never silently swaps roles.

### 1.2 Lateral resolution

All quantities refer to the sample plane.

1. Airy unit and spot sizes, per path `i ∈ {ill, det}`:
   - `AU = 1.22 λ / NA` (diameter of the first Airy zero) [standard, Born & Wolf].
   - Wide-field PSF FWHM `≈ 0.51 λ / NA`.
   - Ideal confocal (point pinhole) PSF FWHM `≈ 0.37 λ / NA` (product of two Airy intensities) [verify: Wilson 1990, "Confocal Microscopy"].
2. Pinhole in sample space: `d_i' = D_pi · m_i`. Normalized pinhole radius `v_p,i = (2π/λ) · NA · (d_i'/2)`. Pinhole size in Airy units: `d_i' / AU`. With a fixed pinhole, this ratio changes by 1.44× between 450 and 650 nm, so show it per λ.
3. **Confocal PSF is a product, not a convolution.** `h_conf(r) = [|h_ill|² ⊗ P_1'] · [|h_det|² ⊗ P_2']`. Here `P_i'` is the geometric image of pinhole `i` (a top hat of diameter `d_i'`) and `h_ill`, `h_det` are the two amplitude PSFs. The user's "Airy convolved by pinhole" is correct for each factor separately.
4. Aberration hook. Maréchal: `S ≈ exp[-(2π σ_W)²]` (σ_W RMS wavefront in waves) [verify]. Element wavefront errors add in quadrature, `σ_tot² = Σ σ_i²`. The peak intensity falls by `S`; the PSF broadens (the numeric PSF in phase 2 handles this properly).
5. Scan sampling: step `≈ FWHM_lat / 2` for Nyquist.

MVP uses closed forms for items 1, 2 and 5, plus a numeric radial integration for item 3 (cheap and not hard). Item 4 is a single "RMS wavefront error" slider per element.

### 1.3 Chromatic axial encoding

1. **Focal shift vs wavelength**, `z(λ)`. For a collimated input, `z(λ) = f_o(λ) − f_ref`. Supported models, selectable via the source radio:
   - Linear: `z = z₀ + S (λ − λ₀)`.
   - Thin singlet, `f(λ) = f₀ (n₀ − 1) / (n(λ) − 1)`, with `n(λ)` from Sellmeier (glass catalog).
   - Hybrid refractive/diffractive: `f_d(λ) = f₀ λ₀ / λ` (diffractive Abbe number −3.45), combined by optical powers.
   - Tabulated: CSV of `z(λ)` (for example from Zemax).
   - Rule of thumb: `Δz ≈ f / V` for a singlet.
2. **Collimator chromatism.** A refractive collimator's `f_c(λ)` changes the vergence entering the objective, which shifts the focus per λ. Model with paraxial vergence algebra: `z_ill(λ)`, `z_det(λ)` each include the objective plus the collimator contribution. OAP collimators have zero chromatism.
3. **Axial response depends on the sample kernel.** Define `Δ = 2 z_s − z_ill − z_det` (offset between the mirror image of the illumination focus and the detection pinhole's conjugate).
   - *Plane mirror, ideal point pinhole:* `I(u) = sinc²(u/2)` with `u = (8π/λ) n sin²(α/2) z` [verify: Wilson & Sheppard]. `FWHM_z = 0.443 λ / (n (1 − cos α)) ≈ 0.886 λ n / NA²` (paraxial). I derived 0.443 from the sinc² half-point at x ≈ 1.392. For a mirror only `Δ` matters, so a collimator/de-collimator mismatch just shifts the peak to `z_eff = (z_ill + z_det)/2`.
   - *Point scatterer / diffuse surface:* the response is the **product of the two single-pass responses**, `[sinc(u/4)]⁴` for a point object [verify: Wilson], which is wider (the team's ideation doc quotes `1.4 n λ / NA²`, about 1.4–1.6× the mirror value). Here a collimator/de-collimator mismatch **weakens and broadens** the peak instead of only shifting it.
   - Sample presets carry a specular kernel and a diffuse kernel; the output is their weighted sum by the specular/diffuse split.
4. **Finite pinholes (numeric, incoherent source).** The LED fills the pinhole incoherently, so intensity adds across source points:
   `I_det(Δ) = ∫∫ P_1(r_s) P_2(r_d) |U(r_d − r_s; Δ)|² d²r_s d²r_d`
   with `U` the defocused Airy amplitude (Lommel integral). The pinhole-pair term reduces to a kernel `K(r)` = overlap area of two circles of radii `R_1`, `R_2` at separation `r` (closed form, so unequal pinholes cost nothing extra), leaving a 1-D radial integral. This gives `FWHM_z(v_p1, v_p2)`, the on-peak power fraction, and the out-of-focus background level.
   - **Speed.** In paraxial normalized units `(u, v_p1, v_p2)` the response does not depend on λ or NA. Precompute it once as a lookup table and rescale per λ; recompute only when the pinhole ratios change, not every frame.
   - The encircled-energy fraction `η_p = 1 − J₀²(v_p) − J₁²(v_p)` is the **point-source limit** (pinhole 1 infinitely small, pinhole 2 finite) and is only quoted for that case.
   - *Analytic alternative for MVP:* a Gaussian-beam model of the finite-pinhole axial resolution (de Grauw et al., J. Microscopy 1999, Utah `confocal_intro.pdf` in the search results) gives a closed-form approximation; compare it with the numeric result later.
5. **Forward spectral model.** The spectrometer sees
   `S(λ; z_s) = L_led(λ) · T_path(λ) · R_s(λ) · η_grating(λ) · QE(λ) · I_conf(2(z_s − z_eff(λ)); λ)`
   Peak width in wavelength: `Δλ_peak = FWHM_z(λ) / |dz_eff/dλ|` (convolved with the spectrometer's optical resolution). This is why z(λ) nonlinearity makes sensitivity vary across the range.
6. **Two different "axial" outputs, shown separately:**
   - Axial *resolution* (peak FWHM in z): how far apart two surfaces must be to be told apart.
   - Axial *precision* `σ_z`: peak-centre estimation error from noise, `σ_z = σ_λ · |dz/dλ|`. This is far smaller than the FWHM. Plot it versus z.

### 1.4 Spectrometer

Default: the entrance aperture is pinhole 2 (or a slit, selectable); spectrometer collimator `f_s1`; grating; camera lens `f_s2`; epc901.

| Item | Equation |
|---|---|
| Grating | `m λ = d (sin α + sin β)` |
| Linear dispersion at the detector | `dx/dλ = f_s2 · m / (d cos β)` |
| Dispersion source (radio) | (a) **Fixed span**: 450–650 nm over 1024 px, `Δλ_px = 0.195 nm/px` (default). (b) **Derived** from grating (lines/mm, order, angle) and `f_s2`; check that the span fits on 7.68 mm and warn otherwise. |
| Image of the entrance aperture | `w_img = w_s · f_s2 / f_s1` |
| Optical resolution (quadrature, approx.) | `Δλ_res² ≈ Δλ_slit² + Δλ_pix² + Δλ_diff² + Δλ_aber²`, with `Δλ_slit = w_img · dλ/dx`, `Δλ_diff = λ / (m N)`; the pixel term includes the epc901 crosstalk (about 20 %, 4 µm σ). |
| Sampling | `w_img ≳ 2 px` for Nyquist in λ (0.195 nm/px is *sampling*, not resolution) |
| Efficiency | `η_g(λ)`: blaze curve (parametric triangular default, or a table) |
| Pixel height | 120 µm: pinhole image should fit (`w_img_y ≤ 120 µm`) |

### 1.5 Photon budget (radiometry)

Per λ, on the shared wavelength grid.

1. **Source.** `L(λ) = M(λ) / π` for Lambertian emission (the datasheet gives 120° viewing angle). Absolute scale from `Φ_v` (122 lm typical at 4000 K, 350 mA, 85 °C) via `Φ_e = Φ_v / (683 ∫ V(λ) s(λ) dλ)`, with `s(λ)` the relative spectral emission. Emitting area `A_LED = 1.1 × 1.1 mm²` (datasheet dimension drawing, the dashed inner square). Drive current scaling from the relative-flux-vs-current curve (digitize) and junction-temperature droop.
2. **Étendue-limited coupling.** `P_pin(λ) = η_prox · L(λ) · A_p1 · π NA_c²` for a pinhole in near-contact with the LED (default, `η_prox = 0.95`). A condenser/relay element can be added: `P ≤ L · min(A_LED m², A_p1) · π NA²`. You cannot beat étendue by adding optics.
3. **Forward path.** `T_coll · η_pupil · T_BS(λ) · T_obj(λ)`. Beam-splitter double pass: `T_BS,tot = R·T ≤ 25 %` for a 50/50. A polarizing-BS + QWP option removes that limit.
4. **Sample.** `R_s(λ)`, split into specular and diffuse parts. Diffuse collection fraction `≈ NA²` (Lambertian into a cone). Specular collection falls with tilt `ψ` (overlap of the returning cone and the pupil, lens-area function), so `ψ_max ≈ asin(NA)`. Include a slope-limited signal curve.
5. **Return path.** `T_obj · T_BS · T_decoll · η_p2(Δ)` (the second pinhole transmission, from §1.3 item 4), then spectrometer `T_slit · η_g(λ) · T_optics`.
6. **Detector.** `photons/s = P(λ) λ/(hc)`. `e⁻ = QE(λ) · photons · t_exp`. Spread across pixels by the dispersion mapping.
7. **Stray light and background (important for confocal).** Back-surface reflections from lenses and the beam-splitter go toward the detector; model as `ghost_fraction` × source. Out-of-focus leakage from §1.3 item 4. Dark current. Ambient light (parameter, default 0).
8. **Peak-concentrated signal.** The sample only returns a narrow band near its in-focus wavelength, so the photons that matter are `P_spectral · Δλ_peak`, not the full LED output. This is the main reason chromatic confocal is light-limited.

**Étendue is the controlling number.** A pinhole matched to about 1 Airy unit has `G = A·π·NA² = (π²/4)(1.22 λ)² ≈ 3.7 λ² ≈ 1.1e-12 m²·sr` at 550 nm, *independent of the pinhole size and NA*. A larger pinhole is not more confocal, it is just less confocal, so it is not a way to buy more light. Order-of-magnitude chain (Python scratch calculation, every factor rough and `[verify]`):
- LED radiance `L ≈ 1.4e5 W/(m²·sr)` (radiant flux ≈ 0.54 W from 122 lm with a ≈ 0.33 luminous-efficacy factor, over 1.21 mm²).
- Spectral power into a matched pinhole `≈ 0.4 nW/nm` (including `η_prox = 0.95`).
- After the beam splitter (25 %), optics (≈ 0.8), second-pinhole loss (≈ 0.5) and spectrometer (≈ 0.6): about 2.4e-11 W/nm at the detector side.
- Per 0.195 nm pixel that is about **1e4 e⁻/pixel per ms for a specular sample** (shot-noise SNR ≈ 100, far below the 400 ke⁻ full well) and about **30 e⁻/pixel per ms for a diffuse sample** (the `NA²` collection fraction times a reflectivity of 0.3 at NA 0.1), which is **below the ≈ 140 e⁻ read noise**.
- This per-pixel number does not depend on NA, because the peak simply gets wider in pixels as NA drops.

**Correction to an earlier draft:** the first version of this section used a 50 µm pinhole at `NA_c = 0.25` and concluded that the in-peak signal was near saturation at 1 ms. That was wrong: that pinhole is about 18 Airy units wide, which is not confocal, and its étendue is about 400× larger than a matched pinhole's. The correct picture is: specular samples give modest SNR, and diffuse samples are photon-starved (hence longer exposures, binning, or a brighter source).

### 1.6 epc901 detector model

From the datasheet (v7.1):

| Parameter | Value | Section |
|---|---|---|
| Pixels | 1024 × (7.5 µm × 120 µm), 100 % fill, active length 7.68 mm | §3.5 |
| Conversion gain | 5 µV/e⁻ (3–8) | §3.5 |
| Full well | 400 ke⁻ (≈ 2 V swing, differential) | §3.5 |
| Read noise | 700 µV rms diff / 500 µV single-ended at 1 MHz (≈ 140 e⁻) | §3.5 |
| Noise vs read clock | SNR 70 dB at 1 MHz (2.0 Vpp), 58 dB at 10 MHz (1.5 Vpp), 49 dB at 54 MHz (0.5 Vpp). The corresponding noise is about 0.63, 1.9 and 1.8 mV rms (derived from SNR and swing). | §3.8 |
| Dark signal (pixel) | 0.8 V/s typ (≈ 1.6e5 e⁻/s), doubling every ≈ 8 K; frame store 0.05 V/s | §3.5 |
| PRNU / DSNU | 2 % / ±0.3 % FW | §3.5 |
| QE | digitize Fig. 6: about 12 % at 400 nm, 60 % at 500 nm, 80 % at 600 nm, 90 % at 650 nm | §3.6 |
| MTF | digitize Fig. 7 (chip MTF at 630 nm: 0.5 at 40 lp/mm) | §3.7 |
| Readout | read clock 0.1–54 MHz; 1027 clocks for the full line (515 binned ×2, 259 ROI/bin ×4); frame store holds a frame ≥ 10 ms | §3.3, §5.5 |
| Binning / ROI | ×1, ×2, ×4 horizontal; centre ROI px 256–767 | §5.5 |
| Gain | 1, 2, 4 | §8.3 |

Timing model (datasheet §5.2–5.3, checked): exposure runs while `SHUTTER` is high; on its falling edge the charge shifts into the frame store (`T_SHIFT` ≈ 25 clocks, still photosensitive during the shift), and a 4-frame FIFO lets the next exposure start while the previous frame is read out. So `t_read = N_clk / f_read` and, with overlap, `t_exp ≈ T − t_flush − t_shift` as long as `t_read ≤ T`. An "overlap readout with exposure" toggle falls back to `t_exp = T − t_read` for designs that cannot overlap (for example because of readout-clock coupling). If the read clock is too slow, `t_read` is longer than the period and the achievable rate falls (for example 1 kHz needs more than about 1.03 MHz). Samples-per-second is a slider from about 1 Hz to above 46 kHz (the datasheet limit); the app shows which constraint binds.

SNR per pixel: `SNR = N_e / sqrt(N_e + N_dark + σ_read² + σ_quant² + (PRNU · N_e)²)` (PRNU removable by flat-field, parameter). External ADC: bits, full-scale, noise.


### 1.6b Optional ADC and readout chain

The ADC is an optional element of the spectrometer (a switch on its panel). Off means continuous output: no quantization, no sampling limit, no link limit.

Parameters: `bits`, `SNR` (or ENOB), `f_s,max`, input range (V), common mode, input type (differential / single-ended), interface (SPI / parallel), `ADC_noise` (LSB rms), reference.

Derived outputs:
- **LSB in electrons:** `LSB_e = (V_range / 2^bits) / CG`. Quantization noise `LSB_e / √12`. For a 2 V range and 5 µV/e⁻ (400 ke⁻ full well) a 12-bit ADC has `LSB_e ≈ 98 e⁻` (quantization noise ≈ 28 e⁻), so it is negligible next to the ≈140 e⁻ read noise at 1 MHz and gets *more* negligible at higher read clocks.
- **Bits matched to the sensor.** epc901 SNR (datasheet Table 7) is 70 dB at 1 MHz (≈ 11.3 bits), 58 dB at 10 MHz (≈ 9.3 bits) and 49 dB at 54 MHz. So 12 bits is enough and a 16-bit ADC only helps at the slow end. The app flags "ADC bits above sensor SNR" and "ADC noise dominates".
- **Pixel clock vs ADC rate:** `f_read = f_s,ADC`. Maximum spectrum rate `= f_s,ADC / N_clk`, with `N_clk` = 1027 (full), 515 (×2 bin or centre ROI), 259 (×4 bin or ROI+×2) or 131. So 1 MSPS gives about 970 full spectra/s (a bit under your 1 kHz target) and 3.9 kHz with ×4 binning; 10 MSPS gives about 9.7 kHz.
- **Interface limit on the Pico:** most SAR ADCs must clock their data out inside the acquisition window, so `SCLK ≥ bits / (T_s − t_conv)` with `T_s = 1/f_s` and `t_conv` the conversion time (a parameter, from the datasheet timing table, `[verify]`). Example: 16 bits at 1 MSPS with a ≈ 0.7 µs conversion leaves ≈ 0.3 µs, so `SCLK ≈ 50 MHz`, close to what a Pico PIO state machine can do. 10 MSPS serial is out of reach, so 10 MSPS means a parallel bus read by PIO and DMA, with 12 data pins plus the clock on one Pico (`LSB`/`MSB` on separate Picos is not needed at 12 bits). If the data can be read during the *next* conversion (pipelined parts), the relaxed form `bits · f_s` applies, selectable per ADC.
- **Host link.** Data rate `= bits/8 · N_px · f_spectra`. A Pico's full-speed USB is about 1 MB/s in practice [verify], which is about 500 full 1024-px 16-bit spectra/s, so **the host link, not the ADC, may be the binding limit at 1 kHz**. The app shows this limit and the mitigations: on-Pico peak extraction (send only the peak position, bytes per spectrum ≈ a few), ×2/×4 binning, ROI, 8-bit packing, or a faster link. `bytes_per_pixel`, `link_rate_MBps` and `on_device_processing` are parameters.

ADC component presets (seeded in the library; datasheet details `[verify]` before ordering):

| Part | Type | Notes |
|---|---|---|
| **LTC2225** (Analog Devices/Linear) | 12-bit, 10 MSPS, parallel, 3 V supply | 71.3 dB SNR, 1–2 Vp-p fully differential input per the listing. Matches the epc901's differential output and ~1.5 V common mode, and its 3 V logic is Pico-compatible. Needs a fast differential driver stage. Candidate for the 10 kSPS target. |
| **ADS8860** (TI) | 16-bit, 1 MSPS, SPI, 2.7–3.6 V | 93 dB SNR, unipolar single-ended input `0 … V_REF` (2.5–5 V reference). Pairs with the epc901's single-ended mode (1.6 Vpp swing, 500 µV noise, 2 % linearity) rather than the differential one, and caps you at about 1 kHz. Candidate for the simple 1 kSPS build. |
| *Ideal / continuous* | none | ADC switched off. |

### 1.7 Sub-pixel peak localization

- **Fisher information (closed form per pixel)**: for model `μ_i(x₀)`, `I(x₀) = Σ_i (∂μ_i/∂x₀)² / (μ_i + σ_r²)` and `σ_x₀ ≥ 1/√I` (Cramér–Rao). Valid for Poisson plus Gaussian read noise. This is generic, so no closed form per peak shape is needed, and it also gives `σ_λ` and `σ_z`.
- **Monte Carlo (phase 2):** draw noisy synthetic spectra and run the real estimators (centroid, Gaussian fit, parabola) to get the achievable σ and bias. This is the harness the firmware will later use.
- Bias sources: pixel-locking of the centroid, asymmetric peak shape, spectrometer drift (parameter).
- **LED spectral shape.** The 4000 K LED has a steep blue pump peak (≈ 450 nm) and a dip near 480 nm, which biases the peak centroid. The harness divides by a reference spectrum (as the real instrument would) and reports a **usable range**: the z-interval over which `SNR` is above a threshold.

### 1.8 Throughput / imaging speed

`f_s` samples/s, step `= FWHM_lat/2`, so `area_rate = f_s · step²`, `t_frame = N_x N_y / f_s`. Motion blur `= v t_exp` with `v = step · f_s`. Compare with the lateral PSF. LED duty and strobing option.

### 1.9 Other parameters I'm adding (full inventory per element)

Per element: `T(λ)` (AR coating, table or parametric), `R(λ)` for BS, clear aperture, working distance, back focal, surface/wavefront quality (RMS waves), wedge/tilt, `f(λ)` source (constant / glass+shape / table), centration tolerance (a bonus). Specifics:

- **LED**: CCT, current, Tj, flux scale, emitting area, spectrum table.
- **Pinhole 1 and 2**: diameter, thickness (vignetting for high NA), transmission (≈ 0 outside), misalignment (lateral offset → loss), shape (round/slit; slit adds width and height).
- **Collimator / de-collimator**: type (refractive / OAP), focal length, clear aperture, `f(λ)` model, transmission.
- **Beam splitter**: cube / plate, R:T split, wavelength dependence, polarization option, ghost reflections.
- **Objective**: NA, focal length(s), pupil D, working distance, dispersion model (`z(λ)` source), range, transmission, aberration RMS, immersion index.
- **Sample**: reflectivity vs λ, specular/diffuse split, roughness, tilt, step height (for a synthetic scene).
- **Spectrometer**: grating (lines/mm, order, blaze, efficiency, angles), collimator/camera focal length, aperture, optics transmission, aberration blur, thermal drift.
- **Detector and electronics**: all of §1.6, plus exposure, rate, gain, binning, ROI, ADC bits/range/noise, temperature.
- **Environment**: ambient light, temperature.

---

## 2. Software architecture

### 2.1 Core idea: one scalar relation, defined once

```text
struct Relation { id, vars: &[VarId], residual: fn(&Values) -> f64, eq: &str, refs: &[Ref] }
```

- A relation is written once as `f(vars) = 0`. The solver finds **whichever variable is selected as the output** with a bracketed root find (Brent) inside the slider range. The {f, D, NA, M} groups get "choose the dependent one" with no hand-written inverse.
- Spectral relations are forward-only (no inversion).
- `eq` text and `refs` live next to the math, so tooltips can't drift from the code.

### 2.1b The optical train is data, not a fixed schema

```text
struct Train { elements: Vec<ElementInstance> }
struct ElementInstance { kind: ElementKind, name: String, vars: ... }   // variables namespaced "pinhole2.d"
```

- The train is an ordered list of element instances. The default is the user's chain; the user can add, remove or reorder elements (condenser, extra relay lens, slit, QWP/polarizer, second beam splitter, two instances of one kind).
- Panels and component pickers are generated from the list. Variable ids are `<instance>.<param>`, so there are no hard-coded `objective.na`-style ids anywhere.
- `T_path(λ)` and the vergence/ABCD chain are computed by **walking the list** (each `ElementKind` implements one small trait: its transmission, its ABCD matrix at λ, its étendue limit, and its extra variables).
- Defaults express the user's intent through sources, not duplicated numbers: `pinhole2.d = Link(pinhole1.d)` ("identical pinhole").
- The Zemax-lite lens in phase 2 is the same idea one level down: a list of surfaces.

### 2.2 Variable sources

```text
enum Source { Manual(f64), Rule(RelationId), Link(VarId), Component(String) }
```

One enum covers all the user's requirements: a value typed in manually, derived by a relation, linked to *any other variable anywhere in the app*, or loaded from a component file. Every parameter has a collapsible source selector, hidden behind a small dropdown by default.

### 2.3 Evaluation

- Dependency graph over variables. Evaluate every frame in topological order. Cycles are detected and highlighted instead of crashing.
- Values: `Scalar(f64)` and `Spectrum` (samples on one global wavelength grid, default 400–800 nm at 1 nm, 450–650 nm for the main band). Spectral values are first-class from the start, because retrofitting them later is expensive.
- A **sweep** = clone the inputs → vary one variable → evaluate → collect any outputs. Works for any variable, any output, including spectral outputs reduced to scalars (peak width, σ_z, ...). 2-D sweeps (heatmaps) come in phase 2.

### 2.4 GUI

- `param_widget(ui, var)`: label, slider (log scale where appropriate), units, source selector, hover tooltip. Written once, used for every parameter.
- A panel is just a list of variable ids. Panels per element: Source, Pinhole 1, Collimator, Beam splitter, Objective, Sample, De-collimator, Pinhole 2 / Slit, Spectrometer, Detector, Acquisition.
- Tooltips show the section's equations and 1–2 source links (clickable). Each `Relation` / `Doc` supplies its own text.
- Tabs for outputs: Dashboard (key numbers with units and sanity warnings), Plots (PSF, axial response, spectrum at the detector, z(λ), SNR vs exposure, ...), Sweep (pick X, pick Y(s), range, `egui_plot`), Compare (store snapshots A/B).
- A schematic of the train (clickable elements jump to their panel).
- Save/load a scenario as TOML (`serde`).

### 2.5 Component library

- `components/*.toml`, one file per component: `kind = "objective"`, `name`, parameters keyed by the same names as the variables (`na`, `f`, ...), optional curve tables (`z_vs_lambda`, `transmission`) inline or as CSV paths. Defaults embedded in the binary with `include_str!`; extra files read from a directory at runtime.
- Selecting a component sets each of that element's variables to `Source::Component(...)`. Any one of them can still be overridden with the radio.
- The same naming makes a Zemax-lite lens description (phase 2) just another component source.

### 2.6 Module layout (clear separation: math ≠ model ≠ UI)

```text
src/
  main.rs
  engine/        # no optics, no UI
    var.rs         VarId, VarDef (range, unit, scale)
    source.rs      Source enum
    graph.rs       dependency graph, topo order, cycle detection
    solve.rs       Brent root finder
    value.rs       Scalar / Spectrum, wavelength grid
    sweep.rs
  physics/       # pure functions, one file per topic
    geometry.rs    NA/f/D, magnification, pupil fill
    airy.rs        Airy, encircled energy, lateral PSF
    confocal.rs    axial response, finite pinhole
    dispersion.rs  z(λ) models, vergence algebra
    spectrometer.rs
    radiometry.rs
    detector.rs    epc901 model, noise, timing
    localization.rs  Fisher/CRLB (and Monte Carlo)
  model/         # Train/ElementKind definitions, relations, default train; the one place to add a parameter or element
  components/    # loader, TOML + CSV
  data/          # digitized curves: LED spectrum, QE, V(λ), MTF
  ui/            # param_widget, panels, plots, tooltips, schematic
  lens/          # phase 2: lens spec, glass catalog, ray tracer, importers
```

Rules: nothing in `engine/` or `physics/` imports `egui`. Nothing is written twice; repeated patterns become helpers. Comments only where essential, one or two lines.


## 2.7 Seed component library (first batch, small on purpose)

Numbers come from vendor listings and aggregator pages seen during research. Each entry is flagged `[verify]` until checked against the vendor drawing, and the full catalogs come later.

| Element | Newport | Thorlabs | Edmund |
|---|---|---|---|
| Objective / dispersive lens | **SFPX040**: N-SF11 plano-convex, Ø12.7 mm, EFL 12.7 mm at 587.6 nm (±1 %), clear aperture 11.7 mm, uncoated; **SFPX040.AR14** = 425–675 nm coated | — | — |
| Collimator / de-collimator (refractive) | — | **AC127-019-A**: achromat f = 19 mm, Ø12.7 mm, 400–700 nm AR, BFL 15.7 mm | **12.5 mm × 25 mm VIS achromat** (stock #47-662, 425–675 nm) |
| Collimator (reflective) | — | **MPD01M9-P01**: 90° OAP, Ø12.7 mm, RFL 33 mm, protected silver | — |
| Pinhole | — | **P50D** (50 µm class; the discontinued P50C was 50 ± 3 µm, 25 µm thick) | **#56-282**: 50 µm ± 5 µm, 0.03 mm thick, stainless |
| Beam splitter | — | **BS010**: 10 mm cube, N-BK7, 50:50, 400–700 nm | — |
| Grating | — | **GT25-03**: 300 grooves/mm transmission, 17.5° blaze, 25 × 25 mm | 300 grooves/mm, 17.5° blaze, 25 mm sq. (Edmund product-page id 10092; stock number `[verify]`) |

**Spherical aberration of the singlet (Python marginal-ray trace, curved side toward the collimated beam, `R₁ = f(n_d − 1) ≈ 9.97 mm`, 4 mm centre thickness assumed, `[verify]` against the Newport drawing):** longitudinal spherical aberration versus NA at 550 nm is about 17 µm at NA 0.05, 69 µm at 0.10, 156 µm at 0.15, 281 µm at 0.20, 658 µm at 0.30 and 1.7 mm at 0.46. Compared with the diffraction depth of focus `λ/NA²` (55 µm at NA 0.1, 6 µm at NA 0.3), the SFPX040 is only near diffraction-limited at **NA ≲ 0.1**; at NA 0.3 the aberration is as large as the whole 0.67 mm chromatic range. So the usable NA of a single N-SF11 singlet is about 0.1, well below the 0.25–0.3 in your ideation doc. A corrected doublet or an asphere would be needed for that, which is an objective decision for the team.

Worked example from the first row: using the Sellmeier equation for N-SF11 (coefficients reproduce the catalog `n_d = 1.7847`), the SFPX040's focal length is 12.17 mm at 450 nm, 12.60 mm at 550 nm and 12.83 mm at 650 nm, so the **axial chromatic range across 450–650 nm is about 0.67 mm** (a clean match for the 0.3–1 mm target in the ideation doc). Fully filled, `NA = CA/(2f) = 11.7/25.4 ≈ 0.46`. The fully filled aperture is useless because of the spherical aberration above, so the default scenario stops the objective down to NA ≈ 0.1 (beam diameter `2·NA·f ≈ 2.5 mm`).

**Default scenario row** (the app's startup state; part numbers `[verify]`):

| Item | Value |
|---|---|
| Objective | SFPX040 stopped to NA 0.1, `f_o = 12.7 mm` |
| Pinhole 1 and 2 | 25 µm (identical, `pinhole2.d = Link(pinhole1.d)`) |
| Collimator / de-collimator | longer achromat, f ≈ 50 mm (Thorlabs AC254-050-A class, 400–700 nm, Ø25.4; chosen so the pinhole images to ≈ 1 AU: `f_c ≈ f_o·D_p/(k·AU)`) |
| Magnification pinhole → sample | `m = f_o/f_c ≈ 0.25`, pinhole image ≈ 6.4 µm |
| Airy unit at the sample | `1.22 λ/NA ≈ 6.7 µm`, so the pinhole is ≈ 0.95 AU |
| Lateral FWHM | about 2–3 µm (ideal confocal `0.37 λ/NA ≈ 2.0 µm`, broadened slightly by the finite pinhole) |
| Axial FWHM (mirror) | `0.886 λ/NA² ≈ 49 µm`; chromatic range 0.67 mm, so ≈ 14 resolvable layers, with centre-finding precision far smaller than the FWHM |
| Peak width in wavelength | `≈ 49 µm / 3.3 µm/nm ≈ 15 nm ≈ 76 px` |
| Signal | specular ≈ 1e4 e⁻/pixel/ms (SNR ≈ 100); diffuse ≈ 30 e⁻/pixel/ms (read-noise limited) |

Collimator choice note: the 19 mm AC127-019-A listed above would give `m ≈ 0.67`, a pinhole image ≈ 17 µm for a 25 µm pinhole (~2.5 AU), which is too loose. It stays in the library as a "what happens if you pick the wrong one" example, and the longer achromats (AC254 family) are added to the seed list with `[verify]`.

Grating example: 300 grooves/mm in first order, spreading 200 nm over the 7.68 mm array (38.4 µm/nm), needs a camera focal length of about 128 mm × cos β, so around 120 mm. A 600 grooves/mm grating would need about 64 mm.

---

## 3. Phases

### Phase 0 — scaffold
- Bump `eframe` (and add `egui_plot`, `serde`, `toml`) in `Cargo.toml`; check the flake still builds.
- **Fix the nix source filter.** `craneLib.cleanCargoSource` keeps only Cargo-related files, so `data/*.csv` (and possibly other assets) would be missing under `nix build` while `cargo build` in the dev shell still works, breaking `include_str!`. Replace it with a fileset/`lib.cleanSourceWith` filter that also keeps `data/` and `components/`, then verify with `nix build`.
- Use the ideation doc (§5–6) for slider defaults/ranges: depth range 0.3–1 mm, objective f ≈ 20 mm at NA 0.25–0.3, lateral 1–5 µm, 25–50 µm pinholes/fiber cores, depth precision 15–300 nm.
- Digitize data: LED spectrum (4000 K curve), QE, V(λ), MTF, relative flux vs current (datasheet plots, rendered and traced). Store as CSV in `data/`.

### Phase 1 — shortest path to MVP
Goal: a usable calculator for the default train that produces the three headline results (lateral resolution, axial resolution/range, photon budget/SNR).
1. Engine: variables, sources, relations, graph, Brent solver, sweeps.
2. `param_widget`, panels, tooltips, TOML scenario save/load, basic component loader.
3. Geometry + coupled groups ({f, D, NA}, magnification).
4. Lateral: Airy/confocal analytic + numeric pinhole integral, pinhole-in-Airy-units, matched-pinhole rule (`D_p = AU`).
5. Axial: linear and singlet `z(λ)` (Sellmeier for a few built-in glasses), analytic axial FWHM, range, `dz/dλ`.
6. Spectrometer: fixed-span mode, resolution budget, sampling check.
7. Photon budget with the spectral chain (LED × path × sample × grating × QE), epc901 timing/noise/saturation, samples-per-second slider, SNR.
8. Fisher-information σ_λ and σ_z. Plots: PSF, axial response, spectrum on the detector, `z(λ)`, SNR vs exposure.
9. Sweep tab with `egui_plot`.

### Phase 2 — refinement and extras (ordered by value)
1. Numeric finite-pinhole axial response (§1.3 item 4) and background level; compare against the analytic form.
2. Monte Carlo peak-finding harness (centroid / Gaussian fit / parabola), bias and σ vs SNR.
3. Collimator chromatism (vergence algebra), refractive vs OAP, `z_ill`/`z_det` separately.
4. Derived-dispersion spectrometer (grating equation), slit option, entrance-aperture imaging.
5. Aberrations: per-element RMS wavefront (Maréchal), then Zernike tables → FFT PSF (`rustfft`).
6. Sample presets with tilt/slope curves; ghost and stray-light model; condenser option; polarizing BS.
7. 2-D sweeps/heatmaps, snapshot comparison, tolerance (Monte Carlo over component tolerances).
8. **Zemax-lite**: lens spec (radius, thickness, material, conic, aperture, coating) → paraxial ABCD per λ (EFL, `z(λ)`, principal planes) → third-order Seidel → real ray trace (spot, OPD, Zernike fit, FFT PSF). Parse Zemax AGF glass catalogs and `.zmx` prescriptions (text, usually UTF-16; format is undocumented, so treat as best effort, using the Optiland parser as a reference), and the refractiveindex.info database as a free glass source. Thorlabs and Edmund publish ZMX files for their stock lenses, so the component library could be filled straight from them.
9. Import of Zemax text exports (chromatic focal shift, Zernike tables) via CSV-style parsing.

---

## 4. Risks and open points

- **Digitizing datasheet plots** is approximate. Mark digitized curves as such in the UI and let users replace them with measured data.
- **Coherence assumption** for the finite-pinhole integral: valid when the pinhole is incoherently illuminated by the LED (expected here). A single-mode fiber would need a different treatment.
- **Zemax formats** are undocumented (ZMX) or proprietary. The importer is best effort. The paraxial/Seidel tracer in phase 2 reduces the dependence on it.
- **Relation inversion** can be ill-posed (non-monotonic). The solver should report "no root in range" in the UI instead of silently failing.
- Numbers marked `[verify]` need a primary-source check. I'll fetch the originals (Wilson, Sheppard, Tiziani, Molesini, Thompson et al. 2002 for localization precision) when we start.

**Common-path alternative.** The ideation doc leans toward a common-path fiber (one shared aperture plus a coupler instead of a beam splitter). The train-as-list design can express that as a preset: one aperture element and a coupler element. "Identical pinhole" may have meant exactly that, so the preset will be added.

## 5. Questions for you

All earlier questions are answered and folded in above (LED proximity with `η_prox = 0.95`, optional ADC, seed components from Newport, Thorlabs and Edmund). Remaining:
1. Is the Pico an RP2040 or an RP2350 (Pico 2)? That changes PIO speed and USB limits only slightly, but I'd like the right default.
2. Single-ended or differential epc901 output on your board? It decides which ADC preset is the default (see §1.6b).

---

## 6. Implementation status and deviations (end of first autonomous session)

### Done (Phase 0 and Phase 1, plus a few Phase 2 items)

- **Scaffold.** `eframe`/`egui` 0.33.3 with `egui_plot` 0.34.1 (single egui version, checked with `cargo tree -d`), `serde`, `toml`. The flake source filter now keeps `data/` and `components/`; `nix build` succeeded with it (the `include_str!` of the CSVs would have failed otherwise). The dev profile uses `opt-level = 2` so the GUI stays responsive.
- **Engine** (`src/engine`): variables, sources (`Manual`, `Link`, `Rule`, `Component`), lazy evaluation with cycle detection, rule inversion by bisection (linear or log), memoised values, overrides for sweeps, a dependent-variable swap that freezes the previous dependent. Unit tests cover forward/inverse rules, swap, and link cycles.
- **Physics** (`src/physics`): Bessel/Airy, a precomputed defocused-Airy table with numeric finite-pinhole axial responses (mirror and incoherent diffuse plane), lateral confocal PSF, Sellmeier glasses, vergence-based `z(λ)` for the objective plus both collimators, a meridional trace for singlet spherical aberration, radiometry, spectrometer model, epc901 noise/timing/ADC, the full per-pixel spectral signal, Fisher-information precision, and a Monte Carlo centroid estimator. Unit tests pin these conventions: mirror FWHM 0.886 λ/NA² (point pinholes), plane about 1.4× the mirror, encircled-energy 0.838 at the first Airy zero, confocal lateral 0.37 λ/NA, each glass reproducing its catalog `n_d` and `V_d`, the N-SF11 singlet's 0.67 mm range, the singlet SA numbers, centroid recovery, the grating example.
- **Model** (`src/model`): every parameter in one table per element kind, every relation in one rule table with its equation text and references, defaults as sources, 12 warning flags, component application, 18 seed components (Newport, Thorlabs, Edmund, two ADC pairings, four sample presets).
- **GUI** (`src/ui`): inputs panel with a collapsible section per element, one `param` widget used everywhere (slider, combo, source menu with Manual / Rule / Link / Component, equation tooltip, reference links), results tab, five plots (detector spectrum, axial response, focus vs wavelength, lateral PSF, on-focus SNR), sweep tab with multiple outputs, scenario save/load (`scenario.toml`).
- **Verification.** `cargo test` (29 tests) and `cargo clippy --all-targets` are clean, and `nix build` succeeds. A full model evaluation takes about 11 ms. The GUI was launched under Xvfb and screenshotted on all tabs (and with every section open); it renders and responds as intended. A real interactive session (dragging sliders, the source menus) has not been tried.

### Headline numbers from the default scenario (SFPX040 stopped to NA 0.1, 25 µm pinholes, 1 kHz, no ADC)

Pinholes 0.95 AU; Airy unit 6.7 µm; **confocal lateral FWHM 4.8 µm** (both finite pinholes blur the PSF, so a matched 1 AU pair is nowhere near the 2 µm point-pinhole ideal; about 0.87 λ/NA); mirror axial FWHM 65 µm (ideal 49), diffuse plane 104 µm; axial range 0.67 mm; peak width 21.6 nm; power into pinhole 0.11 µW. The read clock defaults to the slowest clock that fits the period (`1.1 × N_clk × rate` ≈ 1.13 MHz at 1 kHz, about 140 e⁻ read noise; a fixed 5 MHz clock would cost about 300 e⁻), giving peak-pixel SNR about 19, Cramér–Rao height precision about 230 nm, and a centroid estimator scatter of about 300 nm. The app flags the host link (needs 2 MB/s, carries 1 MB/s). A pinhole-diameter sweep shows the lateral-resolution versus SNR trade directly: 12 µm pinholes give 2.4 µm lateral FWHM but SNR 3; 50 µm gives 11 µm and SNR 77.

### Deviations from the plan (every place the code simplifies it)

- **§2.1b, train as a list.** `train.rs` holds the list of instances and panels, defaults and ids are generated from it, but the physics reads the canonical instance names (`obj`, `coll`, `pin1`, ...). Adding a second pinhole-like element or a condenser is not supported yet; it needs the per-element transmission/ABCD trait from §2.1b.
- **§1.3 item 4, per-λ pinhole size.** The finite-pinhole shapes use the normalised pinhole radii `v_p1`, `v_p2` at the reference wavelength; only the axial scale (λ/NA(λ)²) follows wavelength. The 1.44× change in Airy units across the band is therefore not in the shape.
- **§1.3 item 3, collimator mismatch.** For the diffuse plane the code evaluates both paths at the common defocus `z − z_eff(λ)`. A mismatch between `z_ill` and `z_det` therefore only shifts the peak (it should also broaden it for a plane). The mirror case is exact in this respect.
- **Unequal NA.** The axial normalisation uses `min(NA_ill, NA_det)`; it assumes the paraxial normalisation (flagged above NA 0.5).
- **Diffuse plane normalisation.** The plane response is derived from my own reasoning (incoherent plane, both paths defocused by z) and checked only for the width ratio (about 1.4× the mirror, matching the Wilson coefficient pair quoted from memory). Verify before relying on absolute diffuse numbers.
- **Detector.** Binning only changes the clock count and link bytes, not the pixel grid or noise. PRNU, the 20 % pixel crosstalk, the MTF, and image lag are not modelled. Noise and swing are interpolated in the read clock from three datasheet points.
- **Spectrometer.** The pixel grid is linear in wavelength (`dlam` constant); no grating-angle nonlinearity. Grating efficiency is a Gaussian envelope, not a measured curve.
- **Digitised data** (LED spectrum, QE, V(λ)) are traced by eye from the datasheet plots and should be replaced by finer traces or measured data.
- **Objective spherical aberration** assumes a plano-convex singlet, curved side toward the collimated beam, with a guessed 4 mm centre thickness. It only feeds the Maréchal Strehl multiplier (`W040 = LSA·NA²/4`, RMS at best focus `0.0745·W040`, corrected after review); the PSF width is not widened.
- **Response tables** cover normalised radius up to 32; beyond a pinhole radius of 24 the app flags the numbers as unreliable instead of extrapolating.
- **Scenario files** store rules by their output id (not an index), so adding rules later does not break old saves; component files in `components/` override embedded ones of the same name, and files that fail to parse are reported as warnings.
- **Slit option for pinhole 2** (and the common-path-fiber preset) are not built. The detection aperture is always a round pinhole.
- **Component library.** Part numbers marked `[verify]` come from vendor listings or my memory. `Component:` selection works per variable and per element; there is no catalog browser.
- **Subagents.** `.claude/agents/{search,grep,docs}.md` exist (Haiku, read-only tools, frontmatter fixed to the documented plain-tool-name form), but the agent list of this session did not include them, so none were spawned. A Claude Code restart may be needed for them to appear.

### Next steps (in priority order)

1. Read the equations behind each `[verify]` tag against primary sources (Wilson 1990 chapter, Sheppard, Tiziani, Thompson et al. 2002) and fix any coefficient that differs; check the seed part numbers against vendor pages.
2. Add the slit option and the optional common-path preset (needs the generic train element trait).
3. Per-λ pinhole shapes; `z_ill ≠ z_det` broadening for the plane.
4. Heatmap (2-D) sweeps and snapshot comparison.
5. Zemax-lite (phase 2.8): lens description to paraxial `f(λ)`, Seidel/real ray trace, ZMX and AGF import.
6. Replace digitised curves with finer traces; add the measured-spectrum reference normalisation to the Monte Carlo harness.
