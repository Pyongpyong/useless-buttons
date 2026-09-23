//! `hyperdrive` — the jump to lightspeed, on a loop.
//!
//! A 3D starfield under perspective projection, with the camera falling
//! *away* from the stars rather than towards them: as a star's depth
//! grows, `x / z` shrinks, so it rushes inward and is swallowed by the
//! vanishing point. Each is drawn as the streak between where it was and
//! where it is, so the faster the drive spools up, the longer the
//! streaks stretch — the same trick the film shot uses.
//!
//! It runs as a cycle: a near-still starfield, a spool-up, a hard punch
//! into lightspeed, a white flash, then back to stillness. Standing at
//! full speed forever would lose the thing that makes the shot work,
//! which is the acceleration.

use super::{Input, Sim};
use crate::paint::{Frame, Rgb, Theme};
use crate::rng::Rng;
use std::f32::consts::TAU;

/// Deep space. Not `theme.paper` — the whole effect is light against
/// black.
const SPACE: Rgb = Rgb::new(0x05, 0x06, 0x12);
/// How hard each frame pulls the canvas back towards `SPACE`. Low, so
/// streaks smear across several frames into proper light trails rather
/// than being redrawn from nothing every time.
const FADE: f32 = 0.34;

const STARS_PER_1000_PX: f32 = 17.0;
const STARS_MIN: usize = 120;
const STARS_MAX: usize = 900;

/// Depth range a star lives in. Small `z` is right on top of the camera
/// (projected way out past the edges of the canvas); `Z_FAR` is the
/// vanishing point it gets pulled into.
const Z_NEAR: f32 = 0.18;
const Z_FAR: f32 = 9.0;
/// Multiplies `1 / z` into canvas px. Scaled by canvas size at runtime.
const PROJECTION: f32 = 0.55;

// --- The cycle. Seconds, in order: idle drift, spool-up, the punch
// itself, then the flash decaying back to idle.
const IDLE_SEC: f32 = 1.5;
const SPOOL_SEC: f32 = 1.6;
const PUNCH_SEC: f32 = 1.1;
const FLASH_SEC: f32 = 0.55;
const CYCLE_SEC: f32 = IDLE_SEC + SPOOL_SEC + PUNCH_SEC + FLASH_SEC;

/// Depth units per second at rest, and at full lightspeed.
const SPEED_IDLE: f32 = 0.22;
const SPEED_LIGHT: f32 = 26.0;
/// How many frames' worth of travel each streak is smeared over, at rest
/// and at full lightspeed.
///
/// It ramps rather than staying fixed because streak length otherwise
/// only grows in step with speed, and in the film the lines don't just
/// get faster — at the moment of the jump they stretch until they span
/// the whole frame. Multiplying the smear by the drive's own intensity
/// is what produces that.
const STREAK_FRAMES_IDLE: f32 = 1.2;
const STREAK_FRAMES_LIGHT: f32 = 7.0;
/// Longest streak drawn, as a multiple of the canvas diagonal. A star
/// passing close to the vanishing point can otherwise produce an
/// arbitrarily long line; this still leaves plenty of room to cross the
/// canvas end to end.
const STREAK_MAX_DIAGONALS: f32 = 3.0;

/// Hovering spools the drive up early; clicking punches it immediately.
const MAX_DT: f32 = 1.0 / 20.0;

#[derive(Clone, Copy)]
struct Star {
    /// Position on the plane perpendicular to travel, in world units.
    x: f32,
    y: f32,
    z: f32,
    /// Streaks are mostly white but tinted, so the tunnel isn't
    /// monochrome.
    hue: f32,
}

pub struct Hyperdrive {
    w: f32,
    h: f32,
    stars: Vec<Star>,
    /// Position within the cycle, in seconds.
    cycle_t: f32,
    /// Brightness of the lightspeed flash, decaying to 0.
    flash: f32,
    needs_clear: bool,
}

fn target_count(w: usize, h: usize) -> usize {
    (((w * h) as f32 / 1000.0) * STARS_PER_1000_PX) as usize
}

/// Speed of the drive at a point in the cycle, in depth units/sec.
///
/// Split out from `step` so the ramp can be tested directly: it's the
/// shape of this curve, not the starfield, that decides whether the
/// thing reads as a jump to lightspeed or just as fast-moving dots.
fn drive_speed(cycle_t: f32) -> f32 {
    if cycle_t < IDLE_SEC {
        SPEED_IDLE
    } else if cycle_t < IDLE_SEC + SPOOL_SEC {
        // Spool-up: ease *in*, so it creeps at first and the real
        // acceleration lands late. A linear ramp here reads as "already
        // moving fast" rather than as building up.
        let t = (cycle_t - IDLE_SEC) / SPOOL_SEC;
        SPEED_IDLE + (SPEED_LIGHT * 0.35 - SPEED_IDLE) * t * t * t
    } else if cycle_t < IDLE_SEC + SPOOL_SEC + PUNCH_SEC {
        // The punch: the rest of the way to lightspeed, fast.
        let t = (cycle_t - IDLE_SEC - SPOOL_SEC) / PUNCH_SEC;
        SPEED_LIGHT * 0.35 + (SPEED_LIGHT - SPEED_LIGHT * 0.35) * (1.0 - (1.0 - t) * (1.0 - t))
    } else {
        // Dropping back out: decay towards idle.
        let t = (cycle_t - IDLE_SEC - SPOOL_SEC - PUNCH_SEC) / FLASH_SEC;
        SPEED_LIGHT * (1.0 - t).max(0.0).powi(3) + SPEED_IDLE
    }
}

impl Hyperdrive {
    pub fn new(w: usize, h: usize, rng: &mut Rng) -> Self {
        let mut d = Hyperdrive {
            w: w.max(1) as f32,
            h: h.max(1) as f32,
            stars: Vec::new(),
            cycle_t: 0.0,
            flash: 0.0,
            needs_clear: true,
        };
        d.populate(rng);
        d
    }

    fn populate(&mut self, rng: &mut Rng) {
        let n = target_count(self.w as usize, self.h as usize).clamp(STARS_MIN, STARS_MAX);
        self.stars.clear();
        self.stars.reserve(n);
        for _ in 0..n {
            let mut s = Self::spawn_star(rng);
            // Spread the initial depths across the whole range instead of
            // starting them all at the camera, or the first jump is one
            // solid wall of streaks arriving together.
            s.z = rng.range_f32(Z_NEAR, Z_FAR);
            self.stars.push(s);
        }
    }

    fn spawn_star(rng: &mut Rng) -> Star {
        // Uniform in angle, biased outward in radius so stars don't pile
        // up on the axis (where they'd barely move on screen).
        let ang = rng.range_f32(0.0, TAU);
        let r = 0.25 + rng.next_f32().sqrt() * 1.35;
        Star {
            x: ang.cos() * r,
            y: ang.sin() * r,
            z: Z_NEAR,
            hue: rng.range_f32(196.0, 232.0),
        }
    }

    /// Projects a star at depth `z` onto the canvas. Returns `None` when
    /// it's behind or on the camera plane.
    fn project(&self, s: &Star, z: f32) -> Option<(f32, f32)> {
        if z <= 1e-3 {
            return None;
        }
        let scale = self.w.max(self.h) * PROJECTION / z;
        Some((self.w * 0.5 + s.x * scale, self.h * 0.5 + s.y * scale))
    }

    #[cfg(test)]
    pub(crate) fn star_count(&self) -> usize {
        self.stars.len()
    }

    #[cfg(test)]
    pub(crate) fn depths(&self) -> Vec<f32> {
        self.stars.iter().map(|s| s.z).collect()
    }

    #[cfg(test)]
    pub(crate) fn cycle_position(&self) -> f32 {
        self.cycle_t
    }
}

impl Sim for Hyperdrive {
    fn resize(&mut self, w: usize, h: usize, rng: &mut Rng) {
        let changed = (w.max(1) as f32) != self.w || (h.max(1) as f32) != self.h;
        self.w = w.max(1) as f32;
        self.h = h.max(1) as f32;
        if self.stars.len() != target_count(w, h).clamp(STARS_MIN, STARS_MAX) {
            self.populate(rng);
        }
        if changed {
            self.needs_clear = true;
        }
    }

    fn step(&mut self, dt: f32, input: &Input, rng: &mut Rng) {
        let dt = if dt.is_finite() { dt.clamp(0.0, MAX_DT) } else { 0.0 };
        if dt <= 0.0 || self.stars.is_empty() {
            return;
        }

        // Clicking punches the drive straight to the jump; hovering skips
        // the idle stretch so it spools up without waiting.
        if input.clicks > 0 {
            self.cycle_t = IDLE_SEC + SPOOL_SEC;
        } else if input.hover && self.cycle_t < IDLE_SEC {
            self.cycle_t = IDLE_SEC;
        }

        let before = self.cycle_t;
        self.cycle_t = (self.cycle_t + dt) % CYCLE_SEC;
        // Crossing into the flash stage lights it up.
        let flash_at = IDLE_SEC + SPOOL_SEC + PUNCH_SEC;
        if before < flash_at && (self.cycle_t >= flash_at || self.cycle_t < before) {
            self.flash = 1.0;
        }
        self.flash = (self.flash - dt / FLASH_SEC).max(0.0);

        let speed = drive_speed(self.cycle_t);
        for s in &mut self.stars {
            s.z += speed * dt;
            if s.z >= Z_FAR {
                // Swallowed by the vanishing point -- back to the camera.
                *s = Self::spawn_star(rng);
            }
        }
    }

    fn render(&mut self, frame: &mut Frame, theme: &Theme) {
        let _ = theme; // lightspeed is white on black, whatever the page's palette
        if self.needs_clear {
            frame.fill(SPACE);
            self.needs_clear = false;
        } else {
            frame.fade_to(SPACE, FADE);
        }

        let speed = drive_speed(self.cycle_t);
        // 0 at rest, 1 at lightspeed -- drives how long, how hot and how
        // thick the streaks are drawn.
        let intensity = ((speed - SPEED_IDLE) / (SPEED_LIGHT - SPEED_IDLE)).clamp(0.0, 1.0);
        // How far back along its own path each star gets smeared. Both
        // terms grow with the drive, so the stretch at the jump is far
        // more than linear in speed -- see `STREAK_FRAMES_IDLE`.
        let smear_frames = STREAK_FRAMES_IDLE + (STREAK_FRAMES_LIGHT - STREAK_FRAMES_IDLE) * intensity;
        let trail_z = speed * smear_frames * (1.0 / 60.0);
        let max_streak = (self.w * self.w + self.h * self.h).sqrt() * STREAK_MAX_DIAGONALS;

        for s in &self.stars {
            let Some((x1, y1)) = self.project(s, s.z) else { continue };
            let from_z = (s.z - trail_z).max(Z_NEAR * 0.5);
            let Some((x0, y0)) = self.project(s, from_z) else { continue };

            let dx = x1 - x0;
            let dy = y1 - y0;
            let len = (dx * dx + dy * dy).sqrt();
            if !len.is_finite() {
                continue;
            }
            // Trim from the tail rather than dropping the streak, so a
            // star passing near the vanishing point still draws the part
            // of its trail that matters.
            let (x0, y0) = if len > max_streak && len > 0.0 {
                let k = max_streak / len;
                (x1 - dx * k, y1 - dy * k)
            } else {
                (x0, y0)
            };

            // Nearer stars are bigger and brighter; everything gets
            // hotter and whiter as the drive spools up.
            let depth_t = 1.0 - ((s.z - Z_NEAR) / (Z_FAR - Z_NEAR)).clamp(0.0, 1.0);
            let sat = (0.55 - intensity * 0.5).max(0.0);
            let val = (0.45 + depth_t * 0.35 + intensity * 0.35).min(1.0);
            let color = Rgb::from_hsv(s.hue, sat, val);
            let thickness = 0.7 + depth_t * 0.9 + intensity * 0.5;

            // Drawn in three segments rather than one, so the trail can
            // fade from a faint tail to a bright head -- a single flat
            // line reads as a sliding bar instead of something being
            // drawn out at speed. `Frame::line` clips, so the portion
            // that runs off the canvas costs nothing.
            let head_alpha = 0.55 + 0.45 * intensity;
            const SEGMENTS: usize = 3;
            for i in 0..SEGMENTS {
                let t0 = i as f32 / SEGMENTS as f32;
                let t1 = (i + 1) as f32 / SEGMENTS as f32;
                let alpha = head_alpha * (0.25 + 0.75 * t1);
                frame.line(
                    (x0 + (x1 - x0) * t0, y0 + (y1 - y0) * t0),
                    (x0 + (x1 - x0) * t1, y0 + (y1 - y0) * t1),
                    thickness,
                    color,
                    alpha,
                );
            }
        }

        // The white-out as the jump lands.
        if self.flash > 0.0 {
            let f = self.flash * self.flash;
            frame.fade_to(Rgb::new(0xF2, 0xF6, 0xFF), f * 0.75);
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

    fn make(w: usize, h: usize, seed: u32) -> Hyperdrive {
        let mut rng = Rng::new(seed);
        Hyperdrive::new(w, h, &mut rng)
    }

    #[test]
    fn star_count_within_bounds_for_any_size() {
        for (w, h) in [(320, 96), (1, 1), (4, 4), (4000, 4000)] {
            let sim = make(w, h, 1);
            assert!(sim.star_count() >= STARS_MIN && sim.star_count() <= STARS_MAX);
        }
    }

    #[test]
    fn stars_are_pulled_into_the_vanishing_point() {
        // The whole effect: increasing depth means `x / z` shrinks, so a
        // star's projection marches towards the center of the canvas.
        let mut rng = Rng::new(2);
        let sim = make(320, 96, 2);
        let center = (160.0f32, 48.0f32);

        let star = Star { x: 1.0, y: 0.6, z: 1.0, hue: 210.0 };
        let mut last = f32::INFINITY;
        for step in 0..12 {
            let z = 1.0 + step as f32 * 0.5;
            let (x, y) = sim.project(&star, z).expect("in front of the camera");
            let d = ((x - center.0).powi(2) + (y - center.1).powi(2)).sqrt();
            assert!(d < last, "star moved away from the vanishing point at z={z}");
            last = d;
        }
        let _ = &mut rng;
    }

    #[test]
    fn drive_ramps_from_idle_to_lightspeed_and_back() {
        // Idle, then strictly building through the spool-up and punch,
        // then falling back. If this curve is wrong the shot reads as
        // "dots moving fast", not as a jump.
        assert!((drive_speed(0.0) - SPEED_IDLE).abs() < 1e-3);
        assert!((drive_speed(IDLE_SEC * 0.9) - SPEED_IDLE).abs() < 1e-3);

        let mut prev = drive_speed(IDLE_SEC);
        let mut t = IDLE_SEC;
        // Stay strictly inside the ramp: stepping *onto* the boundary
        // samples the decay phase, which is meant to be slower.
        while t + 0.05 < IDLE_SEC + SPOOL_SEC + PUNCH_SEC {
            t += 0.05;
            let now = drive_speed(t);
            assert!(now >= prev - 1e-3, "drive slowed down mid-jump at t={t}: {prev} -> {now}");
            prev = now;
        }
        let peak = drive_speed(IDLE_SEC + SPOOL_SEC + PUNCH_SEC - 1e-3);
        assert!(peak > SPEED_LIGHT * 0.9, "never actually reached lightspeed: {peak}");
        assert!(drive_speed(CYCLE_SEC - 1e-3) < peak * 0.5, "never dropped back out of the jump");
    }

    #[test]
    fn stars_recycle_instead_of_running_off_to_infinity() {
        let mut rng = Rng::new(3);
        let mut sim = make(320, 96, 3);
        for _ in 0..4000 {
            sim.step(1.0 / 60.0, &Input::default(), &mut rng);
            for z in sim.depths() {
                assert!(z.is_finite() && (Z_NEAR * 0.5..Z_FAR).contains(&z), "depth {z} escaped");
            }
        }
    }

    #[test]
    fn the_cycle_keeps_looping() {
        let mut rng = Rng::new(4);
        let mut sim = make(320, 96, 4);
        let mut seen_idle = false;
        let mut seen_light = false;
        for _ in 0..(CYCLE_SEC * 60.0 * 3.0) as usize {
            sim.step(1.0 / 60.0, &Input::default(), &mut rng);
            let speed = drive_speed(sim.cycle_position());
            if speed < SPEED_IDLE * 1.5 {
                seen_idle = true;
            }
            if speed > SPEED_LIGHT * 0.9 {
                seen_light = true;
            }
        }
        assert!(seen_idle && seen_light, "cycle never covered both ends (idle={seen_idle}, light={seen_light})");
    }

    #[test]
    fn a_click_punches_straight_to_the_jump() {
        let mut rng = Rng::new(5);
        let mut sim = make(320, 96, 5);
        let idle = drive_speed(sim.cycle_position());
        let input = Input { x: 0.0, y: 0.0, hover: false, down: false, clicks: 1 };
        sim.step(1.0 / 60.0, &input, &mut rng);
        let after = drive_speed(sim.cycle_position());
        assert!(after > idle * 10.0, "click didn't engage the drive: {idle} -> {after}");
    }

    #[test]
    fn large_dt_does_not_explode_or_panic() {
        let mut rng = Rng::new(6);
        let mut sim = make(320, 96, 6);
        for _ in 0..10 {
            sim.step(1.0, &Input::default(), &mut rng);
        }
        for z in sim.depths() {
            assert!(z.is_finite());
        }
    }

    #[test]
    fn renders_streaks_on_black_without_leaving_holes() {
        let mut rng = Rng::new(7);
        let mut sim = make(320, 96, 7);
        let mut frame = Frame::new(320, 96);
        let theme = Theme::default();
        // Run into the jump, where the streaks are longest.
        for _ in 0..((IDLE_SEC + SPOOL_SEC + PUNCH_SEC * 0.5) * 60.0) as usize {
            sim.step(1.0 / 60.0, &Input::default(), &mut rng);
            sim.render(&mut frame, &theme);
        }

        let mut lit = 0;
        for px in frame.pixels.chunks_exact(4) {
            assert_eq!(px[3], 255);
            let lum = px[0] as u32 + px[1] as u32 + px[2] as u32;
            if lum > 120 {
                lit += 1;
            }
        }
        assert!(lit > 200, "only {lit} lit pixels -- no visible streaks");
        assert!(lit < 320 * 96, "the whole canvas is lit -- that's a white-out, not a starfield");
    }

    #[test]
    fn streaks_fill_the_screen_at_the_jump() {
        // The point of the shot: at rest it's a sparse starfield, and at
        // the punch the streaks stretch until they cover most of the
        // frame. A version that only got *faster* without stretching
        // would pass every other test here and still look wrong.
        fn lit_fraction(sim: &mut Hyperdrive, rng: &mut Rng, until: f32) -> f32 {
            let mut frame = Frame::new(320, 96);
            let theme = Theme::default();
            let mut t = 0.0;
            while t < until {
                sim.step(1.0 / 60.0, &Input::default(), rng);
                sim.render(&mut frame, &theme);
                t += 1.0 / 60.0;
            }
            let lit = frame
                .pixels
                .chunks_exact(4)
                .filter(|px| px[0] as u32 + px[1] as u32 + px[2] as u32 > 150)
                .count();
            lit as f32 / (320.0 * 96.0)
        }

        let mut rng = Rng::new(11);
        let mut sim = make(320, 96, 11);
        let idle = lit_fraction(&mut sim, &mut rng, IDLE_SEC * 0.8);
        assert!(idle < 0.12, "idle starfield already covers {:.0}% of the frame", idle * 100.0);

        // Carry on to deep into the punch.
        let jump = lit_fraction(&mut sim, &mut rng, IDLE_SEC + SPOOL_SEC + PUNCH_SEC * 0.95 - IDLE_SEC * 0.8);
        assert!(
            jump > 0.35,
            "at lightspeed the streaks only cover {:.0}% of the frame -- they aren't stretching",
            jump * 100.0
        );
        assert!(jump > idle * 4.0, "the jump barely differs from the idle field");
    }

    #[test]
    fn rendering_the_jump_stays_well_inside_a_frame_budget() {
        // The streaks are long enough to run far off-canvas, and there
        // are hundreds of them; this only stays cheap because
        // `Frame::line` clips before walking the segment.
        let mut rng = Rng::new(12);
        let mut sim = make(320, 96, 12);
        let mut frame = Frame::new(320, 96);
        let theme = Theme::default();

        let mut t = 0.0;
        let mut worst = std::time::Duration::ZERO;
        while t < CYCLE_SEC {
            sim.step(1.0 / 60.0, &Input::default(), &mut rng);
            let started = std::time::Instant::now();
            sim.render(&mut frame, &theme);
            worst = worst.max(started.elapsed());
            t += 1.0 / 60.0;
        }
        // Generous: a debug build is many times slower than the release
        // build that actually ships, so this is a blown-budget alarm,
        // not a benchmark.
        assert!(
            worst < std::time::Duration::from_millis(80),
            "worst frame took {worst:?}, which is nowhere near a 16.7ms budget even allowing for debug"
        );
    }

    #[test]
    fn resize_to_tiny_does_not_panic() {
        let mut rng = Rng::new(8);
        let mut sim = make(320, 96, 8);
        sim.resize(2, 2, &mut rng);
        for _ in 0..60 {
            sim.step(1.0 / 60.0, &Input::default(), &mut rng);
        }
        let mut frame = Frame::new(2, 2);
        sim.render(&mut frame, &Theme::default());
    }

    #[test]
    fn resize_to_zero_does_not_panic() {
        let mut rng = Rng::new(9);
        let mut sim = make(320, 96, 9);
        sim.resize(0, 0, &mut rng);
        sim.step(1.0 / 60.0, &Input::default(), &mut rng);
        let mut frame = Frame::new(1, 1);
        sim.render(&mut frame, &Theme::default());
    }
}

