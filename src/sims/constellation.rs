//! `constellation` — the classic particle network. Bright points drift
//! across a dark sky; any two closer than a set distance are joined by a
//! line that grows fainter as they separate and vanishes when they get
//! too far apart, so constellations form and break up as they wander.
use super::{Input, Sim};
use crate::paint::{Frame, Rgb, Theme};
use crate::rng::Rng;
use std::f32::consts::TAU;

/// Stars per unit of area (frame height = 1).
const STARS_PER_AREA: f32 = 16.0;
const SPEED_MIN: f32 = 0.03;
const SPEED_MAX: f32 = 0.09;
/// Stars closer than this (units) are joined by a line.
const LINK: f32 = 0.3;
const STAR: Rgb = Rgb::new(220, 235, 255);
const LINE: Rgb = Rgb::new(140, 190, 255);

struct Star {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    size: f32,
    twinkle: f32,
}

pub struct Constellation {
    w: f32,
    h: f32,
    stars: Vec<Star>,
    t: f32,
}

impl Constellation {
    pub fn new(w: usize, h: usize, rng: &mut Rng) -> Self {
        let mut s = Self { w: 1.0, h: 1.0, stars: Vec::new(), t: 0.0 };
        s.resize(w, h, rng);
        s
    }

    fn aspect(&self) -> f32 {
        self.w / self.h
    }

    /// Link strength between two stars: 1 when touching, 0 at `LINK` and beyond.
    fn link(a: &Star, b: &Star) -> f32 {
        (1.0 - (a.x - b.x).hypot(a.y - b.y) / LINK).max(0.0)
    }
}

impl Sim for Constellation {
    fn resize(&mut self, w: usize, h: usize, rng: &mut Rng) {
        self.w = w.max(1) as f32;
        self.h = h.max(1) as f32;
        let aspect = self.aspect();
        let n = ((aspect * STARS_PER_AREA) as usize).clamp(8, 150);
        self.stars = (0..n)
            .map(|_| {
                let (a, v) = (rng.range_f32(0.0, TAU), rng.range_f32(SPEED_MIN, SPEED_MAX));
                Star {
                    x: rng.range_f32(0.0, aspect),
                    y: rng.range_f32(0.0, 1.0),
                    vx: a.cos() * v,
                    vy: a.sin() * v,
                    size: rng.range_f32(0.008, 0.016),
                    twinkle: rng.range_f32(0.0, TAU),
                }
            })
            .collect();
    }

    fn step(&mut self, dt: f32, _: &Input, _: &mut Rng) {
        let dt = if dt.is_finite() { dt.clamp(0.0, 0.1) } else { 0.0 };
        self.t = (self.t + dt) % 10_000.0;
        let aspect = self.aspect();
        for s in &mut self.stars {
            s.x += s.vx * dt;
            s.y += s.vy * dt;
            // Bounce off the edges so the sky stays evenly populated.
            if s.x < 0.0 || s.x > aspect {
                s.vx = -s.vx;
                s.x = s.x.clamp(0.0, aspect);
            }
            if s.y < 0.0 || s.y > 1.0 {
                s.vy = -s.vy;
                s.y = s.y.clamp(0.0, 1.0);
            }
            s.twinkle += dt * 2.0;
        }
    }

    fn render(&mut self, frame: &mut Frame, _: &Theme) {
        let u = frame.h.max(1) as f32;
        let (top, bottom) = (Rgb::new(6, 8, 24), Rgb::new(14, 12, 34));
        let denom = (frame.h.max(2) - 1) as f32;
        for y in 0..frame.h {
            let c = top.lerp(bottom, y as f32 / denom);
            let row = y * frame.w * 4;
            for px in frame.pixels[row..row + frame.w * 4].chunks_exact_mut(4) {
                px.copy_from_slice(&[c.r, c.g, c.b, 255]);
            }
        }
        for i in 0..self.stars.len() {
            for j in i + 1..self.stars.len() {
                let (a, b) = (&self.stars[i], &self.stars[j]);
                let k = Self::link(a, b);
                if k > 0.0 {
                    frame.line((a.x * u, a.y * u), (b.x * u, b.y * u), 1.0, LINE, k * 0.8);
                }
            }
        }
        for s in &self.stars {
            let tw = 0.7 + 0.3 * s.twinkle.sin();
            frame.disc(s.x * u, s.y * u, s.size * 2.2 * u, LINE, 0.15 * tw);
            frame.disc(s.x * u, s.y * u, (s.size * u).max(0.9), STAR, tw);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn star(x: f32, y: f32) -> Star {
        Star { x, y, vx: 0.0, vy: 0.0, size: 0.01, twinkle: 0.0 }
    }

    #[test]
    fn lines_fade_with_distance_and_vanish_past_the_link_range() {
        let a = star(0.0, 0.0);
        assert!(Constellation::link(&a, &star(0.05, 0.0)) > Constellation::link(&a, &star(0.2, 0.0)));
        assert_eq!(Constellation::link(&a, &star(LINK + 0.01, 0.0)), 0.0);
    }

    #[test]
    fn stars_keep_moving_and_stay_in_the_sky() {
        let mut rng = Rng::new(1);
        let mut sim = Constellation::new(320, 96, &mut rng);
        let x0: Vec<f32> = sim.stars.iter().map(|s| s.x).collect();
        for _ in 0..60 * 60 {
            sim.step(1.0 / 60.0, &Input::default(), &mut rng);
            for s in &sim.stars {
                assert!((0.0..=sim.aspect()).contains(&s.x) && (0.0..=1.0).contains(&s.y));
            }
        }
        assert!(sim.stars.iter().zip(&x0).any(|(s, &x)| (s.x - x).abs() > 0.05));
    }

    #[test]
    fn renders_opaque_changing_frames_at_odd_and_tiny_sizes() {
        let mut rng = Rng::new(2);
        let mut sim = Constellation::new(320, 96, &mut rng);
        for (w, h) in [(321, 97), (1, 1), (0, 0), (4, 7)] {
            sim.resize(w, h, &mut rng);
            for dt in [f32::NAN, -1.0, 1000.0, 0.1] {
                sim.step(dt, &Input::default(), &mut rng);
            }
            let mut frame = Frame::new(w, h);
            sim.render(&mut frame, &Theme::default());
            assert!(frame.pixels.chunks_exact(4).all(|p| p[3] == 255));
        }
        sim.resize(64, 48, &mut rng);
        let mut frame = Frame::new(64, 48);
        sim.render(&mut frame, &Theme::default());
        let before = frame.pixels.clone();
        sim.step(0.1, &Input::default(), &mut rng);
        sim.render(&mut frame, &Theme::default());
        assert_ne!(before, frame.pixels);
    }
}
