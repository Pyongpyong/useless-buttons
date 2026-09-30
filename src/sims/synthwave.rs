//! `synthwave` — an 80s retro sunset. A striped sun sinks behind
//! neon-rimmed mountains, stars twinkle in a violet sky, and a glowing
//! perspective grid rushes toward you across the ground.
use super::{Input, Sim};
use crate::paint::{Frame, Rgb, Theme};
use crate::rng::Rng;

const HORIZON: f32 = 0.58;
const SUN_R: f32 = 0.34;
/// How fast the grid rolls toward the viewer, in grid rows per second.
const GRID_SPEED: f32 = 1.2;
const GRID_ROWS: usize = 12;
/// Spacing of the grid's lines running off to the vanishing point, at the
/// bottom edge of the frame.
const GRID_COL: f32 = 0.22;
const NEON: Rgb = Rgb::new(255, 60, 200);
const CYAN: Rgb = Rgb::new(60, 230, 255);

pub struct Synthwave {
    w: f32,
    h: f32,
    stars: Vec<(f32, f32, f32)>,
    t: f32,
}

impl Synthwave {
    pub fn new(w: usize, h: usize, rng: &mut Rng) -> Self {
        let mut s = Self { w: 1.0, h: 1.0, stars: Vec::new(), t: 0.0 };
        s.resize(w, h, rng);
        s
    }

    fn aspect(&self) -> f32 {
        self.w / self.h
    }

    /// Mountain ridge height above the horizon at `x`: tall at the sides,
    /// low in the middle so the sun shows through.
    fn ridge(&self, x: f32) -> f32 {
        let mid = (x - self.aspect() * 0.5).abs();
        let side = ((mid - SUN_R * 0.6) / 0.4).clamp(0.0, 1.0);
        let peaks = ((x * 2.3 + 1.0).sin().abs() * 0.12 + (x * 6.1).sin().abs() * 0.05) * (0.3 + 0.7 * side);
        peaks
    }
}

impl Sim for Synthwave {
    fn resize(&mut self, w: usize, h: usize, rng: &mut Rng) {
        self.w = w.max(1) as f32;
        self.h = h.max(1) as f32;
        let n = ((self.aspect() * 25.0) as usize).clamp(10, 120);
        let aspect = self.aspect();
        self.stars = (0..n).map(|_| (rng.range_f32(0.0, aspect), rng.range_f32(0.0, HORIZON - 0.1), rng.range_f32(0.0, 6.3))).collect();
    }

    fn step(&mut self, dt: f32, _: &Input, _: &mut Rng) {
        let dt = if dt.is_finite() { dt.clamp(0.0, 0.1) } else { 0.0 };
        self.t = (self.t + dt) % 10_000.0;
    }

    fn render(&mut self, frame: &mut Frame, _: &Theme) {
        let u = frame.h.max(1) as f32;
        let (fw, fh) = (frame.w, frame.h);
        let horizon_px = HORIZON * u;
        let cx = self.aspect() * 0.5;
        let sun_cy = HORIZON - 0.02;
        // Sky, sun and ground, row by row.
        for py in 0..fh {
            let y = (py as f32 + 0.5) / u;
            let row = py * fw * 4;
            if y < HORIZON {
                let k = y / HORIZON;
                let sky = if k < 0.6 {
                    Rgb::new(12, 4, 40).lerp(Rgb::new(80, 16, 110), k / 0.6)
                } else {
                    Rgb::new(80, 16, 110).lerp(Rgb::new(220, 60, 120), (k - 0.6) / 0.4)
                };
                // The sun: yellow to pink, cut by stripes that widen toward the bottom
                // and slowly drift down.
                let dy = y - sun_cy;
                let half = (SUN_R * SUN_R - dy * dy).max(0.0).sqrt();
                let into = ((y - (sun_cy - SUN_R)) / SUN_R).clamp(0.0, 1.0);
                let stripe = ((y * 26.0 - self.t * 0.6).fract()) < (into - 0.45).max(0.0) * 0.9;
                let sun = Rgb::new(255, 230, 90).lerp(Rgb::new(255, 60, 150), into);
                for px in 0..fw {
                    let x = (px as f32 + 0.5) / u;
                    let c = if (x - cx).abs() < half && !stripe { sun } else { sky };
                    frame.pixels[row + px * 4..row + px * 4 + 4].copy_from_slice(&[c.r, c.g, c.b, 255]);
                }
            } else {
                let c = Rgb::new(30, 0, 50).lerp(Rgb::new(10, 0, 24), (y - HORIZON) / (1.0 - HORIZON));
                for px in frame.pixels[row..row + fw * 4].chunks_exact_mut(4) {
                    px.copy_from_slice(&[c.r, c.g, c.b, 255]);
                }
            }
        }
        for &(x, y, ph) in &self.stars {
            let tw = 0.4 + 0.6 * (self.t * 2.0 + ph).sin().abs();
            frame.rect(x * u, y * u, 1.0, 1.0, Rgb::new(255, 240, 255), tw);
        }
        // Mountains: dark fill with a neon rim.
        for px in 0..fw {
            let x = (px as f32 + 0.5) / u;
            let top = (HORIZON - self.ridge(x)) * u;
            frame.rect(px as f32, top, 1.0, horizon_px - top, Rgb::new(26, 6, 44), 1.0);
            frame.rect(px as f32, top, 1.0, 1.5, CYAN, 0.8);
        }
        // The grid: lines toward the vanishing point, and rows rushing in.
        let glow = |frame: &mut Frame, a: (f32, f32), b: (f32, f32), c: Rgb, alpha: f32| {
            frame.line(a, b, 3.0, c, alpha * 0.25);
            frame.line(a, b, 1.2, c, alpha);
        };
        let vp = (cx * u, horizon_px);
        let cols = ((self.aspect() * 2.5 / GRID_COL) as i32).max(2);
        for i in -cols..=cols {
            glow(frame, vp, ((cx + i as f32 * GRID_COL * 2.5) * u, fh as f32 + 2.0), NEON, 0.7);
        }
        let scroll = (self.t * GRID_SPEED).fract();
        for k in 0..GRID_ROWS {
            // Depth z from far to near; screen y from 1/z.
            let z = (GRID_ROWS - k) as f32 - scroll;
            let y = horizon_px + (1.0 - HORIZON) * u * (1.0 / z.max(0.2)).min(1.2);
            if y > fh as f32 + 2.0 {
                continue;
            }
            glow(frame, (0.0, y), (fw as f32, y), NEON, (0.3 + 0.7 / z.max(1.0)).min(1.0));
        }
        frame.rect(0.0, horizon_px - 0.5, fw as f32, 1.5, Rgb::new(255, 150, 220), 0.9);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mountains_stay_low_in_the_middle_for_the_sun() {
        let mut rng = Rng::new(1);
        let sim = Synthwave::new(320, 96, &mut rng);
        let mid = sim.ridge(sim.aspect() * 0.5);
        let side = (0..20).map(|i| sim.ridge(i as f32 * 0.02)).fold(0.0f32, f32::max);
        assert!(mid < 0.06 && side > mid);
    }

    #[test]
    fn renders_opaque_changing_frames_at_odd_and_tiny_sizes() {
        let mut rng = Rng::new(2);
        let mut sim = Synthwave::new(320, 96, &mut rng);
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
