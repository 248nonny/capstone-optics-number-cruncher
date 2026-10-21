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
- LED radiance `L ≈ 1.1e5 W/(m²·sr)` (radiant flux ≈ 0.41 W from 122 lm with the spectrum-weighted `⟨V⟩ = 0.43`, a luminous efficacy of radiation of 295 lm/W, over 1.21 mm²).
- Spectral power into a matched pinhole `≈ 0.4 nW/nm` (including `η_prox = 0.95`; 0.10 µW over the whole band).
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

Pinholes 0.95 AU; NA 0.101 (`D / 2 f(λ)` at 550 nm); Airy unit 6.7 µm; **confocal lateral FWHM 4.8 µm** (both finite pinholes blur the PSF, so a matched 1 AU pair is nowhere near the 2 µm point-pinhole ideal; about 0.87 λ/NA); mirror axial FWHM 65 µm (ideal 49), diffuse plane 104 µm; axial range 0.67 mm; peak width 21.6 nm; power into pinhole 0.11 µW. The read clock defaults to the slowest clock that fits the period (`1.1 × N_clk × rate` ≈ 1.13 MHz at 1 kHz, about 140 e⁻ read noise; a fixed 5 MHz clock would cost about 300 e⁻), giving peak-pixel SNR about 19, Cramér–Rao height precision about 230 nm, and a centroid estimator scatter of about 300 nm. The app flags the host link (needs 2 MB/s, carries 1 MB/s). A pinhole-diameter sweep shows the lateral-resolution versus SNR trade directly: 12 µm pinholes give 2.4 µm lateral FWHM but SNR 3; 50 µm gives 11 µm and SNR 77.

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

---

## 7. Rework proposal (awaiting approval; no code written for this section)

### 7.1 Findings from profiling the current build

| Measurement | Result | Consequence |
|---|---|---|
| Axial-table rebuild when a pinhole radius changes | about 66 ms | Dragging any coupled slider (stop, focal lengths) changes the normalised pinhole radius, so every frame rebuilds the table: this is the lag |
| Everything else per repaint | about 9 ms | A fresh `Ctx` recomputes all outputs on every repaint, even with no input change |
| Output vs `obj.d`, 0.4 % steps | 65.475, 64.954, 64.910, 64.398, 63.892, 63.392, 63.357, ... | The cache key rounds the normalised radius to 0.05, so outputs move in steps. This also rules out finite-difference gradients until it is fixed |

### 7.2 Performance first (the optimiser depends on it)

1. Cache the disk-integral tables `D(v_p; rho, u)` on a `v_p` grid (step about 0.25), built lazily and shared across threads; interpolate linearly in `v_p`; recompute the mirror, plane and lateral integrals from the interpolated tables. No rebuild on slider drag and no stair-steps. The grid can be warmed on a background thread at startup.
2. Make physics run on a plain `Send` snapshot of resolved values (implement `Get` for that struct) so heavy outputs can run on a worker thread. The UI thread resolves only the cheap couplings and draws the previous outputs (greyed) until the worker finishes; the worker always computes the latest snapshot and skips stale ones. This removes `Rc` memos, `thread_local` caches and boxed closures from the physics path.
3. Recompute by version number, not on every repaint.
4. Optimiser evaluations run in parallel with `std::thread::scope` on the same snapshot type.
5. Turn the profiling test into a timing-budget assertion.

### 7.3 Relations: three tiers instead of removing them all

- **Definitional, always on, bidirectional:** NA = D/(2f), AU = d NA m / (1.22 λ), m = f_obj/f. Without these the sliders can reach states that cannot exist physically.
- **Optional design constraints (checkboxes in the new pane):** pinhole 2 = pinhole 1, pinhole = k × Airy unit, f_read = 1.1 N_clk rate, dispersion from the grating, and custom "A = B" links (this keeps the original cross-app coupling).
- **Outputs:** read-only, computed in the background.
- The pane lists every formula in one view (definitional ones shown as always on), each with its equation, an expandable description and references. The per-parameter gear menus go away; each parameter row keeps only a slider, a component picker (when the part kind has components) and a small marker showing why it is locked.

**Every slider draggable.** Two-variable relations are easy (drag d, AU follows; drag AU, d follows). For three-variable relations each formula declares a priority order of variables that give way: the one that moves is the first that is not being dragged, not set by a component, and not already solved by another active relation. If none qualifies the row is flagged as over-constrained. This reverses my earlier "locked dependent" design.

### 7.4 Optimisation pane

- **Objective** (to be chosen): minimise or maximise an output, with other outputs as constraints (for example minimise `σ_z` subject to lateral FWHM ≤ X and SNR ≥ Y), or a weighted sum. Warnings can be marked as hard constraints so "flag, don't cap" still holds in normal use.
- **Discrete mode:** all combinations of chosen component slots, with restriction checkboxes ("same pinhole on both sides", "same lens on both sides"). The combination count is shown before running; samples are scenario inputs and excluded by default. Each combination is evaluated at the current continuous values, ranked, and a click applies the row.
- **Continuous mode:** gradient descent on normalised coordinates (log scale for log sliders, box bounds from slider ranges), central differences, backtracking line search, NaN treated as infeasible, Monte Carlo outputs excluded (discontinuous). Nelder–Mead offered as a derivative-free alternative. No autodiff: forward-mode dual numbers would make the physics generic over the number type, and the cost still grows with the number of parameters.
- **Stretch:** snap the continuous optimum to the nearest catalog part per slot and evaluate the top three per slot.
- **Known risk:** aberration currently only scales the signal (Strehl) and does not widen the PSF, so an unconstrained optimiser will push NA up. Hard-constraint flags (Strehl, table validity, link limits) address this until the aberration model improves.

### 7.5 Formula rendering

One source `formulas.toml` (id, Typst math, description, references). Start with Unicode text; later `build.rs` renders SVGs with Typst (white text, tinted in egui for the dark theme) loaded through egui's SVG support, with `typst` added to the flake's `nativeBuildInputs` and falling back to Unicode when it is missing. A test checks that every formula id has an implementation and the reverse.

### 7.6 Phases

1. Performance and engine rework (tiers, drag semantics, worker thread).
2. Constraints pane.
3. Discrete optimisation, then continuous, then snapping to catalog parts.
4. Typst rendering.

Each phase ends with tests, clippy and a review.

### 7.7 Decisions (from the user)

- **Objective:** fix the sensor speed (acquisition rate), then minimise a weighted sum of lateral precision and height precision, with a hard limit on each: the cost becomes very large when either precision exceeds its limit. Warnings can additionally be marked as hard constraints.
- **Drag policy:** per-formula priority order of the variable that gives way.
- **Continuous optimiser:** three selectable modes: Nelder-Mead only, gradient descent only, and gradient descent with Nelder-Mead as the fallback.
- **Formulas:** Unicode first, Typst rendering last.

### 7.8 Status of the rework (implemented)

**Performance.** The disk-integral tables are cached on a pinhole-radius grid (step 0.25, built lazily and in parallel at start-up) and interpolated, so changing a pinhole radius no longer rebuilds anything and outputs are smooth. Measured: a slider drag step costs 0.18 ms on the UI thread (settle plus the lock flags); a full evaluation is about 5 ms (about 9 ms with the Monte Carlo outputs) and runs on a worker thread, so the UI shows the previous numbers (greyed only after 150 ms) until the new ones arrive. Sweeps and optimisation also run off the UI thread. Timing budgets are asserted in tests.

**Engine.** There are no per-parameter rules or gear menus any more. State is a plain value vector plus component locks, active constraints and custom links. Formulas are three kinds: always-on definitions (NA = D/2f, magnification, pinhole size in Airy units, spectrometer dispersion from the grating and camera focal length; each draggable in every direction through a per-formula give-way order), optional design constraints (identical pinholes, read clock from rate, exposure from rate, wavefront error from singlet aberration) and read-only outputs computed from a snapshot of the values. Only a component locks a slider; dragging a slider that a constraint sets turns that constraint off and says so in the status line. A part that a component fixes makes any slider that would have to move it unmovable, with the reason in its tooltip.

**Constraints pane.** All formulas in one view with checkbox, Unicode equation, description, give-way order, references, conflict/override status, and custom A = B links.

**Optimise pane.** Objective: fixed acquisition rate; cost = Σ weight · value / limit for lateral FWHM and height precision, plus a steep quadratic penalty past each limit, plus optional hard constraints (Strehl ≥ 0.8, no saturation, pinholes inside the tables). Continuous mode: choose design variables and bounds (unit cube, log-scaled where the slider is), Nelder-Mead, gradient descent (parallel central differences, projected backtracking line search) or gradient descent with Nelder-Mead fallback; runs on a worker with progress, Stop and Apply; tied variables are rejected before the run with the constraint named. Discrete mode: slots for each part kind (same part on both sides optional), combination count shown, parallel ranking, one-click apply, and a snap of the continuous optimum to the three nearest catalog parts per slot. On the default design (after the review fixes of §8), gradient descent with fallback improves the cost from 1.82 to 1.69 in 1.8 s (215 evaluations; height noise 259 → 214 nm, lateral FWHM 4.77 → 4.91 µm, Strehl 0.98 → 0.92) by stopping the objective down slightly and lengthening the collimators; the discrete search over 180 combinations finds a better design (cost 1.61: the AC254-050-A collimators and the AR-coated SFPX040.AR14), because the coating's extra transmission is worth more than any geometry change. Aberration only scales the signal (it does not widen the PSF), so the hard Strehl limit is kept to stop an optimiser from pushing NA up.

**Not done.** Typst formula rendering (Unicode shown), snapshot comparison of optimisation runs, 2-D sweeps. "Lateral precision" is mapped to the confocal lateral FWHM.

---

## 8. Verification and review pass

### 8.1 `[verify]` items resolved

| Item | Result | Source |
|---|---|---|
| Confocal lateral FWHM, point pinholes | 0.37 λ/NA confirmed; the model reproduces 0.369 | arXiv 1404.2408 (thesis quoting Wilson) |
| Confocal axial FWHM, point object (both passes on axis, sinc⁴(u/4)) | 0.64 λ/(n − √(n² − NA²)), paraxially 1.28 λ/NA²; the model's point-object response gives 1.27 (2 % check). The model's incoherent diffuse plane (a different quantity, ∫|U|⁴) is slightly wider at 1.36 λ/NA²; there is no literature figure for it, so it is checked only against the point object (wider, within 15 %) | same |
| Plane-mirror axial response | `sinc²(u/2)`, FWHM 0.443 λ/(1 − cos α) = 0.886 λ/NA² paraxially; the normalised coordinate `u = 4 k z sin²(α/2)` matches Kowalczyk et al.; reproduced exactly | Opt. Appl. 28 (2) 127; Utah `confocal_intro.pdf` |
| Airy / encircled energy | 0.514 λ/NA wide-field FWHM, 0.838 inside the first zero; both reproduced | Born & Wolf |
| Newport SFPX040 | R = 9.97 mm, centre thickness 4 mm, edge 1.72 mm, BFL 10.46 mm, clear aperture 11.7 mm: the ray trace reproduces the 10.46 mm back focal length, so the earlier guessed 4 mm centre thickness was right | Newport/MKS listing |
| ADS8860 | tCONV 500–710 ns, tACQ 290 ns, SCLK up to 66.6 MHz; 16 bits at 66.6 MHz fit in the 290 ns window, so 1 MS/s SPI is feasible. The component now uses 710 ns and 66.6 MHz (it no longer raises the SPI warning at 1 MS/s) | TI SBAS569B |
| LTC2225 | 12-bit, 10 MS/s, 71.3 dB SNR, 1–2 Vpp differential or single-ended, 3 V, parallel CMOS | Analog Devices listing |
| AC254-050-A | Ø25.4 mm, N-BAF10/N-SF10 doublet, 400–700 nm AR, EFL 50.2 mm (±1 %) | distributor listings |
| epc901 numbers | read noise 700 µV (diff) / 500 µV (SE) at 1 MHz, SNR table → 1.9 mV at 10 MHz and 1.8 mV at 54 MHz, full well 400 ke⁻ ↔ 2 V, 515 clocks at 54 MHz = 9.54 µs; all reproduced | datasheet |
| LED | Φ_e / (I · V_F) = 0.42 against the datasheet's 40 % electrical efficiency; luminous efficacy of radiation 295 lm/W | datasheet |
| Digitised curves | the LED (4000 K) and QE curves were re-traced by pixel analysis of the datasheet figures (the earlier eyeballed LED curve overestimated the red tail by up to 40 %); the Verify tab overlays the traced points on the curves | datasheet figures |
| Not confirmable online | Thorlabs P25D/P50D thickness and tolerance, BS010 and GT25-03 efficiency, Edmund 47-662 and the grating stock number; still marked `[verify]` in the component notes | |

### 8.2 Errors found and fixed in the review

- **Strehl applied once, should be twice:** illumination and detection each pass the aberrated PSF, so the confocal peak scales as S² (the Strehl output and flag still report the single-pass value).
- **Pinhole-2 projection:** the image of a circular pinhole projected on the dispersion axis has variance (w/2)²/4 = w²/16, not w²/12 (that is for a slit).
- **Grating illuminated width:** was a free parameter; it is now `min(grating width, 2 f_col NA_pin2)` from the beam that pinhole 2 actually produces.
- **Ghost light:** was multiplied by the objective, beam-splitter and de-collimator transmissions twice; it is now a fraction of the power entering pinhole 1 that reaches the spectrometer unfocused.
- **Singlet radius:** `R = f (n_d − 1)` now uses the index at the design wavelength, not at the working wavelength.
- **Read noise at 1 MHz** now uses the datasheet's 700 µV rather than the value derived from the SNR table.
- **Valid spectral range:** pixels outside 400–800 nm (the range of the LED and QE data) are dark instead of being evaluated, which removed the NaNs at large dispersion, and a warning says when the band leaves that range.
- **Dispersion is now a definition** (grating equation plus camera focal length) instead of a warning.

### 8.3 Parameter audit

Removed: the LED flux-current exponent (now a constant 0.95), the free "illuminated grating width". Added: per-part "Real / Ideal" switches for collimators, beam splitter, objective and spectrometer (ideal = no loss, stray light, chromatic drift, wavefront error or grating-efficiency shape; their imperfection rows are hidden), and the ADC rows are hidden while it is off. Kept although redundant: pinhole size and Airy units, NA / stop / focal length, dispersion / camera focal length, centroid and Cramér–Rao precision.

### 8.4 Verify tab

Self-checks (18 recomputed known results with tolerance and source link), the traced LED spectrum with V(λ) and numbers, the QE curve and the epc901 noise and full-well tables, glass index and focal length, the pixel-to-wavelength map with grating efficiency and the four resolution terms, and a stage-by-stage photon budget that can be multiplied out by hand.

### 8.5 Independent review (a second reader went through physics, model, solver and data)

It found no sign, factor-of-2 or unit error in the photon-budget chain, the vergence algebra, the confocal tables, the Fisher calculation, the Sellmeier data, the ray trace, the grating equation or the detector tables. Defects found and fixed:

- **Pinhole size in Airy units varies across the band** (`v_p ∝ 1/λ`, about 4.4 at 450 nm and 3.1 at 650 nm): the pinhole-2 pass fraction and the axial response shape are now interpolated between five wavelength nodes instead of frozen at the reference wavelength.
- **Binning** now averages `b` physical pixels (512 or 256 output pixels, shot noise divided by `b`, read noise unchanged), instead of only changing the clock count.
- **PRNU** added as a residual gain non-uniformity parameter (default 0.5 % after flat-fielding; datasheet 2 % typical) in the noise variance.
- **Link requirement** is now stored in bytes per second; the Host-link panel used to show it a million times too small.
- **Strehl** is applied as S⁴ for the specular return (the same pupil points are traversed twice, so the wavefront error doubles) and S² for the diffuse return.
- **Objective focal length consistency:** NA, magnification and pinhole size in Airy units now all use the focal length at the working wavelength (`NA = D / 2 f(λ)`), not a mixture of the design-wavelength and working-wavelength values.
- **Smaller:** flux-current exponent 0.82 (fits the datasheet curve at 100 mA and 1 A; 0.95 was 15–20 % off); exposure overhead is the flush time only (the sensor is still sensitive during the shift); the pixel resolution term includes the datasheet's 20 % charge crosstalk; diffraction limit uses the illuminated length `W / cos α`; blur no longer attenuates the band edges; the out-of-focus tail of the axial response uses a smoothed envelope instead of one oscillating grid value; "Electrons in the peak" is renamed (it sums the whole spectrum); new warnings for an ADC range that clips or wastes bits, a conversion time longer than the sample period, and single-ended noise used above 1 MHz.

Known simplifications that remain (documented, not fixed): the diffuse-plane response uses one common defocus for illumination and detection (exact only for a mirror; negligible for the default achromats); the Cramér–Rao bound assumes a known peak shape, amplitude and background (the Monte Carlo centroid is the realistic number); the epc901 frame-store dark current, readout CDS time and the ghost image above 20 MHz are not modelled; the dependent value of a constraint can leave its slider range without being clamped.

---

## 9. Simplification redesign (proposal, awaiting answers; no code written for this section)

### 9.1 Where this sits

The open items before this were: the slit option and common-path preset, 2-D sweeps and snapshot comparison, Zemax-lite (lens description → paraxial `f(λ)`, Seidel and real ray trace, ZMX and AGF import) and finally Typst formula rendering. The redesign comes first. Afterwards Zemax-lite, aberration tables and the slit become features of the real-components panel instead of left-side knobs.

### 9.2 Principles

1. An input is a choice or a requirement. Anything that can be derived is an output.
2. One knob per independent degree of freedom, in its most intuitive form: Airy units rather than micrometres, NA rather than stop diameter, spectral resolution in nm rather than focal lengths.
3. Hardware facts (LED, epc901, beam splitter split, image size, wavelength band) are constants, shown on the Verify tab, not knobs.
4. Second-order effects sit behind default-off toggles.
5. Related variables are explained in tooltips with links, not exposed as sliders.
6. A Sensitivity view ranks every knob by its effect on the key outputs (±10 %, central differences, same machinery as the optimiser), so "which knobs matter" is answered by the tool.

### 9.3 Three facts the current model shows (measured with throwaway probes on the existing engine)

- **Magnification does not matter.** At fixed pinhole size in Airy units, changing the collimator focal length from 20 to 200 mm (pinhole 10 to 99 µm, `m` 0.63 to 0.06) leaves lateral FWHM and axial FWHM identical and changes collected electrons by 1.2 % and σ_z by 0.8 % (only the collimator-chromatism term). Reason: the power into the pinhole is `L · (π²/4)(1.22 λ AU)²`, independent of NA and of the pinhole diameter. So magnification only sets the physical pinhole size and belongs in the parts panel.
- **A "shitty" spectrometer is fine, conditionally.** At NA 0.1 the peak is 21 nm wide and a 4 nm resolution costs 2.7 % in σ_z (8 nm: 10 %). At NA 0.3 the peak is 5.3 nm wide and a 4 nm resolution costs 28 % (8 nm: 88 %). Rule of thumb: resolution below about one fifth of the peak width.
- **Coarser dispersion is better until the peak spans a handful of pixels** (read-noise-limited regime): σ_z falls from 259 nm at 0.2 nm/px to 109 nm at 3 nm/px (SNR 17 → 122), then resolution blur takes over. An optimiser will drive dispersion to about 5–10 pixels across the peak. This is real physics; the model does not credit the saved read-out time (ROI), so a mostly dark sensor still costs the full line read.

### 9.4 Proposed knobs (about 15 inputs)

| Group | Knob | Notes |
|---|---|---|
| Source | LED drive current | excluded from optimisation (no thermal model) |
| Optical system (collimators + objective as one element) | NA (sample side) | |
| | Pinhole size, Airy units | identical source and detection pinholes |
| | Chromatic depth range Δz over the band | a requirement, excluded from optimisation (σ_z scales with dz/dλ) |
| | Dispersion shape | linear, singlet glass (the real normalised curves), diffractive |
| | Throughput, one pass | lumps collimator and objective transmission |
| | Aberrations tick + RMS wavefront at NA 0.1 | scales as NA⁴ (spherical aberration) so the optimiser cannot raise NA for free |
| | Stray light | the largest unknown of a real confocal |
| Sample | Reflectance, specular fraction | tilt and height become advanced / plot-only |
| Spectrometer | Dispersion (nm per pixel) | replaces the grating, focal lengths, slit image, blaze |
| | Spectral resolution (nm FWHM, not below one pixel) | |
| | Efficiency (grating and optics) | |
| Acquisition | Spectra per second | |
| | Sensor temperature | cooled 15 °C or uncooled 35 °C |

Outputs: lateral FWHM, axial FWHM, depth range and number of resolvable layers, σ_z, SNR, power budget, exposure, read clock and binning (all chosen automatically), saturation, usable band fraction, time per 2000 × 2000 image, field of view, area rate.

Constants: LED and epc901 data, 50:50 beam splitter, 450–650 nm band, 2000 × 2000 image.

Removed: host link, ADC, collimator and objective focal lengths and apertures, stop diameter, pinhole 2, grating parameters, spectrometer focal lengths, beam width, binning, gain, read clock, exposure, the constraints tab and every Real/Ideal switch (throughput = 1 and aberrations off give the same thing).

Automatic acquisition: exposure = min(period − overhead, time for the brightest pixel to reach about 80 % of full well); that depends on the sample, and the saturating case is a mirror. Binning ×1/×2/×4 and the slowest read clock that fits the period are evaluated and the one with the lowest σ_z is kept.

### 9.5 Architecture: two layers

- **System layer:** a flat vector of about 15 independent inputs mapped to outputs. No constraints, settle solver, locks or formulas, so the constraints tab, the `State` machinery and the persistence of sources disappear. Sweeps, the optimiser and the sensitivity view work on a plain vector.
- **Parts layer (right panel):** catalog parts plus a few physical free values (stop diameter, pinhole choice) mapped to effective system inputs with the existing verified detailed physics. It offers one-shot write-back to the left side, the combinatorial search and snap-to-parts. Zemax-lite, the slit option and aberration tables later extend this layer only.
- **Faithfulness test:** parts → effective inputs → system outputs must match the detailed model within a few percent (collimator chromatism being the one expected difference).
- **Migration risks:** the self-checks construct detector values directly, the optimiser's `Problem::state_at` and the discrete search use `State`, persistence, and about half of the tests.

### 9.6 Questions for the user

1. Is spectra per second an input requirement, or should it be an output ("the fastest rate that meets a σ_z target")?
2. Is the depth range a fixed requirement, or a design variable with a minimum?
3. Strictly identical detection and source pinholes, or one advanced "detection / source size ratio" knob (a real lever for photon-starved diffuse samples)?
4. Binning and read clock: automatic best, or fixed ×1?

### 9.7 Decisions from the user

1. Spectra per second is an input requirement.
2. The chromatic depth range is a fixed requirement (never optimised).
3. The detection pinhole is identical to the source pinhole by default, with one advanced "detection / source size ratio" knob (default 1).
4. Binning and the read clock are chosen automatically (the best of ×1/×2/×4 with the slowest clock that fits the period).
5. The model must not be monochromatic. Resolution and noise are evaluated across the whole band (fixed at 450–670 nm) and reported as mean and worst case.

### 9.8 Polychromatic evaluation (proposal)

The surface can sit anywhere in the depth range, and the wavelength that is in focus then changes (450 nm at one end of the range, 670 nm at the other). Everything that is a function of wavelength in the system (NA, pinhole size in Airy units, Strehl, LED spectrum, QE, grating efficiency, the nonlinear `z(λ)`) therefore makes every height measure differently. Approach:

- A grid of about nine wavelength nodes across 450–670 nm; node `k` stands for a surface at height `z_k = z_eff(λ_k)`.
- Per node: lateral FWHM at `λ_k` (with `NA(λ_k)` and the pinhole's Airy units at `λ_k`), axial FWHM at `λ_k`, and the Fisher information of the spectrum with the peak at `λ_k`, restricted to a window of pixels around the peak (a few peak widths) so the cost is a few hundred pixels per node, not the whole sensor. That gives `σ_z(λ_k)` and the peak SNR at that height.
- The axial and pinhole tables already have a wavelength bank; per-node FWHM is interpolated from it.
- Reported: mean over height (weights proportional to `|dz/dλ|`, so the mean is over the depth range, not over wavelength) and worst case (largest FWHM and σ_z, smallest SNR) for lateral FWHM, axial FWHM, σ_z and SNR, plus the curves versus wavelength and versus height on the Plots tab. The detector-spectrum plot keeps a sample-height control for illustration only.
- The optimiser's cost uses the worst case by default (switchable to the mean).
- `NA(λ)` follows the focal-length change implied by the dispersion shape and depth range (`f₀` is recovered from the normalised shape and the range, e.g. `f₀ = V_d · Δz` for a singlet); a linear or diffractive shape with no focal length keeps NA constant.
- Cost budget: about 9 nodes × 3 windowed spectra plus 9 lateral profiles, well under 10 ms.

### 9.9 Corrections to the polychromatic plan (from review)

- **The in-focus band is 450–670 nm; the spectrometer covers more.** A surface at either end of the depth range focuses 450 or 670 nm light; if the sensor stopped at exactly those wavelengths half of that peak would be cut off and the end nodes would always be the "worst case". The in-focus band sets the depth range, and the spectrometer covers that band plus about one peak width on each side, computed automatically from the current NA. The dispersion knob sets how many pixels the padded band occupies; a warning appears if it does not fit in 1024 px.
- **One exposure for the whole scan.** Automatic exposure is chosen so that the brightest node (any height) sits at about 80 % of full well. Automatic binning and read-out are chosen on the worst-case σ_z, the metric the optimiser uses.
- **The optimiser works on smooth quantities.** A true maximum over nodes, a best-of-three binning switch and `min(period, fill time)` are all kinks. The optimiser minimises a soft maximum (a p-norm with p ≈ 8) while the interface reports the true maximum; the gradient-descent → Nelder–Mead fallback is what covers the binning switch.
- **Windowing test.** A test requires the windowed Fisher σ_z at the node nearest 550 nm to match the current full-spectrum value within about 2 %; the window width is chosen from that test.
- **Monte Carlo** moves to the Verify tab as a realism check at one height, instead of running at every node.
- **Read-out region.** The epc901 can read only its centre 512 pixels (515 clocks); when the padded band fits in 512 pixels the automatic read-out choice includes that option, which is a real speed gain at coarse dispersion.
- **"Turn off an element" is superseded:** throughput = 1 with aberrations off is the ideal element, so there is no per-element switch.

### 9.10 Migration order (keeps the tree building)

1. A flat system-layer evaluator with the polychromatic nodes, reusing the existing physics.
2. Faithfulness tests: the new layer's 550 nm outputs equal today's outputs, and parts layer → system layer matches the detailed model within a few percent.
3. The new left panel.
4. The parts panel with write-back, the combinatorial search and snap-to-parts.
5. Sweep, optimiser and a Sensitivity view working on the flat vector.
6. Only then delete `State`, the formulas and the constraints tab, and port persistence and the Verify checks.

---

## 10. Progress log (overnight redesign)

- Step 1 done: `src/system/` holds the flat system layer (17 inputs, polychromatic nodes across 450-670 nm, automatic exposure/binning/read-out). Windowed Fisher information matches the full spectrum within 2 %, a full evaluation takes about 4 ms once the tables are warm. The old engine and UI are untouched and still work.
