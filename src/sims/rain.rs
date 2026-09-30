//! `rain` — rain on a window at night. Blurry city lights glow behind the
//! glass; small droplets bead up all over it, and now and then a big drop
//! runs down, wobbling, swallowing the droplets in its path (and growing
//! with them) and leaving a thin trail of new beads behind it.
use super::{Input, Sim};
use crate::paint::{Frame, Rgb, Theme};
use crate::rng::Rng;

/// Beads per unit of glass area (frame height = 1).
const BEADS_PER_AREA: f32 = 60.0;
const BEAD_MIN: f32 = 0.006;
const BEAD_MAX: f32 = 0.018;
const RUNNER_MIN: f32 = 0.035;
const RUNNER_MAX: f32 = 0.055;
const RUNNER_GAP_MIN: f32 = 0.3;
const RUNNER_GAP_MAX: f32 = 1.1;
const MAX_RUNNERS: usize = 12;
/// Runners slide faster the bigger they are.
const SLIDE_PER_RADIUS: f32 = 22.0;
/// Seconds between the beads a runner leaves behind; jittered so the trail
/// doesn't read as a dotted line.
const TRAIL_EVERY: f32 = 0.09;
const LIGHTS: usize = 14;

#[derive(Clone, Copy)]
struct Drop {
    x: f32,
    y: f32,
    r: f32,
}

struct Runner {
    drop: Drop,
    wobble: f32,
    since_trail: f32,
}

struct Light {
    x: f32,
    y: f32,
    r: f32,
    hue: f32,
    phase: f32,
}

pub struct Rain {
    w: f32,
    h: f32,
    beads: Vec<Drop>,
    runners: Vec<Runner>,
    lights: Vec<Light>,
    next_runner: f32,
    t: f32,
}

impl Rain {
    pub fn new(w: usize, h: usize, rng: &mut Rng) -> Self {
        let mut s = Self {
            w: 1.0,
            h: 1.0,
            beads: Vec::new(),
            runners: Vec::new(),
            lights: Vec::new(),
            next_runner: 0.2,
            t: 0.0,
        };
        s.resize(w, h, rng);
        s
    }

    fn aspect(&self) -> f32 {
        self.w / self.h
    }

    fn max_beads(&self) -> usize {
        ((self.aspect() * BEADS_PER_AREA) as usize).clamp(10, 600)
    }

    fn new_bead(&self, rng: &mut Rng) -> Drop {
        Drop {
            x: rng.range_f32(0.0, self.aspect()),
            y: rng.range_f32(0.0, 1.0),
            r: rng.range_f32(BEAD_MIN, BEAD_MAX),
        }
    }
}

impl Sim for Rain {
    fn resize(&mut self, w: usize, h: usize, rng: &mut Rng) {
        self.w = w.max(1) as f32;
        self.h = h.max(1) as f32;
        let aspect = self.aspect();
        self.beads = (0..self.max_beads()).map(|_| self.new_bead(rng)).collect();
        self.runners.clear();
        self.lights = (0..LIGHTS)
            .map(|_| Light {
                x: rng.range_f32(0.0, aspect),
                y: rng.range_f32(0.35, 1.0),
                r: rng.range_f32(0.08, 0.2),
                hue: [30.0, 45.0, 200.0, 330.0, 15.0][rng.range_usize(0, 5)]
                    + rng.range_f32(-10.0, 10.0),
                phase: rng.range_f32(0.0, 6.3),
            })
            .collect();
    }

    fn step(&mut self, dt: f32, _: &Input, rng: &mut Rng) {
        let dt = if dt.is_finite() {
            dt.clamp(0.0, 0.1)
        } else {
            0.0
        };
        self.t = (self.t + dt) % 1000.0;
        // New beads keep landing, up to the cap.
        let max = self.max_beads();
        let landing = (max as f32 * dt * 0.6).ceil() as usize;
        for _ in 0..landing {
            if self.beads.len() < max {
                let b = self.new_bead(rng);
                self.beads.push(b);
            }
        }
        self.next_runner -= dt;
        if self.next_runner <= 0.0 && self.runners.len() < MAX_RUNNERS {
            self.next_runner = rng.range_f32(RUNNER_GAP_MIN, RUNNER_GAP_MAX);
            let x = rng.range_f32(0.0, self.aspect());
            self.runners.push(Runner {
                drop: Drop {
                    x,
                    y: rng.range_f32(-0.05, 0.4),
                    r: rng.range_f32(RUNNER_MIN, RUNNER_MAX),
                },
                wobble: rng.range_f32(0.0, 6.3),
                since_trail: 0.0,
            });
        }
        let mut trail = Vec::new();
        for run in &mut self.runners {
            let d = &mut run.drop;
            d.y += d.r * SLIDE_PER_RADIUS * dt;
            run.wobble += dt * 5.0;
            d.x += run.wobble.sin() * 0.03 * dt;
            // Swallow the beads it runs over, growing a little each time.
            let mut grown = 0.0;
            self.beads.retain(|b| {
                let hit = (b.x - d.x).hypot(b.y - d.y) < d.r + b.r;
                if hit {
                    grown += b.r * b.r;
                }
                !hit
            });
            d.r = (d.r * d.r + grown * 0.5).sqrt().min(RUNNER_MAX * 1.4);
            run.since_trail += dt;
            if run.since_trail >= TRAIL_EVERY {
                run.since_trail = rng.range_f32(-TRAIL_EVERY, 0.0);
                trail.push(Drop {
                    x: d.x + rng.range_f32(-0.01, 0.01),
                    y: d.y - d.r * 1.2,
                    r: rng.range_f32(BEAD_MIN * 0.6, BEAD_MIN * 1.2),
                });
            }
        }
        self.beads.extend(trail);
        self.runners.retain(|r| r.drop.y - r.drop.r < 1.05);
        let max = self.max_beads() + 80;
        if self.beads.len() > max {
            self.beads.drain(0..self.beads.len() - max);
        }
    }

    fn render(&mut self, frame: &mut Frame, _: &Theme) {
        let u = frame.h.max(1) as f32;
        // Night street through wet glass: dark gradient and soft bokeh.
        let (top, bottom) = (Rgb::new(10, 14, 30), Rgb::new(30, 26, 44));
        let denom = (frame.h.max(2) - 1) as f32;
        for y in 0..frame.h {
            let c = top.lerp(bottom, y as f32 / denom);
            let row = y * frame.w * 4;
            for px in frame.pixels[row..row + frame.w * 4].chunks_exact_mut(4) {
                px.copy_from_slice(&[c.r, c.g, c.b, 255]);
            }
        }
        for l in &self.lights {
            let pulse = 0.75 + 0.25 * (self.t * 0.8 + l.phase).sin();
            let c = Rgb::from_hsv(l.hue, 0.7, 1.0);
            for (k, a) in [(1.0, 0.1), (0.7, 0.12), (0.45, 0.16)] {
                frame.disc(l.x * u, l.y * u, l.r * k * u, c, a * pulse);
            }
        }
        // Each drop is a little lens: dark rim, light body, a bright glint.
        let draw = |frame: &mut Frame, d: &Drop| {
            let (x, y, r) = (d.x * u, d.y * u, (d.r * u).max(0.8));
            frame.disc(x, y + r * 0.15, r, Rgb::new(0, 0, 0), 0.35);
            frame.disc(x, y, r * 0.85, Rgb::new(170, 190, 220), 0.35);
            frame.disc(
                x - r * 0.3,
                y - r * 0.35,
                (r * 0.3).max(0.6),
                Rgb::new(255, 255, 255),
                0.7,
            );
        };
        for b in &self.beads {
            draw(frame, b);
        }
        for r in &self.runners {
            draw(frame, &r.drop);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runners_slide_down_and_swallow_beads() {
        let mut rng = Rng::new(1);
        let mut sim = Rain::new(320, 96, &mut rng);
        sim.runners.push(Runner {
            drop: Drop {
                x: 1.0,
                y: 0.1,
                r: RUNNER_MIN,
            },
            wobble: 0.0,
            since_trail: 0.0,
        });
        sim.beads.push(Drop {
            x: 1.0,
            y: 0.2,
            r: BEAD_MAX,
        });
        let before = sim.beads.len();
        for _ in 0..10 {
            sim.step(1.0 / 60.0, &Input::default(), &mut rng);
        }
        let run = &sim.runners[0];
        assert!(run.drop.y > 0.1);
        assert!(
            run.drop.r > RUNNER_MIN,
            "didn't grow from what it swallowed"
        );
        assert!(sim.beads.len() >= before.min(sim.max_beads()) - 5);
    }

    #[test]
    fn drop_counts_stay_bounded_over_a_long_run() {
        let mut rng = Rng::new(2);
        let mut sim = Rain::new(640, 144, &mut rng);
        for _ in 0..60 * 60 {
            sim.step(1.0 / 60.0, &Input::default(), &mut rng);
            assert!(sim.runners.len() <= MAX_RUNNERS);
            assert!(sim.beads.len() <= sim.max_beads() + 80);
        }
    }

    #[test]
    fn renders_opaque_changing_frames_at_odd_and_tiny_sizes() {
        let mut rng = Rng::new(3);
        let mut sim = Rain::new(320, 96, &mut rng);
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
            sim.step(1.0 / 60.0, &Input::default(), &mut rng);
        }
        sim.render(&mut frame, &Theme::default());
        assert_ne!(before, frame.pixels);
    }
}
