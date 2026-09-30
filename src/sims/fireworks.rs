//! `fireworks` — a fireworks show over a city skyline. Rockets climb on a
//! trail of sparks and burst at the top of their arc into a sphere, a ring
//! or a drooping willow of embers, which fall under gravity and drag,
//! twinkle and fade. The sky is darkened a little each frame instead of
//! cleared, so everything leaves a glowing trail.
use super::{Input, Sim};
use crate::paint::{Frame, Rgb, Theme};
use crate::rng::Rng;
use std::f32::consts::TAU;

const GRAVITY: f32 = 0.5;
const LAUNCH_MIN: f32 = 0.35;
const LAUNCH_MAX: f32 = 1.0;
const MAX_SPARKS: usize = 1500;
const SKY: Rgb = Rgb::new(6, 6, 20);
/// How much of the way to the sky color each frame fades (per 1/60 s).
const TRAIL_FADE: f32 = 0.16;

#[derive(Clone, Copy, PartialEq)]
enum Shape {
    Sphere,
    Ring,
    Willow,
}

struct Rocket {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    hue: f32,
    shape: Shape,
}

struct Spark {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    life: f32,
    max_life: f32,
    hue: f32,
    drag: f32,
    twinkle: f32,
}

pub struct Fireworks {
    w: f32,
    h: f32,
    rockets: Vec<Rocket>,
    sparks: Vec<Spark>,
    next_launch: f32,
    skyline: Vec<f32>,
    t: f32,
}

impl Fireworks {
    pub fn new(w: usize, h: usize, rng: &mut Rng) -> Self {
        let mut s = Self {
            w: 1.0,
            h: 1.0,
            rockets: Vec::new(),
            sparks: Vec::new(),
            next_launch: 0.1,
            skyline: Vec::new(),
            t: 0.0,
        };
        s.resize(w, h, rng);
        s
    }

    fn aspect(&self) -> f32 {
        self.w / self.h
    }

    fn launch(&mut self, rng: &mut Rng) {
        let x = rng.range_f32(0.15, (self.aspect() - 0.15).max(0.15));
        let shape = [Shape::Sphere, Shape::Ring, Shape::Willow][rng.range_usize(0, 3)];
        self.rockets.push(Rocket {
            x,
            y: 1.0,
            vx: rng.range_f32(-0.12, 0.12),
            // Apex between 15% and 50% of the way down the sky.
            vy: -rng.range_f32(0.72, 0.92),
            hue: rng.range_f32(0.0, 360.0),
            shape,
        });
    }

    fn burst(&mut self, r: &Rocket, rng: &mut Rng) {
        let n = match r.shape {
            Shape::Sphere => 70,
            Shape::Ring => 48,
            Shape::Willow => 90,
        };
        for i in 0..n {
            if self.sparks.len() >= MAX_SPARKS {
                return;
            }
            let a = i as f32 / n as f32 * TAU + rng.range_f32(-0.05, 0.05);
            let speed = match r.shape {
                Shape::Sphere => rng.range_f32(0.1, 0.55),
                Shape::Ring => 0.45 + rng.range_f32(-0.02, 0.02),
                Shape::Willow => rng.range_f32(0.15, 0.4),
            };
            let (life, drag, hue) = match r.shape {
                Shape::Willow => (rng.range_f32(1.6, 2.4), 2.2, 42.0),
                _ => (
                    rng.range_f32(0.9, 1.5),
                    1.4,
                    r.hue + rng.range_f32(-15.0, 15.0),
                ),
            };
            self.sparks.push(Spark {
                x: r.x,
                y: r.y,
                vx: a.cos() * speed + r.vx,
                vy: a.sin() * speed * 0.9 + r.vy * 0.2,
                life,
                max_life: life,
                hue,
                drag,
                twinkle: rng.range_f32(0.0, TAU),
            });
        }
    }
}

impl Sim for Fireworks {
    fn resize(&mut self, w: usize, h: usize, rng: &mut Rng) {
        self.w = w.max(1) as f32;
        self.h = h.max(1) as f32;
        // Building heights across the bottom, one per 0.12 units.
        let n = ((self.aspect() / 0.12).ceil() as usize).max(1);
        self.skyline = (0..n).map(|_| rng.range_f32(0.05, 0.2)).collect();
    }

    fn step(&mut self, dt: f32, _: &Input, rng: &mut Rng) {
        let dt = if dt.is_finite() {
            dt.clamp(0.0, 0.1)
        } else {
            0.0
        };
        self.t = (self.t + dt) % 1000.0;
        self.next_launch -= dt;
        if self.next_launch <= 0.0 {
            self.launch(rng);
            self.next_launch = rng.range_f32(LAUNCH_MIN, LAUNCH_MAX);
        }
        let mut bursting = Vec::new();
        for (i, r) in self.rockets.iter_mut().enumerate() {
            r.vy += GRAVITY * dt;
            r.x += r.vx * dt;
            r.y += r.vy * dt;
            if r.vy >= -0.05 {
                bursting.push(i);
            }
        }
        for &i in bursting.iter().rev() {
            let r = self.rockets.remove(i);
            self.burst(&r, rng);
        }
        // Rocket trails: a few short-lived sparks behind each one.
        let trail: Vec<Spark> = self
            .rockets
            .iter()
            .map(|r| Spark {
                x: r.x,
                y: r.y,
                vx: rng.range_f32(-0.03, 0.03),
                vy: 0.05,
                life: 0.3,
                max_life: 0.3,
                hue: 40.0,
                drag: 3.0,
                twinkle: 0.0,
            })
            .collect();
        for s in trail {
            if self.sparks.len() < MAX_SPARKS {
                self.sparks.push(s);
            }
        }
        for s in &mut self.sparks {
            let k = (-s.drag * dt).exp();
            s.vx *= k;
            s.vy = s.vy * k + GRAVITY * dt;
            s.x += s.vx * dt;
            s.y += s.vy * dt;
            s.life -= dt;
            s.twinkle += dt * 25.0;
        }
        self.sparks.retain(|s| s.life > 0.0 && s.y < 1.1);
    }

    fn render(&mut self, frame: &mut Frame, _: &Theme) {
        let u = frame.h.max(1) as f32;
        frame.fade_to(SKY, TRAIL_FADE);
        for r in &self.rockets {
            frame.disc(
                r.x * u,
                r.y * u,
                (0.008 * u).max(1.0),
                Rgb::new(255, 230, 180),
                1.0,
            );
        }
        for s in &self.sparks {
            let k = (s.life / s.max_life).clamp(0.0, 1.0);
            let flicker = if k < 0.4 {
                0.5 + 0.5 * s.twinkle.sin().abs()
            } else {
                1.0
            };
            let c = Rgb::from_hsv(s.hue, 0.75 - 0.5 * k, 1.0);
            let r = (0.006 * u * (0.6 + k)).max(0.8);
            frame.disc(s.x * u, s.y * u, r, c, k.sqrt() * flicker);
        }
        // Skyline silhouette with a few lit windows.
        let bw = 0.12 * u;
        for (i, &bh) in self.skyline.iter().enumerate() {
            let x = i as f32 * bw;
            let top = (1.0 - bh) * u;
            frame.rect(x, top, bw * 0.92, bh * u + 1.0, Rgb::new(12, 12, 24), 1.0);
            let mut wy = top + 0.03 * u;
            let mut k = i;
            while wy < frame.h as f32 - 0.03 * u {
                for col in 0..2 {
                    k = k.wrapping_mul(1103515245).wrapping_add(12345);
                    if (k >> 16) % 3 == 0 {
                        frame.rect(
                            x + bw * (0.2 + col as f32 * 0.4),
                            wy,
                            (0.02 * u).max(1.0),
                            (0.02 * u).max(1.0),
                            Rgb::new(255, 210, 120),
                            0.6,
                        );
                    }
                }
                wy += 0.045 * u;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rockets_climb_then_burst_into_sparks() {
        let mut rng = Rng::new(1);
        let mut sim = Fireworks::new(320, 96, &mut rng);
        sim.launch(&mut rng);
        let start_y = sim.rockets[0].y;
        sim.step(1.0 / 60.0, &Input::default(), &mut rng);
        assert!(sim.rockets[0].y < start_y);
        let mut burst = false;
        for _ in 0..60 * 4 {
            sim.next_launch = 1e9;
            sim.step(1.0 / 60.0, &Input::default(), &mut rng);
            if sim.rockets.is_empty() && sim.sparks.len() > 30 {
                burst = true;
                break;
            }
        }
        assert!(burst, "the rocket never burst");
    }

    #[test]
    fn sparks_stay_bounded_and_die_out() {
        let mut rng = Rng::new(2);
        let mut sim = Fireworks::new(640, 144, &mut rng);
        for _ in 0..60 * 30 {
            sim.step(1.0 / 60.0, &Input::default(), &mut rng);
            assert!(sim.sparks.len() <= MAX_SPARKS);
        }
        for _ in 0..60 * 5 {
            sim.next_launch = 1e9;
            sim.step(1.0 / 60.0, &Input::default(), &mut rng);
        }
        assert!(sim.sparks.is_empty() && sim.rockets.is_empty());
    }

    #[test]
    fn renders_opaque_at_odd_and_tiny_sizes() {
        let mut rng = Rng::new(3);
        let mut sim = Fireworks::new(320, 96, &mut rng);
        for (w, h) in [(321, 97), (1, 1), (0, 0), (4, 7)] {
            sim.resize(w, h, &mut rng);
            for dt in [f32::NAN, -1.0, 1000.0, 0.1] {
                sim.step(dt, &Input::default(), &mut rng);
            }
            let mut frame = Frame::new(w, h);
            sim.render(&mut frame, &Theme::default());
            assert!(frame.pixels.chunks_exact(4).all(|p| p[3] == 255));
        }
    }
}
