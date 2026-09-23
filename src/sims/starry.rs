//! `starry` — Van Gogh's *Starry Night*, as a pile of rotating spirals.
//!
//! Each "star" is a multi-armed spiral drawn as a chain of overlapping
//! discs, tapering from a hot white core out to cool blue tips. They all
//! rotate, each at its own rate and direction, and drift slowly across
//! the sky.
//!
//! The sky between them is not a flat backdrop either: short brush
//! strokes ride a slowly-churning flow field, which is what gives the
//! background its streaming, painted current.
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
/// Very slow on purpose: `pick_spot` goes to some trouble to space the
/// swirls out at construction, and anything faster than a crawl undoes
/// that within seconds by drifting them back into each other.
const DRIFT_SPEED: f32 = 1.1; // px/sec

const RADIUS_MIN: f32 = 7.0;
const RADIUS_MAX: f32 = 22.0;
/// Darts thrown per swirl by `pick_spot`. More is better-spaced, and
/// costs `candidates * already_placed` distance checks — all of it once,
/// at construction.
const PLACEMENT_CANDIDATES: usize = 24;

// --- The sky's own current.
//
// Starry Night's sky isn't a backdrop with stars sitting on it, it's all
// movement: long curved strokes streaming between the stars. These are
// short brush strokes carried along a smooth, slowly-turning flow field
// (a couple of sines — no noise tables needed). Combined with the canvas
// being faded rather than cleared, each one smears into a streak.
const FLOW_STROKES_PER_1000_PX: f32 = 7.5;
const FLOW_STROKES_MIN: usize = 40;
const FLOW_STROKES_MAX: usize = 420;
const FLOW_SPEED: f32 = 26.0; // px/sec
/// Spatial frequency of the flow field, in radians per px. Low, so
/// neighboring strokes head the same way and form visible currents
/// rather than a directionless scatter.
const FLOW_FREQ_X: f32 = 0.021;
const FLOW_FREQ_Y: f32 = 0.028;
/// How fast the field itself churns, in rad/sec.
const FLOW_CHURN: f32 = 0.22;
/// How long a stroke rides the current before being recycled to a fresh
/// random spot, in seconds.
///
/// This is what keeps the sky evenly covered. The field is not
/// divergence-free, so left to themselves the strokes all migrate into
/// its attractors within a few seconds and the rest of the canvas goes
/// bare — exactly the flat backdrop this is meant to replace. Recycling
/// keeps the density uniform whatever the field does.
const FLOW_LIFE_MIN: f32 = 1.6;
const FLOW_LIFE_MAX: f32 = 4.2;
/// Seconds spent fading in at birth and out at death, so recycling a
/// stroke doesn't pop.
const FLOW_FADE_SEC: f32 = 0.45;

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

/// One short brush stroke in the sky's current.
#[derive(Clone, Copy)]
struct Flow {
    x: f32,
    y: f32,
    /// Length of the streak drawn this frame, in px.
    len: f32,
    /// Slight per-stroke hue variation, so the sky isn't one flat blue.
    hue: f32,
    val: f32,
    /// Seconds since this stroke was (re)spawned, and how long it lives
    /// before being recycled — see `FLOW_LIFE_MIN`.
    age: f32,
    life: f32,
}

pub struct Starry {
    w: f32,
    h: f32,
    swirls: Vec<Swirl>,
    flow: Vec<Flow>,
    /// Drives the flow field's churn. Tracked separately from the
    /// per-swirl angles because the field turns as a whole.
    time: f32,
    /// Set until the first render, so the very first frame lays down the
    /// night sky instead of fading up from an empty black buffer.
    needs_clear: bool,
}

fn target_count(w: usize, h: usize) -> usize {
    ((w * h) / 2600).clamp(MIN_SWIRLS, MAX_SWIRLS)
}

/// Shortest signed distance along one axis of a torus of the given size.
/// Swirls wrap around the canvas, so spacing has to be measured the same
/// way — otherwise two swirls hugging opposite edges score as far apart
/// when they are in fact touching.
fn wrap_delta(a: f32, b: f32, size: f32) -> f32 {
    if size <= 0.0 {
        return a - b;
    }
    let mut d = a - b;
    let half = size * 0.5;
    if d > half {
        d -= size;
    } else if d < -half {
        d += size;
    }
    d
}

impl Starry {
    pub fn new(w: usize, h: usize, rng: &mut Rng) -> Self {
        let mut s = Starry {
            w: w.max(1) as f32,
            h: h.max(1) as f32,
            swirls: Vec::new(),
            flow: Vec::new(),
            time: 0.0,
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
            let radius = rng.range_f32(RADIUS_MIN, max_radius);
            let (x, y) = self.pick_spot(radius, rng);
            self.swirls.push(Swirl {
                x,
                y,
                vx: drift.cos() * speed,
                vy: drift.sin() * speed,
                radius,
                angle: rng.range_f32(0.0, TAU),
                spin: rng.range_f32(SPIN_MIN, SPIN_MAX) * if rng.bool_p(0.5) { 1.0 } else { -1.0 },
                arms: rng.range_i32(ARMS_MIN as i32, ARMS_MAX as i32 + 1) as u32,
                // Van Gogh's sky is yellows and golds against blue, so
                // the arms live in that band rather than the full wheel.
                hue: rng.range_f32(30.0, 62.0),
            });
        }
        self.populate_flow(rng);
    }

    fn populate_flow(&mut self, rng: &mut Rng) {
        let n = ((self.w * self.h / 1000.0) * FLOW_STROKES_PER_1000_PX) as usize;
        let n = n.clamp(FLOW_STROKES_MIN, FLOW_STROKES_MAX);
        self.flow.clear();
        self.flow.reserve(n);
        for _ in 0..n {
            let mut f = Self::spawn_stroke(self.w, self.h, rng);
            // Stagger the starting ages so they don't all recycle in
            // lockstep on the first pass.
            f.age = rng.range_f32(0.0, f.life);
            self.flow.push(f);
        }
    }

    fn spawn_stroke(w: f32, h: f32, rng: &mut Rng) -> Flow {
        Flow {
            x: rng.range_f32(0.0, w),
            y: rng.range_f32(0.0, h),
            len: rng.range_f32(7.0, 22.0),
            hue: rng.range_f32(198.0, 232.0),
            val: rng.range_f32(0.34, 0.72),
            age: 0.0,
            life: rng.range_f32(FLOW_LIFE_MIN, FLOW_LIFE_MAX),
        }
    }

    /// Fade envelope for a stroke: ramps up from birth, holds, ramps back
    /// down into recycling.
    fn stroke_alpha(f: &Flow) -> f32 {
        let fade_in = (f.age / FLOW_FADE_SEC).clamp(0.0, 1.0);
        let fade_out = ((f.life - f.age) / FLOW_FADE_SEC).clamp(0.0, 1.0);
        fade_in.min(fade_out)
    }

    /// Direction of the sky's current at a point. Two sines at
    /// incommensurate frequencies, slowly rotating — enough to read as
    /// swirling currents without a noise table.
    fn flow_angle(&self, x: f32, y: f32) -> f32 {
        let a = (x * FLOW_FREQ_X + self.time * FLOW_CHURN).sin();
        let b = (y * FLOW_FREQ_Y - self.time * FLOW_CHURN * 0.7).cos();
        (a + b) * 1.6
    }

    /// Mitchell's best-candidate sampling: throw a handful of darts and
    /// keep whichever lands furthest from everything already placed.
    ///
    /// Uniform random placement clumps — that is what uniform random
    /// *does* — and with a dozen swirls on a small canvas those clumps
    /// read as one blob rather than as separate stars. This spreads them
    /// out without the unbounded retry loop rejection sampling would
    /// need: it always terminates after `PLACEMENT_CANDIDATES` tries,
    /// however crowded the canvas already is.
    fn pick_spot(&self, radius: f32, rng: &mut Rng) -> (f32, f32) {
        let mut best = (rng.range_f32(0.0, self.w), rng.range_f32(0.0, self.h));
        if self.swirls.is_empty() {
            return best;
        }
        let mut best_score = f32::NEG_INFINITY;
        for _ in 0..PLACEMENT_CANDIDATES {
            let cx = rng.range_f32(0.0, self.w);
            let cy = rng.range_f32(0.0, self.h);
            // Score by the *gap between rims*, not center distance, so a
            // big swirl is given the room it actually needs.
            let mut score = f32::INFINITY;
            for s in &self.swirls {
                let dx = wrap_delta(cx, s.x, self.w);
                let dy = wrap_delta(cy, s.y, self.h);
                score = score.min((dx * dx + dy * dy).sqrt() - (radius + s.radius));
            }
            if score > best_score {
                best_score = score;
                best = (cx, cy);
            }
        }
        best
    }

    #[cfg(test)]
    pub(crate) fn swirl_count(&self) -> usize {
        self.swirls.len()
    }

    #[cfg(test)]
    pub(crate) fn radii(&self) -> Vec<f32> {
        self.swirls.iter().map(|s| s.radius).collect()
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
            if changed {
                // Stroke count scales with area, and strokes outside the
                // new bounds would never wrap back into view.
                self.populate_flow(rng);
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

        self.time += dt;

        // Carry the sky's brush strokes along the current, recycling the
        // ones that have lived out their span — see `FLOW_LIFE_MIN`.
        for i in 0..self.flow.len() {
            let f = self.flow[i];
            if f.age >= f.life {
                self.flow[i] = Self::spawn_stroke(self.w, self.h, rng);
                continue;
            }
            let ang = self.flow_angle(f.x, f.y);
            self.flow[i].x = (f.x + ang.cos() * FLOW_SPEED * dt).rem_euclid(self.w);
            self.flow[i].y = (f.y + ang.sin() * FLOW_SPEED * dt).rem_euclid(self.h);
            self.flow[i].age = f.age + dt;
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

    }

    fn render(&mut self, frame: &mut Frame, theme: &Theme) {
        let _ = theme; // the night sky is the night sky
        if self.needs_clear {
            frame.fill(NIGHT);
            self.needs_clear = false;
        } else {
            frame.fade_to(NIGHT, FADE);
        }

        // The sky's own current, under the stars: each stroke is drawn
        // along the flow direction at its position, so neighbors line up
        // into long curved bands rather than scattering.
        for f in &self.flow {
            let alpha = Self::stroke_alpha(f) * 0.62;
            if alpha <= 0.01 {
                continue;
            }
            let ang = self.flow_angle(f.x, f.y);
            let (dx, dy) = (ang.cos(), ang.sin());
            let color = Rgb::from_hsv(f.hue, 0.55, f.val);
            let steps = (f.len / 1.5).ceil().max(1.0) as usize;
            for i in 0..steps {
                let t = i as f32 / steps as f32;
                frame.disc(f.x + dx * f.len * t, f.y + dy * f.len * t, 1.6, color, alpha);
            }
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
    fn swirls_are_spaced_out_rather_than_clumped() {
        // Best-candidate placement should beat uniform random on the
        // worst-case gap between rims. Compared directly against uniform
        // random on the same canvas with the same radii, averaged over
        // several seeds so one lucky draw can't carry the result.
        fn worst_gap(pos: &[(f32, f32)], radii: &[f32], w: f32, h: f32) -> f32 {
            let mut worst = f32::INFINITY;
            for i in 0..pos.len() {
                for j in (i + 1)..pos.len() {
                    let dx = wrap_delta(pos[i].0, pos[j].0, w);
                    let dy = wrap_delta(pos[i].1, pos[j].1, h);
                    worst = worst.min((dx * dx + dy * dy).sqrt() - (radii[i] + radii[j]));
                }
            }
            worst
        }

        let (w, h) = (320.0f32, 96.0f32);
        let seeds = 12;
        let mut placed_total = 0.0;
        let mut uniform_total = 0.0;
        for seed in 0..seeds {
            let sim = make(w as usize, h as usize, seed + 40);
            let radii = sim.radii();
            placed_total += worst_gap(&sim.positions(), &radii, w, h);

            let mut rng = Rng::new(seed + 900);
            let uniform: Vec<(f32, f32)> =
                (0..radii.len()).map(|_| (rng.range_f32(0.0, w), rng.range_f32(0.0, h))).collect();
            uniform_total += worst_gap(&uniform, &radii, w, h);
        }

        let placed = placed_total / seeds as f32;
        let uniform = uniform_total / seeds as f32;
        assert!(
            placed > uniform,
            "spaced placement ({placed:.1}) is no better than uniform random ({uniform:.1})"
        );
    }

    #[test]
    fn the_sky_itself_flows() {
        // The background strokes are what make it a painted sky rather
        // than dots on flat blue: they have to exist, and they have to
        // move.
        let mut rng = Rng::new(31);
        let mut sim = make(320, 96, 31);
        assert!(!sim.flow.is_empty(), "no background brush strokes at all");

        let before: Vec<(f32, f32)> = sim.flow.iter().map(|f| (f.x, f.y)).collect();
        for _ in 0..30 {
            sim.step(1.0 / 60.0, &Input::default(), &mut rng);
        }
        let after: Vec<(f32, f32)> = sim.flow.iter().map(|f| (f.x, f.y)).collect();

        let moved = before
            .iter()
            .zip(after.iter())
            .filter(|((x0, y0), (x1, y1))| (x0 - x1).abs() > 0.1 || (y0 - y1).abs() > 0.1)
            .count();
        assert!(moved > before.len() / 2, "only {moved} of {} strokes moved", before.len());
        for (x, y) in after {
            assert!(x.is_finite() && (0.0..320.0).contains(&x));
            assert!(y.is_finite() && (0.0..96.0).contains(&y));
        }
    }

    #[test]
    fn neighboring_strokes_follow_the_same_current() {
        // A flow *field*, not per-stroke randomness: two strokes close
        // together must head in nearly the same direction, which is what
        // forms visible currents instead of a directionless scatter.
        let sim = make(320, 96, 32);
        let a = sim.flow_angle(100.0, 40.0);
        let b = sim.flow_angle(104.0, 43.0);
        let diff = (a - b).abs();
        assert!(diff < 0.35, "nearby flow directions differ by {diff} rad -- that's noise, not a current");
    }

    #[test]
    fn strokes_stay_spread_across_the_sky_over_time() {
        // The whole point of recycling strokes: the flow field is not
        // divergence-free, so without it they all migrate into its
        // attractors and most of the canvas ends up bare. Check every
        // quadrant still holds strokes after a long run.
        let mut rng = Rng::new(33);
        let mut sim = make(320, 96, 33);
        for _ in 0..3600 {
            sim.step(1.0 / 60.0, &Input::default(), &mut rng);
        }
        let mut quadrants = [0usize; 4];
        for f in &sim.flow {
            let qx = usize::from(f.x >= 160.0);
            let qy = usize::from(f.y >= 48.0);
            quadrants[qy * 2 + qx] += 1;
        }
        let total: usize = quadrants.iter().sum();
        for (i, count) in quadrants.iter().enumerate() {
            assert!(
                *count * 8 >= total,
                "quadrant {i} holds only {count} of {total} strokes -- the sky has gone bare there"
            );
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
