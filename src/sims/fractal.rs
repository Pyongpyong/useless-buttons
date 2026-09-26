//! `fractal` — a Mandelbrot explorer that never stops zooming in.
//!
//! Rendered at half resolution (each computed pixel becomes a 2x2 block)
//! because full-resolution escape-time iteration blows the 60fps budget
//! the moment the view is zoomed in. Coordinates are `f64`: at any
//! meaningful zoom depth `f32` loses enough precision that the image
//! visibly degrades into blocky noise.
//!
//! Zoom only ever goes one direction: in, at a slow, steady rate, so the
//! button is never visually static.
//!
//! A fixed center zoomed in on forever will, sooner or later, drift into
//! either the solid interior of the set or open space far outside it —
//! both flat, single-colored, and boring to look at, since neither has
//! any boundary detail nearby. Every step, a handful of cheap probe
//! samples around the current center check whether there's still
//! escape-time *variation* nearby (i.e. an actual boundary to zoom into)
//! and nudge gently towards whichever direction has the most of it. If
//! the view stays flat for too long despite that (it wandered somewhere
//! with genuinely no detail at the current scale, or crossed the
//! zoom-depth recycle threshold), it relocates to a new hand-picked
//! coordinate and keeps going — an endless zoom-deeper-then-relocate
//! cycle that should never dead-end on a blank frame.

use super::{Input, Sim};
use crate::paint::{Frame, Rgb, Theme};
use crate::rng::Rng;

const MAX_DT: f64 = 0.1;
const BASE_SPAN: f64 = 2.5;
const MIN_ZOOM: f64 = 1.0;
const MAX_ZOOM: f64 = 1.0e13;
/// Zoom multiplier per second — always forward, never reset, so the
/// fractal keeps visibly deepening.
const ZOOM_RATE: f64 = 1.18;
/// Once zoom crosses this — still many orders of magnitude short of
/// where `f64` precision would start visibly degrading the image —
/// relocate to a new preset and keep zooming from there. Keeps the
/// "always increasing" zoom going forever instead of grinding to a halt
/// against `MAX_ZOOM`.
const RECYCLE_ZOOM: f64 = 1.0e9;
const HOME_CENTER: (f64, f64) = (-0.5, 0.0);
const HOME_ZOOM: f64 = 1.0;
const COLOR_PERIOD: f64 = 22.0;

/// Radius (in device px at the *current* zoom) of the boundary-detail
/// probes cast around the center every step.
const PROBE_RADIUS_PX: f64 = 14.0;
/// A probe difference (in escape-time iterations) below this counts as
/// "no detail nearby" for that direction.
const BORING_DIFF_THRESHOLD: f64 = 2.0;
/// Per-second exponential-approach rate for nudging towards whichever
/// probe direction found the most boundary detail.
const BOUNDARY_NUDGE_RATE: f64 = 14.0;
/// If every probe direction has looked flat/boring for this many
/// *accumulated seconds* (not frames — so it's independent of render
/// rate), stop waiting and relocate immediately rather than continuing
/// to zoom into nothing.
const BORING_SECONDS_BEFORE_RECYCLE: f64 = 0.6;

/// A handful of hand-picked coordinates, all already close to actual
/// boundary detail, to relocate to automatically once the
/// current spot has gone flat or been zoomed in on enough. They don't
/// need to be pixel-perfect: the boundary-seeking nudge above corrects
/// onto real detail within the first few steps after landing. None of
/// these start at `zoom == 1` on purpose — an automatic relocation
/// should never look like a jarring reset back out to the boring
/// full-set overview.
const PRESETS: [(f64, f64, f64); 5] = [
    (-0.743_643_887_037_151, 0.131_825_904_205_33, 5.0e4),
    (-0.745_3, 0.112_7, 2.0e5),
    (-0.160_701_35, 1.037_566_5, 8.0e4),
    (-1.401_155, 0.0, 3.0e3),
    (-0.748, 0.1, 2.5e3),
];

pub struct Fractal {
    w: usize,
    h: usize,
    lw: usize,
    lh: usize,
    center: (f64, f64),
    zoom: f64,
    preset_idx: usize,
    /// Accumulated seconds the boundary probes have found nothing
    /// nearby, back to back. Reset the moment any probe finds detail.
    boring_time: f64,
}

fn low_res_dims(w: usize, h: usize) -> (usize, usize) {
    (w.div_ceil(2).max(1), h.div_ceil(2).max(1))
}

/// Same adaptive iteration budget used for both the actual render and
/// the cheap boundary probes, so probing stays accurate at any depth.
fn adaptive_iter(zoom: f64) -> u32 {
    (80.0 + 28.0 * zoom.max(1.0).ln()).clamp(16.0, 600.0) as u32
}

impl Fractal {
    pub fn new(w: usize, h: usize, rng: &mut Rng) -> Self {
        let (lw, lh) = low_res_dims(w, h);
        Fractal {
            w: w.max(1),
            h: h.max(1),
            lw,
            lh,
            center: HOME_CENTER,
            zoom: HOME_ZOOM,
            preset_idx: rng.range_usize(0, PRESETS.len()),
            boring_time: 0.0,
        }
    }

    fn scale(&self) -> f64 {
        BASE_SPAN / self.zoom / (self.h.max(1) as f64)
    }

    /// Relocate straight to the next preset in the rotation (used by the
    /// deep-zoom recycle and by the gone-boring-for-too-long recycle).
    fn jump_to_preset(&mut self) {
        let preset = PRESETS[self.preset_idx % PRESETS.len()];
        self.preset_idx = (self.preset_idx + 1) % PRESETS.len();
        self.center = (preset.0, preset.1);
        self.zoom = preset.2.clamp(MIN_ZOOM, MAX_ZOOM);
        self.boring_time = 0.0;
    }

    /// Casts a few cheap escape-time probes around the current center
    /// and nudges towards whichever direction has the most variation
    /// (i.e. actual boundary nearby), or accumulates `boring_time` if
    /// none of them found any.
    fn seek_boundary(&mut self, dt: f64, rng: &mut Rng) {
        let scale = self.scale();
        let r = PROBE_RADIUS_PX * scale;
        if !r.is_finite() || r <= 0.0 {
            self.boring_time = 0.0;
            return;
        }

        let iter = adaptive_iter(self.zoom);
        let center_val = probe_escape(self.center.0, self.center.1, iter);

        // Rotate the probe directions by a random angle each step so
        // detail sitting at a diagonal from the center isn't
        // systematically missed by always sampling N/S/E/W.
        let angle0 = rng.range_f32(0.0, std::f32::consts::TAU) as f64;
        let mut best_diff = 0.0f64;
        let mut best_dir = (0.0f64, 0.0f64);
        for k in 0..4u32 {
            let angle = angle0 + (k as f64) * std::f64::consts::FRAC_PI_2;
            let dx = angle.cos() * r;
            let dy = angle.sin() * r;
            let v = probe_escape(self.center.0 + dx, self.center.1 + dy, iter);
            let diff = (v - center_val).abs();
            if diff > best_diff {
                best_diff = diff;
                best_dir = (dx, dy);
            }
        }

        if best_diff < BORING_DIFF_THRESHOLD {
            self.boring_time += dt;
        } else {
            self.boring_time = 0.0;
            let t = 1.0 - (-BOUNDARY_NUDGE_RATE * dt).exp();
            self.center.0 += best_dir.0 * t;
            self.center.1 += best_dir.1 * t;
        }
    }

    #[cfg(test)]
    pub(crate) fn zoom(&self) -> f64 {
        self.zoom
    }

    #[cfg(test)]
    pub(crate) fn center(&self) -> (f64, f64) {
        self.center
    }
}

impl Sim for Fractal {
    fn resize(&mut self, w: usize, h: usize, rng: &mut Rng) {
        let _ = rng;
        self.w = w.max(1);
        self.h = h.max(1);
        let (lw, lh) = low_res_dims(self.w, self.h);
        self.lw = lw;
        self.lh = lh;
    }

    fn step(&mut self, dt: f32, _: &Input, rng: &mut Rng) {
        let dt = if dt.is_finite() {
            (dt as f64).clamp(0.0, MAX_DT)
        } else {
            0.0
        };

        self.zoom *= ZOOM_RATE.powf(dt);

        self.seek_boundary(dt, rng);

        if self.boring_time >= BORING_SECONDS_BEFORE_RECYCLE || self.zoom >= RECYCLE_ZOOM {
            self.jump_to_preset();
        }

        self.zoom = self.zoom.clamp(MIN_ZOOM, MAX_ZOOM);
        if !self.zoom.is_finite() {
            self.zoom = HOME_ZOOM;
        }
        if !self.center.0.is_finite() || !self.center.1.is_finite() {
            self.center = HOME_CENTER;
        }
    }

    fn render(&mut self, frame: &mut Frame, theme: &Theme) {
        let scale = self.scale();
        let iter = adaptive_iter(self.zoom);

        for ly in 0..self.lh {
            for lx in 0..self.lw {
                let fx = lx as f64 * 2.0 + 1.0;
                let fy = ly as f64 * 2.0 + 1.0;
                let re = self.center.0 + (fx - self.w as f64 / 2.0) * scale;
                let im = self.center.1 + (fy - self.h as f64 / 2.0) * scale;
                let color = sample_color(re, im, iter, theme);
                frame.rect((lx * 2) as f32, (ly * 2) as f32, 2.0, 2.0, color, 1.0);
            }
        }
    }

    fn preferred_fps(&self) -> f32 {
        60.0
    }
}

/// Escape-time sample with cardioid / period-2 bulb short-circuiting for
/// interior points (the expensive, always-hits-`max_iter` case) and
/// smooth (banding-free) coloring for exterior points.
fn sample_color(cre: f64, cim: f64, max_iter: u32, theme: &Theme) -> Rgb {
    // Main cardioid.
    let q = (cre - 0.25).powi(2) + cim * cim;
    if q * (q + (cre - 0.25)) < 0.25 * cim * cim {
        return theme.ink;
    }
    // Period-2 bulb.
    if (cre + 1.0).powi(2) + cim * cim < 0.0625 {
        return theme.ink;
    }

    let mut zr = 0.0f64;
    let mut zi = 0.0f64;
    for i in 0..max_iter {
        let zr2 = zr * zr;
        let zi2 = zi * zi;
        if zr2 + zi2 > 4.0 {
            let modulus = (zr2 + zi2).sqrt();
            let mu = i as f64 + 1.0 - (modulus.ln().ln()) / std::f64::consts::LN_2;
            return palette(mu, theme);
        }
        let new_zi = 2.0 * zr * zi + cim;
        let new_zr = zr2 - zi2 + cre;
        zr = new_zr;
        zi = new_zi;
    }
    theme.ink
}

/// Like `sample_color` but returns the raw escape iteration (or
/// `max_iter` for interior/non-escaping points) instead of a color —
/// used by `seek_boundary` to compare nearby points cheaply, without
/// needing a `Theme` or doing any color math.
fn probe_escape(cre: f64, cim: f64, max_iter: u32) -> f64 {
    let q = (cre - 0.25).powi(2) + cim * cim;
    if q * (q + (cre - 0.25)) < 0.25 * cim * cim {
        return max_iter as f64;
    }
    if (cre + 1.0).powi(2) + cim * cim < 0.0625 {
        return max_iter as f64;
    }

    let mut zr = 0.0f64;
    let mut zi = 0.0f64;
    for i in 0..max_iter {
        let zr2 = zr * zr;
        let zi2 = zi * zi;
        if zr2 + zi2 > 4.0 {
            return i as f64;
        }
        let new_zi = 2.0 * zr * zi + cim;
        let new_zr = zr2 - zi2 + cre;
        zr = new_zr;
        zi = new_zi;
    }
    max_iter as f64
}

fn palette(mu: f64, theme: &Theme) -> Rgb {
    let _ = theme; // exterior points now get a full-spectrum rainbow, not a theme-derived ramp
    let mu = if mu.is_finite() { mu.max(0.0) } else { 0.0 };
    let t = (mu / COLOR_PERIOD).rem_euclid(1.0);
    // Sweep the full hue wheel instead of a two-stop paper->accent->ink
    // gradient — classic "rainbow Mandelbrot" banding, and genuinely uses
    // every RGB hue rather than one accent color's neighborhood.
    Rgb::from_hsv(t as f32 * 360.0, 0.75, 0.55 + 0.35 * (1.0 - (t as f32 - 0.5).abs() * 2.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn low_res_dims_never_zero() {
        assert_eq!(low_res_dims(0, 0), (1, 1));
        assert_eq!(low_res_dims(320, 96), (160, 48));
        assert_eq!(low_res_dims(3, 3), (2, 2));
    }

    #[test]
    fn zoom_keeps_increasing_on_its_own() {
        let mut rng = Rng::new(2);
        let mut sim = Fractal::new(320, 96, &mut rng);
        let idle = Input::default();
        let start_zoom = sim.zoom();
        for _ in 0..300 {
            sim.step(1.0 / 60.0, &idle, &mut rng);
        }
        assert!(
            sim.zoom() > start_zoom,
            "expected zoom to keep increasing on its own even with zero interaction"
        );
    }

    #[test]
    fn deep_zoom_recycles_to_a_new_area() {
        let mut rng = Rng::new(8);
        let mut sim = Fractal::new(320, 96, &mut rng);
        sim.zoom = RECYCLE_ZOOM * 1.001; // just past the recycle threshold
        let idle = Input::default();
        sim.step(1.0 / 60.0, &idle, &mut rng);
        assert!(
            sim.zoom() < RECYCLE_ZOOM,
            "expected zoom to drop back down (relocate to a new preset) after crossing the recycle threshold"
        );
    }

    #[test]
    fn recycles_away_from_a_patternless_interior_spot() {
        let mut rng = Rng::new(15);
        let mut sim = Fractal::new(320, 96, &mut rng);
        // Force the view deep inside the period-2 bulb: solid interior
        // color, zero escape-time variation nearby at any sane probe
        // radius — exactly the "zoomed into a blank screen" bug.
        sim.center = (-1.0, 0.0);
        sim.zoom = 5000.0;
        let idle = Input::default();
        let mut relocated = false;
        for _ in 0..120 {
            sim.step(1.0 / 60.0, &idle, &mut rng);
            if sim.center() != (-1.0, 0.0) {
                relocated = true;
                break;
            }
        }
        assert!(
            relocated,
            "expected the view to relocate away from a patternless interior spot instead of sitting on it forever"
        );
    }

    #[test]
    fn large_dt_stays_finite() {
        let mut rng = Rng::new(4);
        let mut sim = Fractal::new(320, 96, &mut rng);
        let input = Input::default();
        for _ in 0..20 {
            sim.step(10.0, &input, &mut rng);
        }
        assert!(sim.zoom().is_finite());
        assert!(sim.center().0.is_finite() && sim.center().1.is_finite());
        assert!(sim.zoom() >= MIN_ZOOM && sim.zoom() <= MAX_ZOOM);
    }

    #[test]
    fn resize_to_tiny_does_not_panic() {
        let mut rng = Rng::new(5);
        let mut sim = Fractal::new(320, 96, &mut rng);
        sim.resize(4, 4, &mut rng);
        let input = Input::default();
        for _ in 0..10 {
            sim.step(1.0 / 60.0, &input, &mut rng);
        }
        let mut frame = Frame::new(4, 4);
        sim.render(&mut frame, &Theme::default());
    }

    #[test]
    fn resize_to_zero_does_not_panic() {
        let mut rng = Rng::new(6);
        let mut sim = Fractal::new(320, 96, &mut rng);
        sim.resize(0, 0, &mut rng);
        let mut frame = Frame::new(1, 1);
        sim.step(1.0 / 60.0, &Input::default(), &mut rng);
        sim.render(&mut frame, &Theme::default());
    }

    #[test]
    fn deep_zoom_scale_stays_nonzero_thanks_to_f64() {
        let mut rng = Rng::new(7);
        let mut sim = Fractal::new(320, 96, &mut rng);
        sim.zoom = 1.0e11;
        assert!(sim.scale() > 0.0, "scale underflowed to zero at deep zoom");
    }

    #[test]
    fn iteration_count_is_bounded() {
        assert_eq!(adaptive_iter(1.0), 80);
        assert!(adaptive_iter(1.0e12) <= 600);
    }
}
