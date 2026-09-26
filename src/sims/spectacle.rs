//! Eight bounded software effects. Shared lifecycle handling, distinct
//! geometry and shading for each material. No textures or external assets.
use super::{Input, Sim};
use crate::paint::{Frame, Rgb, Theme};
use crate::rng::Rng;
use std::f32::consts::{PI, TAU};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    Blackhole,
    Chrome,
    Plasma,
    Glass,
    Aurora,
    Ripple,
    Hologram,
    Supernova,
}

#[derive(Clone)]
struct Shard {
    vertices: [usize; 3],
    hue: f32,
}

pub struct Spectacle {
    effect: Effect,
    w: f32,
    h: f32,
    time: f32,
    phase: f32,
    shards: Vec<Shard>,
    glass_vertices: Vec<(f32, f32)>,
}

fn glow(distance: f32, width: f32) -> f32 {
    let x = distance / width;
    1.0 / (1.0 + x * x)
}
fn rgb(r: f32, g: f32, b: f32) -> Rgb {
    Rgb::new(
        r.clamp(0.0, 255.0) as u8,
        g.clamp(0.0, 255.0) as u8,
        b.clamp(0.0, 255.0) as u8,
    )
}

impl Spectacle {
    pub fn new(effect: Effect, w: usize, h: usize, rng: &mut Rng) -> Self {
        let mut shards = Vec::new();
        let mut glass_vertices = Vec::new();
        if effect == Effect::Glass {
            let mut points = [[(0.0, 0.0); 9]; 4];
            for (y, row) in points.iter_mut().enumerate() {
                for (x, point) in row.iter_mut().enumerate() {
                    let jx = if x == 0 || x == 8 {
                        0.0
                    } else {
                        rng.range_f32(-0.10, 0.10)
                    };
                    let jy = if y == 0 || y == 3 {
                        0.0
                    } else {
                        rng.range_f32(-0.10, 0.10)
                    };
                    *point = ((x as f32 + jx) / 8.0, (y as f32 + jy) / 3.0);
                }
            }
            glass_vertices.extend(points.into_iter().flatten());
            for y in 0..3 {
                for x in 0..8 {
                    let a = y * 9 + x;
                    let b = y * 9 + x + 1;
                    let c = (y + 1) * 9 + x;
                    let d = (y + 1) * 9 + x + 1;
                    for vertices in [[a, b, c], [b, d, c]] {
                        shards.push(Shard {
                            vertices,
                            hue: rng.range_f32(0.0, 360.0),
                        });
                    }
                }
            }
        }
        Self {
            effect,
            w: w.max(1) as f32,
            h: h.max(1) as f32,
            time: 0.0,
            phase: rng.range_f32(0.0, TAU),
            shards,
            glass_vertices,
        }
    }

    // Camera motion is computed once per frame, not once per shaded pixel.
    // Supernova uses a synthetic 144 BPM kick; no microphone/audio is needed.
    fn camera(&self) -> (f32, f32, f32, f32) {
        let t = self.time;
        match self.effect {
            Effect::Blackhole => {
                let orbit = t * 1.8 + self.phase;
                (
                    0.48 * orbit.sin(),
                    0.11 * orbit.cos(),
                    1.65 + 1.0 * orbit.cos(),
                    0.65 * orbit.sin(),
                )
            }
            Effect::Supernova => {
                let beat = (t * 2.4).fract();
                let kick = (-beat * 9.0).exp();
                (0.0, 0.0, 1.35 + kick * 1.2, t * 0.65)
            }
            _ => (
                0.0,
                0.0,
                1.05 + 0.18 * (t * 3.0).sin(),
                0.18 * (t * 2.0 + self.phase).sin(),
            ),
        }
    }

    fn shade(&self, x: f32, y: f32, camera: (f32, f32, f32, f32, f32)) -> Rgb {
        let t = self.time * 2.6 + self.phase;
        let (cx, cy, zoom, cos_roll, sin_roll) = camera;
        let sx = ((x - 0.5) * self.w / self.h - cx) / zoom;
        let sy = (y - 0.5 - cy) / zoom;
        // Rotate the scene about the moving camera center.
        let u = sx * cos_roll - sy * sin_roll;
        let v = sx * sin_roll + sy * cos_roll;
        let screen_x = x;
        let screen_y = y;
        let x = u * self.h / self.w + 0.5;
        let y = v + 0.5;
        let r = (u * u + v * v).sqrt();
        let a = v.atan2(u);
        // Distance from the center of the button itself, unaffected by
        // the camera's zoom and roll.
        let px = (screen_x - 0.5) * self.w / self.h;
        let py = screen_y - 0.5;
        let pr = (px * px + py * py).sqrt();
        match self.effect {
            Effect::Blackhole => {
                let hole = 0.16;
                let warp = (r - hole).abs();
                let lens = glow(warp, 0.008) * 1.8;
                let tilt = v + u * 0.14;
                let disk_r = (u * u + tilt * tilt * 24.0).sqrt();
                let band = glow(disk_r - 0.36, 0.075);
                let streak = 0.5 + 0.5 * (disk_r * 160.0 - a * 9.0 - t * 4.0).sin();
                let disk = band * (0.4 + streak * 0.6);
                // Lensed upper image of the far side of the accretion disk.
                let arc = glow(r - hole * 1.3, 0.016) * if v < 0.0 { 0.8 } else { 0.15 };
                if r < hole {
                    return rgb(1.0, 1.0, 5.0);
                }
                rgb(
                    4.0 + disk * 250.0 + lens * 220.0 + arc * 180.0,
                    3.0 + disk * 135.0 + lens * 150.0 + arc * 110.0,
                    12.0 + disk * 55.0 + lens * 100.0 + arc * 180.0,
                )
            }
            Effect::Chrome => {
                let wave = (u * 5.0 + t).sin() + (v * 11.0 - t * 1.3).cos();
                let nx = (u * 6.0 + wave + 1.0).sin();
                let ny = (v * 8.0 + wave * 0.7 - t * 0.8).cos();
                let reflection = (nx * 1.7 + ny * 1.2).sin();
                let metal = 28.0 + (reflection * 0.5 + 0.5).powf(3.0) * 190.0;
                let strip = glow(reflection - 0.35, 0.045) * 140.0;
                let cyan = glow(nx + ny - 0.5, 0.13) * 100.0;
                let pink = glow(nx - ny + 0.5, 0.13) * 100.0;
                rgb(
                    metal + strip + pink,
                    metal + strip + cyan * 0.8,
                    metal + strip + cyan + pink,
                )
            }
            Effect::Plasma => {
                let mut bolt = 0.0;
                // Filaments bridge the two edges and converge on the center.
                for k in 0..6 {
                    let k = k as f32;
                    let envelope = (PI * x).sin();
                    let target = 0.5 + (k - 2.5) * 0.10 * (1.0 - envelope);
                    let noise = (x * 49.0 + t * 9.0 + k * 2.0).sin() * 0.025
                        + (x * 117.0 - t * 13.0 + k).sin() * 0.012;
                    let path = target + noise + (x * 13.0 + k + t * 2.0).sin() * 0.12 * envelope;
                    bolt += glow(y - path, 0.004);
                }
                let core = glow(pr, 0.035) * 0.4;
                rgb(
                    8.0 + bolt * 150.0 + core * 150.0,
                    3.0 + bolt * 100.0 + core * 220.0,
                    22.0 + bolt * 250.0 + core * 240.0,
                )
            }
            Effect::Aurora => {
                let mut green = 0.0;
                let mut violet = 0.0;
                for k in 0..4 {
                    let k = k as f32;
                    let fold = (x * 9.0 + t * 0.4 + k).sin() * 0.10
                        + (x * 19.0 - t * 0.3 + k * 1.7).sin() * 0.035;
                    let curtain = 0.32 + k * 0.09 + fold;
                    let d = y - curtain;
                    let beam = (0.6 + 0.4 * (x * 150.0 + (x * 23.0 + t).sin() * 8.0).sin())
                        * if d < 0.0 {
                            (d * 9.0).exp()
                        } else {
                            (-d * 45.0).exp()
                        };
                    green += beam * (1.0 - k * 0.16);
                    violet += beam * k * 0.25;
                }
                rgb(
                    3.0 + violet * 100.0,
                    8.0 + green * 100.0,
                    22.0 + green * 65.0 + violet * 130.0,
                )
            }
            Effect::Ripple => {
                let wave =
                    (r * 28.0 - t * 3.0).sin() * 0.035 + (u * 9.0 + v * 13.0 + t).sin() * 0.025;
                let nx = (u * 13.0 + wave * 25.0).sin();
                let ny = (v * 18.0 - t + wave * 30.0).cos();
                let caustic = glow(nx + ny, 0.065);
                rgb(
                    2.0 + caustic * 95.0,
                    15.0 + caustic * 190.0,
                    27.0 + caustic * 220.0,
                )
            }
            Effect::Hologram => {
                let rot = t * 0.35;
                let qx = u * rot.cos() - v * rot.sin();
                let qy = u * rot.sin() + v * rot.cos();
                let size = 0.23;
                let mut wire = 0.0;
                for k in 0..5 {
                    let offset = k as f32 * 0.025;
                    let box_d = ((qx + offset).abs().max((qy - offset).abs()) - size).abs();
                    wire += glow(box_d, 0.0035);
                }
                let scan = glow((y - t * 0.2).rem_euclid(1.0) - 0.5, 0.012);
                let grid = glow((x * 24.0).fract().min(1.0 - (x * 24.0).fract()), 0.025)
                    + glow((y * 10.0).fract().min(1.0 - (y * 10.0).fract()), 0.025);
                let stripe = 0.65 + 0.35 * (y * self.h * PI).cos().abs();
                rgb(
                    2.0 + wire * 50.0,
                    (12.0 + wire * 200.0 + grid * 30.0 + scan * 65.0) * stripe,
                    (20.0 + wire * 240.0 + grid * 40.0 + scan * 95.0) * stripe,
                )
            }
            Effect::Supernova => {
                let radius = 0.20;
                let edge =
                    radius + (a * 9.0 + t * 3.0).sin() * 0.018 + (a * 17.0 - t * 4.0).sin() * 0.008;
                let corona = glow(r - edge, 0.035) * 1.1;
                let surface = if r < edge {
                    0.65 + 0.35 * (u * 45.0 + (v * 37.0 + t).sin() * 3.0 + t * 3.0).sin()
                } else {
                    0.0
                };
                let ray = ((a * 23.0 + t * 0.4).sin().abs()).powf(14.0) * glow(r - 0.3, 0.2) * 0.25;
                rgb(
                    8.0 + surface * 230.0 + corona * 255.0 + ray * 160.0,
                    3.0 + surface * 160.0 + corona * 100.0 + ray * 65.0,
                    12.0 + surface * 65.0 + corona * 28.0 + ray * 70.0,
                )
            }
            Effect::Glass => unreachable!("glass uses triangle rasterization"),
        }
    }

    // All incident triangles reference the same vertex. Outer vertices stay
    // on their canvas edge, and each point stays within 0.22 of its grid cell
    // coordinate (including initial jitter), so triangles cannot turn inside out.
    fn glass_mesh(&self) -> Vec<(f32, f32)> {
        self.glass_vertices
            .iter()
            .enumerate()
            .map(|(i, &(x, y))| {
                let phase = self.phase + i as f32 * 1.73;
                let dx = if i % 9 == 0 || i % 9 == 8 {
                    0.0
                } else {
                    (self.time * 2.8 + phase).sin() * 0.12 / 8.0
                };
                let dy = if i / 9 == 0 || i / 9 == 3 {
                    0.0
                } else {
                    (self.time * 2.3 + phase * 1.37).cos() * 0.12 / 3.0
                };
                (x + dx, y + dy)
            })
            .collect()
    }

    fn render_glass(&self, frame: &mut Frame) {
        frame.fill(Rgb::new(5, 5, 18));
        let mesh = self.glass_mesh();
        for shard in &self.shards {
            let points = shard.vertices.map(|i| (mesh[i].0 * frame.w as f32, mesh[i].1 * frame.h as f32));
            let min_x = points
                .iter()
                .map(|p| p.0)
                .fold(f32::INFINITY, f32::min)
                .floor()
                .max(0.0) as usize;
            let max_x = (points
                .iter()
                .map(|p| p.0)
                .fold(f32::NEG_INFINITY, f32::max)
                .ceil()
                .max(0.0) as usize)
                .min(frame.w);
            let min_y = points
                .iter()
                .map(|p| p.1)
                .fold(f32::INFINITY, f32::min)
                .floor()
                .max(0.0) as usize;
            let max_y = (points
                .iter()
                .map(|p| p.1)
                .fold(f32::NEG_INFINITY, f32::max)
                .ceil()
                .max(0.0) as usize)
                .min(frame.h);
            let cross = |a: (f32, f32), b: (f32, f32), p: (f32, f32)| {
                (b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0)
            };
            let color = Rgb::from_hsv(shard.hue, 0.7, 0.85);
            for y in min_y..max_y {
                for x in min_x..max_x {
                    let p = (x as f32 + 0.5, y as f32 + 0.5);
                    if (0..3).all(|i| cross(points[i], points[(i + 1) % 3], p) >= 0.0) {
                        let light = 0.5
                            + 0.5
                                * (x as f32 * 0.025
                                    + y as f32 * 0.02
                                    + self.time * 2.6
                                    + shard.hue)
                                    .sin()
                                    .abs();
                        let i = (y * frame.w + x) * 4;
                        frame.pixels[i] = (color.r as f32 * light) as u8;
                        frame.pixels[i + 1] = (color.g as f32 * light) as u8;
                        frame.pixels[i + 2] = (color.b as f32 * light) as u8;
                    }
                }
            }
            for i in 0..3 {
                frame.line(
                    points[i],
                    points[(i + 1) % 3],
                    1.2,
                    Rgb::new(220, 235, 255),
                    0.65,
                );
            }
        }
    }
}

impl Sim for Spectacle {
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
        self.time = (self.time + dt).rem_euclid(600.0 * TAU);
    }
    fn render(&mut self, frame: &mut Frame, _: &Theme) {
        if self.effect == Effect::Glass {
            self.render_glass(frame);
            return;
        }
        // Adaptive blocks bound shading work even on unusually large buttons.
        let block = ((frame.w as f64 * frame.h as f64 / 18000.0).sqrt().ceil() as usize).max(2);
        let (cx, cy, zoom, roll) = self.camera();
        let camera = (cx, cy, zoom, roll.cos(), roll.sin());
        for y in (0..frame.h).step_by(block) {
            for x in (0..frame.w).step_by(block) {
                let c = self.shade(
                    (x as f32 + 0.5) / frame.w as f32,
                    (y as f32 + 0.5) / frame.h as f32,
                    camera,
                );
                for yy in y..(y + block).min(frame.h) {
                    for xx in x..(x + block).min(frame.w) {
                        let i = (yy * frame.w + xx) * 4;
                        frame.pixels[i..i + 4].copy_from_slice(&[c.r, c.g, c.b, 255]);
                    }
                }
            }
        }
    }
    fn preferred_fps(&self) -> f32 {
        60.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const EFFECTS: [Effect; 8] = [
        Effect::Blackhole,
        Effect::Chrome,
        Effect::Plasma,
        Effect::Glass,
        Effect::Aurora,
        Effect::Ripple,
        Effect::Hologram,
        Effect::Supernova,
    ];
    #[test]
    fn glass_junctions_move_while_edges_stay_fixed_and_faces_cover_canvas() {
        for seed in [1, 33, 42, 100] {
            let mut sim = Spectacle::new(Effect::Glass, 320, 96, &mut Rng::new(seed));
            let initial = sim.glass_mesh();
            sim.time = 0.5;
            assert_ne!(initial[10], sim.glass_mesh()[10]);
            for step in 0..180 {
                sim.time = step as f32 * 0.1;
                let mesh = sim.glass_mesh();
                let mut area = 0.0;
                for (i, &(x, y)) in mesh.iter().enumerate() {
                    if i % 9 == 0 {
                        assert_eq!(x, 0.0);
                    }
                    if i % 9 == 8 {
                        assert_eq!(x, 1.0);
                    }
                    if i / 9 == 0 {
                        assert_eq!(y, 0.0);
                    }
                    if i / 9 == 3 {
                        assert_eq!(y, 1.0);
                    }
                }
                for shard in &sim.shards {
                    let [a, b, c] = shard.vertices.map(|i| mesh[i]);
                    let signed = ((b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0)) * 0.5;
                    assert!(signed > 0.0, "glass triangle folded over");
                    area += signed;
                }
                assert!((area - 1.0).abs() < 1e-5, "mesh must cover the full canvas");
            }
        }
    }

    #[test]
    fn blackhole_orbits_and_changes_distance_without_clicking() {
        let mut sim = Spectacle::new(Effect::Blackhole, 320, 96, &mut Rng::new(1));
        sim.phase = 0.0;
        let near = sim.camera();
        sim.time = PI / 1.8;
        let far = sim.camera();
        assert!(near.2 > far.2 * 3.0, "orbit must visibly change distance");
        sim.time = PI / 3.6;
        let side = sim.camera();
        assert!((side.0 - near.0).abs() > 0.4, "camera must travel sideways");
        assert!(side.3.abs() > 0.5, "camera must roll during its orbit");
    }

    #[test]
    fn supernova_kicks_and_recovers_every_144_bpm_beat() {
        let mut sim = Spectacle::new(Effect::Supernova, 320, 96, &mut Rng::new(1));
        let kick = sim.camera().2;
        sim.time = 0.35;
        assert!(kick > sim.camera().2 * 1.7);
        sim.time = 1.0 / 2.4;
        assert!((sim.camera().2 - kick).abs() < 0.01);
    }

    #[test]
    fn every_effect_animates_at_rest() {
        for effect in EFFECTS {
            let mut rng = Rng::new(42);
            let mut sim = Spectacle::new(effect, 161, 49, &mut rng);
            let mut frame = Frame::new(161, 49);
            sim.render(&mut frame, &Theme::default());
            let initial = frame.pixels.clone();
            for _ in 0..10 {
                sim.step(0.05, &Input::default(), &mut rng);
            }
            sim.render(&mut frame, &Theme::default());
            assert!(initial != frame.pixels, "{effect:?} must animate at rest");
        }
    }
    #[test]
    fn invalid_dt_resize_and_long_running_state_stay_safe() {
        for effect in EFFECTS {
            let mut rng = Rng::new(1);
            let mut sim = Spectacle::new(effect, 0, 0, &mut rng);
            for (w, h) in [(0, 0), (1, 7), (7, 1), (65, 33)] {
                sim.resize(w, h, &mut rng);
                for dt in [f32::NAN, f32::INFINITY, -1.0, 10000.0] {
                    sim.step(dt, &Input::default(), &mut rng);
                }
                let mut frame = Frame::new(w, h);
                sim.render(&mut frame, &Theme::default());
                assert!(frame.pixels.chunks_exact(4).all(|p| p[3] == 255));
            }
            for _ in 0..100_000 {
                sim.step(0.1, &Input::default(), &mut rng);
            }
            assert!(sim.time.is_finite() && sim.time < 600.0 * TAU);
            assert!(sim.shards.len() <= 48);
        }
    }
    #[test]
    fn effects_have_distinct_images_and_repeatable_seeds() {
        let mut images = Vec::new();
        for effect in EFFECTS {
            let mut a = Spectacle::new(effect, 80, 40, &mut Rng::new(9));
            let mut b = Spectacle::new(effect, 80, 40, &mut Rng::new(9));
            let mut fa = Frame::new(80, 40);
            let mut fb = Frame::new(80, 40);
            a.render(&mut fa, &Theme::default());
            b.render(&mut fb, &Theme::default());
            assert_eq!(fa.pixels, fb.pixels);
            assert!(
                !images.contains(&fa.pixels),
                "{effect:?} should have distinct geometry"
            );
            images.push(fa.pixels);
        }
    }
}
