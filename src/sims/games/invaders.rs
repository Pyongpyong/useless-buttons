//! Invaders: a Space Invaders-style formation of ten marches side to
//! side, dropping a row each time it reaches an edge. Press anywhere and
//! your cannon slides under that spot and fires once it gets there — one
//! shot in the air at a time. Invaders drop bombs, aimed at you more
//! often than not. Get bombed, or let them land, and it's game over;
//! shoot all ten to clear.
use super::{
    draw_cleared_flash, draw_game_over, draw_progress, draw_sprite, sanitize_dt, substeps, unit,
    vgradient, Confetti, Phase, GOAL,
};
use crate::paint::{Frame, Rgb, Theme};
use crate::rng::Rng;
use crate::sims::{Input, Sim};

/// The classic crab, in two marching frames, 11 x 8.
const CRAB: [[u16; 8]; 2] = [
    [0b00100000100, 0b00010001000, 0b00111111100, 0b01101110110, 0b11111111111, 0b10111111101, 0b10100000101, 0b00011011000],
    [0b00100000100, 0b10010001001, 0b10111111101, 0b11101110111, 0b11111111111, 0b01111111110, 0b00100000100, 0b01000000010],
];
const SPRITE_W: usize = 11;
const PX: f32 = 0.012;
const INV_W: f32 = SPRITE_W as f32 * PX;
const INV_H: f32 = 8.0 * PX;
const COLS: usize = 5;
const ROWS: usize = 2;
const DX: f32 = 0.22;
const DY: f32 = 0.14;
const START_X: f32 = 0.2;
const START_Y: f32 = 0.16;
const MARCH_SPEED: f32 = 0.55;
/// The formation speeds up with every kill, like the original.
const MARCH_PER_KILL: f32 = 0.05;
const DROP: f32 = 0.1;
const EDGE: f32 = 0.05;
/// An invader whose bottom reaches this line has landed.
const LAND_Y: f32 = 0.82;
const CANNON_Y: f32 = 0.88;
const CANNON_W: f32 = 0.14;
const CANNON_H: f32 = 0.06;
const CANNON_SPEED: f32 = 2.2;
const SHOT_SPEED: f32 = 2.4;
const BOMB_SPEED: f32 = 0.55;
const BOMB_GAP_MIN: f32 = 1.0;
const BOMB_GAP_MAX: f32 = 1.8;
const FIRST_BOMB_SEC: f32 = 1.5;
/// Chance a bomb comes from the invader closest above the cannon.
const AIMED_BOMB_P: f32 = 0.6;

struct Invader {
    col: usize,
    row: usize,
    alive: bool,
}

pub struct Invaders {
    w: f32,
    h: f32,
    invaders: Vec<Invader>,
    /// Formation's top-left.
    origin: (f32, f32),
    dir: f32,
    march_t: f32,
    cannon: f32,
    target: f32,
    /// A press is waiting for the cannon to arrive before it fires.
    armed: bool,
    shot: Option<(f32, f32)>,
    bombs: Vec<(f32, f32)>,
    next_bomb: f32,
    kills: u32,
    /// Explosions: position and age.
    booms: Vec<(f32, f32, f32)>,
    phase: Phase,
    confetti: Confetti,
    next_burst: f32,
}

impl Invaders {
    pub fn new(w: usize, h: usize, _: &mut Rng) -> Self {
        let mut s = Self {
            w: w.max(1) as f32,
            h: h.max(1) as f32,
            invaders: Vec::new(),
            origin: (START_X, START_Y),
            dir: 1.0,
            march_t: 0.0,
            cannon: 0.0,
            target: 0.0,
            armed: false,
            shot: None,
            bombs: Vec::new(),
            next_bomb: FIRST_BOMB_SEC,
            kills: 0,
            booms: Vec::new(),
            phase: Phase::Playing,
            confetti: Confetti::default(),
            next_burst: 0.0,
        };
        s.reset();
        s
    }

    fn aspect(&self) -> f32 {
        self.w / self.h
    }

    fn reset(&mut self) {
        self.invaders = (0..ROWS).flat_map(|row| (0..COLS).map(move |col| Invader { col, row, alive: true })).collect();
        self.origin = (START_X, START_Y);
        self.dir = 1.0;
        self.cannon = self.aspect() * 0.5;
        self.target = self.cannon;
        self.armed = false;
        self.shot = None;
        self.bombs.clear();
        self.next_bomb = FIRST_BOMB_SEC;
        self.kills = 0;
        self.booms.clear();
        self.phase = Phase::Playing;
        self.confetti.clear();
    }

    /// Top-left of an invader.
    fn pos(&self, inv: &Invader) -> (f32, f32) {
        (self.origin.0 + inv.col as f32 * DX, self.origin.1 + inv.row as f32 * DY)
    }

    fn march_speed(&self) -> f32 {
        MARCH_SPEED + MARCH_PER_KILL * self.kills as f32
    }

    fn clamp_cannon(&self, x: f32) -> f32 {
        let half = CANNON_W * 0.5;
        x.clamp(half, (self.aspect() - half).max(half))
    }

    fn drop_bomb(&mut self, rng: &mut Rng) {
        // Only the lowest invader in each column can bomb.
        let mut bottoms: Vec<(f32, f32)> = Vec::new();
        for col in 0..COLS {
            if let Some(inv) = self.invaders.iter().filter(|i| i.alive && i.col == col).max_by_key(|i| i.row) {
                let (x, y) = self.pos(inv);
                bottoms.push((x + INV_W * 0.5, y + INV_H));
            }
        }
        if bottoms.is_empty() {
            return;
        }
        let from = if rng.bool_p(AIMED_BOMB_P) {
            let c = self.cannon;
            *bottoms.iter().min_by(|a, b| (a.0 - c).abs().total_cmp(&(b.0 - c).abs())).unwrap()
        } else {
            bottoms[rng.range_usize(0, bottoms.len())]
        };
        self.bombs.push(from);
    }

    /// One physics substep. Returns `true` on a game over.
    fn physics(&mut self, h: f32, rng: &mut Rng) -> bool {
        let step = CANNON_SPEED * h;
        self.cannon = self.clamp_cannon(self.cannon + (self.target - self.cannon).clamp(-step, step));
        if self.armed && self.shot.is_none() && (self.target - self.cannon).abs() < 1e-3 {
            self.shot = Some((self.cannon, CANNON_Y));
            self.armed = false;
        }

        // March, bouncing and dropping at the edges of the frame.
        self.origin.0 += self.dir * self.march_speed() * h;
        self.march_t += h;
        let alive: Vec<(f32, f32)> = self.invaders.iter().filter(|i| i.alive).map(|i| self.pos(i)).collect();
        let left = alive.iter().map(|p| p.0).fold(f32::INFINITY, f32::min);
        let right = alive.iter().map(|p| p.0 + INV_W).fold(f32::NEG_INFINITY, f32::max);
        if (self.dir > 0.0 && right > self.aspect() - EDGE) || (self.dir < 0.0 && left < EDGE) {
            self.dir = -self.dir;
            self.origin.1 += DROP;
        }
        if alive.iter().any(|p| p.1 + INV_H >= LAND_Y) {
            return true;
        }

        if let Some((x, y)) = &mut self.shot {
            *y -= SHOT_SPEED * h;
            let (sx, sy) = (*x, *y);
            if sy < 0.0 {
                self.shot = None;
            } else if let Some(i) = (0..self.invaders.len()).find(|&i| {
                let inv = &self.invaders[i];
                let (ix, iy) = self.pos(inv);
                inv.alive && sx >= ix - 0.01 && sx <= ix + INV_W + 0.01 && sy >= iy && sy <= iy + INV_H
            }) {
                let (ix, iy) = self.pos(&self.invaders[i]);
                self.invaders[i].alive = false;
                self.shot = None;
                self.kills += 1;
                self.booms.push((ix + INV_W * 0.5, iy + INV_H * 0.5, 0.0));
            }
        }

        self.next_bomb -= h;
        if self.next_bomb <= 0.0 {
            self.drop_bomb(rng);
            self.next_bomb = rng.range_f32(BOMB_GAP_MIN, BOMB_GAP_MAX);
        }
        let half = CANNON_W * 0.5;
        let cannon = self.cannon;
        let mut hit = false;
        for b in &mut self.bombs {
            b.1 += BOMB_SPEED * h;
            if (b.0 - cannon).abs() <= half && b.1 >= CANNON_Y && b.1 <= CANNON_Y + CANNON_H {
                hit = true;
            }
        }
        self.bombs.retain(|b| b.1 < 1.05);
        hit
    }

    fn step_playing(&mut self, dt: f32, input: &Input, rng: &mut Rng) {
        if let Some(p) = input.presses().iter().rev().find(|p| p.is_pointed()) {
            self.target = self.clamp_cannon(p.x * self.aspect());
            self.armed = true;
        }
        let (n, h) = substeps(dt);
        for _ in 0..n {
            if self.physics(h, rng) {
                self.booms.push((self.cannon, CANNON_Y + CANNON_H * 0.5, 0.0));
                self.phase = Phase::Over(0.0);
                return;
            }
            if self.kills >= GOAL {
                self.phase = Phase::Cleared(0.0);
                self.bombs.clear();
                self.confetti.burst(self.aspect() * 0.5, 0.5, 90, rng);
                self.next_burst = 0.5;
                return;
            }
        }
    }
}

impl Sim for Invaders {
    fn resize(&mut self, w: usize, h: usize, _: &mut Rng) {
        self.w = w.max(1) as f32;
        self.h = h.max(1) as f32;
        self.cannon = self.clamp_cannon(self.cannon);
        self.target = self.clamp_cannon(self.target);
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
        vgradient(frame, Rgb::new(4, 4, 16), Rgb::new(10, 6, 30));
        // A sparse, fixed starfield.
        for i in 0..40u32 {
            let hx = (i.wrapping_mul(2654435761) >> 8) as f32 / 16_777_216.0;
            let hy = (i.wrapping_mul(2246822519) >> 8) as f32 / 16_777_216.0;
            frame.rect(hx * frame.w as f32, hy * frame.h as f32, 1.0, 1.0, Rgb::new(200, 200, 255), 0.6);
        }
        let frame_i = ((self.march_t * self.march_speed() * 4.0) as usize) % 2;
        for inv in self.invaders.iter().filter(|i| i.alive) {
            let (x, y) = self.pos(inv);
            let c = if inv.row == 0 { Rgb::new(255, 110, 200) } else { Rgb::new(120, 255, 140) };
            draw_sprite(frame, &CRAB[frame_i], SPRITE_W, x * u, y * u, (PX * u).max(1.0), c);
        }
        for &(x, y, age) in &self.booms {
            let k = age / 0.4;
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::TAU / 8.0;
                let d = (0.02 + k * 0.08) * u;
                let s = (PX * u).max(1.0);
                frame.rect(x * u + a.cos() * d, y * u + a.sin() * d, s, s, Rgb::new(255, 240, 150), 1.0 - k);
            }
        }
        if let Some((x, y)) = self.shot {
            frame.rect(x * u - 1.0, y * u, (0.01 * u).max(2.0), 0.05 * u, Rgb::new(255, 255, 255), 1.0);
        }
        for &(x, y) in &self.bombs {
            let zig = if ((y * 30.0) as i32) % 2 == 0 { -1.0 } else { 1.0 };
            frame.rect(x * u + zig, y * u - 0.04 * u, (0.01 * u).max(2.0), 0.04 * u, Rgb::new(255, 120, 80), 1.0);
        }
        // Cannon: a base with a barrel, the original's green.
        if !matches!(self.phase, Phase::Over(_)) {
            let green = Rgb::new(80, 255, 90);
            let (x, y) = ((self.cannon - CANNON_W * 0.5) * u, CANNON_Y * u);
            frame.rect(x, y + CANNON_H * 0.4 * u, CANNON_W * u, CANNON_H * 0.6 * u, green, 1.0);
            frame.rect(x + CANNON_W * 0.4 * u, y, CANNON_W * 0.2 * u, CANNON_H * 0.5 * u, green, 1.0);
        }
        frame.rect(0.0, (CANNON_Y + CANNON_H + 0.01) * u, frame.w as f32, (0.01 * u).max(1.0), Rgb::new(80, 255, 90), 0.6);
        self.confetti.render(frame);
        draw_progress(frame, self.kills);
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

    /// Bombs low enough to matter.
    fn threats(sim: &Invaders) -> Vec<f32> {
        sim.bombs.iter().filter(|b| b.1 > CANNON_Y - 0.6).map(|b| b.0).collect()
    }

    /// Sliding from `from` to `to` neither ends under a bomb nor passes
    /// beneath one on the way. Moving away from a bomb overhead is fine.
    fn path_clear(threats: &[f32], from: f32, to: f32) -> bool {
        let (lo, hi) = (from.min(to), from.max(to));
        threats.iter().all(|&b| (to - b).abs() > CANNON_W && !(b > lo && b < hi))
    }

    /// Dodges bombs first — never sliding under one — and otherwise leads
    /// the lowest invader it can reach safely and fires.
    fn bot_input(sim: &Invaders, since: f32) -> Option<Input> {
        let aspect = sim.aspect();
        let threats = threats(sim);
        if !path_clear(&threats, sim.cannon, sim.target) {
            // Nearest spot the cannon can reach without passing under a bomb.
            let safe = (0..=40)
                .map(|k| sim.clamp_cannon(k as f32 / 40.0 * aspect))
                .filter(|&x| path_clear(&threats, sim.cannon, x))
                .min_by(|a, b| (a - sim.cannon).abs().total_cmp(&(b - sim.cannon).abs()));
            if let Some(x) = safe {
                if (x - sim.target).abs() > 1e-3 {
                    return Some(Input::press(x / aspect, 0.5));
                }
            }
            return None;
        }
        if since < 0.12 || sim.armed || sim.shot.is_some() {
            return None;
        }
        let inv = sim.invaders.iter().filter(|i| i.alive).max_by(|a, b| {
            let (ay, by) = (a.row, b.row);
            ay.cmp(&by).then_with(|| {
                let (ax, _) = sim.pos(a);
                let (bx, _) = sim.pos(b);
                (bx - sim.cannon).abs().total_cmp(&(ax - sim.cannon).abs())
            })
        })?;
        let (x, y) = sim.pos(inv);
        let mut aim = x + INV_W * 0.5;
        // Lead the target: time to slide there plus time for the shot to climb.
        for _ in 0..3 {
            let t = (aim - sim.cannon).abs() / CANNON_SPEED + (CANNON_Y - y - INV_H * 0.5) / SHOT_SPEED;
            aim = x + INV_W * 0.5 + sim.dir * sim.march_speed() * t;
        }
        let aim = sim.clamp_cannon(aim);
        if !path_clear(&threats, sim.cannon, aim) {
            return None;
        }
        Some(Input::press(aim / aspect, 0.5))
    }

    #[test]
    fn a_press_slides_the_cannon_over_then_fires_once() {
        let mut rng = Rng::new(1);
        let mut sim = Invaders::new(320, 96, &mut rng);
        sim.next_bomb = 1e9;
        sim.step(DT, &Input::press(0.2, 0.5), &mut rng);
        assert!(sim.shot.is_none(), "fired before getting there");
        for _ in 0..60 {
            sim.step(DT, &Input::default(), &mut rng);
            if sim.shot.is_some() {
                break;
            }
        }
        let (x, _) = sim.shot.expect("never fired");
        assert!((x - 0.2 * sim.aspect()).abs() < 0.01);
        assert!(!sim.armed);
    }

    #[test]
    fn idle_cannon_is_bombed_or_overrun_and_the_game_restarts() {
        let mut rng = Rng::new(2);
        let mut sim = Invaders::new(320, 96, &mut rng);
        let mut overs = 0;
        let mut was_over = false;
        for _ in 0..60 * 40 {
            sim.step(DT, &Input::default(), &mut rng);
            let over = matches!(sim.phase, Phase::Over(_));
            if over && !was_over {
                overs += 1;
            }
            was_over = over;
            assert!(!sim.cleared());
        }
        assert!(overs >= 2, "only {overs} game overs");
    }

    #[test]
    fn invaders_that_are_never_shot_eventually_land() {
        let mut rng = Rng::new(3);
        let mut sim = Invaders::new(320, 96, &mut rng);
        sim.next_bomb = 1e9;
        for _ in 0..60 * 60 {
            sim.step(DT, &Input::default(), &mut rng);
            sim.next_bomb = 1e9;
            if matches!(sim.phase, Phase::Over(_)) {
                return;
            }
        }
        panic!("the formation never landed");
    }

    #[test]
    fn a_sharp_shooter_downs_all_ten() {
        for seed in 1..11 {
            let mut rng = Rng::new(seed);
            let mut sim = Invaders::new(320, 96, &mut rng);
            let mut since = 1.0;
            for _ in 0..60 * 90 {
                let input = bot_input(&sim, since);
                since = if input.is_some() { 0.0 } else { since + DT };
                sim.step(DT, &input.unwrap_or_default(), &mut rng);
                assert!(!matches!(sim.phase, Phase::Over(_)), "seed {seed} lost at {} kills", sim.kills);
                if sim.cleared() {
                    break;
                }
            }
            assert!(sim.cleared(), "seed {seed} never cleared ({} kills)", sim.kills);
            for _ in 0..600 {
                sim.step(DT, &Input::press(0.5, 0.5), &mut rng);
            }
            assert!(sim.cleared(), "clearing must be permanent");
        }
    }

    #[test]
    fn renders_opaque_at_odd_and_tiny_sizes_with_bad_dt() {
        let mut rng = Rng::new(5);
        let mut sim = Invaders::new(320, 96, &mut rng);
        for (w, h) in [(321, 97), (1, 1), (0, 0), (4, 7)] {
            sim.resize(w, h, &mut rng);
            for dt in [f32::NAN, -1.0, 1000.0, DT] {
                sim.step(dt, &Input::press(0.5, 0.5), &mut rng);
            }
            let mut frame = Frame::new(w, h);
            sim.render(&mut frame, &Theme::default());
            assert!(frame.pixels.chunks_exact(4).all(|p| p[3] == 255));
        }
    }
}
