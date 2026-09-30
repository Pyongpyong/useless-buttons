//! `metaball` — glossy 3D metaballs. Candy-colored balls drift around in
//! depth (nearer ones are bigger) and melt into each other where they
//! meet. Unlike `lava`'s flat wax, each pixel of the surface gets a real
//! normal: the field is turned into a height that is exactly a hemisphere
//! for a lone ball, and its slope gives the normal, which is lit with
//! diffuse light, a sharp specular highlight, a Fresnel rim and a
//! reflection of the sky. The balls cast a soft shadow on the floor.
//! Computed at half resolution.
use super::{Input, Sim};
use crate::paint::{Frame, Rgb, Theme};
use crate::rng::Rng;

const BALLS: usize = 7;
const R_MIN: f32 = 0.1;
const R_MAX: f32 = 0.17;
const BLOCK: usize = 2;
/// Light direction (toward the light), normalized below; up and to the left.
const LIGHT: (f32, f32, f32) = (-0.45, -0.65, 0.62);
const SHININESS: f32 = 48.0;
/// Shadow offset on the floor, in units.
const SHADOW: (f32, f32) = (0.05, 0.08);

struct Ball {
    /// Orbit center and radii (x, y, depth), speeds and phases.
    home: (f32, f32),
    amp: (f32, f32, f32),
    speed: (f32, f32, f32),
    phase: f32,
    r: f32,
    hue: f32,
}

pub struct Metaball {
    w: f32,
    h: f32,
    balls: Vec<Ball>,
    t: f32,
}

/// A ball as seen right now: screen position, radius and color.
#[derive(Clone, Copy)]
struct Placed {
    x: f32,
    y: f32,
    r: f32,
    color: Rgb,
}

impl Metaball {
    pub fn new(w: usize, h: usize, rng: &mut Rng) -> Self {
        let mut s = Self { w: w.max(1) as f32, h: h.max(1) as f32, balls: Vec::new(), t: 0.0 };
        let aspect = s.w / s.h;
        s.balls = (0..BALLS)
            .map(|i| Ball {
                home: ((i as f32 + 0.5) / BALLS as f32 * aspect, rng.range_f32(0.35, 0.65)),
                amp: (rng.range_f32(0.15, 0.4), rng.range_f32(0.1, 0.22), rng.range_f32(0.2, 0.45)),
                speed: (rng.range_f32(0.2, 0.5), rng.range_f32(0.25, 0.6), rng.range_f32(0.2, 0.45)),
                phase: rng.range_f32(0.0, 6.3),
                r: rng.range_f32(R_MIN, R_MAX),
                hue: [330.0, 190.0, 45.0, 280.0, 150.0, 10.0, 220.0][i % 7] + rng.range_f32(-10.0, 10.0),
            })
            .collect();
        s
    }

    fn aspect(&self) -> f32 {
        self.w / self.h
    }

    fn placed(&self) -> Vec<Placed> {
        let aspect = self.aspect();
        self.balls
            .iter()
            .map(|b| {
                let t = self.t;
                let depth = (t * b.speed.2 + b.phase * 1.7).sin() * b.amp.2;
                // Nearer (depth > 0) looks bigger.
                let r = b.r * (1.0 + depth * 0.6);
                let x = (b.home.0 + (t * b.speed.0 + b.phase).sin() * b.amp.0).clamp(0.05, aspect - 0.05);
                let y = b.home.1 + (t * b.speed.1 + b.phase * 2.3).sin() * b.amp.1;
                Placed { x, y, r, color: Rgb::from_hsv(b.hue, 0.8, 0.95) }
            })
            .collect()
    }
}

/// Field value, its gradient, and the color blended by each ball's share.
fn sample(balls: &[Placed], x: f32, y: f32) -> (f32, f32, f32, Rgb) {
    let (mut f, mut gx, mut gy) = (0.0, 0.0, 0.0);
    let (mut cr, mut cg, mut cb) = (0.0, 0.0, 0.0);
    for b in balls {
        let (dx, dy) = (x - b.x, y - b.y);
        let d2 = (dx * dx + dy * dy).max(1e-5);
        let v = b.r * b.r / d2;
        f += v;
        // d/dx of r²/d² is -2 r² dx / d⁴.
        let k = -2.0 * v / d2;
        gx += k * dx;
        gy += k * dy;
        cr += v * b.color.r as f32;
        cg += v * b.color.g as f32;
        cb += v * b.color.b as f32;
    }
    let inv = 1.0 / f.max(1e-6);
    (f, gx, gy, Rgb::new((cr * inv) as u8, (cg * inv) as u8, (cb * inv) as u8))
}

impl Sim for Metaball {
    fn resize(&mut self, w: usize, h: usize, _: &mut Rng) {
        self.w = w.max(1) as f32;
        self.h = h.max(1) as f32;
    }

    fn step(&mut self, dt: f32, _: &Input, _: &mut Rng) {
        let dt = if dt.is_finite() { dt.clamp(0.0, 0.1) } else { 0.0 };
        self.t = (self.t + dt) % 10_000.0;
    }

    fn render(&mut self, frame: &mut Frame, _: &Theme) {
        let u = frame.h.max(1) as f32;
        let balls = self.placed();
        let ln = (LIGHT.0 * LIGHT.0 + LIGHT.1 * LIGHT.1 + LIGHT.2 * LIGHT.2).sqrt();
        let light = (LIGHT.0 / ln, LIGHT.1 / ln, LIGHT.2 / ln);
        let r_avg = balls.iter().map(|b| b.r).sum::<f32>() / balls.len().max(1) as f32;
        let mut by = 0;
        while by < frame.h {
            let mut bx = 0;
            while bx < frame.w {
                let (x, y) = ((bx as f32 + 1.0) / u, (by as f32 + 1.0) / u);
                let (f, gx, gy, base) = sample(&balls, x, y);
                let c = if f >= 1.0 {
                    // Height: exactly a unit hemisphere for a single ball.
                    let h = (1.0 - 1.0 / f).max(1e-4).sqrt();
                    // Slope of that height, via dh/df = 1 / (2 h f²).
                    let s = 1.0 / (2.0 * h * f * f);
                    let (hx, hy) = (gx * s * r_avg, gy * s * r_avg);
                    let nl = (hx * hx + hy * hy + 1.0).sqrt();
                    let n = (-hx / nl, -hy / nl, 1.0 / nl);
                    let diffuse = (n.0 * light.0 + n.1 * light.1 + n.2 * light.2).max(0.0);
                    // Blinn-Phong highlight, viewer straight on.
                    let (hx2, hy2, hz2) = (light.0, light.1, light.2 + 1.0);
                    let hl = (hx2 * hx2 + hy2 * hy2 + hz2 * hz2).sqrt();
                    let spec = ((n.0 * hx2 + n.1 * hy2 + n.2 * hz2) / hl).max(0.0).powf(SHININESS);
                    let fresnel = (1.0 - n.2).powi(3);
                    // The sky above reflects in the upper side of each ball.
                    let sky = (-n.1).max(0.0) * 0.35;
                    let lit = 0.18 + 0.82 * diffuse;
                    let mix = |c: u8, sky_c: f32| (c as f32 * lit + sky * sky_c + fresnel * 90.0 + spec * 255.0).min(255.0) as u8;
                    Rgb::new(mix(base.r, 120.0), mix(base.g, 160.0), mix(base.b, 255.0))
                } else {
                    // Floor: a vertical gradient, darkened under the balls' shadow.
                    let floor = Rgb::new(40, 36, 60).lerp(Rgb::new(16, 14, 28), y);
                    let (fs, _, _, _) = sample(&balls, x - SHADOW.0, y - SHADOW.1);
                    let shade = (fs * 0.6).clamp(0.0, 0.6);
                    let glow = (f * f * 0.25).min(0.25);
                    floor.lerp(Rgb::new(0, 0, 0), shade).lerp(base, glow)
                };
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

    #[test]
    fn a_lone_ball_is_lit_like_a_sphere() {
        // One ball, rendered at 100 px per unit: brighter toward the light
        // (upper left) than on the far side, as a lit sphere would be.
        let mut sim = Metaball::new(400, 100, &mut Rng::new(1));
        sim.balls.truncate(1);
        let ball = sim.placed()[0];
        let mut frame = Frame::new(400, 100);
        sim.render(&mut frame, &Theme::default());
        let brightness = |dx: f32, dy: f32| {
            let (fx, fy) = (((ball.x + dx) * 100.0) as usize, ((ball.y + dy) * 100.0) as usize);
            let i = (fy.min(99) * 400 + fx.min(399)) * 4;
            frame.pixels[i] as u32 + frame.pixels[i + 1] as u32 + frame.pixels[i + 2] as u32
        };
        let r = ball.r;
        assert!(brightness(-r * 0.4, -r * 0.4) > brightness(r * 0.6, r * 0.6), "not lit from the upper left");
    }

    #[test]
    fn balls_melt_together_where_they_meet() {
        let a = Placed { x: 0.0, y: 0.0, r: 0.1, color: Rgb::new(255, 0, 0) };
        let b = Placed { x: 0.19, y: 0.0, r: 0.1, color: Rgb::new(0, 0, 255) };
        // Midway between two nearly touching balls the field bridges the gap.
        let (f, _, _, c) = sample(&[a, b], 0.095, 0.0);
        assert!(f >= 1.0);
        assert!(c.r > 60 && c.b > 60, "the join should blend both colors");
    }

    #[test]
    fn renders_opaque_changing_frames_at_odd_and_tiny_sizes() {
        let mut rng = Rng::new(2);
        let mut sim = Metaball::new(320, 96, &mut rng);
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
