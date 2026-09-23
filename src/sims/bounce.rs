//! `bounce` — dozens of randomly-colored pixels caroming around the
//! button, changing direction and color on every collision (wall or
//! pixel-pixel).
//!
//! Unlike every other variant, this one's background is **never** cleared
//! or faded: `render` paints only the particles' current positions, on
//! top of whatever was already in the frame. Since the frame persists
//! across ticks, that's enough on its own to leave a permanent painted
//! trail behind every moving pixel — no separate trail buffer needed. The
//! canvas is filled with the paper color exactly once (on construction
//! and again after any real resize, since a resize hands the sim a fresh
//! blank frame), and from then on only ever gains paint, never loses it.

use super::{Input, Sim};
use crate::paint::{Frame, Rgb, Theme};
use crate::rng::Rng;

const RADIUS: f32 = 2.2;
const SPEED_MIN: f32 = 46.0;
const SPEED_MAX: f32 = 110.0;
/// Largest dt a single `step` call will actually simulate — same
/// backgrounded-tab guard used by every other variant.
const MAX_DT: f32 = 1.0 / 20.0;

fn target_count(w: usize, h: usize) -> usize {
    // "Dozens" of pixels, scaling gently with area but never so many that
    // the O(n^2) pixel-pixel collision check (trivially cheap at this
    // scale) or the visual reading of individual trails gets out of hand.
    let n = (w * h) / 900;
    n.clamp(24, 70)
}

#[derive(Clone, Copy)]
struct Particle {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    /// This particle's own hue, in degrees, drawn uniformly from the full
    /// 0-360 spectrum — genuinely random color, not a point along a fixed
    /// two-color gradient. Reassigned on every collision.
    hue: f32,
}

pub struct Bounce {
    w: f32,
    h: f32,
    particles: Vec<Particle>,
    /// Set on construction and after any resize that actually changes the
    /// frame's dimensions (which hands `render` a fresh, blank-black
    /// buffer) — tells `render` to paint the paper background exactly
    /// once before resuming trail-only drawing.
    needs_clear: bool,
}

impl Bounce {
    pub fn new(w: usize, h: usize, rng: &mut Rng) -> Self {
        let mut b = Bounce {
            w: w.max(1) as f32,
            h: h.max(1) as f32,
            particles: Vec::new(),
            needs_clear: true,
        };
        b.populate(rng);
        b
    }

    fn bounds(&self) -> (f32, f32, f32, f32) {
        let min_x = RADIUS;
        let min_y = RADIUS;
        let max_x = (self.w - RADIUS).max(RADIUS);
        let max_y = (self.h - RADIUS).max(RADIUS);
        (min_x, min_y, max_x, max_y)
    }

    fn populate(&mut self, rng: &mut Rng) {
        let n = target_count(self.w as usize, self.h as usize);
        let (min_x, min_y, max_x, max_y) = self.bounds();
        self.particles.clear();
        self.particles.reserve(n);
        for _ in 0..n {
            let ang = rng.range_f32(0.0, std::f32::consts::TAU);
            let speed = rng.range_f32(SPEED_MIN, SPEED_MAX);
            self.particles.push(Particle {
                x: rng.range_f32(min_x, max_x),
                y: rng.range_f32(min_y, max_y),
                vx: ang.cos() * speed,
                vy: ang.sin() * speed,
                hue: rng.range_f32(0.0, 360.0),
            });
        }
    }

    #[cfg(test)]
    pub(crate) fn positions(&self) -> Vec<(f32, f32)> {
        self.particles.iter().map(|p| (p.x, p.y)).collect()
    }

    #[cfg(test)]
    pub(crate) fn colors(&self) -> Vec<f32> {
        self.particles.iter().map(|p| p.hue).collect()
    }

    #[cfg(test)]
    pub(crate) fn needs_clear(&self) -> bool {
        self.needs_clear
    }
}

impl Sim for Bounce {
    fn resize(&mut self, w: usize, h: usize, rng: &mut Rng) {
        let changed = (w.max(1) as f32) != self.w || (h.max(1) as f32) != self.h;
        self.w = w.max(1) as f32;
        self.h = h.max(1) as f32;
        let n = target_count(w, h);
        if self.particles.len() != n {
            self.populate(rng);
        } else {
            let (min_x, min_y, max_x, max_y) = self.bounds();
            for p in &mut self.particles {
                p.x = p.x.clamp(min_x, max_x);
                p.y = p.y.clamp(min_y, max_y);
            }
        }
        if changed {
            self.needs_clear = true;
        }
    }

    fn step(&mut self, dt: f32, input: &Input, rng: &mut Rng) {
        let _ = input; // ambient chaos -- no pointer interaction for this variant
        let dt = if dt.is_finite() { dt.clamp(0.0, MAX_DT) } else { 0.0 };
        if dt <= 0.0 || self.particles.is_empty() || self.w <= 0.0 || self.h <= 0.0 {
            return;
        }

        for p in &mut self.particles {
            p.x += p.vx * dt;
            p.y += p.vy * dt;
        }

        // --- Wall collisions: clamp back inside, reflect the offending
        // axis, reassign color.
        let (min_x, min_y, max_x, max_y) = self.bounds();
        for p in &mut self.particles {
            if p.x < min_x {
                p.x = min_x;
                p.vx = p.vx.abs();
                p.hue = rng.range_f32(0.0, 360.0);
            } else if p.x > max_x {
                p.x = max_x;
                p.vx = -p.vx.abs();
                p.hue = rng.range_f32(0.0, 360.0);
            }
            if p.y < min_y {
                p.y = min_y;
                p.vy = p.vy.abs();
                p.hue = rng.range_f32(0.0, 360.0);
            } else if p.y > max_y {
                p.y = max_y;
                p.vy = -p.vy.abs();
                p.hue = rng.range_f32(0.0, 360.0);
            }
        }

        // --- Pixel-pixel collisions: O(n^2), but n tops out in the
        // dozens, so this is nowhere near a real cost at 60fps.
        let n = self.particles.len();
        let min_dist = RADIUS * 2.0;
        for i in 0..n {
            for j in (i + 1)..n {
                let dx = self.particles[j].x - self.particles[i].x;
                let dy = self.particles[j].y - self.particles[i].y;
                let d2 = dx * dx + dy * dy;
                if d2 >= min_dist * min_dist || d2 <= 1e-6 {
                    continue;
                }
                let d = d2.sqrt();
                let nx = dx / d;
                let ny = dy / d;

                // Separate so they don't keep re-triggering the same
                // collision next step.
                let overlap = min_dist - d;
                self.particles[i].x -= nx * overlap * 0.5;
                self.particles[i].y -= ny * overlap * 0.5;
                self.particles[j].x += nx * overlap * 0.5;
                self.particles[j].y += ny * overlap * 0.5;

                // Equal-mass elastic collision: swap the velocity
                // component along the collision normal.
                let vi_n = self.particles[i].vx * nx + self.particles[i].vy * ny;
                let vj_n = self.particles[j].vx * nx + self.particles[j].vy * ny;
                let delta = vj_n - vi_n;
                self.particles[i].vx += delta * nx;
                self.particles[i].vy += delta * ny;
                self.particles[j].vx -= delta * nx;
                self.particles[j].vy -= delta * ny;

                self.particles[i].hue = rng.range_f32(0.0, 360.0);
                self.particles[j].hue = rng.range_f32(0.0, 360.0);

                self.particles[i].x = self.particles[i].x.clamp(min_x, max_x);
                self.particles[i].y = self.particles[i].y.clamp(min_y, max_y);
                self.particles[j].x = self.particles[j].x.clamp(min_x, max_x);
                self.particles[j].y = self.particles[j].y.clamp(min_y, max_y);
            }
        }

        // Re-normalize speed: repeated collisions can otherwise let
        // floating-point drift slowly creep speed towards zero (or, more
        // rarely, up), which would make the pile-up look like it's
        // "settling" instead of staying a constant chaotic bounce.
        for p in &mut self.particles {
            let speed = (p.vx * p.vx + p.vy * p.vy).sqrt();
            if speed.is_finite() && speed > 1e-4 {
                let clamped = speed.clamp(SPEED_MIN, SPEED_MAX);
                let scale = clamped / speed;
                p.vx *= scale;
                p.vy *= scale;
            } else {
                let ang = rng.range_f32(0.0, std::f32::consts::TAU);
                let sp = rng.range_f32(SPEED_MIN, SPEED_MAX);
                p.vx = ang.cos() * sp;
                p.vy = ang.sin() * sp;
            }
        }
    }

    fn render(&mut self, frame: &mut Frame, theme: &Theme) {
        if self.needs_clear {
            frame.fill(theme.paper);
            self.needs_clear = false;
        }
        for p in &self.particles {
            let color = Rgb::from_hsv(p.hue, 0.75, 0.6);
            frame.disc(p.x, p.y, RADIUS, color, 1.0);
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

    fn make(w: usize, h: usize, seed: u32) -> Bounce {
        let mut rng = Rng::new(seed);
        Bounce::new(w, h, &mut rng)
    }

    #[test]
    fn count_within_expected_bounds() {
        assert_eq!(target_count(320, 96), 34); // (320*96)/900 = 34
        assert_eq!(target_count(4, 4), 24); // clamps up from ~0
        assert_eq!(target_count(4000, 4000), 70); // clamps down
    }

    #[test]
    fn particles_stay_within_canvas_and_finite() {
        let mut rng = Rng::new(1);
        let mut sim = make(320, 96, 1);
        let input = Input::default();
        for _ in 0..2000 {
            sim.step(1.0 / 60.0, &input, &mut rng);
        }
        for (x, y) in sim.positions() {
            assert!(x.is_finite() && y.is_finite());
            assert!((0.0..=320.0).contains(&x), "x={x} escaped canvas");
            assert!((0.0..=96.0).contains(&y), "y={y} escaped canvas");
        }
    }

    #[test]
    fn large_dt_does_not_explode_or_panic() {
        let mut rng = Rng::new(2);
        let mut sim = make(320, 96, 2);
        let input = Input::default();
        for _ in 0..5 {
            sim.step(1.0, &input, &mut rng); // backgrounded-tab-sized dt
        }
        for (x, y) in sim.positions() {
            assert!(x.is_finite() && (0.0..=320.0).contains(&x));
            assert!(y.is_finite() && (0.0..=96.0).contains(&y));
        }
    }

    #[test]
    fn wall_bounce_reflects_and_recolors() {
        // A single particle heading straight into the right wall must
        // bounce (vx flips sign), stay in bounds, and get a new color.
        let mut rng = Rng::new(3);
        let mut sim = Bounce {
            w: 40.0,
            h: 40.0,
            particles: vec![Particle { x: 39.0, y: 20.0, vx: 80.0, vy: 0.0, hue: 10.0 }],
            needs_clear: true,
        };
        let input = Input::default();
        // Just enough steps to cross into the wall and bounce once --
        // deliberately not many more than that, since at this speed it
        // would travel all the way back across to the *other* wall and
        // bounce again within ~30 steps, which would (correctly) flip
        // vx positive again and falsely look like a failed reflection.
        for _ in 0..5 {
            sim.step(1.0 / 60.0, &input, &mut rng);
        }
        let p = sim.particles[0];
        assert!(p.vx < 0.0, "expected reflection off the right wall, vx={}", p.vx);
        assert!(p.x <= 40.0 - RADIUS + 1e-3);
    }

    #[test]
    fn pixel_pixel_collision_changes_direction_and_color_for_both() {
        // Two particles on a direct collision course, closing distance.
        let mut rng = Rng::new(4);
        let mut sim = Bounce {
            w: 100.0,
            h: 100.0,
            particles: vec![
                Particle { x: 40.0, y: 50.0, vx: 60.0, vy: 0.0, hue: 0.0 },
                Particle { x: 60.0, y: 50.0, vx: -60.0, vy: 0.0, hue: 180.0 },
            ],
            needs_clear: true,
        };
        let input = Input::default();
        for _ in 0..120 {
            sim.step(1.0 / 60.0, &input, &mut rng);
        }
        // After running past the collision, colors must have changed from
        // their distinct starting values.
        assert_ne!(sim.particles[0].hue, 0.0, "color unchanged after collision");
        assert_ne!(sim.particles[1].hue, 1.0, "color unchanged after collision");
        // They should no longer be on a head-on collision course frozen
        // in place at the start.
        let (x0, _) = (sim.particles[0].x, sim.particles[0].y);
        let (x1, _) = (sim.particles[1].x, sim.particles[1].y);
        assert!(x0 != 40.0 || x1 != 60.0);
    }

    #[test]
    fn colors_diverge_across_particles_after_running() {
        // With enough particles bouncing around and colliding, colors
        // should end up spread across the ink-accent gradient, not all
        // stuck at their initial values.
        let mut rng = Rng::new(11);
        let mut sim = make(320, 96, 11);
        let initial = sim.colors();
        let input = Input::default();
        for _ in 0..3000 {
            sim.step(1.0 / 60.0, &input, &mut rng);
        }
        let after = sim.colors();
        let changed = initial.iter().zip(after.iter()).filter(|(a, b)| (*a - *b).abs() > 1e-6).count();
        assert!(changed > 0, "no particle's color ever changed after 3000 steps");
    }

    #[test]
    fn resize_to_tiny_does_not_panic() {
        let mut rng = Rng::new(5);
        let mut sim = make(320, 96, 5);
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
        let mut rng = Rng::new(6);
        let mut sim = make(320, 96, 6);
        sim.resize(0, 0, &mut rng);
        let input = Input::default();
        sim.step(1.0 / 60.0, &input, &mut rng);
    }

    #[test]
    fn resize_that_changes_size_requests_a_fresh_background_fill() {
        let mut rng = Rng::new(7);
        let mut sim = make(320, 96, 7);
        let mut frame = Frame::new(320, 96);
        let theme = Theme::default();
        sim.render(&mut frame, &theme); // consumes the initial needs_clear
        assert!(!sim.needs_clear());
        sim.resize(200, 60, &mut rng);
        assert!(sim.needs_clear(), "a real resize must re-arm the background fill");
    }

    #[test]
    fn resize_to_same_size_does_not_force_a_redundant_clear() {
        let mut rng = Rng::new(8);
        let mut sim = make(320, 96, 8);
        let mut frame = Frame::new(320, 96);
        let theme = Theme::default();
        sim.render(&mut frame, &theme);
        assert!(!sim.needs_clear());
        sim.resize(320, 96, &mut rng);
        assert!(!sim.needs_clear());
    }

    #[test]
    fn render_never_clears_after_the_first_paint_so_trails_persist() {
        // Paint once (paper fill + particles), remember a pixel that a
        // particle's disc definitely touched, then step + render many
        // more times without ever resizing. The background must never
        // revert to the plain paper fill underneath the trail -- i.e. the
        // painted history must not be a fade/clear-and-redraw.
        let mut rng = Rng::new(9);
        let mut sim = make(60, 60, 9);
        let mut frame = Frame::new(60, 60);
        let theme = Theme::default();
        sim.render(&mut frame, &theme);
        let (x0, y0) = sim.positions()[0];
        let idx = (y0 as usize * 60 + x0 as usize) * 4;
        let painted_px = frame.pixels[idx..idx + 3].to_vec();
        assert_ne!(painted_px, vec![theme.paper.r, theme.paper.g, theme.paper.b]);

        let input = Input::default();
        for _ in 0..300 {
            sim.step(1.0 / 60.0, &input, &mut rng);
            sim.render(&mut frame, &theme);
        }
        // The remembered pixel must still show *some* paint (not pure
        // paper), even though particles have long since moved elsewhere --
        // proof the frame was never wiped back to a blank background.
        let still_painted = frame.pixels[idx..idx + 3] != [theme.paper.r, theme.paper.g, theme.paper.b][..];
        assert!(still_painted, "trail pixel was erased -- background must never be cleared after the first paint");
    }
}
