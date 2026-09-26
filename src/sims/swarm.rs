//! `swarm` — a boids flock.
//!
//! Neighbor queries (separation/alignment/cohesion) run against a uniform
//! spatial grid rebuilt every step, cell size equal to the perception
//! radius. That keeps the whole step O(n) in practice instead of O(n²),
//! which matters once we're pushing several hundred boids at 60fps.

use super::{Input, Sim};
use crate::paint::{Frame, Rgb, Theme};
use crate::rng::Rng;

const PERCEPTION_R: f32 = 30.0;
const SEPARATION_R: f32 = 14.0;
const MIN_SPEED: f32 = 26.0;
const MAX_SPEED: f32 = 74.0;
const SEPARATION_FORCE: f32 = 260.0;
const ALIGN_FORCE: f32 = 2.6;
const COHESION_FORCE: f32 = 1.1;
/// Largest dt a single `step` call will actually simulate. Anything above
/// this (a backgrounded tab waking up after a second or more) is clamped
/// so the flock can't teleport, overshoot its speed cap, or otherwise
/// blow up.
const MAX_DT: f32 = 1.0 / 20.0;

/// Constant-magnitude force applied along each boid's own slowly-drifting
/// `wander_angle`. Alignment + cohesion alone are a classic recipe for the
/// whole flock eventually synchronizing onto one heading and cruising that
/// way forever (a stable equilibrium with nothing left to perturb it) —
/// this per-boid Reynolds-style "wander" steering is the fix: a small,
/// smoothly-turning nudge that never goes away, so the flock can never
/// fully settle into permanent one-directional drift.
const WANDER_FORCE: f32 = 150.0;
/// Max rate the wander angle can turn, in radians/sec. Small relative to a
/// full turn so it reads as organic drift rather than jitter.
const WANDER_TURN_RATE: f32 = 2.4;

/// Upper bound on speed under any circumstance — used by callers/tests as
/// the contract ceiling.
pub const MAX_SPEED_EVER: f32 = MAX_SPEED;
/// Speed at which a boid is drawn at full brightness. Above `MAX_SPEED`
/// on purpose, so even the fastest boids stay a little short of it.
const BRIGHTEST_SPEED: f32 = MAX_SPEED * 1.6;

#[derive(Clone, Copy)]
struct Boid {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    /// Persistent per-boid heading for the "wander" force — slowly random
    /// walks every step (see `WANDER_FORCE`) instead of being redrawn from
    /// scratch, which is what keeps the nudge smooth instead of jittery.
    wander_angle: f32,
    /// This boid's own hue (degrees), fixed for its lifetime. Full
    /// spectrum, chosen independently per boid, rather than every boid
    /// sharing one accent-derived gradient — see `render`.
    hue: f32,
}

pub struct Swarm {
    w: f32,
    h: f32,
    boids: Vec<Boid>,
}

fn target_count(w: usize, h: usize) -> usize {
    // Denser than a "reasonable" boid count on purpose — this is a
    // decorative button, not a physically plausible flock, and a thicker
    // swarm reads much more alive.
    let n = (w * h) / 80;
    n.clamp(300, 2200)
}

/// Shortest signed distance from `b` to `a` along one axis of a torus of
/// the given `size` (i.e. the "wrapped" delta, at most `size / 2` in
/// magnitude).
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

impl Swarm {
    pub fn new(w: usize, h: usize, rng: &mut Rng) -> Self {
        let mut s = Swarm {
            w: (w.max(1)) as f32,
            h: (h.max(1)) as f32,
            boids: Vec::new(),
        };
        s.populate(rng);
        s
    }

    fn populate(&mut self, rng: &mut Rng) {
        let n = target_count(self.w as usize, self.h as usize);
        self.boids.clear();
        self.boids.reserve(n);
        for _ in 0..n {
            let ang = rng.range_f32(0.0, std::f32::consts::TAU);
            let speed = rng.range_f32(MIN_SPEED, MAX_SPEED);
            self.boids.push(Boid {
                x: rng.range_f32(0.0, self.w),
                y: rng.range_f32(0.0, self.h),
                vx: ang.cos() * speed,
                vy: ang.sin() * speed,
                wander_angle: rng.range_f32(0.0, std::f32::consts::TAU),
                hue: rng.range_f32(0.0, 360.0),
            });
        }
    }

    #[cfg(test)]
    pub(crate) fn positions(&self) -> Vec<(f32, f32)> {
        self.boids.iter().map(|b| (b.x, b.y)).collect()
    }

    #[cfg(test)]
    pub(crate) fn velocities(&self) -> Vec<(f32, f32)> {
        self.boids.iter().map(|b| (b.vx, b.vy)).collect()
    }
}

impl Sim for Swarm {
    fn resize(&mut self, w: usize, h: usize, rng: &mut Rng) {
        self.w = w.max(1) as f32;
        self.h = h.max(1) as f32;
        let n = target_count(w, h);
        if self.boids.len() != n {
            self.populate(rng);
        } else {
            for b in &mut self.boids {
                b.x = b.x.rem_euclid(self.w);
                b.y = b.y.rem_euclid(self.h);
            }
        }
    }

    fn step(&mut self, dt: f32, _: &Input, rng: &mut Rng) {
        let dt = if dt.is_finite() { dt.clamp(0.0, MAX_DT) } else { 0.0 };
        if dt <= 0.0 || self.boids.is_empty() || self.w <= 0.0 || self.h <= 0.0 {
            return;
        }

        // --- Uniform spatial grid, cell size == perception radius.
        let cell = PERCEPTION_R;
        let cols = ((self.w / cell).ceil() as i32).max(1);
        let rows = ((self.h / cell).ceil() as i32).max(1);
        let cell_index =
            |cx: i32, cy: i32| -> usize { (cy.rem_euclid(rows) * cols + cx.rem_euclid(cols)) as usize };

        let mut grid: Vec<Vec<u32>> = vec![Vec::new(); (cols * rows) as usize];
        for (i, b) in self.boids.iter().enumerate() {
            let cx = (b.x / cell) as i32;
            let cy = (b.y / cell) as i32;
            grid[cell_index(cx, cy)].push(i as u32);
        }

        let n = self.boids.len();
        let mut accel = vec![(0.0f32, 0.0f32); n];

        for i in 0..n {
            // Slowly random-walk this boid's own wander heading every step
            // (persistent state, not redrawn from scratch) so the nudge
            // reads as smooth organic drift rather than jitter.
            self.boids[i].wander_angle = (self.boids[i].wander_angle
                + rng.range_f32(-1.0, 1.0) * WANDER_TURN_RATE * dt)
                .rem_euclid(std::f32::consts::TAU);
            let bi = self.boids[i];
            let ci_x = (bi.x / cell) as i32;
            let ci_y = (bi.y / cell) as i32;

            let mut sep = (0.0f32, 0.0f32);
            let mut align_sum = (0.0f32, 0.0f32);
            let mut coh_sum = (0.0f32, 0.0f32);
            let mut neighbors = 0u32;

            for oy in -1..=1 {
                for ox in -1..=1 {
                    let idx = cell_index(ci_x + ox, ci_y + oy);
                    for &j in &grid[idx] {
                        if j as usize == i {
                            continue;
                        }
                        let bj = self.boids[j as usize];
                        let dx = wrap_delta(bj.x, bi.x, self.w);
                        let dy = wrap_delta(bj.y, bi.y, self.h);
                        let d2 = dx * dx + dy * dy;
                        if d2 > PERCEPTION_R * PERCEPTION_R {
                            continue;
                        }
                        if d2 < SEPARATION_R * SEPARATION_R && d2 > 1e-6 {
                            let d = d2.sqrt();
                            let strength = (SEPARATION_R - d) / SEPARATION_R;
                            sep.0 -= dx / d * strength;
                            sep.1 -= dy / d * strength;
                        }
                        align_sum.0 += bj.vx;
                        align_sum.1 += bj.vy;
                        coh_sum.0 += dx;
                        coh_sum.1 += dy;
                        neighbors += 1;
                    }
                }
            }

            let mut ax = sep.0 * SEPARATION_FORCE + bi.wander_angle.cos() * WANDER_FORCE;
            let mut ay = sep.1 * SEPARATION_FORCE + bi.wander_angle.sin() * WANDER_FORCE;
            if neighbors > 0 {
                let inv = 1.0 / neighbors as f32;
                ax += align_sum.0 * inv * ALIGN_FORCE;
                ay += align_sum.1 * inv * ALIGN_FORCE;
                ax += coh_sum.0 * inv * COHESION_FORCE;
                ay += coh_sum.1 * inv * COHESION_FORCE;
            }

            accel[i] = (ax, ay);
        }

        for (i, b) in self.boids.iter_mut().enumerate() {
            b.vx += accel[i].0 * dt;
            b.vy += accel[i].1 * dt;

            let speed = (b.vx * b.vx + b.vy * b.vy).sqrt();
            if speed.is_finite() && speed > 1e-5 {
                let clamped = speed.clamp(MIN_SPEED, MAX_SPEED);
                let scale = clamped / speed;
                b.vx *= scale;
                b.vy *= scale;
            } else {
                // Degenerate velocity (near-zero or non-finite): reset to
                // a sane heading instead of letting NaNs propagate.
                b.vx = MIN_SPEED;
                b.vy = 0.0;
            }

            b.x = (b.x + b.vx * dt).rem_euclid(self.w);
            b.y = (b.y + b.vy * dt).rem_euclid(self.h);
        }
    }

    fn render(&mut self, frame: &mut Frame, theme: &Theme) {
        frame.fade_to(theme.paper, 0.26);
        let speed_range = (BRIGHTEST_SPEED - MIN_SPEED).max(1.0);
        for b in &self.boids {
            let speed = (b.vx * b.vx + b.vy * b.vy).sqrt();
            let t = ((speed - MIN_SPEED) / speed_range).clamp(0.0, 1.0);
            // Each boid keeps its own hue (full spectrum, not a shared
            // accent-derived ramp); speed still modulates brightness so
            // fast boids visibly "light up".
            let color = Rgb::from_hsv(b.hue, 0.65, 0.45 + t * 0.5);
            frame.disc(b.x, b.y, 2.6, color, 1.0);
            if speed > 1e-3 {
                let tx = b.x - b.vx / speed * 5.0;
                let ty = b.y - b.vy / speed * 5.0;
                frame.disc(tx, ty, 1.4, color, 0.35);
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

    fn make(w: usize, h: usize, seed: u32) -> Swarm {
        let mut rng = Rng::new(seed);
        Swarm::new(w, h, &mut rng)
    }

    #[test]
    fn count_within_expected_bounds() {
        assert_eq!(target_count(320, 96), 384); // (320*96)/80 = 384
        assert_eq!(target_count(4, 4), 300); // clamps up from ~0
        assert_eq!(target_count(4000, 4000), 2200); // clamps down
    }

    #[test]
    fn boids_stay_within_canvas_and_finite() {
        let mut rng = Rng::new(1);
        let mut sim = make(320, 96, 1);
        let input = Input::default();
        for _ in 0..600 {
            sim.step(1.0 / 60.0, &input, &mut rng);
        }
        for (x, y) in sim.positions() {
            assert!(x.is_finite() && y.is_finite());
            assert!((0.0..320.0).contains(&x), "x={x} escaped canvas");
            assert!((0.0..96.0).contains(&y), "y={y} escaped canvas");
        }
        for (vx, vy) in sim.velocities() {
            assert!(vx.is_finite() && vy.is_finite());
            let speed = (vx * vx + vy * vy).sqrt();
            assert!(speed <= MAX_SPEED_EVER + 1.0, "speed {speed} exceeded ceiling");
        }
    }

    #[test]
    fn large_dt_does_not_explode() {
        let mut rng = Rng::new(2);
        let mut sim = make(320, 96, 2);
        let input = Input::default();
        // Simulate a tab that was backgrounded for a full second.
        for _ in 0..5 {
            sim.step(1.0, &input, &mut rng);
        }
        for (x, y) in sim.positions() {
            assert!(x.is_finite() && (0.0..320.0).contains(&x));
            assert!(y.is_finite() && (0.0..96.0).contains(&y));
        }
        for (vx, vy) in sim.velocities() {
            let speed = (vx * vx + vy * vy).sqrt();
            assert!(speed.is_finite());
            assert!(speed <= MAX_SPEED_EVER + 1.0);
        }
    }

    #[test]
    fn resize_to_tiny_size_does_not_panic() {
        let mut rng = Rng::new(4);
        let mut sim = make(320, 96, 4);
        sim.resize(4, 4, &mut rng);
        let input = Input::default();
        for _ in 0..50 {
            sim.step(1.0 / 60.0, &input, &mut rng);
        }
        let mut frame = Frame::new(4, 4);
        let theme = Theme::default();
        sim.render(&mut frame, &theme);
    }

    #[test]
    fn resize_to_zero_does_not_panic() {
        let mut rng = Rng::new(5);
        let mut sim = make(320, 96, 5);
        sim.resize(0, 0, &mut rng);
        let input = Input::default();
        sim.step(1.0 / 60.0, &input, &mut rng);
    }

    #[test]
    fn flock_heading_keeps_drifting_instead_of_locking_onto_one_direction() {
        // Regression test for a real bug: alignment + cohesion alone are a
        // classic recipe for the whole flock synchronizing onto a single
        // heading and cruising that way forever once settled, since
        // nothing then perturbs it. Run long enough to reach that settled
        // state, then keep going and confirm the flock's average heading
        // actually keeps changing rather than staying locked.
        let mut rng = Rng::new(42);
        let mut sim = make(320, 96, 42);
        let input = Input::default();

        fn mean_heading(sim: &Swarm) -> f32 {
            let (mut sx, mut sy) = (0.0f32, 0.0f32);
            for (vx, vy) in sim.velocities() {
                let speed = (vx * vx + vy * vy).sqrt().max(1e-6);
                sx += vx / speed;
                sy += vy / speed;
            }
            sy.atan2(sx)
        }

        // Let it settle into whatever steady flocking pattern it's going to.
        for _ in 0..3000 {
            sim.step(1.0 / 60.0, &input, &mut rng);
        }

        let mut headings = Vec::new();
        for _ in 0..6 {
            for _ in 0..600 {
                sim.step(1.0 / 60.0, &input, &mut rng);
            }
            headings.push(mean_heading(&sim));
        }

        // If the flock were permanently locked onto one direction, every
        // sampled heading would sit within a hair of the first one. Assert
        // at least one later sample has drifted meaningfully away.
        let first = headings[0];
        let drifted = headings[1..].iter().any(|&h| {
            let mut d = (h - first).abs();
            if d > std::f32::consts::PI {
                d = std::f32::consts::TAU - d;
            }
            d > 0.35 // ~20 degrees
        });
        assert!(
            drifted,
            "flock heading never drifted away from its initial settled direction: {headings:?}"
        );
    }
}
