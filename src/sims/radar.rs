//! `radar` — a radar scope. A sweep line turns around the dish, leaving a
//! fading green afterglow; contacts drift about and light up only as the
//! sweep passes over them, then fade. Range rings and a crosshair sit on
//! the glass, and on wide buttons instrument panels flank the scope: a
//! bank of level meters on one side, a scrolling signal trace on the other.
use super::{Input, Sim};
use crate::paint::{Frame, Rgb, Theme};
use crate::rng::Rng;
use std::f32::consts::TAU;

const SCOPE_R: f32 = 0.46;
const SWEEP_SPEED: f32 = 1.6;
/// How quickly the afterglow fades with angle behind the sweep.
const GLOW_FALLOFF: f32 = 2.2;
const CONTACTS: usize = 6;
const BLIP_FADE: f32 = 0.5;
const BLOCK: usize = 2;
const GREEN: Rgb = Rgb::new(70, 255, 120);

struct Contact {
    r: f32,
    a: f32,
    vr: f32,
    va: f32,
    /// Brightness, set to 1 as the sweep passes and fading after.
    blip: f32,
}

pub struct Radar {
    w: f32,
    h: f32,
    sweep: f32,
    contacts: Vec<Contact>,
    t: f32,
}

impl Radar {
    pub fn new(w: usize, h: usize, rng: &mut Rng) -> Self {
        let contacts = (0..CONTACTS)
            .map(|_| Contact {
                r: rng.range_f32(0.1, SCOPE_R * 0.9),
                a: rng.range_f32(0.0, TAU),
                vr: rng.range_f32(-0.02, 0.02),
                va: rng.range_f32(-0.1, 0.1),
                blip: 0.0,
            })
            .collect();
        Self { w: w.max(1) as f32, h: h.max(1) as f32, sweep: 0.0, contacts, t: 0.0 }
    }

    fn center(&self) -> (f32, f32) {
        (self.w / self.h * 0.5, 0.5)
    }
}

impl Sim for Radar {
    fn resize(&mut self, w: usize, h: usize, _: &mut Rng) {
        self.w = w.max(1) as f32;
        self.h = h.max(1) as f32;
    }

    fn step(&mut self, dt: f32, _: &Input, rng: &mut Rng) {
        let dt = if dt.is_finite() { dt.clamp(0.0, 0.1) } else { 0.0 };
        self.t = (self.t + dt) % 10_000.0;
        let before = self.sweep;
        self.sweep = (self.sweep + SWEEP_SPEED * dt) % TAU;
        let swept = |a: f32| {
            let d = (a - before).rem_euclid(TAU);
            d <= (self.sweep - before).rem_euclid(TAU)
        };
        let lit: Vec<bool> = self.contacts.iter().map(|c| swept(c.a)).collect();
        for (c, lit) in self.contacts.iter_mut().zip(lit) {
            c.a = (c.a + c.va * dt).rem_euclid(TAU);
            c.r += c.vr * dt;
            if !(0.08..SCOPE_R * 0.92).contains(&c.r) {
                c.vr = -c.vr;
                c.r = c.r.clamp(0.08, SCOPE_R * 0.92);
            }
            if rng.bool_p(0.002) {
                c.va = rng.range_f32(-0.1, 0.1);
            }
            c.blip = if lit { 1.0 } else { (c.blip - dt * BLIP_FADE).max(0.0) };
        }
    }

    fn render(&mut self, frame: &mut Frame, _: &Theme) {
        let u = frame.h.max(1) as f32;
        let (cx, cy) = self.center();
        let (fw, fh) = (frame.w as f32, frame.h as f32);
        frame.fill(Rgb::new(4, 12, 7));
        // Instrument-panel grid outside the scope.
        let div = 0.1 * u;
        let mut x = 0.0;
        while x < fw {
            frame.rect(x, 0.0, 1.0, fh, Rgb::new(20, 60, 32), 0.4);
            x += div;
        }
        let mut y = 0.0;
        while y < fh {
            frame.rect(0.0, y, fw, 1.0, Rgb::new(20, 60, 32), 0.4);
            y += div;
        }
        // Side panels, when there's room: level meters left, a signal trace right.
        let side = cx - SCOPE_R - 0.08;
        if side > 0.25 {
            let bars = 8;
            let bw = side / bars as f32;
            for i in 0..bars {
                let level = 0.2 + 0.6 * ((self.t * (1.3 + i as f32 * 0.37) + i as f32).sin() * 0.5 + 0.5);
                let x = 0.04 + i as f32 * bw;
                frame.rect(x * u, (0.9 - level * 0.75) * u, bw * 0.6 * u, level * 0.75 * u, GREEN, 0.55);
            }
            let x0 = cx + SCOPE_R + 0.08;
            let mut prev: Option<(f32, f32)> = None;
            let steps = 60;
            for k in 0..=steps {
                let fx = x0 + (side - 0.04) * k as f32 / steps as f32;
                let fy = 0.5 + 0.25 * ((fx * 9.0 - self.t * 4.0).sin() * (fx * 2.3 + self.t).sin());
                if let Some(p) = prev {
                    frame.line(p, (fx * u, fy * u), 1.2, GREEN, 0.8);
                }
                prev = Some((fx * u, fy * u));
            }
        }
        // The scope: dark glass with the sweep's afterglow.
        let r_px = SCOPE_R * u;
        let x0 = ((cx * u - r_px).max(0.0) as usize / BLOCK) * BLOCK;
        let x1 = ((cx * u + r_px).ceil() as usize).min(frame.w);
        let mut by = 0;
        while by < frame.h {
            let mut bx = x0;
            while bx < x1 {
                let (dx, dy) = ((bx as f32 + 1.0) / u - cx, (by as f32 + 1.0) / u - cy);
                let r = dx.hypot(dy);
                if r <= SCOPE_R {
                    let behind = (self.sweep - dy.atan2(dx)).rem_euclid(TAU);
                    let glow = (-behind * GLOW_FALLOFF).exp();
                    let c = Rgb::new(2, 20, 8).lerp(GREEN, 0.06 + 0.55 * glow);
                    for py in by..(by + BLOCK).min(frame.h) {
                        for px in bx..(bx + BLOCK).min(frame.w) {
                            let i = (py * frame.w + px) * 4;
                            frame.pixels[i..i + 4].copy_from_slice(&[c.r, c.g, c.b, 255]);
                        }
                    }
                }
                bx += BLOCK;
            }
            by += BLOCK;
        }
        let (cxp, cyp) = (cx * u, cy * u);
        for k in 1..=4 {
            super::games::ring(frame, cxp, cyp, r_px * k as f32 / 4.0, 1.0, GREEN, if k == 4 { 0.8 } else { 0.35 });
        }
        frame.line((cxp - r_px, cyp), (cxp + r_px, cyp), 1.0, GREEN, 0.3);
        frame.line((cxp, cyp - r_px), (cxp, cyp + r_px), 1.0, GREEN, 0.3);
        frame.line((cxp, cyp), (cxp + self.sweep.cos() * r_px, cyp + self.sweep.sin() * r_px), 1.6, Rgb::new(200, 255, 210), 0.9);
        for c in &self.contacts {
            if c.blip > 0.01 {
                let (x, y) = (cxp + c.a.cos() * c.r * u, cyp + c.a.sin() * c.r * u);
                frame.disc(x, y, (0.03 * u).max(1.5), GREEN, 0.3 * c.blip);
                frame.disc(x, y, (0.014 * u).max(1.0), Rgb::new(220, 255, 225), c.blip);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contacts_light_up_as_the_sweep_passes_and_then_fade() {
        let mut rng = Rng::new(1);
        let mut sim = Radar::new(320, 96, &mut rng);
        let mut lit_seen = vec![false; CONTACTS];
        for _ in 0..(60.0 * TAU / SWEEP_SPEED * 1.2) as usize {
            sim.step(1.0 / 60.0, &Input::default(), &mut rng);
            for (i, c) in sim.contacts.iter().enumerate() {
                lit_seen[i] |= c.blip > 0.99;
                assert!((0.0..=1.0).contains(&c.blip));
                assert!(c.r > 0.0 && c.r < SCOPE_R);
            }
        }
        assert!(lit_seen.iter().all(|&l| l), "a full turn should light every contact");
    }

    #[test]
    fn renders_opaque_changing_frames_at_odd_and_tiny_sizes() {
        let mut rng = Rng::new(2);
        let mut sim = Radar::new(320, 96, &mut rng);
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
