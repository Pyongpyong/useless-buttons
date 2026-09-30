//! `lissajous` — an oscilloscope in X-Y mode. Two sine waves drive the
//! beam, one horizontally and one vertically, tracing a Lissajous figure
//! in green phosphor over a graticule. The phase between them drifts so
//! the figure seems to turn in 3D, and every few seconds the frequency
//! ratio eases over to the next one (1:2, 3:2, 3:4, ...).
use super::{Input, Sim};
use crate::paint::{Frame, Rgb, Theme};
use crate::rng::Rng;
use std::f32::consts::TAU;

const RATIOS: [(f32, f32); 8] = [
    (1.0, 2.0),
    (3.0, 2.0),
    (3.0, 4.0),
    (5.0, 4.0),
    (2.0, 3.0),
    (5.0, 6.0),
    (1.0, 3.0),
    (4.0, 5.0),
];
const HOLD_SEC: f32 = 7.0;
const MORPH_SEC: f32 = 1.5;
const PHASE_SPEED: f32 = 0.6;
const SAMPLES: usize = 700;
const AMP_Y: f32 = 0.36;
const PHOSPHOR: Rgb = Rgb::new(90, 255, 130);

pub struct Lissajous {
    w: f32,
    h: f32,
    ratio: usize,
    since_change: f32,
    phase: f32,
    beam: f32,
}

impl Lissajous {
    pub fn new(w: usize, h: usize, rng: &mut Rng) -> Self {
        Self {
            w: w.max(1) as f32,
            h: h.max(1) as f32,
            ratio: rng.range_usize(0, RATIOS.len()),
            since_change: 0.0,
            phase: rng.range_f32(0.0, TAU),
            beam: 0.0,
        }
    }

    fn aspect(&self) -> f32 {
        self.w / self.h
    }

    /// Current frequencies, easing from the previous ratio to this one.
    fn freqs(&self) -> (f32, f32) {
        let (a1, b1) = RATIOS[self.ratio];
        let (a0, b0) = RATIOS[(self.ratio + RATIOS.len() - 1) % RATIOS.len()];
        let k = (self.since_change / MORPH_SEC).clamp(0.0, 1.0);
        let k = k * k * (3.0 - 2.0 * k);
        (a0 + (a1 - a0) * k, b0 + (b1 - b0) * k)
    }

    fn point(&self, s: f32, a: f32, b: f32) -> (f32, f32) {
        let amp_x = (self.aspect() * 0.5 - 0.12).max(0.1);
        (
            self.aspect() * 0.5 + amp_x * (a * s + self.phase).sin(),
            0.5 + AMP_Y * (b * s).sin(),
        )
    }
}

impl Sim for Lissajous {
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
        self.phase = (self.phase + PHASE_SPEED * dt) % TAU;
        self.beam = (self.beam + dt * 1.8) % TAU;
        self.since_change += dt;
        if self.since_change >= HOLD_SEC {
            self.since_change = 0.0;
            self.ratio = (self.ratio + 1) % RATIOS.len();
        }
    }

    fn render(&mut self, frame: &mut Frame, _: &Theme) {
        let u = frame.h.max(1) as f32;
        let (fw, fh) = (frame.w as f32, frame.h as f32);
        frame.fill(Rgb::new(4, 14, 8));
        // Graticule: faint divisions every 0.125 units, brighter center axes.
        let div = 0.125 * u;
        let mut x = fw * 0.5 % div;
        while x < fw {
            frame.rect(x, 0.0, 1.0, fh, Rgb::new(30, 80, 45), 0.35);
            x += div;
        }
        let mut y = fh * 0.5 % div;
        while y < fh {
            frame.rect(0.0, y, fw, 1.0, Rgb::new(30, 80, 45), 0.35);
            y += div;
        }
        frame.rect(fw * 0.5, 0.0, 1.0, fh, Rgb::new(40, 110, 60), 0.5);
        frame.rect(0.0, fh * 0.5, fw, 1.0, Rgb::new(40, 110, 60), 0.5);
        // The trace: a wide dim glow, then the bright core.
        let (a, b) = self.freqs();
        let pts: Vec<(f32, f32)> = (0..=SAMPLES)
            .map(|i| {
                let (x, y) = self.point(i as f32 / SAMPLES as f32 * TAU, a, b);
                (x * u, y * u)
            })
            .collect();
        for (width, alpha) in [(4.0, 0.12), (2.2, 0.3), (1.1, 0.9)] {
            for w in pts.windows(2) {
                frame.line(w[0], w[1], width, PHOSPHOR, alpha);
            }
        }
        let (bx, by) = self.point(self.beam, a, b);
        frame.disc(bx * u, by * u, (0.025 * u).max(1.5), PHOSPHOR, 0.35);
        frame.disc(
            bx * u,
            by * u,
            (0.011 * u).max(1.0),
            Rgb::new(220, 255, 230),
            1.0,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_trace_stays_on_the_screen() {
        let mut rng = Rng::new(1);
        let mut sim = Lissajous::new(320, 96, &mut rng);
        for _ in 0..(60.0 * HOLD_SEC * 2.5) as usize {
            sim.step(1.0 / 60.0, &Input::default(), &mut rng);
            let (a, b) = sim.freqs();
            for i in 0..50 {
                let (x, y) = sim.point(i as f32 * 0.13, a, b);
                assert!((0.0..=sim.aspect()).contains(&x) && (0.0..=1.0).contains(&y));
            }
        }
    }

    #[test]
    fn the_ratio_moves_on_and_eases_between_values() {
        let mut rng = Rng::new(2);
        let mut sim = Lissajous::new(320, 96, &mut rng);
        let start = sim.ratio;
        sim.since_change = HOLD_SEC - 0.01;
        sim.step(0.02, &Input::default(), &mut rng);
        assert_eq!(sim.ratio, (start + 1) % RATIOS.len());
        let (a0, _) = sim.freqs();
        assert_eq!(
            a0, RATIOS[start].0,
            "a ratio change must start from the previous figure"
        );
        sim.since_change = MORPH_SEC;
        assert_eq!(sim.freqs(), RATIOS[sim.ratio]);
    }

    #[test]
    fn renders_opaque_changing_frames_at_odd_and_tiny_sizes() {
        let mut rng = Rng::new(3);
        let mut sim = Lissajous::new(320, 96, &mut rng);
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
