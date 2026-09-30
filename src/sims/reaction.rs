//! `reaction` — Gray-Scott reaction-diffusion. Two chemicals spread across
//! a grid: U feeds in everywhere, V eats U to make more of itself and
//! slowly decays. From a few random seeds that alone grows coral, spots
//! and worm-like stripes (Turing patterns). The feed rate drifts slowly so
//! the pattern keeps changing character, and fresh seeds keep landing so
//! it never dies out.
use super::{Input, Sim};
use crate::paint::{Frame, Rgb, Theme};
use crate::rng::Rng;

/// Grid rows across the frame's height; cells are sized to match.
const ROWS: usize = 48;
/// Solver iterations per second of wall time, and per call at most.
const ITERS_PER_SEC: f32 = 300.0;
const MAX_ITERS: u32 = 16;
const DU: f32 = 1.0;
const DV: f32 = 0.5;
/// Kill rate, and the feed rate the drift swings around.
const KILL: f32 = 0.062;
const FEED_MID: f32 = 0.046;
const FEED_SWING: f32 = 0.009;
const SEED_EVERY: f32 = 2.5;
const INITIAL_SEEDS: usize = 18;

pub struct Reaction {
    cell: usize,
    cols: usize,
    rows: usize,
    u: Vec<f32>,
    v: Vec<f32>,
    nu: Vec<f32>,
    nv: Vec<f32>,
    acc: f32,
    t: f32,
    next_seed: f32,
    hue: f32,
}

impl Reaction {
    pub fn new(w: usize, h: usize, rng: &mut Rng) -> Self {
        let mut s = Self {
            cell: 1,
            cols: 1,
            rows: 1,
            u: Vec::new(),
            v: Vec::new(),
            nu: Vec::new(),
            nv: Vec::new(),
            acc: 0.0,
            t: rng.range_f32(0.0, 100.0),
            next_seed: SEED_EVERY,
            hue: rng.range_f32(0.0, 360.0),
        };
        s.resize(w, h, rng);
        s
    }

    fn seed(&mut self, rng: &mut Rng) {
        let r = rng.range_i32(2, 5);
        let cx = rng.range_i32(0, self.cols as i32);
        let cy = rng.range_i32(0, self.rows as i32);
        for dy in -r..=r {
            for dx in -r..=r {
                let x = (cx + dx).rem_euclid(self.cols as i32) as usize;
                let y = (cy + dy).rem_euclid(self.rows as i32) as usize;
                let i = y * self.cols + x;
                self.u[i] = 0.5;
                self.v[i] = 0.25 + rng.range_f32(0.0, 0.1);
            }
        }
    }

    /// One explicit Euler step of Gray-Scott on a torus, 3x3 Laplacian.
    fn iterate(&mut self, feed: f32) {
        let (cols, rows) = (self.cols, self.rows);
        for y in 0..rows {
            let (ym, yp) = ((y + rows - 1) % rows, (y + 1) % rows);
            for x in 0..cols {
                let (xm, xp) = ((x + cols - 1) % cols, (x + 1) % cols);
                let i = y * cols + x;
                let lap = |f: &[f32]| {
                    -f[i]
                        + 0.2
                            * (f[y * cols + xm]
                                + f[y * cols + xp]
                                + f[ym * cols + x]
                                + f[yp * cols + x])
                        + 0.05
                            * (f[ym * cols + xm]
                                + f[ym * cols + xp]
                                + f[yp * cols + xm]
                                + f[yp * cols + xp])
                };
                let (u, v) = (self.u[i], self.v[i]);
                let uvv = u * v * v;
                self.nu[i] = (u + DU * lap(&self.u) - uvv + feed * (1.0 - u)).clamp(0.0, 1.0);
                self.nv[i] = (v + DV * lap(&self.v) + uvv - (feed + KILL) * v).clamp(0.0, 1.0);
            }
        }
        std::mem::swap(&mut self.u, &mut self.nu);
        std::mem::swap(&mut self.v, &mut self.nv);
    }

    fn total_v(&self) -> f32 {
        self.v.iter().sum()
    }
}

impl Sim for Reaction {
    fn resize(&mut self, w: usize, h: usize, rng: &mut Rng) {
        let (w, h) = (w.max(1), h.max(1));
        self.cell = (h / ROWS).max(2);
        self.cols = w.div_ceil(self.cell).max(3);
        self.rows = h.div_ceil(self.cell).max(3);
        let n = self.cols * self.rows;
        self.u = vec![1.0; n];
        self.v = vec![0.0; n];
        self.nu = vec![0.0; n];
        self.nv = vec![0.0; n];
        let seeds = (INITIAL_SEEDS * self.cols / (ROWS * 3)).max(4);
        for _ in 0..seeds {
            self.seed(rng);
        }
    }

    fn step(&mut self, dt: f32, _: &Input, rng: &mut Rng) {
        let dt = if dt.is_finite() {
            dt.clamp(0.0, 0.1)
        } else {
            0.0
        };
        self.t = (self.t + dt) % 10_000.0;
        self.hue = (self.hue + dt * 4.0) % 360.0;
        self.next_seed -= dt;
        if self.next_seed <= 0.0 {
            self.next_seed = SEED_EVERY;
            self.seed(rng);
        }
        let feed = FEED_MID + FEED_SWING * (self.t * 0.05).sin();
        self.acc += dt * ITERS_PER_SEC;
        let iters = (self.acc as u32).min(MAX_ITERS);
        self.acc = (self.acc - iters as f32).min(MAX_ITERS as f32);
        for _ in 0..iters {
            self.iterate(feed);
        }
        // Keep it alive if a feed swing ever starves V out.
        if self.total_v() < self.v.len() as f32 * 0.002 {
            for _ in 0..4 {
                self.seed(rng);
            }
        }
    }

    fn render(&mut self, frame: &mut Frame, _: &Theme) {
        let cell = self.cell;
        let deep = Rgb::from_hsv(self.hue + 200.0, 0.7, 0.18);
        let mid = Rgb::from_hsv(self.hue + 170.0, 0.75, 0.75);
        let hot = Rgb::from_hsv(self.hue + 40.0, 0.3, 1.0);
        for y in 0..self.rows {
            for x in 0..self.cols {
                let v = self.v[y * self.cols + x];
                let k = (v * 3.0).clamp(0.0, 1.0);
                let c = if k < 0.5 {
                    deep.lerp(mid, k * 2.0)
                } else {
                    mid.lerp(hot, (k - 0.5) * 2.0)
                };
                let (px0, py0) = (x * cell, y * cell);
                for py in py0..(py0 + cell).min(frame.h) {
                    for px in px0..(px0 + cell).min(frame.w) {
                        let i = (py * frame.w + px) * 4;
                        frame.pixels[i..i + 4].copy_from_slice(&[c.r, c.g, c.b, 255]);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patterns_grow_from_the_seeds_and_stay_bounded() {
        let mut rng = Rng::new(1);
        let mut sim = Reaction::new(320, 96, &mut rng);
        let start = sim.total_v();
        for _ in 0..60 * 20 {
            sim.step(1.0 / 60.0, &Input::default(), &mut rng);
        }
        assert!(sim.total_v() > start, "the pattern never spread");
        assert!(sim
            .u
            .iter()
            .chain(&sim.v)
            .all(|&c| (0.0..=1.0).contains(&c)));
        // Real structure, not a flat field: both high and low V are present.
        let hi = sim.v.iter().filter(|&&v| v > 0.2).count();
        let lo = sim.v.iter().filter(|&&v| v < 0.05).count();
        assert!(hi > 20 && lo > 20, "hi {hi} lo {lo}");
    }

    #[test]
    fn renders_opaque_changing_frames_at_odd_and_tiny_sizes() {
        let mut rng = Rng::new(2);
        let mut sim = Reaction::new(320, 96, &mut rng);
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
        for _ in 0..10 {
            sim.step(0.05, &Input::default(), &mut rng);
        }
        sim.render(&mut frame, &Theme::default());
        assert_ne!(before, frame.pixels);
    }
}
