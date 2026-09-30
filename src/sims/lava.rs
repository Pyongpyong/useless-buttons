//! `lava` — a lava lamp. Blobs of wax warm up at the bottom, float up,
//! cool off at the top and sink back down, merging and pinching apart as
//! they pass each other. The goo is a metaball field (each blob adds
//! r² / d², and the surface is where the sum crosses 1), shaded from hot
//! yellow at the core to magenta at the rim with a soft glow around it.
//! Computed at half resolution.
use super::{Input, Sim};
use crate::paint::{Frame, Rgb, Theme};
use crate::rng::Rng;

const BLOBS: usize = 7;
const R_MIN: f32 = 0.17;
const R_MAX: f32 = 0.28;
/// Blobs are taller than they are wide, like wax in a lamp.
const STRETCH_Y: f32 = 0.55;
/// Buoyancy: warm blobs rise, cool ones sink.
const LIFT: f32 = 0.35;
const DRAG: f32 = 1.4;
/// Heat gained per second at the very bottom, lost at the very top.
const HEAT_RATE: f32 = 0.6;
const BLOCK: usize = 2;

struct Blob {
    x: f32,
    y: f32,
    vy: f32,
    r: f32,
    /// -1 (cold, sinks) .. 1 (hot, rises).
    heat: f32,
    sway: f32,
}

pub struct Lava {
    w: f32,
    h: f32,
    blobs: Vec<Blob>,
    t: f32,
}

impl Lava {
    pub fn new(w: usize, h: usize, rng: &mut Rng) -> Self {
        let mut s = Self {
            w: 1.0,
            h: 1.0,
            blobs: Vec::new(),
            t: 0.0,
        };
        s.resize(w, h, rng);
        s
    }

    fn aspect(&self) -> f32 {
        self.w / self.h
    }

    /// The metaball field at `(x, y)`, in units.
    fn field(&self, x: f32, y: f32) -> f32 {
        let aspect = self.aspect();
        self.blobs
            .iter()
            .map(|b| {
                // Blobs sway sideways a little as they travel.
                let bx = (b.x + (self.t * 0.3 + b.sway).sin() * 0.08).rem_euclid(aspect.max(1e-3));
                let d2 = (x - bx).powi(2) + (y - b.y).powi(2) * STRETCH_Y;
                b.r * b.r / d2.max(1e-5)
            })
            .sum()
    }
}

impl Sim for Lava {
    fn resize(&mut self, w: usize, h: usize, rng: &mut Rng) {
        let fresh = self.blobs.is_empty();
        self.w = w.max(1) as f32;
        self.h = h.max(1) as f32;
        let aspect = self.aspect();
        if fresh {
            self.blobs = (0..BLOBS)
                .map(|i| Blob {
                    x: (i as f32 + 0.5) / BLOBS as f32 * aspect,
                    y: rng.range_f32(0.2, 0.9),
                    vy: 0.0,
                    r: rng.range_f32(R_MIN, R_MAX),
                    heat: rng.range_f32(-1.0, 1.0),
                    sway: rng.range_f32(0.0, 6.3),
                })
                .collect();
        } else {
            for b in &mut self.blobs {
                b.x = b.x.rem_euclid(aspect.max(1e-3));
            }
        }
    }

    fn step(&mut self, dt: f32, _: &Input, _: &mut Rng) {
        let dt = if dt.is_finite() {
            dt.clamp(0.0, 0.1)
        } else {
            0.0
        };
        self.t = (self.t + dt) % 10_000.0;
        for b in &mut self.blobs {
            // Warm near the bottom (y = 1), cool near the top (y = 0).
            b.heat = (b.heat + (b.y - 0.5) * 2.0 * HEAT_RATE * dt).clamp(-1.0, 1.0);
            b.vy += (-b.heat * LIFT - b.vy * DRAG) * dt;
            b.y += b.vy * dt;
            if b.y < b.r * 0.5 {
                b.y = b.r * 0.5;
                b.vy = b.vy.abs() * 0.3;
            } else if b.y > 1.0 - b.r * 0.5 {
                b.y = 1.0 - b.r * 0.5;
                b.vy = -b.vy.abs() * 0.3;
            }
        }
    }

    fn render(&mut self, frame: &mut Frame, _: &Theme) {
        let u = frame.h.max(1) as f32;
        let bg_top = Rgb::new(40, 10, 60);
        let bg_bottom = Rgb::new(90, 20, 70);
        let rim = Rgb::new(230, 40, 120);
        let core = Rgb::new(255, 200, 60);
        let mut by = 0;
        while by < frame.h {
            let mut bx = 0;
            while bx < frame.w {
                let (x, y) = ((bx as f32 + 1.0) / u, (by as f32 + 1.0) / u);
                let f = self.field(x, y);
                let bg = bg_top.lerp(bg_bottom, y);
                let c = if f >= 1.0 {
                    // Inside the wax: rim color at the surface, hot core deeper in.
                    rim.lerp(core, ((f - 1.0) / 2.5).clamp(0.0, 1.0))
                } else {
                    // Outside: a soft glow that falls off with the field.
                    bg.lerp(rim, (f * f * 0.45).clamp(0.0, 0.45))
                };
                let (w, h) = (BLOCK.min(frame.w - bx), BLOCK.min(frame.h - by));
                for py in by..by + h {
                    for px in bx..bx + w {
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

    #[test]
    fn blobs_rise_when_hot_and_sink_when_cold_and_stay_inside() {
        let mut rng = Rng::new(1);
        let mut sim = Lava::new(320, 96, &mut rng);
        let mut ys: Vec<Vec<f32>> = vec![Vec::new(); BLOBS];
        for _ in 0..60 * 60 {
            sim.step(1.0 / 60.0, &Input::default(), &mut rng);
            for (i, b) in sim.blobs.iter().enumerate() {
                assert!(b.y >= 0.0 && b.y <= 1.0 && b.vy.is_finite());
                ys[i].push(b.y);
            }
        }
        // Every blob should make real trips up and down, not sit still.
        for (i, y) in ys.iter().enumerate() {
            let (lo, hi) = y
                .iter()
                .fold((1.0f32, 0.0f32), |(lo, hi), &v| (lo.min(v), hi.max(v)));
            assert!(hi - lo > 0.3, "blob {i} only moved {:.2}", hi - lo);
        }
    }

    #[test]
    fn the_field_is_strong_inside_a_blob_and_weak_far_away() {
        let mut rng = Rng::new(2);
        let sim = Lava::new(320, 96, &mut rng);
        let b = &sim.blobs[0];
        let bx = (b.x + (b.sway).sin() * 0.08).rem_euclid(sim.aspect());
        assert!(sim.field(bx, b.y) > 1.0);
        assert!(
            sim.field(bx, b.y) > sim.field((bx + 1.5).rem_euclid(sim.aspect()), (b.y + 0.5) % 1.0)
        );
    }

    #[test]
    fn renders_opaque_changing_frames_at_odd_and_tiny_sizes() {
        let mut rng = Rng::new(3);
        let mut sim = Lava::new(320, 96, &mut rng);
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
        for _ in 0..30 {
            sim.step(1.0 / 30.0, &Input::default(), &mut rng);
        }
        sim.render(&mut frame, &Theme::default());
        assert_ne!(before, frame.pixels);
    }
}
