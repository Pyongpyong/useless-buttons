//! `spirograph` — a toy Spirograph at work. A gear rolls around inside a
//! fixed ring while a pen in the gear traces a hypotrochoid; once a curve
//! closes, a new one starts with a different gear, pen hole and ink.
//! Finished curves slowly fade from the paper. Wide buttons get several
//! drawing side by side.
use super::{Input, Sim};
use crate::paint::{Frame, Rgb, Theme};
use crate::rng::Rng;
use std::f32::consts::TAU;

/// Fixed ring radius, in units (frame height = 1).
const RING_R: f32 = 0.42;
/// Gear-to-ring tooth ratios (p/q): a curve closes after p trips around.
const RATIOS: [(u32, u32); 8] = [
    (3, 7),
    (2, 5),
    (5, 12),
    (4, 9),
    (3, 8),
    (5, 13),
    (2, 7),
    (4, 11),
];
/// Seconds to draw one whole curve.
const CURVE_SEC: f32 = 8.0;
/// Fraction of the way to blank paper per 1/60 s.
const PAPER_FADE: f32 = 0.006;
const PAPER: Rgb = Rgb::new(250, 246, 236);
const PEN_PX: f32 = 2.2;

struct Spiro {
    cx: f32,
    r: f32,
    pen: f32,
    turns: u32,
    t: f32,
    hue: f32,
}

impl Spiro {
    fn point(&self, t: f32) -> (f32, f32) {
        let k = (RING_R - self.r) / self.r;
        (
            self.cx + (RING_R - self.r) * t.cos() + self.pen * (k * t).cos(),
            0.5 + (RING_R - self.r) * t.sin() - self.pen * (k * t).sin(),
        )
    }

    fn end(&self) -> f32 {
        TAU * self.turns as f32
    }
}

pub struct Spirograph {
    w: f32,
    h: f32,
    paper: Frame,
    spiros: Vec<Spiro>,
}

impl Spirograph {
    pub fn new(w: usize, h: usize, rng: &mut Rng) -> Self {
        let mut s = Self {
            w: 1.0,
            h: 1.0,
            paper: Frame::new(1, 1),
            spiros: Vec::new(),
        };
        s.resize(w, h, rng);
        s
    }

    fn aspect(&self) -> f32 {
        self.w / self.h
    }

    fn fresh(cx: f32, rng: &mut Rng) -> Spiro {
        let (p, q) = RATIOS[rng.range_usize(0, RATIOS.len())];
        let r = RING_R * p as f32 / q as f32;
        Spiro {
            cx,
            r,
            pen: r * rng.range_f32(0.55, 1.2),
            turns: p,
            t: 0.0,
            hue: rng.range_f32(0.0, 360.0),
        }
    }
}

impl Sim for Spirograph {
    fn resize(&mut self, w: usize, h: usize, rng: &mut Rng) {
        self.w = w.max(1) as f32;
        self.h = h.max(1) as f32;
        self.paper = Frame::new(w, h);
        self.paper.fill(PAPER);
        let aspect = self.aspect();
        let n = ((aspect / 1.6).round() as usize).clamp(1, 3);
        self.spiros = (0..n)
            .map(|i| Self::fresh(aspect * (i as f32 + 0.5) / n as f32, rng))
            .collect();
    }

    fn step(&mut self, dt: f32, _: &Input, rng: &mut Rng) {
        let dt = if dt.is_finite() {
            dt.clamp(0.0, 0.1)
        } else {
            0.0
        };
        let u = self.paper.h as f32;
        self.paper.fade_to(PAPER, PAPER_FADE * dt * 60.0);
        for s in &mut self.spiros {
            let rate = s.end() / CURVE_SEC;
            let steps = ((rate * dt) / 0.02).ceil().max(1.0) as usize;
            for _ in 0..steps {
                let (x0, y0) = s.point(s.t);
                s.t = (s.t + rate * dt / steps as f32).min(s.end());
                let (x1, y1) = s.point(s.t);
                let ink = Rgb::from_hsv(s.hue + s.t * 4.0, 0.8, 0.7);
                self.paper
                    .line((x0 * u, y0 * u), (x1 * u, y1 * u), PEN_PX, ink, 0.9);
            }
            if s.t >= s.end() {
                *s = Self::fresh(s.cx, rng);
            }
        }
    }

    fn render(&mut self, frame: &mut Frame, _: &Theme) {
        if frame.pixels.len() == self.paper.pixels.len() {
            frame.pixels.copy_from_slice(&self.paper.pixels);
        } else {
            frame.fill(PAPER);
        }
        let u = frame.h.max(1) as f32;
        for s in &self.spiros {
            // The fixed ring, the rolling gear and the pen in it.
            super::games::ring(
                frame,
                s.cx * u,
                0.5 * u,
                RING_R * u,
                1.0,
                Rgb::new(120, 110, 100),
                0.25,
            );
            let gear = (
                s.cx + (RING_R - s.r) * s.t.cos(),
                0.5 + (RING_R - s.r) * s.t.sin(),
            );
            super::games::ring(
                frame,
                gear.0 * u,
                gear.1 * u,
                s.r * u,
                1.0,
                Rgb::new(90, 120, 200),
                0.45,
            );
            let (px, py) = s.point(s.t);
            frame.line(
                (gear.0 * u, gear.1 * u),
                (px * u, py * u),
                1.0,
                Rgb::new(90, 120, 200),
                0.45,
            );
            frame.disc(
                px * u,
                py * u,
                (0.012 * u).max(1.2),
                Rgb::from_hsv(s.hue + s.t * 4.0, 0.8, 0.6),
                1.0,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curves_close_and_a_new_one_starts() {
        let mut rng = Rng::new(1);
        let mut sim = Spirograph::new(320, 96, &mut rng);
        let s = &sim.spiros[0];
        let (a, b) = (s.point(0.0), s.point(s.end()));
        assert!(
            (a.0 - b.0).abs() < 1e-3 && (a.1 - b.1).abs() < 1e-3,
            "curve doesn't close"
        );
        let first_hue = sim.spiros[0].hue;
        for _ in 0..(60.0 * (CURVE_SEC + 0.5)) as usize {
            sim.step(1.0 / 60.0, &Input::default(), &mut rng);
        }
        assert_ne!(sim.spiros[0].hue, first_hue);
    }

    #[test]
    fn the_pen_stays_inside_the_frame() {
        let mut rng = Rng::new(2);
        for _ in 0..50 {
            let s = Spirograph::fresh(1.0, &mut rng);
            for k in 0..500 {
                let (_, y) = s.point(s.end() * k as f32 / 500.0);
                assert!((0.0..=1.0).contains(&y), "pen left the paper at y {y}");
            }
        }
    }

    #[test]
    fn renders_opaque_changing_frames_at_odd_and_tiny_sizes() {
        let mut rng = Rng::new(3);
        let mut sim = Spirograph::new(320, 96, &mut rng);
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
