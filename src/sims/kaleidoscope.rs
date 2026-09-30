//! `kaleidoscope` — a row of kaleidoscope rosettes. Each pixel's angle
//! around its rosette's center is folded into one mirrored wedge, and
//! that wedge shows a few colored glass chips drifting on their own
//! paths: every pixel takes the color of the nearest chip, with dark lead
//! lines where two chips meet. The mirror turns slowly and the colors
//! cycle. Computed at half resolution.
use super::{Input, Sim};
use crate::paint::{Frame, Rgb, Theme};
use crate::rng::Rng;
use std::f32::consts::{PI, TAU};

/// Mirror wedges per rosette (even, so the pattern is fully symmetric).
const WEDGES: f32 = 8.0;
const CHIPS: usize = 9;
/// Rosettes sit one per unit of width.
const ROSETTE: f32 = 1.0;
const TURN_SPEED: f32 = 0.25;
const BLOCK: usize = 2;
/// Chip boundaries: where the nearest two chips are this close, draw lead.
const LEAD: f32 = 0.012;

struct Chip {
    /// Orbit center, radius, speed and phase, in wedge space.
    ax: f32,
    ay: f32,
    rx: f32,
    ry: f32,
    speed: f32,
    phase: f32,
    hue: f32,
}

pub struct Kaleidoscope {
    w: f32,
    h: f32,
    chips: Vec<Chip>,
    t: f32,
}

impl Kaleidoscope {
    pub fn new(w: usize, h: usize, rng: &mut Rng) -> Self {
        // Chips orbit inside the wedge itself (angles 0 to half a wedge),
        // or most of them would never be seen.
        let half = (TAU / WEDGES * 0.5).tan();
        let chips = (0..CHIPS)
            .map(|i| {
                let ax = rng.range_f32(0.06, 0.46);
                Chip {
                    ax,
                    ay: rng.range_f32(0.0, ax * half),
                    rx: rng.range_f32(0.04, 0.12),
                    ry: rng.range_f32(0.03, 0.09),
                    speed: rng.range_f32(0.4, 1.0) * if i % 2 == 0 { 1.0 } else { -1.0 },
                    phase: rng.range_f32(0.0, TAU),
                    hue: i as f32 * 360.0 / CHIPS as f32 + rng.range_f32(-15.0, 15.0),
                }
            })
            .collect();
        Self {
            w: w.max(1) as f32,
            h: h.max(1) as f32,
            chips,
            t: 0.0,
        }
    }

    fn chip_pos(&self, c: &Chip) -> (f32, f32) {
        let a = self.t * c.speed + c.phase;
        (c.ax + c.rx * a.cos(), c.ay + c.ry * (a * 1.3).sin())
    }

    /// Color of a point, given in units relative to the frame.
    fn color_at(&self, x: f32, y: f32, chips: &[(f32, f32, f32)]) -> Rgb {
        // Which rosette, and where in it.
        let index = (x / ROSETTE).floor();
        let cx = (index + 0.5) * ROSETTE;
        let (dx, dy) = (x - cx, y - 0.5);
        let r = dx.hypot(dy);
        let seg = TAU / WEDGES;
        // Neighboring rosettes turn opposite ways from different starting angles.
        let dir = if index as i64 % 2 == 0 { 1.0 } else { -1.0 };
        let mut a = (dy.atan2(dx) + dir * self.t * TURN_SPEED + index * 0.7).rem_euclid(seg);
        if a > seg * 0.5 {
            a = seg - a; // mirror every other wedge
        }
        let (px, py) = (r * a.cos(), r * a.sin());
        let (mut best, mut second, mut hue) = (f32::INFINITY, f32::INFINITY, 0.0);
        for &(cx, cy, h) in chips {
            let d = (px - cx).hypot(py - cy);
            if d < best {
                second = best;
                best = d;
                hue = h;
            } else if d < second {
                second = d;
            }
        }
        if second - best < LEAD {
            return Rgb::new(20, 16, 30);
        }
        // Brighter toward the middle of each chip, dimmer at the rosette's rim.
        let rim = (1.0 - (r / 0.5).powi(4)).clamp(0.25, 1.0);
        Rgb::from_hsv(
            hue + self.t * 12.0,
            0.75,
            (0.55 + 0.45 * (1.0 - best * 6.0).max(0.0)) * rim,
        )
    }
}

impl Sim for Kaleidoscope {
    fn resize(&mut self, w: usize, h: usize, _: &mut Rng) {
        self.w = w.max(1) as f32;
        self.h = h.max(1) as f32;
    }

    fn step(&mut self, dt: f32, _: &Input, _: &mut Rng) {
        let dt = if dt.is_finite() {
            dt.clamp(0.0, 0.1)
        } else {
            0.0
        };
        self.t = (self.t + dt) % (1000.0 * PI);
    }

    fn render(&mut self, frame: &mut Frame, _: &Theme) {
        let u = frame.h.max(1) as f32;
        let chips: Vec<(f32, f32, f32)> = self
            .chips
            .iter()
            .map(|c| {
                let (x, y) = self.chip_pos(c);
                (x, y, c.hue)
            })
            .collect();
        let mut by = 0;
        while by < frame.h {
            let mut bx = 0;
            while bx < frame.w {
                let c = self.color_at((bx as f32 + 1.0) / u, (by as f32 + 1.0) / u, &chips);
                for py in by..(by + BLOCK).min(frame.h) {
                    for px in bx..(bx + BLOCK).min(frame.w) {
                        let i = (py * frame.w + px) * 4;
                        frame.pixels[i..i + 4].copy_from_slice(&[c.r, c.g, c.b, 255]);
                    }
                }
                bx += BLOCK;
            }
            by += BLOCK;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chips(sim: &Kaleidoscope) -> Vec<(f32, f32, f32)> {
        sim.chips
            .iter()
            .map(|c| {
                let (x, y) = sim.chip_pos(c);
                (x, y, c.hue)
            })
            .collect()
    }

    #[test]
    fn every_rosette_is_mirror_symmetric() {
        let mut rng = Rng::new(1);
        let sim = Kaleidoscope::new(320, 96, &mut rng);
        let ch = chips(&sim);
        // At t = 0, reflecting a point across a wedge's mirror axis
        // (horizontal, through the rosette's center) gives the same color.
        for k in 0..200 {
            let (dx, dy) = ((k as f32 * 0.37).sin() * 0.4, (k as f32 * 0.53).cos() * 0.4);
            let a = sim.color_at(0.5 + dx, 0.5 + dy, &ch);
            let b = sim.color_at(0.5 + dx, 0.5 - dy, &ch);
            assert_eq!(a, b);
        }
    }

    #[test]
    fn renders_opaque_changing_frames_at_odd_and_tiny_sizes() {
        let mut rng = Rng::new(2);
        let mut sim = Kaleidoscope::new(320, 96, &mut rng);
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
