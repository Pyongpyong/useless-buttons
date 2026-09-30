//! `snow` — a quiet snowfall at night. Flakes fall in three depths (the
//! nearer, the bigger, faster and brighter), swaying on their own and on a
//! slowly gusting wind. The ones that reach the ground pile up into a
//! drift that slumps smooth, then slowly settles, so it never buries the
//! button. A row of pines stands in the distance.
use super::{Input, Sim};
use crate::paint::{Frame, Rgb, Theme};
use crate::rng::Rng;

/// Flakes per unit of area (frame height = 1).
const FLAKES_PER_AREA: f32 = 90.0;
/// Size, fall speed and brightness for the far, middle and near layers.
const LAYERS: [(f32, f32, f32); 3] = [(0.006, 0.08, 0.45), (0.009, 0.13, 0.7), (0.014, 0.2, 1.0)];
/// Width of one column of the ground drift, in units.
const DRIFT_COL: f32 = 0.02;
/// Height one landed flake adds to its column (the slump spreads it out).
const DRIFT_PER_FLAKE: f32 = 0.02;
const DRIFT_MIN: f32 = 0.02;
const DRIFT_START: f32 = 0.06;
const DRIFT_MAX: f32 = 0.2;
/// The drift settles by this fraction of its height per second. Settling
/// in proportion to height balances the snowfall at a steady depth (about
/// 0.08 of the frame) whatever the button's width.
const DRIFT_SETTLE: f32 = 0.03;

struct Flake {
    x: f32,
    y: f32,
    layer: usize,
    phase: f32,
}

pub struct Snow {
    w: f32,
    h: f32,
    flakes: Vec<Flake>,
    drift: Vec<f32>,
    pines: Vec<(f32, f32)>,
    wind: f32,
    t: f32,
}

impl Snow {
    pub fn new(w: usize, h: usize, rng: &mut Rng) -> Self {
        let mut s = Self {
            w: 1.0,
            h: 1.0,
            flakes: Vec::new(),
            drift: Vec::new(),
            pines: Vec::new(),
            wind: 0.0,
            t: rng.range_f32(0.0, 100.0),
        };
        s.resize(w, h, rng);
        s
    }

    fn aspect(&self) -> f32 {
        self.w / self.h
    }

    fn drift_at(&self, x: f32) -> f32 {
        let i = ((x / DRIFT_COL) as isize).clamp(0, self.drift.len() as isize - 1) as usize;
        self.drift.get(i).copied().unwrap_or(0.0)
    }

    fn respawn(&self, f: &mut Flake, rng: &mut Rng) {
        f.x = rng.range_f32(-0.2, self.aspect() + 0.2);
        f.y = rng.range_f32(-0.1, -0.01);
        f.phase = rng.range_f32(0.0, 6.3);
    }
}

impl Sim for Snow {
    fn resize(&mut self, w: usize, h: usize, rng: &mut Rng) {
        self.w = w.max(1) as f32;
        self.h = h.max(1) as f32;
        let aspect = self.aspect();
        let n = ((aspect * FLAKES_PER_AREA) as usize).clamp(20, 900);
        self.flakes = (0..n)
            .map(|i| Flake {
                x: rng.range_f32(0.0, aspect),
                y: rng.range_f32(0.0, 1.0),
                layer: i % LAYERS.len(),
                phase: rng.range_f32(0.0, 6.3),
            })
            .collect();
        self.drift = vec![DRIFT_START; ((aspect / DRIFT_COL).ceil() as usize).max(1)];
        let mut x = rng.range_f32(-0.1, 0.1);
        self.pines.clear();
        while x < aspect + 0.2 {
            self.pines.push((x, rng.range_f32(0.18, 0.34)));
            x += rng.range_f32(0.12, 0.3);
        }
    }

    fn step(&mut self, dt: f32, _: &Input, rng: &mut Rng) {
        let dt = if dt.is_finite() {
            dt.clamp(0.0, 0.1)
        } else {
            0.0
        };
        self.t = (self.t + dt) % 10_000.0;
        // Wind drifts between calm and gusty.
        self.wind = (self.t * 0.23).sin() * 0.06 + (self.t * 0.071).sin() * 0.04;
        let wind = self.wind;
        let mut landed = Vec::new();
        for i in 0..self.flakes.len() {
            let f = &mut self.flakes[i];
            let (_, speed, _) = LAYERS[f.layer];
            f.phase += dt * 1.5;
            f.x += (wind * (0.5 + speed * 4.0) + f.phase.sin() * 0.02) * dt;
            f.y += speed * dt;
            let (x, y) = (f.x, f.y);
            if y >= 1.0 - self.drift_at(x) {
                landed.push(i);
            } else if x < -0.3 || x > self.aspect() + 0.3 {
                let mut f = std::mem::replace(
                    &mut self.flakes[i],
                    Flake {
                        x: 0.0,
                        y: 0.0,
                        layer: 0,
                        phase: 0.0,
                    },
                );
                self.respawn(&mut f, rng);
                self.flakes[i] = f;
            }
        }
        for i in landed {
            let x = self.flakes[i].x;
            if self.flakes[i].layer == LAYERS.len() - 1 {
                // Only the near layer is actually on the ground in front of us.
                let c = ((x / DRIFT_COL) as isize).clamp(0, self.drift.len() as isize - 1) as usize;
                self.drift[c] = (self.drift[c] + DRIFT_PER_FLAKE).min(DRIFT_MAX);
            }
            let mut f = std::mem::replace(
                &mut self.flakes[i],
                Flake {
                    x: 0.0,
                    y: 0.0,
                    layer: 0,
                    phase: 0.0,
                },
            );
            self.respawn(&mut f, rng);
            self.flakes[i] = f;
        }
        // Slump toward neighbors, then settle.
        let n = self.drift.len();
        if n > 1 {
            let prev = self.drift.clone();
            for i in 0..n {
                let l = prev[i.saturating_sub(1)];
                let r = prev[(i + 1).min(n - 1)];
                self.drift[i] += ((l + r) * 0.5 - prev[i]) * (dt * 4.0).min(1.0);
            }
        }
        for d in &mut self.drift {
            *d = (*d - *d * DRIFT_SETTLE * dt).max(DRIFT_MIN);
        }
    }

    fn render(&mut self, frame: &mut Frame, _: &Theme) {
        let u = frame.h.max(1) as f32;
        let (top, bottom) = (Rgb::new(8, 14, 36), Rgb::new(40, 56, 96));
        let denom = (frame.h.max(2) - 1) as f32;
        for y in 0..frame.h {
            let c = top.lerp(bottom, y as f32 / denom);
            let row = y * frame.w * 4;
            for px in frame.pixels[row..row + frame.w * 4].chunks_exact_mut(4) {
                px.copy_from_slice(&[c.r, c.g, c.b, 255]);
            }
        }
        // Distant pines: stacked triangles, drawn as narrowing rows.
        for &(x, hgt) in &self.pines {
            let base = 0.93 * u;
            let rows = (hgt * u).ceil().max(1.0) as usize;
            for r in 0..rows {
                let k = r as f32 / rows as f32;
                let half = (1.0 - k) * hgt * 0.35 * u * (0.7 + 0.3 * ((k * 3.0).fract()));
                frame.rect(
                    x * u - half,
                    base - r as f32,
                    half * 2.0,
                    1.0,
                    Rgb::new(20, 36, 50),
                    1.0,
                );
            }
        }
        for layer in 0..LAYERS.len() {
            let (size, _, bright) = LAYERS[layer];
            let c = Rgb::new(235, 242, 255);
            for f in self.flakes.iter().filter(|f| f.layer == layer) {
                frame.disc(f.x * u, f.y * u, (size * u).max(0.7), c, bright);
            }
        }
        // The drift, per pixel column, with a soft blue shadow on its crest.
        for px in 0..frame.w {
            let d = self.drift_at((px as f32 + 0.5) / u);
            let top = (1.0 - d) * u;
            frame.rect(
                px as f32,
                top,
                1.0,
                frame.h as f32 - top + 1.0,
                Rgb::new(236, 242, 252),
                1.0,
            );
            frame.rect(
                px as f32,
                top,
                1.0,
                (0.012 * u).max(1.0),
                Rgb::new(190, 210, 240),
                1.0,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flakes_fall_and_the_drift_stays_below_its_cap() {
        let mut rng = Rng::new(1);
        let mut sim = Snow::new(320, 96, &mut rng);
        let y0: f32 = sim.flakes.iter().map(|f| f.y).sum();
        sim.step(0.05, &Input::default(), &mut rng);
        let y1: f32 = sim.flakes.iter().map(|f| f.y).sum();
        assert!(y1 != y0);
        for _ in 0..60 * 120 {
            sim.step(1.0 / 60.0, &Input::default(), &mut rng);
        }
        assert!(sim.drift.iter().all(|&d| (0.0..=DRIFT_MAX).contains(&d)));
        let avg = sim.drift.iter().sum::<f32>() / sim.drift.len() as f32;
        assert!((0.04..0.14).contains(&avg), "drift settled at {avg}");
    }

    #[test]
    fn renders_opaque_changing_frames_at_odd_and_tiny_sizes() {
        let mut rng = Rng::new(2);
        let mut sim = Snow::new(320, 96, &mut rng);
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
