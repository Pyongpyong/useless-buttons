//! Shooter: a side-scrolling shoot-'em-up. Your fighter sits on the left
//! and fires on its own; press the top half of the button to climb a
//! step, the bottom half to dive one. Drones can be shot down; meteors
//! can't, and come at whatever height you're flying, so they have to be
//! dodged. Touch either and it's game over. Reach the finish line.
use super::{
    draw_cleared_flash, draw_game_over, draw_progress, sanitize_dt, substeps, unit, vgradient,
    Confetti, Phase, GOAL,
};
use crate::paint::{Frame, Rgb, Theme};
use crate::rng::Rng;
use crate::sims::{Input, Sim};

const SHIP_X: f32 = 0.35;
const SHIP_LEN: f32 = 0.16;
const SHIP_H: f32 = 0.07;
const TOP: f32 = 0.2;
const BOTTOM: f32 = 0.9;
/// One press moves the ship this far up or down.
const STEP_Y: f32 = 0.18;
const GLIDE: f32 = 1.6;
const FIRE_SEC: f32 = 0.16;
/// Twin cannons, this far above and below the ship's center line: between
/// them they cover every drone that could touch the ship.
const GUN_OFFSET: f32 = 0.025;
/// How far above or below a shot's line it still hits a drone. With the
/// twin cannons this covers more than the band a drone can hit the ship
/// in, with room for its wobble between shots.
const SHOT_REACH: f32 = DRONE_R * 1.8;
const SHOT_SPEED: f32 = 3.2;
/// Distance to the finish, in units; the pips mark tenths of it.
const FINISH: f32 = 26.0;
const SCROLL: f32 = 1.0;
/// No new enemies this close to the finish.
const FINISH_CLEAR: f32 = 3.0;
const DRONE_R: f32 = 0.04;
const DRONE_GAP_MIN: f32 = 0.5;
const DRONE_GAP_MAX: f32 = 0.9;
const METEOR_R: f32 = 0.07;
const METEOR_SPEED: f32 = 1.2;
const METEOR_GAP_MIN: f32 = 1.6;
const METEOR_GAP_MAX: f32 = 2.4;
const FIRST_METEOR: f32 = 1.2;

struct Drone {
    x: f32,
    y: f32,
    speed: f32,
    wobble: f32,
}

pub struct Shooter {
    w: f32,
    h: f32,
    y: f32,
    target: f32,
    dist: f32,
    shots: Vec<(f32, f32)>,
    next_shot: f32,
    drones: Vec<Drone>,
    meteors: Vec<(f32, f32, f32)>,
    next_drone: f32,
    next_meteor: f32,
    /// Explosions and sparks: position, age, size.
    booms: Vec<(f32, f32, f32, f32)>,
    t: f32,
    phase: Phase,
    confetti: Confetti,
    next_burst: f32,
}

impl Shooter {
    pub fn new(w: usize, h: usize, _: &mut Rng) -> Self {
        let mut s = Self {
            w: w.max(1) as f32,
            h: h.max(1) as f32,
            y: 0.0,
            target: 0.0,
            dist: 0.0,
            shots: Vec::new(),
            next_shot: 0.0,
            drones: Vec::new(),
            meteors: Vec::new(),
            next_drone: 0.8,
            next_meteor: FIRST_METEOR,
            booms: Vec::new(),
            t: 0.0,
            phase: Phase::Playing,
            confetti: Confetti::default(),
            next_burst: 0.0,
        };
        s.reset();
        s
    }

    fn reset(&mut self) {
        self.y = (TOP + BOTTOM) * 0.5;
        self.target = self.y;
        self.dist = 0.0;
        self.shots.clear();
        self.drones.clear();
        self.meteors.clear();
        self.booms.clear();
        self.next_drone = 0.8;
        self.next_meteor = FIRST_METEOR;
        self.phase = Phase::Playing;
        self.confetti.clear();
    }

    fn aspect(&self) -> f32 {
        self.w / self.h
    }

    fn progress(&self) -> u32 {
        ((self.dist / FINISH * GOAL as f32) as u32).min(GOAL)
    }

    fn hits_ship(&self, x: f32, y: f32, r: f32) -> bool {
        (x - SHIP_X).abs() < SHIP_LEN * 0.5 + r * 0.8 && (y - self.y).abs() < SHIP_H * 0.5 + r * 0.8
    }

    fn step_playing(&mut self, dt: f32, input: &Input, rng: &mut Rng) {
        for p in input.presses().iter().filter(|p| p.y.is_finite()) {
            let dir = if p.y < 0.5 { -1.0 } else { 1.0 };
            self.target = (self.target + dir * STEP_Y).clamp(TOP, BOTTOM);
        }
        let (n, h) = substeps(dt);
        for _ in 0..n {
            self.t += h;
            self.dist += SCROLL * h;
            let step = GLIDE * h;
            self.y += (self.target - self.y).clamp(-step, step);

            self.next_shot -= h;
            if self.next_shot <= 0.0 {
                self.next_shot += FIRE_SEC;
                for dy in [-GUN_OFFSET, GUN_OFFSET] {
                    self.shots.push((SHIP_X + SHIP_LEN * 0.3, self.y + dy));
                }
            }
            let far = self.aspect() + 0.2;
            for s in &mut self.shots {
                s.0 += SHOT_SPEED * h;
            }

            let spawning = self.dist < FINISH - FINISH_CLEAR;
            self.next_drone -= h;
            if spawning && self.next_drone <= 0.0 {
                self.next_drone = rng.range_f32(DRONE_GAP_MIN, DRONE_GAP_MAX);
                self.drones.push(Drone {
                    x: far,
                    y: rng.range_f32(TOP, BOTTOM),
                    speed: rng.range_f32(0.8, 1.1),
                    wobble: rng.range_f32(0.0, 6.3),
                });
            }
            self.next_meteor -= h;
            if spawning && self.next_meteor <= 0.0 {
                // Aimed at wherever you're flying right now.
                self.next_meteor = rng.range_f32(METEOR_GAP_MIN, METEOR_GAP_MAX);
                self.meteors.push((far, self.y, rng.range_f32(0.0, 6.3)));
            }
            for d in &mut self.drones {
                d.x -= d.speed * h;
                d.wobble += h * 3.0;
            }
            for m in &mut self.meteors {
                m.0 -= METEOR_SPEED * h;
                m.2 += h;
            }

            // Shots: drones pop, meteors just eat them.
            let mut kept = Vec::with_capacity(self.shots.len());
            for s in self.shots.drain(..) {
                if s.0 > far {
                    continue;
                }
                if let Some(i) = self.drones.iter().position(|d| (d.x - s.0).abs() < DRONE_R * 1.3 && (d.y + d.wobble.sin() * 0.04 - s.1).abs() < SHOT_REACH) {
                    let d = self.drones.remove(i);
                    self.booms.push((d.x, d.y + d.wobble.sin() * 0.04, 0.0, 1.0));
                    continue;
                }
                if self.meteors.iter().any(|m| (m.0 - s.0).hypot(m.1 - s.1) < METEOR_R) {
                    self.booms.push((s.0, s.1, 0.0, 0.3));
                    continue;
                }
                kept.push(s);
            }
            self.shots = kept;
            self.drones.retain(|d| d.x > -0.2);
            self.meteors.retain(|m| m.0 > -0.2);

            let drone_hit = self.drones.iter().any(|d| self.hits_ship(d.x, d.y + d.wobble.sin() * 0.04, DRONE_R));
            let meteor_hit = self.meteors.iter().any(|m| self.hits_ship(m.0, m.1, METEOR_R));
            if drone_hit || meteor_hit {
                self.booms.push((SHIP_X, self.y, 0.0, 1.6));
                self.phase = Phase::Over(0.0);
                return;
            }
            if self.dist >= FINISH {
                self.phase = Phase::Cleared(0.0);
                self.confetti.burst(SHIP_X, self.y, 90, rng);
                self.next_burst = 0.5;
                return;
            }
        }
    }
}

impl Sim for Shooter {
    fn resize(&mut self, w: usize, h: usize, _: &mut Rng) {
        self.w = w.max(1) as f32;
        self.h = h.max(1) as f32;
    }

    fn step(&mut self, dt: f32, input: &Input, rng: &mut Rng) {
        let dt = sanitize_dt(dt);
        self.confetti.step(dt);
        for b in &mut self.booms {
            b.2 += dt;
        }
        self.booms.retain(|b| b.2 < 0.4);
        match self.phase {
            Phase::Playing => self.step_playing(dt, input, rng),
            Phase::Over(_) => {
                if self.phase.advance(dt) {
                    self.reset();
                }
            }
            Phase::Cleared(t) => {
                self.phase.advance(dt);
                self.t += dt;
                self.dist += SCROLL * dt;
                for p in input.presses().iter().filter(|p| p.is_pointed()) {
                    self.confetti.burst(p.x * self.aspect(), p.y, 30, rng);
                }
                if t < 3.0 && t >= self.next_burst {
                    self.next_burst += 0.6;
                    self.confetti.burst(rng.range_f32(0.2, self.aspect() - 0.2), 0.6, 30, rng);
                }
            }
        }
    }

    fn render(&mut self, frame: &mut Frame, _: &Theme) {
        let u = unit(frame);
        vgradient(frame, Rgb::new(6, 6, 22), Rgb::new(16, 8, 36));
        // Two layers of stars streaming past at different speeds.
        for (layer, speed, alpha) in [(0u32, 0.3, 0.4), (1, 0.9, 0.8)] {
            for i in 0..30u32 {
                let seed = i.wrapping_mul(2654435761).wrapping_add(layer * 977);
                let hx = (seed >> 8) as f32 / 16_777_216.0;
                let hy = (seed.wrapping_mul(2246822519) >> 8) as f32 / 16_777_216.0;
                let span = self.aspect() + 0.2;
                let x = (hx * span - self.dist * speed).rem_euclid(span) - 0.1;
                frame.rect(x * u, hy * u, (0.01 * u * (1.0 + speed)).max(1.0), 1.0, Rgb::new(200, 210, 255), alpha);
            }
        }
        // Finish line.
        let fx = (FINISH - self.dist) * SCROLL + SHIP_X;
        if fx < self.aspect() + 0.1 {
            let tile = 0.05 * u;
            let mut y = TOP * u - tile;
            let mut row = 0;
            while y < BOTTOM * u + tile {
                for k in 0..2 {
                    let c = if (row + k) % 2 == 0 { Rgb::new(240, 240, 240) } else { Rgb::new(30, 30, 30) };
                    frame.rect(fx * u + k as f32 * tile, y, tile, tile, c, 0.9);
                }
                y += tile;
                row += 1;
            }
        }
        for m in &self.meteors {
            let (x, y, r) = (m.0 * u, m.1 * u, METEOR_R * u);
            frame.disc(x, y, r, Rgb::new(120, 100, 90), 1.0);
            frame.disc(x - r * 0.2, y - r * 0.25, r * 0.7, Rgb::new(150, 128, 112), 1.0);
            for (dx, dy, s) in [(0.3, 0.2, 0.25), (-0.35, 0.3, 0.18), (0.1, -0.4, 0.15)] {
                let a = m.2 * 1.5;
                let (rx, ry) = (dx * a.cos() - dy * a.sin(), dx * a.sin() + dy * a.cos());
                frame.disc(x + rx * r, y + ry * r, r * s, Rgb::new(90, 74, 66), 1.0);
            }
        }
        for d in &self.drones {
            let (x, y, r) = (d.x * u, (d.y + d.wobble.sin() * 0.04) * u, DRONE_R * u);
            frame.rect(x - r * 1.3, y - r * 0.25, r * 2.6, r * 0.5, Rgb::new(200, 60, 70), 1.0);
            frame.disc(x, y - r * 0.2, r * 0.6, Rgb::new(255, 120, 120), 1.0);
            frame.disc(x - r * 0.2, y - r * 0.3, r * 0.2, Rgb::new(255, 240, 200), 1.0);
        }
        for s in &self.shots {
            frame.rect(s.0 * u - 0.03 * u, s.1 * u - 1.0, 0.03 * u, 2.0, Rgb::new(150, 255, 255), 1.0);
        }
        if !matches!(self.phase, Phase::Over(_)) {
            let (x, y) = (SHIP_X * u, self.y * u);
            let (l, hh) = (SHIP_LEN * u, SHIP_H * u * 0.5);
            let flame = 0.6 + 0.4 * (self.t * 40.0).sin().abs();
            frame.rect(x - l * 0.5 - l * 0.25 * flame, y - hh * 0.3, l * 0.25 * flame, hh * 0.6, Rgb::new(255, 170, 60), 1.0);
            super::fill_quad(frame, [(x - l * 0.5, y - hh), (x + l * 0.5, y), (x - l * 0.5, y + hh), (x - l * 0.3, y)], Rgb::new(200, 210, 230));
            super::fill_quad(frame, [(x - l * 0.35, y - hh * 1.6), (x - l * 0.05, y - hh * 0.3), (x - l * 0.2, y), (x - l * 0.45, y - hh * 0.2)], Rgb::new(120, 140, 200));
            super::fill_quad(frame, [(x - l * 0.35, y + hh * 1.6), (x - l * 0.05, y + hh * 0.3), (x - l * 0.2, y), (x - l * 0.45, y + hh * 0.2)], Rgb::new(120, 140, 200));
            frame.disc(x + l * 0.1, y, hh * 0.35, Rgb::new(90, 200, 255), 1.0);
        }
        for &(x, y, age, size) in &self.booms {
            let k = age / 0.4;
            frame.disc(x * u, y * u, (0.03 + k * 0.08) * size * u, Rgb::new(255, 200, 90), 0.8 * (1.0 - k));
        }
        self.confetti.render(frame);
        draw_progress(frame, self.progress());
        match self.phase {
            Phase::Over(t) => draw_game_over(frame, t),
            Phase::Cleared(t) => draw_cleared_flash(frame, t),
            Phase::Playing => {}
        }
    }

    fn cleared(&self) -> bool {
        matches!(self.phase, Phase::Cleared(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;

    fn up() -> Input {
        Input::press(0.5, 0.1)
    }
    fn down() -> Input {
        Input::press(0.5, 0.9)
    }

    /// Steps out of the way of meteors lined up with it (drones in its
    /// own lane get shot down), toward a lane that's clear of meteors and
    /// of drones already too close to shoot.
    fn bot_input(sim: &Shooter, since: f32) -> Option<Input> {
        if since < 0.12 {
            return None;
        }
        let meteor = |y: f32| {
            sim.meteors.iter().any(|m| {
                m.0 > SHIP_X - SHIP_LEN * 0.5 - METEOR_R - 0.05 && m.0 < SHIP_X + 1.3 && (m.1 - y).abs() < SHIP_H * 0.5 + METEOR_R + 0.03
            })
        };
        let drone = |y: f32| {
            // Farther-off drones in the new lane get shot down on arrival.
            sim.drones.iter().any(|d| d.x > SHIP_X - 0.15 && d.x < SHIP_X + 0.35 && (d.y - y).abs() < SHIP_H * 0.5 + DRONE_R + 0.06)
        };
        if !meteor(sim.target) {
            return None;
        }
        let safe = |y: f32| !meteor(y) && !drone(y);
        let up_y = (sim.target - STEP_Y).max(TOP);
        let down_y = (sim.target + STEP_Y).min(BOTTOM);
        if up_y < sim.target && safe(up_y) {
            Some(up())
        } else if down_y > sim.target && safe(down_y) {
            Some(down())
        } else if sim.meteors.iter().all(|m| m.0 > SHIP_X + 0.6 || (m.1 - sim.target).abs() >= SHIP_H * 0.5 + METEOR_R + 0.03) {
            // The meteor's still a way off: let the drone blocking the way out go by first.
            None
        } else if up_y < sim.target && !meteor(up_y) {
            Some(up())
        } else if down_y > sim.target && !meteor(down_y) {
            Some(down())
        } else if sim.target - TOP > BOTTOM - sim.target {
            Some(up())
        } else {
            Some(down())
        }
    }

    #[test]
    fn top_and_bottom_halves_step_the_ship() {
        let mut rng = Rng::new(1);
        let mut sim = Shooter::new(320, 96, &mut rng);
        let start = sim.target;
        sim.step(DT, &up(), &mut rng);
        assert!((sim.target - (start - STEP_Y)).abs() < 1e-5);
        sim.step(DT, &down(), &mut rng);
        sim.step(DT, &down(), &mut rng);
        assert!((sim.target - (start + STEP_Y)).abs() < 1e-5);
    }

    #[test]
    fn shots_destroy_drones_but_not_meteors() {
        let mut rng = Rng::new(2);
        let mut sim = Shooter::new(320, 96, &mut rng);
        sim.next_meteor = 1e9;
        sim.next_drone = 1e9;
        sim.drones.push(Drone { x: 1.5, y: sim.y, speed: 0.0, wobble: 0.0 });
        sim.meteors.push((2.5, sim.y + 0.3, 0.0));
        for _ in 0..60 {
            sim.step(DT, &Input::default(), &mut rng);
        }
        assert!(sim.drones.is_empty());
        assert_eq!(sim.meteors.len(), 1);
    }

    #[test]
    fn idle_ship_is_hit_and_the_game_restarts_without_ever_clearing() {
        let mut rng = Rng::new(3);
        let mut sim = Shooter::new(320, 96, &mut rng);
        let mut overs = 0;
        let mut was_over = false;
        for _ in 0..60 * 30 {
            sim.step(DT, &Input::default(), &mut rng);
            let over = matches!(sim.phase, Phase::Over(_));
            if over && !was_over {
                overs += 1;
            }
            was_over = over;
            assert!(!sim.cleared());
        }
        assert!(overs >= 3, "only {overs} game overs");
    }

    #[test]
    fn a_pilot_who_dodges_reaches_the_finish() {
        for seed in 1..11 {
            let mut rng = Rng::new(seed);
            let mut sim = Shooter::new(320, 96, &mut rng);
            let mut since = 1.0;
            for _ in 0..60 * 60 {
                let input = bot_input(&sim, since);
                since = if input.is_some() { 0.0 } else { since + DT };
                sim.step(DT, &input.unwrap_or_default(), &mut rng);
                assert!(!matches!(sim.phase, Phase::Over(_)), "seed {seed} was hit at {}/10", sim.progress());
                if sim.cleared() {
                    break;
                }
            }
            assert!(sim.cleared(), "seed {seed} never finished");
            for _ in 0..600 {
                sim.step(DT, &up(), &mut rng);
            }
            assert!(sim.cleared(), "clearing must be permanent");
        }
    }

    #[test]
    fn renders_opaque_at_odd_and_tiny_sizes_with_bad_dt() {
        let mut rng = Rng::new(5);
        let mut sim = Shooter::new(320, 96, &mut rng);
        for (w, h) in [(321, 97), (1, 1), (0, 0), (4, 7)] {
            sim.resize(w, h, &mut rng);
            for dt in [f32::NAN, -1.0, 1000.0, DT] {
                sim.step(dt, &down(), &mut rng);
            }
            let mut frame = Frame::new(w, h);
            sim.render(&mut frame, &Theme::default());
            assert!(frame.pixels.chunks_exact(4).all(|p| p[3] == 255));
        }
    }
}
