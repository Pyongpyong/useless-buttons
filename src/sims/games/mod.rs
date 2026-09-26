//! Game variants: tiny arcade games that run on their own in the button's
//! background and keep its label locked until the player clears them.
//!
//! Every game follows the same loop. It starts by itself and plays on its
//! own; left alone it crashes within a few seconds, holds the game-over
//! screen for `OVER_SEC`, and restarts. Clearing `GOAL` obstacles is
//! terminal: `Sim::cleared` turns true and stays true, which is what the
//! JS side watches to reveal the label and let clicks through.
//!
//! All game state is kept in *units* of the frame's height (the button is
//! 1.0 tall and `w / h` wide), so difficulty doesn't depend on how many
//! device pixels the canvas happens to have.

pub mod crossy;
pub mod flappy;
pub mod runner;
pub mod timing;

use crate::paint::{Frame, Rgb};
use crate::rng::Rng;
use std::f32::consts::PI;

/// Obstacles (pipes, chasms, hits) needed to clear a game.
pub const GOAL: u32 = 10;
/// How long the game-over screen holds before the run restarts.
pub const OVER_SEC: f32 = 1.2;
/// Physics substep, so a throttled host (`fps="6"`) can't make a fast
/// object tunnel through a thin obstacle in one oversized step.
const SUBSTEP: f32 = 1.0 / 120.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Phase {
    Playing,
    /// Crashed; carries the seconds since the crash.
    Over(f32),
    /// Won; carries the seconds since winning. Never left again.
    Cleared(f32),
}

impl Phase {
    /// Advance the phase clock. Returns `true` once a game-over has held
    /// long enough and the run should restart.
    pub fn advance(&mut self, dt: f32) -> bool {
        match self {
            Phase::Playing => false,
            Phase::Over(t) => {
                *t += dt;
                *t >= OVER_SEC
            }
            Phase::Cleared(t) => {
                *t += dt;
                false
            }
        }
    }
}

/// Same contract as every other sim: `dt` may be NaN, negative or huge.
pub fn sanitize_dt(dt: f32) -> f32 {
    if dt.is_finite() {
        dt.clamp(0.0, 0.1)
    } else {
        0.0
    }
}

/// Split `dt` into equal substeps no longer than `SUBSTEP`.
pub fn substeps(dt: f32) -> (usize, f32) {
    let n = (dt / SUBSTEP).ceil().max(1.0) as usize;
    (n, dt / n as f32)
}

/// Pixels per unit for a frame.
pub fn unit(frame: &Frame) -> f32 {
    frame.h.max(1) as f32
}

/// Fill the whole frame with a vertical gradient.
pub fn vgradient(frame: &mut Frame, top: Rgb, bottom: Rgb) {
    let denom = (frame.h.max(2) - 1) as f32;
    for y in 0..frame.h {
        let c = top.lerp(bottom, y as f32 / denom);
        let row = y * frame.w * 4;
        for px in frame.pixels[row..row + frame.w * 4].chunks_exact_mut(4) {
            px[0] = c.r;
            px[1] = c.g;
            px[2] = c.b;
            px[3] = 255;
        }
    }
}

/// Paint column `x` from `y0` (in pixels, may be fractional or off-frame)
/// down to the bottom edge.
pub fn column_from(frame: &mut Frame, x: usize, y0: f32, c: Rgb) {
    if x >= frame.w || !y0.is_finite() {
        return;
    }
    let start = y0.max(0.0).ceil() as usize;
    for y in start..frame.h {
        let i = (y * frame.w + x) * 4;
        frame.pixels[i] = c.r;
        frame.pixels[i + 1] = c.g;
        frame.pixels[i + 2] = c.b;
    }
}

/// Anti-aliased ring (circle outline), clipped to the frame.
pub fn ring(frame: &mut Frame, cx: f32, cy: f32, r: f32, thickness: f32, c: Rgb, alpha: f32) {
    if r <= 0.0 || thickness <= 0.0 || !cx.is_finite() || !cy.is_finite() || !r.is_finite() {
        return;
    }
    let half = thickness * 0.5;
    let reach = r + half + 1.0;
    let x0 = (cx - reach).floor().max(0.0) as usize;
    let y0 = (cy - reach).floor().max(0.0) as usize;
    let x1 = ((cx + reach).ceil().max(0.0) as usize).min(frame.w);
    let y1 = ((cy + reach).ceil().max(0.0) as usize).min(frame.h);
    for py in y0..y1 {
        for px in x0..x1 {
            let dx = px as f32 + 0.5 - cx;
            let dy = py as f32 + 0.5 - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            let coverage = (half + 0.5 - (dist - r).abs()).clamp(0.0, 1.0);
            if coverage > 0.0 {
                frame.blend_pixel(px as i64, py as i64, c, alpha * coverage);
            }
        }
    }
}

/// The row of `GOAL` pips along the top edge showing how far the run got.
pub fn draw_progress(frame: &mut Frame, done: u32) {
    let u = unit(frame);
    let r = (u * 0.026).max(1.0);
    let gap = r * 2.9;
    let left = frame.w as f32 * 0.5 - gap * (GOAL - 1) as f32 * 0.5;
    let cy = u * 0.075;
    for i in 0..GOAL {
        let x = left + gap * i as f32;
        frame.disc(x, cy, r + (u * 0.01).max(1.0), Rgb::new(20, 20, 30), 0.45);
        if i < done {
            frame.disc(x, cy, r, Rgb::new(255, 214, 64), 1.0);
        } else {
            frame.disc(x, cy, r, Rgb::new(255, 255, 255), 0.4);
        }
    }
}

/// Red flash that fades into a steady tint while the game-over screen holds.
pub fn draw_game_over(frame: &mut Frame, age: f32) {
    let flash = (1.0 - age / 0.25).max(0.0);
    let (w, h) = (frame.w as f32, frame.h as f32);
    frame.rect(0.0, 0.0, w, h, Rgb::new(200, 20, 30), 0.28 + flash * 0.4);
}

/// Warm flash right after clearing, so the win registers even before the
/// label fades in.
pub fn draw_cleared_flash(frame: &mut Frame, age: f32) {
    let flash = (1.0 - age / 0.5).max(0.0);
    if flash > 0.0 {
        let (w, h) = (frame.w as f32, frame.h as f32);
        frame.rect(0.0, 0.0, w, h, Rgb::new(255, 240, 180), flash * 0.6);
    }
}

struct Bit {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    life: f32,
    size: f32,
    color: Rgb,
}

/// Celebration particles, in units.
#[derive(Default)]
pub struct Confetti {
    bits: Vec<Bit>,
}

impl Confetti {
    const MAX: usize = 400;

    pub fn burst(&mut self, x: f32, y: f32, n: usize, rng: &mut Rng) {
        for _ in 0..n {
            if self.bits.len() >= Self::MAX {
                return;
            }
            let angle = rng.range_f32(-PI * 0.95, -PI * 0.05);
            let speed = rng.range_f32(0.6, 1.9);
            self.bits.push(Bit {
                x,
                y,
                vx: angle.cos() * speed,
                vy: angle.sin() * speed,
                life: rng.range_f32(0.9, 1.8),
                size: rng.range_f32(0.025, 0.045),
                color: Rgb::from_hsv(rng.range_f32(0.0, 360.0), 0.75, 1.0),
            });
        }
    }

    pub fn step(&mut self, dt: f32) {
        let drag = (-dt * 1.5).exp();
        for b in &mut self.bits {
            b.vy += 2.2 * dt;
            b.vx *= drag;
            b.vy *= drag;
            b.x += b.vx * dt;
            b.y += b.vy * dt;
            b.life -= dt;
        }
        self.bits.retain(|b| b.life > 0.0 && b.y < 1.5);
    }

    pub fn render(&self, frame: &mut Frame) {
        let u = unit(frame);
        for b in &self.bits {
            let s = (b.size * u).max(1.0);
            frame.rect(b.x * u - s * 0.5, b.y * u - s * 0.5, s, s, b.color, (b.life * 2.0).min(1.0));
        }
    }

    pub fn clear(&mut self) {
        self.bits.clear();
    }

    pub fn len(&self) -> usize {
        self.bits.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bits.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn over_phase_restarts_after_hold_and_cleared_never_does() {
        let mut p = Phase::Over(0.0);
        assert!(!p.advance(OVER_SEC * 0.5));
        assert!(p.advance(OVER_SEC));
        let mut c = Phase::Cleared(0.0);
        for _ in 0..100 {
            assert!(!c.advance(1.0));
        }
    }

    #[test]
    fn substeps_cover_dt_exactly() {
        for dt in [0.0, 0.001, 1.0 / 60.0, 0.1] {
            let (n, h) = substeps(dt);
            assert!(n >= 1);
            assert!(h <= SUBSTEP + 1e-6);
            assert!((h * n as f32 - dt).abs() < 1e-5);
        }
    }

    #[test]
    fn confetti_is_capped_and_dies_out() {
        let mut rng = Rng::new(1);
        let mut c = Confetti::default();
        for _ in 0..50 {
            c.burst(0.5, 0.5, 40, &mut rng);
        }
        assert_eq!(c.len(), Confetti::MAX);
        for _ in 0..200 {
            c.step(0.05);
        }
        assert!(c.is_empty());
    }
}
