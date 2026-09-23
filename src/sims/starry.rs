//! `starry` — Van Gogh's *Starry Night*, as a pile of rotating spirals.
//!
//! Each "star" is a multi-armed spiral drawn as a chain of overlapping
//! discs, tapering from a hot white core out to cool blue tips. They all
//! rotate, each at its own rate and direction, and drift slowly across
//! the sky.
//!
//! The frame is only ever *faded* towards the night color rather than
//! cleared, so every stroke smears a little into the frames after it.
//! That smear is doing most of the work: a spiral of hard-edged dots
//! reads as clip art, while the same spiral leaving a short trail behind
//! it reads as brushwork.

use super::{Input, Sim};
use crate::paint::{Frame, Rgb, Theme};
use crate::rng::Rng;
use std::f32::consts::TAU;

/// The night itself. Deliberately not `theme.paper`: "starry night" on a
/// cream background is just not the painting.
const NIGHT: Rgb = Rgb::new(0x10, 0x1C, 0x3A);
/// How far each frame pulls the canvas back towards `NIGHT`. Low enough
/// to leave a visible smear behind every stroke, high enough that the
/// smear is a brush trail and not a permanent smudge.
const FADE: f32 = 0.22;

/// At least eight swirls, as many as twenty once there's room for them.
const MIN_SWIRLS: usize = 8;
const MAX_SWIRLS: usize = 20;

const ARMS_MIN: u32 = 2;
const ARMS_MAX: u32 = 4;
/// How far round each arm wraps, in turns.
const ARM_TURNS: f32 = 0.85;
/// Discs per arm. The spiral is drawn by stamping overlapping discs along
/// it, so this is really "sample density".
const ARM_STEPS: usize = 18;

const SPIN_MIN: f32 = 0.35; // rad/sec
const SPIN_MAX: f32 = 1.6;
const DRIFT_SPEED: f32 = 4.0; // px/sec

const RADIUS_MIN: f32 = 7.0;
const RADIUS_MAX: f32 = 22.0;

/// Hovering speeds up the swirls near the cursor, clicking flips every
/// swirl's direction at once.
const HOVER_R: f32 = 70.0;
const HOVER_SPIN_MULT: f32 = 3.2;
const MAX_DT: f32 = 1.0 / 20.0;

#[derive(Clone, Copy)]
struct Swirl {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    radius: f32,
    angle: f32,
    /// Signed, so half of them turn the other way.
    spin: f32,
    arms: u32,
    /// Hue of the *outer* end of the arms; the core is always near-white.
    hue: f32,
}

pub struct Starry {
    w: f32,
    h: f32,
    swirls: Vec<Swirl>,
    /// Set until the first render, so the very first frame lays down the
    /// night sky instead of fading up from an empty black buffer.
    needs_clear: bool,
}

fn target_count(w: usize, h: usize) -> usize {
    ((w * h) / 2600).clamp(MIN_SWIRLS, MAX_SWIRLS)
}

impl Starry {
    pub fn new(w: usize, h: usize, rng: &mut Rng) -> Self {
        let mut s = Starry {
            w: w.max(1) as f32,
            h: h.max(1) as f32,
            swirls: Vec::new(),
            needs_clear: true,
        };
        s.populate(rng);
        s
    }

    fn populate(&mut self, rng: &mut Rng) {
        let n = target_count(self.w as usize, self.h as usize);
        // Keep swirls comfortably smaller than the canvas on short
        // buttons, or every one of them covers the whole thing.
        let max_radius = RADIUS_MAX.min(self.h * 0.45).max(RADIUS_MIN);
        self.swirls.clear();
        self.swirls.reserve(n);
        for _ in 0..n {
            let drift = rng.range_f32(0.0, TAU);
            let speed = rng.range_f32(0.0, DRIFT_SPEED);
            self.swirls.push(Swirl {
                x: rng.range_f32(0.0, self.w),
                y: rng.range_f32(0.0, self.h),
                vx: drift.cos() * speed,
                vy: drift.sin() * speed,
                radius: rng.range_f32(RADIUS_MIN, max_radius),
                angle: rng.range_f32(0.0, TAU),
                spin: rng.range_f32(SPIN_MIN, SPIN_MAX) * if rng.bool_p(0.5) { 1.0 } else { -1.0 },
                arms: rng.range_i32(ARMS_MIN as i32, ARMS_MAX as i32 + 1) as u32,
                // Van Gogh's sky is yellows and golds against blue, so
                // the arms live in that band rather than the full wheel.
                hue: rng.range_f32(30.0, 62.0),
            });
        }
    }

    #[cfg(test)]
    pub(crate) fn swirl_count(&self) -> usize {
        self.swirls.len()
    }

    #[cfg(test)]
    pub(crate) fn angles(&self) -> Vec<f32> {
        self.swirls.iter().map(|s| s.angle).collect()
    }

    #[cfg(test)]
    pub(crate) fn spins(&self) -> Vec<f32> {
        self.swirls.iter().map(|s| s.spin).collect()
    }

    #[cfg(test)]
    pub(crate) fn positions(&self) -> Vec<(f32, f32)> {
        self.swirls.iter().map(|s| (s.x, s.y)).collect()
    }
}

impl Sim for Starry {
    fn resize(&mut self, w: usize, h: usize, rng: &mut Rng) {
        let changed = (w.max(1) as f32) != self.w || (h.max(1) as f32) != self.h;
        self.w = w.max(1) as f32;
        self.h = h.max(1) as f32;
        if self.swirls.len() != target_count(w, h) {
            self.populate(rng);
        } else {
            for s in &mut self.swirls {
                s.x = s.x.rem_euclid(self.w);
                s.y = s.y.rem_euclid(self.h);
            }
        }
        if changed {
            self.needs_clear = true;
        }
    }

    fn step(&mut self, dt: f32, input: &Input, rng: &mut Rng) {
        let dt = if dt.is_finite() { dt.clamp(0.0, MAX_DT) } else { 0.0 };
        if dt <= 0.0 || self.swirls.is_empty() || self.w <= 0.0 || self.h <= 0.0 {
            return;
        }

        // A click sends the whole sky the other way round.
        if input.clicks > 0 {
            for s in &mut self.swirls {
                s.spin = -s.spin;
            }
        }

        for s in &mut self.swirls {
            let mut rate = s.spin;
            if input.hover {
                let dx = s.x - input.x;
                let dy = s.y - input.y;
                let d = (dx * dx + dy * dy).sqrt();
                if d < HOVER_R {
                    // Ramps up towards the cursor rather than switching on
                    // at the boundary, so there's no visible ring.
                    let t = 1.0 - d / HOVER_R;
                    rate *= 1.0 + (HOVER_SPIN_MULT - 1.0) * t;
                }
            }
            s.angle = (s.angle + rate * dt).rem_euclid(TAU);

            // Wrap rather than bounce: a sky has no edges.
            s.x = (s.x + s.vx * dt).rem_euclid(self.w);
            s.y = (s.y + s.vy * dt).rem_euclid(self.h);
        }

        let _ = rng; // all randomness is baked in at populate time
    }

    fn render(&mut self, frame: &mut Frame, theme: &Theme) {
        let _ = theme; // the night sky is the night sky
        if self.needs_clear {
            frame.fill(NIGHT);
            self.needs_clear = false;
        } else {
            frame.fade_to(NIGHT, FADE);
        }

        for s in &self.swirls {
            for arm in 0..s.arms {
                let arm_offset = TAU * arm as f32 / s.arms as f32;
                for i in 0..ARM_STEPS {
                    // `t` runs from the core out to the tip of the arm.
                    let t = (i as f32 + 0.5) / ARM_STEPS as f32;
                    let theta = s.angle + arm_offset + t * TAU * ARM_TURNS;
                    let r = s.radius * t;
                    let x = s.x + theta.cos() * r;
                    let y = s.y + theta.sin() * r;

                    // Strokes are fattest mid-arm and taper at both ends,
                    // which is what stops the spiral reading as a wire.
                    let taper = (t * (1.0 - t) * 4.0).clamp(0.15, 1.0);
                    let size = 0.8 + taper * s.radius * 0.16;

                    // Hot white core cooling to the swirl's own hue, then
                    // on out to the blue the sky is painted in.
                    let color = if t < 0.35 {
                        Rgb::from_hsv(s.hue, 0.18 + t, 1.0)
                    } else {
                        let k = (t - 0.35) / 0.65;
                        Rgb::from_hsv(s.hue, 0.85, 1.0).lerp(Rgb::from_hsv(212.0, 0.72, 0.85), k)
                    };

                    frame.disc(x, y, size, color, 0.9);
                }
            }
        }
    }

    fn preferred_fps(&self) -> f32 {
        60.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paint::Theme;

    fn make(w: usize, h: usize, seed: u32) -> Starry {
        let mut rng = Rng::new(seed);
        Starry::new(w, h, &mut rng)
    }

    #[test]
    fn always_at_least_eight_swirls() {
        for (w, h) in [(320, 96), (4, 4), (1, 1), (2000, 2000)] {
            let sim = make(w, h, 1);
            assert!(
                sim.swirl_count() >= MIN_SWIRLS,
                "{w}x{h} produced only {} swirls",
                sim.swirl_count()
            );
            assert!(sim.swirl_count() <= MAX_SWIRLS);
        }
    }

    #[test]
    fn every_swirl_keeps_rotating() {
        let mut rng = Rng::new(2);
        let mut sim = make(320, 96, 2);
        let before = sim.angles();
        for _ in 0..120 {
            sim.step(1.0 / 60.0, &Input::default(), &mut rng);
        }
        let after = sim.angles();
        for (i, (a, b)) in before.iter().zip(after.iter()).enumerate() {
            assert!((a - b).abs() > 1e-4, "swirl {i} never rotated");
            assert!(b.is_finite());
        }
    }

    #[test]
    fn swirls_turn_both_ways() {
        // A sky where every swirl turns the same way looks mechanical.
        let sim = make(320, 96, 3);
        let spins = sim.spins();
        assert!(spins.iter().any(|&s| s > 0.0), "no swirl turns clockwise");
        assert!(spins.iter().any(|&s| s < 0.0), "no swirl turns anticlockwise");
    }

    #[test]
    fn click_reverses_every_swirl() {
        let mut rng = Rng::new(4);
        let mut sim = make(320, 96, 4);
        let before = sim.spins();
        let input = Input { x: 0.0, y: 0.0, hover: false, down: false, clicks: 1 };
        sim.step(1.0 / 60.0, &input, &mut rng);
        let after = sim.spins();
        for (a, b) in before.iter().zip(after.iter()) {
            assert!((a + b).abs() < 1e-5, "spin {a} did not reverse (got {b})");
        }
    }

    #[test]
    fn swirls_stay_on_canvas_and_finite() {
        let mut rng = Rng::new(5);
        let mut sim = make(320, 96, 5);
        let input = Input { x: 100.0, y: 40.0, hover: true, down: false, clicks: 0 };
        for _ in 0..3000 {
            sim.step(1.0 / 60.0, &input, &mut rng);
        }
        for (x, y) in sim.positions() {
            assert!(x.is_finite() && y.is_finite());
            assert!((0.0..320.0).contains(&x), "x={x} left the canvas");
            assert!((0.0..96.0).contains(&y), "y={y} left the canvas");
        }
    }

    #[test]
    fn large_dt_does_not_explode_or_panic() {
        let mut rng = Rng::new(6);
        let mut sim = make(320, 96, 6);
        for _ in 0..10 {
            sim.step(1.0, &Input::default(), &mut rng);
        }
        for a in sim.angles() {
            assert!(a.is_finite());
        }
    }

    #[test]
    fn renders_a_night_sky_with_bright_cores() {
        let mut rng = Rng::new(7);
        let mut sim = make(320, 96, 7);
        let mut frame = Frame::new(320, 96);
        let theme = Theme::default();
        for _ in 0..30 {
            sim.step(1.0 / 60.0, &Input::default(), &mut rng);
            sim.render(&mut frame, &theme);
        }

        let mut dark = 0;
        let mut bright = 0;
        for px in frame.pixels.chunks_exact(4) {
            let lum = px[0] as u32 + px[1] as u32 + px[2] as u32;
            if lum < 200 {
                dark += 1;
            }
            if lum > 500 {
                bright += 1;
            }
            assert_eq!(px[3], 255);
        }
        assert!(dark > 0, "no night sky left -- the swirls covered everything");
        assert!(bright > 0, "no bright star cores were drawn");
    }

    #[test]
    fn resize_to_tiny_does_not_panic() {
        let mut rng = Rng::new(8);
        let mut sim = make(320, 96, 8);
        sim.resize(3, 3, &mut rng);
        for _ in 0..30 {
            sim.step(1.0 / 60.0, &Input::default(), &mut rng);
        }
        let mut frame = Frame::new(3, 3);
        sim.render(&mut frame, &Theme::default());
    }

    #[test]
    fn resize_to_zero_does_not_panic() {
        let mut rng = Rng::new(9);
        let mut sim = make(320, 96, 9);
        sim.resize(0, 0, &mut rng);
        sim.step(1.0 / 60.0, &Input::default(), &mut rng);
    }
}
