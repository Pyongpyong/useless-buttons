//! Neon tunnel: inverse-radius perspective, twisted rails and glowing rings.
//! Shaded at half resolution with fixed work per pixel and no growing geometry.
use super::{Input, Sim};
use crate::paint::{Frame, Rgb, Theme};
use crate::rng::Rng;
use std::f32::consts::TAU;

pub struct Tunnel {
    w: f32,
    h: f32,
    travel: f32,
    turn: f32,
    hue: f32,
    boost: f32,
    center: (f32, f32),
}

impl Tunnel {
    pub fn new(w: usize, h: usize, rng: &mut Rng) -> Self {
        Self {
            w: w.max(1) as f32,
            h: h.max(1) as f32,
            travel: 0.0,
            turn: 0.0,
            hue: rng.range_f32(0.0, 360.0),
            boost: 0.0,
            center: (0.5, 0.5),
        }
    }
}

impl Sim for Tunnel {
    fn resize(&mut self, w: usize, h: usize, _: &mut Rng) {
        self.w = w.max(1) as f32;
        self.h = h.max(1) as f32;
    }

    fn step(&mut self, dt: f32, input: &Input, _: &mut Rng) {
        let dt = if dt.is_finite() {
            dt.clamp(0.0, 0.1)
        } else {
            0.0
        };
        if input.clicks > 0 {
            self.boost = 1.0;
            self.hue = (self.hue + 110.0).rem_euclid(360.0);
        }
        self.boost = (self.boost - dt * 0.65).max(0.0);
        let speed = 1.8 + self.boost * 6.0 + if input.down { 2.0 } else { 0.0 };
        self.travel = (self.travel + dt * speed).rem_euclid(16.0);
        self.turn = (self.turn + dt * (0.75 + self.boost * 1.5)).rem_euclid(TAU);
        let target = if input.hover && input.x.is_finite() && input.y.is_finite() {
            (
                0.5 + (input.x / self.w - 0.5).clamp(-0.5, 0.5) * 0.4,
                0.5 + (input.y / self.h - 0.5).clamp(-0.5, 0.5) * 0.4,
            )
        } else {
            (0.5, 0.5)
        };
        let follow = 1.0 - (-dt * 5.0).exp();
        self.center.0 += (target.0 - self.center.0) * follow;
        self.center.1 += (target.1 - self.center.1) * follow;
    }

    fn render(&mut self, frame: &mut Frame, _: &Theme) {
        let palette: [Rgb; 256] =
            std::array::from_fn(|i| Rgb::from_hsv(self.hue + i as f32 * 360.0 / 256.0, 0.8, 1.0));
        let scale = frame.w.min(frame.h).max(1) as f32 * 0.65;
        let cx = frame.w as f32 * self.center.0;
        let cy = frame.h as f32 * self.center.1;
        for y in (0..frame.h).step_by(2) {
            for x in (0..frame.w).step_by(2) {
                let u = (x as f32 + 0.5 - cx) / scale;
                let v = (y as f32 + 0.5 - cy) / scale;
                let r = (u * u + v * v).sqrt().max(0.025);
                let depth = 1.0 / r;
                let angle = v.atan2(u) + self.turn + depth * 0.18;
                let ring_phase = (depth + self.travel).fract();
                let ring_dist = ring_phase.min(1.0 - ring_phase);
                let rail_dist = (angle * 6.0).sin().abs();
                // Soft bloom plus a narrow white-hot filament, no full-screen flash.
                let ring = 0.0016 / (ring_dist * ring_dist + 0.0016);
                let rail = 0.008 / (rail_dist * rail_dist + 0.008);
                let filament =
                    (1.0 - ring_dist / 0.018).max(0.0) + (1.0 - rail_dist / 0.035).max(0.0) * 0.5;
                let fog = (r * 3.0).min(1.0);
                let light = (0.055 + ring * 0.65 + rail * 0.4) * fog;
                let color_phase =
                    (angle / TAU + depth * 0.065 + self.travel * 0.0625).rem_euclid(1.0);
                let color = palette[(color_phase * 255.0) as usize];
                let white = filament * 100.0 * fog;
                let rgb = [
                    (5.0 + color.r as f32 * light + white).min(255.0) as u8,
                    (3.0 + color.g as f32 * light + white).min(255.0) as u8,
                    (14.0 + color.b as f32 * light + white).min(255.0) as u8,
                ];
                for yy in y..(y + 2).min(frame.h) {
                    for xx in x..(x + 2).min(frame.w) {
                        let i = (yy * frame.w + xx) * 4;
                        frame.pixels[i..i + 3].copy_from_slice(&rgb);
                        frame.pixels[i + 3] = 255;
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
    fn click_accelerates_and_changes_dimension_then_recovers() {
        let mut rng = Rng::new(1);
        let mut idle = Tunnel::new(320, 96, &mut rng);
        let mut clicked = Tunnel::new(320, 96, &mut rng);
        let hue = clicked.hue;
        idle.step(1.0 / 60.0, &Input::default(), &mut rng);
        clicked.step(
            1.0 / 60.0,
            &Input {
                clicks: 1,
                ..Input::default()
            },
            &mut rng,
        );
        assert!(clicked.travel > idle.travel * 2.0);
        assert_ne!(clicked.hue, hue);
        for _ in 0..180 {
            clicked.step(1.0 / 60.0, &Input::default(), &mut rng);
        }
        assert_eq!(clicked.boost, 0.0);
    }

    #[test]
    fn renders_opaque_changing_frames_at_odd_and_tiny_sizes() {
        let mut rng = Rng::new(2);
        let mut sim = Tunnel::new(320, 96, &mut rng);
        for (w, h) in [(321, 97), (1, 1), (0, 0), (4, 7)] {
            sim.resize(w, h, &mut rng);
            let mut frame = Frame::new(w, h);
            sim.render(&mut frame, &Theme::default());
            let before = frame.pixels.clone();
            sim.step(
                0.1,
                &Input {
                    clicks: 1,
                    ..Input::default()
                },
                &mut rng,
            );
            sim.render(&mut frame, &Theme::default());
            assert_ne!(before, frame.pixels);
            assert!(frame.pixels.chunks_exact(4).all(|p| p[3] == 255));
        }
    }

    #[test]
    fn invalid_input_and_long_frames_keep_state_bounded() {
        let mut rng = Rng::new(3);
        let mut sim = Tunnel::new(1, 1, &mut rng);
        for dt in [f32::NAN, f32::INFINITY, -1.0, 1000.0] {
            sim.step(
                dt,
                &Input {
                    hover: true,
                    x: f32::NAN,
                    y: f32::INFINITY,
                    ..Input::default()
                },
                &mut rng,
            );
            assert!((0.0..16.0).contains(&sim.travel));
            assert!((0.0..TAU).contains(&sim.turn));
            assert!(sim.center.0.is_finite() && sim.center.1.is_finite());
        }
    }
}
