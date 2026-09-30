//! `pendulum` — chaos, demonstrated. Six double pendulums hang from one
//! pivot, released from angles a thousandth of a radian apart. For a few
//! seconds they swing as one; then the tiny differences blow up and they
//! fly apart, each dragging a colored trail. Every so often they're all
//! reset to a new starting angle to show it again.
use super::{Input, Sim};
use crate::paint::{Frame, Rgb, Theme};
use crate::rng::Rng;
use std::collections::VecDeque;

const PENDULUMS: usize = 6;
/// Starting-angle difference between neighbors, radians.
const NUDGE: f32 = 1e-3;
/// Arm lengths in units (frame height = 1); both arms together reach 0.42.
const ARM: f32 = 0.21;
const PIVOT_Y: f32 = 0.5;
/// Gravity over arm length, in the pendulum's own time scale.
const G: f32 = 9.81;
/// Simulated seconds per real second (the physics runs in unit-length arms).
const TIME_SCALE: f32 = 1.4;
const SUBSTEP: f32 = 1.0 / 240.0;
const RESET_SEC: f32 = 20.0;
const TRAIL: usize = 70;

#[derive(Clone, Copy)]
struct State {
    a1: f32,
    a2: f32,
    w1: f32,
    w2: f32,
}

/// Angular accelerations of a double pendulum with equal masses and unit arms.
fn accel(s: State) -> (f32, f32) {
    let d = s.a1 - s.a2;
    let den = 3.0 - (2.0 * d).cos();
    let a1 = (-3.0 * G * s.a1.sin()
        - G * (s.a1 - 2.0 * s.a2).sin()
        - 2.0 * d.sin() * (s.w2 * s.w2 + s.w1 * s.w1 * d.cos()))
        / den;
    let a2 =
        (2.0 * d.sin() * (2.0 * s.w1 * s.w1 + 2.0 * G * s.a1.cos() + s.w2 * s.w2 * d.cos())) / den;
    (a1, a2)
}

fn rk4(s: State, h: f32) -> State {
    let f = |s: State| {
        let (a1, a2) = accel(s);
        State {
            a1: s.w1,
            a2: s.w2,
            w1: a1,
            w2: a2,
        }
    };
    let add = |s: State, k: State, m: f32| State {
        a1: s.a1 + k.a1 * m,
        a2: s.a2 + k.a2 * m,
        w1: s.w1 + k.w1 * m,
        w2: s.w2 + k.w2 * m,
    };
    let k1 = f(s);
    let k2 = f(add(s, k1, h * 0.5));
    let k3 = f(add(s, k2, h * 0.5));
    let k4 = f(add(s, k3, h));
    State {
        a1: s.a1 + h / 6.0 * (k1.a1 + 2.0 * k2.a1 + 2.0 * k3.a1 + k4.a1),
        a2: s.a2 + h / 6.0 * (k1.a2 + 2.0 * k2.a2 + 2.0 * k3.a2 + k4.a2),
        w1: s.w1 + h / 6.0 * (k1.w1 + 2.0 * k2.w1 + 2.0 * k3.w1 + k4.w1),
        w2: s.w2 + h / 6.0 * (k1.w2 + 2.0 * k2.w2 + 2.0 * k3.w2 + k4.w2),
    }
}

pub struct Pendulum {
    w: f32,
    h: f32,
    states: Vec<State>,
    trails: Vec<VecDeque<(f32, f32)>>,
    since_reset: f32,
    hue: f32,
}

impl Pendulum {
    pub fn new(w: usize, h: usize, rng: &mut Rng) -> Self {
        let mut s = Self {
            w: w.max(1) as f32,
            h: h.max(1) as f32,
            states: Vec::new(),
            trails: Vec::new(),
            since_reset: 0.0,
            hue: 0.0,
        };
        s.reset(rng);
        s
    }

    fn reset(&mut self, rng: &mut Rng) {
        let a1 = rng.range_f32(1.8, 2.8) * if rng.bool_p(0.5) { 1.0 } else { -1.0 };
        let a2 = a1 + rng.range_f32(-0.6, 0.6);
        self.states = (0..PENDULUMS)
            .map(|i| State {
                a1: a1 + i as f32 * NUDGE,
                a2,
                w1: 0.0,
                w2: 0.0,
            })
            .collect();
        self.trails = vec![VecDeque::with_capacity(TRAIL); PENDULUMS];
        self.since_reset = 0.0;
        self.hue = rng.range_f32(0.0, 360.0);
    }

    fn pivot(&self) -> (f32, f32) {
        (self.w / self.h * 0.5, PIVOT_Y)
    }

    /// Elbow and tip positions, in units.
    fn joints(&self, s: State) -> ((f32, f32), (f32, f32)) {
        let (px, py) = self.pivot();
        let e = (px + ARM * s.a1.sin(), py + ARM * s.a1.cos());
        (e, (e.0 + ARM * s.a2.sin(), e.1 + ARM * s.a2.cos()))
    }
}

impl Sim for Pendulum {
    fn resize(&mut self, w: usize, h: usize, _: &mut Rng) {
        self.w = w.max(1) as f32;
        self.h = h.max(1) as f32;
        for t in &mut self.trails {
            t.clear();
        }
    }

    fn step(&mut self, dt: f32, _: &Input, rng: &mut Rng) {
        let dt = if dt.is_finite() {
            dt.clamp(0.0, 0.1)
        } else {
            0.0
        };
        self.since_reset += dt;
        if self.since_reset >= RESET_SEC {
            self.reset(rng);
        }
        let sim_dt = dt * TIME_SCALE;
        let n = (sim_dt / SUBSTEP).ceil().max(1.0) as usize;
        for s in &mut self.states {
            for _ in 0..n {
                *s = rk4(*s, sim_dt / n as f32);
            }
            // Guard against numerical blow-up ever producing NaN.
            if !(s.a1.is_finite() && s.a2.is_finite() && s.w1.is_finite() && s.w2.is_finite()) {
                *s = State {
                    a1: 0.5,
                    a2: 0.5,
                    w1: 0.0,
                    w2: 0.0,
                };
            }
        }
        for i in 0..PENDULUMS {
            let (_, tip) = self.joints(self.states[i]);
            let trail = &mut self.trails[i];
            if trail.len() >= TRAIL {
                trail.pop_front();
            }
            trail.push_back(tip);
        }
    }

    fn render(&mut self, frame: &mut Frame, _: &Theme) {
        let u = frame.h.max(1) as f32;
        let (top, bottom) = (Rgb::new(14, 14, 24), Rgb::new(24, 20, 36));
        let denom = (frame.h.max(2) - 1) as f32;
        for y in 0..frame.h {
            let c = top.lerp(bottom, y as f32 / denom);
            let row = y * frame.w * 4;
            for px in frame.pixels[row..row + frame.w * 4].chunks_exact_mut(4) {
                px.copy_from_slice(&[c.r, c.g, c.b, 255]);
            }
        }
        let (px, py) = self.pivot();
        for i in 0..PENDULUMS {
            let c = Rgb::from_hsv(self.hue + i as f32 * 360.0 / PENDULUMS as f32, 0.7, 1.0);
            let trail = &self.trails[i];
            for (k, w) in trail.iter().zip(trail.iter().skip(1)).enumerate() {
                let a = k as f32 / TRAIL as f32;
                frame.line(
                    (w.0 .0 * u, w.0 .1 * u),
                    (w.1 .0 * u, w.1 .1 * u),
                    (1.5 * a).max(0.6),
                    c,
                    a * 0.8,
                );
            }
        }
        for i in 0..PENDULUMS {
            let c = Rgb::from_hsv(self.hue + i as f32 * 360.0 / PENDULUMS as f32, 0.7, 1.0);
            let (e, tip) = self.joints(self.states[i]);
            frame.line(
                (px * u, py * u),
                (e.0 * u, e.1 * u),
                1.2,
                Rgb::new(200, 200, 215),
                0.5,
            );
            frame.line(
                (e.0 * u, e.1 * u),
                (tip.0 * u, tip.1 * u),
                1.2,
                Rgb::new(200, 200, 215),
                0.5,
            );
            frame.disc(e.0 * u, e.1 * u, (0.015 * u).max(1.0), c, 0.8);
            frame.disc(tip.0 * u, tip.1 * u, (0.022 * u).max(1.2), c, 1.0);
        }
        frame.disc(
            px * u,
            py * u,
            (0.012 * u).max(1.0),
            Rgb::new(230, 230, 240),
            1.0,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn energy(s: State) -> f32 {
        // Equal unit masses and arms, heights measured downward-positive.
        let v1 = s.w1 * s.w1;
        let v2 = s.w1 * s.w1 + s.w2 * s.w2 + 2.0 * s.w1 * s.w2 * (s.a1 - s.a2).cos();
        0.5 * v1 + 0.5 * v2 - G * (2.0 * s.a1.cos() + s.a2.cos())
    }

    #[test]
    fn the_integrator_conserves_energy() {
        let s0 = State {
            a1: 2.0,
            a2: 1.5,
            w1: 0.0,
            w2: 0.0,
        };
        let mut s = s0;
        for _ in 0..240 * 10 {
            s = rk4(s, SUBSTEP);
        }
        let (e0, e1) = (energy(s0), energy(s));
        assert!(
            (e1 - e0).abs() < 0.05 * e0.abs().max(1.0),
            "energy drifted {e0} -> {e1}"
        );
    }

    #[test]
    fn nearly_identical_pendulums_fly_apart() {
        let mut rng = Rng::new(1);
        let mut sim = Pendulum::new(320, 96, &mut rng);
        let spread = |sim: &Pendulum| {
            let a: Vec<f32> = sim.states.iter().map(|s| s.a2).collect();
            a.iter().cloned().fold(f32::NEG_INFINITY, f32::max)
                - a.iter().cloned().fold(f32::INFINITY, f32::min)
        };
        let early = spread(&sim);
        for _ in 0..60 * 15 {
            sim.step(1.0 / 60.0, &Input::default(), &mut rng);
        }
        assert!(
            spread(&sim) > early * 100.0,
            "no divergence: {early} -> {}",
            spread(&sim)
        );
    }

    #[test]
    fn renders_opaque_changing_frames_at_odd_and_tiny_sizes() {
        let mut rng = Rng::new(2);
        let mut sim = Pendulum::new(320, 96, &mut rng);
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
