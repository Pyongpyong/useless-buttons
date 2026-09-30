//! `crt` — a CRT television on the blink. It shows color bars that
//! jitter, tear sideways and bleed their colors apart, and every so often
//! loses the signal to a burst of snowy static. Scanlines, a slow rolling
//! bar, a flicker and darkened, bulging corners sell the tube.
use super::{Input, Sim};
use crate::paint::{Frame, Rgb, Theme};
use crate::rng::Rng;

const BARS_MIN: f32 = 3.0;
const BARS_MAX: f32 = 6.0;
const STATIC_MIN: f32 = 0.6;
const STATIC_MAX: f32 = 1.6;
/// Top color bars (75% white, yellow, cyan, green, magenta, red, blue)
/// and the bottom castellation strip.
const TOP: [Rgb; 7] = [
    Rgb::new(192, 192, 192),
    Rgb::new(192, 192, 0),
    Rgb::new(0, 192, 192),
    Rgb::new(0, 192, 0),
    Rgb::new(192, 0, 192),
    Rgb::new(192, 0, 0),
    Rgb::new(0, 0, 192),
];
const BOTTOM: [Rgb; 7] = [
    Rgb::new(0, 0, 192),
    Rgb::new(19, 19, 19),
    Rgb::new(192, 0, 192),
    Rgb::new(19, 19, 19),
    Rgb::new(0, 192, 192),
    Rgb::new(19, 19, 19),
    Rgb::new(192, 192, 192),
];

pub struct Crt {
    w: f32,
    h: f32,
    t: f32,
    frame_no: u32,
    /// Seconds left in the current state; `static_on` says which.
    left: f32,
    static_on: bool,
    /// Current horizontal tear: rows affected and how far they're shifted.
    tear: Option<(f32, f32, f32)>,
}

fn hash(a: u32, b: u32, c: u32) -> u32 {
    let mut h = a.wrapping_mul(0x9E37_79B1) ^ b.wrapping_mul(0x85EB_CA77) ^ c.wrapping_mul(0xC2B2_AE3D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^ (h >> 12)
}

impl Crt {
    pub fn new(w: usize, h: usize, rng: &mut Rng) -> Self {
        Self { w: w.max(1) as f32, h: h.max(1) as f32, t: 0.0, frame_no: 0, left: rng.range_f32(BARS_MIN, BARS_MAX), static_on: false, tear: None }
    }

    /// Color bars at a point, `u`/`v` in 0..1 across and down the screen.
    fn bars(u: f32, v: f32) -> Rgb {
        let i = ((u * 7.0) as usize).min(6);
        if v < 0.72 {
            TOP[i]
        } else {
            BOTTOM[i]
        }
    }
}

impl Sim for Crt {
    fn resize(&mut self, w: usize, h: usize, _: &mut Rng) {
        self.w = w.max(1) as f32;
        self.h = h.max(1) as f32;
    }

    fn step(&mut self, dt: f32, _: &Input, rng: &mut Rng) {
        let dt = if dt.is_finite() { dt.clamp(0.0, 0.1) } else { 0.0 };
        self.t = (self.t + dt) % 10_000.0;
        self.frame_no = self.frame_no.wrapping_add(1);
        self.left -= dt;
        if self.left <= 0.0 {
            self.static_on = !self.static_on;
            self.left = if self.static_on { rng.range_f32(STATIC_MIN, STATIC_MAX) } else { rng.range_f32(BARS_MIN, BARS_MAX) };
        }
        // Tears come and go at random.
        self.tear = match self.tear {
            Some((y, hgt, _)) if rng.bool_p(0.85) => Some((y, hgt, rng.range_f32(-0.12, 0.12))),
            Some(_) => None,
            None if rng.bool_p(0.03) => Some((rng.range_f32(0.0, 1.0), rng.range_f32(0.05, 0.2), rng.range_f32(-0.12, 0.12))),
            None => None,
        };
    }

    fn render(&mut self, frame: &mut Frame, _: &Theme) {
        let (fw, fh) = (frame.w as f32, frame.h as f32);
        let flicker = 0.92 + 0.08 * (self.t * 60.0).sin().abs();
        let roll = (self.t * 0.25).fract() * 1.3 - 0.15;
        // A small horizontal wobble, shared by every row, plus the tear.
        let jitter = ((self.t * 13.0).sin() * (self.t * 7.3).sin()) * 0.004;
        let bleed = 2.5 / fw.max(1.0);
        for py in 0..frame.h {
            let v = (py as f32 + 0.5) / fh;
            let mut shift = jitter;
            if let Some((y, hgt, dx)) = self.tear {
                if v > y && v < y + hgt {
                    shift += dx;
                }
            }
            let scan = if py % 2 == 0 { 1.0 } else { 0.72 };
            let band = (1.0 - ((v - roll) / 0.08).abs()).max(0.0) * 0.18;
            for px in 0..frame.w {
                let u = (px as f32 + 0.5) / fw;
                let c = if self.static_on {
                    let n = (hash(px as u32 / 2, py as u32 / 2, self.frame_no) & 0xff) as f32;
                    Rgb::new(n as u8, n as u8, n as u8)
                } else {
                    // Red is read a little to the right: the colors bleed apart.
                    let a = Self::bars((u + shift).rem_euclid(1.0), v);
                    let r = Self::bars((u + shift + bleed).rem_euclid(1.0), v);
                    Rgb::new(r.r, a.g, a.b)
                };
                // Darker toward the corners, like the curve of the tube.
                let (cx, cy) = (u * 2.0 - 1.0, v * 2.0 - 1.0);
                let vignette = (1.0 - (cx * cx * 0.35 + cy * cy * 0.5)).clamp(0.0, 1.0);
                let k = (scan * vignette * flicker + band).min(1.3);
                let i = (py * frame.w + px) * 4;
                frame.pixels[i] = (c.r as f32 * k).min(255.0) as u8;
                frame.pixels[i + 1] = (c.g as f32 * k).min(255.0) as u8;
                frame.pixels[i + 2] = (c.b as f32 * k).min(255.0) as u8;
                frame.pixels[i + 3] = 255;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alternates_between_bars_and_static() {
        let mut rng = Rng::new(1);
        let mut sim = Crt::new(320, 96, &mut rng);
        let mut seen_static = false;
        let mut seen_bars = false;
        for _ in 0..60 * 20 {
            sim.step(1.0 / 60.0, &Input::default(), &mut rng);
            seen_static |= sim.static_on;
            seen_bars |= !sim.static_on;
        }
        assert!(seen_static && seen_bars);
    }

    #[test]
    fn bars_are_the_classic_seven() {
        assert_eq!(Crt::bars(0.05, 0.1), TOP[0]);
        assert_eq!(Crt::bars(0.95, 0.1), TOP[6]);
        assert_eq!(Crt::bars(0.95, 0.9), BOTTOM[6]);
    }

    #[test]
    fn renders_opaque_changing_frames_at_odd_and_tiny_sizes() {
        let mut rng = Rng::new(2);
        let mut sim = Crt::new(320, 96, &mut rng);
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
        sim.step(0.05, &Input::default(), &mut rng);
        sim.render(&mut frame, &Theme::default());
        assert_ne!(before, frame.pixels);
    }
}
