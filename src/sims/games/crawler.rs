//! Crawler: a first-person dungeon crawl on top of the `dungeon`
//! raycaster. The camera explores the maze on its own; every so often a
//! monster steps into the corridor ahead, the walk stops, and the monster
//! lumbers toward you. Click its head to drop it in one hit, or its body
//! three times. If it reaches you it's game over. Ten monsters; the last
//! one is bigger and guards the exit portal.
use super::{draw_cleared_flash, draw_game_over, draw_progress, ring, sanitize_dt, Confetti, Phase, GOAL};
use crate::paint::{Frame, Rgb, Theme};
use crate::rng::Rng;
use crate::sims::dungeon::Dungeon;
use crate::sims::{Input, Sim};

/// Seconds of walking between encounters.
const WALK_MIN: f32 = 1.8;
const WALK_MAX: f32 = 3.2;
/// A monster appears this many cells down the corridor (as far as it's open).
const SPAWN_MIN_CELLS: i32 = 2;
const SPAWN_MAX_CELLS: i32 = 3;
const MONSTER_SPEED: f32 = 0.45;
/// Closer than this (cells) and it gets you.
const STRIKE_DIST: f32 = 0.6;
const BODY_HITS: u32 = 3;
/// Monster proportions, in wall heights.
const BODY_H: f32 = 0.42;
const BODY_W: f32 = 0.32;
const HEAD_R: f32 = 0.11;
const BOSS_SCALE: f32 = 1.35;
/// Clicks count a little outside the drawn head and body.
const HIT_SLACK: f32 = 1.2;
const DYING_SEC: f32 = 0.4;

struct Monster {
    x: f32,
    y: f32,
    hits_left: u32,
    flash: f32,
    boss: bool,
    /// Seconds since it was killed, if it was.
    dying: Option<f32>,
    /// Where the exit portal stands, behind the boss.
    portal: Option<(f32, f32)>,
}

/// A monster projected onto the screen, in pixels.
struct Sprite {
    x: f32,
    floor: f32,
    /// Pixels per wall height at the monster's distance.
    scale: f32,
}

pub struct Crawler {
    w: f32,
    h: f32,
    dungeon: Dungeon,
    monster: Option<Monster>,
    walk_left: f32,
    kills: u32,
    t: f32,
    phase: Phase,
    confetti: Confetti,
    next_burst: f32,
}

impl Crawler {
    pub fn new(w: usize, h: usize, rng: &mut Rng) -> Self {
        Self {
            w: w.max(1) as f32,
            h: h.max(1) as f32,
            dungeon: Dungeon::new(w, h, rng),
            monster: None,
            walk_left: rng.range_f32(WALK_MIN, WALK_MAX),
            kills: 0,
            t: 0.0,
            phase: Phase::Playing,
            confetti: Confetti::default(),
            next_burst: 0.0,
        }
    }

    fn reset(&mut self, rng: &mut Rng) {
        self.dungeon = Dungeon::new(self.w as usize, self.h as usize, rng);
        self.monster = None;
        self.walk_left = rng.range_f32(WALK_MIN, WALK_MAX);
        self.kills = 0;
        self.phase = Phase::Playing;
        self.confetti.clear();
    }

    /// Put a monster down the corridor ahead, if the camera is facing
    /// along one with at least `SPAWN_MIN_CELLS` of open floor.
    fn try_spawn(&mut self) -> bool {
        let Some((dx, dy)) = self.dungeon.heading() else { return false };
        let (cx, cy, _) = self.dungeon.pose();
        let (cell_x, cell_y) = (cx.floor() as i32, cy.floor() as i32);
        let mut reach = 0;
        while reach < SPAWN_MAX_CELLS && self.dungeon.is_open(cell_x + dx * (reach + 1), cell_y + dy * (reach + 1)) {
            reach += 1;
        }
        if reach < SPAWN_MIN_CELLS {
            return false;
        }
        let (mx, my) = ((cell_x + dx * reach) as f32 + 0.5, (cell_y + dy * reach) as f32 + 0.5);
        let boss = self.kills + 1 == GOAL;
        let portal = boss.then(|| (mx + dx as f32 * 0.45, my + dy as f32 * 0.45));
        self.monster = Some(Monster { x: mx, y: my, hits_left: BODY_HITS, flash: 0.0, boss, dying: None, portal });
        true
    }

    /// Project a world point standing on the floor onto the screen.
    fn project(&self, x: f32, y: f32) -> Option<Sprite> {
        let (cx, cy, angle) = self.dungeon.pose();
        let (rx, ry) = (x - cx, y - cy);
        let (c, s) = (angle.cos(), angle.sin());
        let fwd = rx * c + ry * s;
        let side = -rx * s + ry * c;
        if fwd < 0.1 {
            return None;
        }
        let t = side.atan2(fwd) / Dungeon::fov();
        let scale = self.h * Dungeon::wall_height_scale() / fwd;
        Some(Sprite { x: self.w * (0.5 + t), floor: self.h * 0.5 + scale * 0.5, scale })
    }

    fn monster_parts(&self, m: &Monster) -> Option<(Sprite, (f32, f32, f32), (f32, f32, f32, f32))> {
        let sp = self.project(m.x, m.y)?;
        let k = sp.scale * if m.boss { BOSS_SCALE } else { 1.0 };
        let body = (sp.x - BODY_W * k * 0.5, sp.floor - BODY_H * k, BODY_W * k, BODY_H * k);
        let head = (sp.x, body.1 - HEAD_R * k * 0.8, HEAD_R * k);
        Some((sp, head, body))
    }

    fn shoot(&mut self, px: f32, py: f32, rng: &mut Rng) {
        let Some(m) = &self.monster else { return };
        if m.dying.is_some() {
            return;
        }
        let Some((_, head, body)) = self.monster_parts(m) else { return };
        let on_head = (px - head.0).hypot(py - head.1) <= head.2 * HIT_SLACK;
        let (bx, by, bw, bh) = body;
        let slack_w = bw * (HIT_SLACK - 1.0) * 0.5;
        let on_body = px >= bx - slack_w && px <= bx + bw + slack_w && py >= by && py <= by + bh * HIT_SLACK;
        let m = self.monster.as_mut().unwrap();
        if on_head {
            m.hits_left = 0;
        } else if on_body {
            m.hits_left -= 1;
            m.flash = 0.12;
        } else {
            return;
        }
        if m.hits_left == 0 {
            m.dying = Some(0.0);
            self.kills += 1;
            if self.kills >= GOAL {
                self.phase = Phase::Cleared(0.0);
                self.confetti.burst(0.5 * self.w / self.h, 0.5, 90, rng);
                self.next_burst = 0.5;
            }
        }
    }

    fn step_playing(&mut self, dt: f32, input: &Input, rng: &mut Rng) {
        for p in input.presses().iter().filter(|p| p.is_pointed()) {
            self.shoot(p.x * self.w, p.y * self.h, rng);
            if self.phase != Phase::Playing {
                return;
            }
        }
        match &mut self.monster {
            Some(m) if m.dying.is_none() => {
                // Lumber straight at the camera; the walk waits.
                let (cx, cy, _) = self.dungeon.pose();
                let (dx, dy) = (cx - m.x, cy - m.y);
                let d = dx.hypot(dy);
                if d <= STRIKE_DIST {
                    self.phase = Phase::Over(0.0);
                    return;
                }
                let step = (MONSTER_SPEED * dt).min(d);
                m.x += dx / d * step;
                m.y += dy / d * step;
                m.flash = (m.flash - dt).max(0.0);
            }
            Some(m) => {
                let age = m.dying.unwrap() + dt;
                m.dying = Some(age);
                if age >= DYING_SEC {
                    self.monster = None;
                    self.walk_left = rng.range_f32(WALK_MIN, WALK_MAX);
                }
            }
            None => {
                self.dungeon.step(dt, &Input::default(), rng);
                self.walk_left -= dt;
                if self.walk_left <= 0.0 {
                    self.try_spawn();
                }
            }
        }
    }

    fn draw_monster(&self, frame: &mut Frame, m: &Monster) {
        let Some((sp, head, body)) = self.monster_parts(m) else { return };
        let fade = m.dying.map_or(1.0, |a| (1.0 - a / DYING_SEC).max(0.0));
        if fade <= 0.0 {
            return;
        }
        // Hidden behind a wall? (Can't happen in a straight corridor, but be safe.)
        let (cx, cy, _) = self.dungeon.pose();
        let dir = (m.y - cy).atan2(m.x - cx);
        let dist = (m.x - cx).hypot(m.y - cy);
        if self.dungeon.ray_dist(dir) + 0.3 < dist {
            return;
        }
        let skin = if m.boss { Rgb::new(200, 40, 50) } else { Rgb::new(90, 170, 70) };
        let skin = skin.lerp(Rgb::new(255, 255, 255), m.flash / 0.12);
        let dark = skin.lerp(Rgb::new(0, 0, 0), 0.45);
        let (bx, by, bw, bh) = body;
        // Sinks into the floor as it dies.
        let sink = (1.0 - fade) * bh;
        let (by, head_y) = (by + sink, head.1 + sink);
        let k = sp.scale * if m.boss { BOSS_SCALE } else { 1.0 };
        // Legs, arms, body, head.
        frame.rect(bx + bw * 0.15, sp.floor - bh * 0.3 + sink, bw * 0.22, bh * 0.3 - sink.min(bh * 0.3), dark, fade);
        frame.rect(bx + bw * 0.63, sp.floor - bh * 0.3 + sink, bw * 0.22, bh * 0.3 - sink.min(bh * 0.3), dark, fade);
        frame.rect(bx - bw * 0.25, by + bh * 0.15, bw * 0.25, bh * 0.45, dark, fade);
        frame.rect(bx + bw, by + bh * 0.15, bw * 0.25, bh * 0.45, dark, fade);
        frame.rect(bx, by, bw, bh * 0.75, skin, fade);
        frame.disc(head.0, head_y, head.2, skin, fade);
        // Horns on the boss, eyes and teeth on everyone.
        if m.boss {
            for dx in [-0.7, 0.7] {
                frame.line((head.0 + dx * head.2, head_y - head.2 * 0.6), (head.0 + dx * head.2 * 1.4, head_y - head.2 * 1.5), (k * 0.03).max(1.0), Rgb::new(240, 230, 200), fade);
            }
        }
        for dx in [-0.4, 0.4] {
            frame.disc(head.0 + dx * head.2, head_y - head.2 * 0.15, (head.2 * 0.22).max(0.8), Rgb::new(255, 230, 60), fade);
        }
        frame.rect(head.0 - head.2 * 0.4, head_y + head.2 * 0.35, head.2 * 0.8, (head.2 * 0.18).max(1.0), Rgb::new(250, 250, 250), fade);
        // Hits left, as pips over its head.
        if m.dying.is_none() {
            let r = (head.2 * 0.18).max(1.0);
            for i in 0..BODY_HITS {
                let c = if i < m.hits_left { Rgb::new(255, 80, 80) } else { Rgb::new(70, 70, 70) };
                frame.disc(head.0 + (i as f32 - 1.0) * r * 3.0, head_y - head.2 * 1.6, r, c, 1.0);
            }
        }
    }

    fn draw_portal(&self, frame: &mut Frame, at: (f32, f32)) {
        let Some(sp) = self.project(at.0, at.1) else { return };
        let (cx, cy, _) = self.dungeon.pose();
        if self.dungeon.ray_dist((at.1 - cy).atan2(at.0 - cx)) + 0.3 < (at.0 - cx).hypot(at.1 - cy) {
            return;
        }
        let r = sp.scale * 0.4;
        let y = sp.floor - r;
        let pulse = 0.8 + 0.2 * (self.t * 5.0).sin();
        frame.disc(sp.x, y, r * 1.15, Rgb::new(140, 60, 255), 0.35 * pulse);
        frame.disc(sp.x, y, r, Rgb::new(40, 10, 80), 1.0);
        for k in 0..3 {
            ring(frame, sp.x, y, r * (0.4 + k as f32 * 0.25), (sp.scale * 0.02).max(1.0), Rgb::new(200, 140, 255), pulse);
        }
    }
}

impl Sim for Crawler {
    fn resize(&mut self, w: usize, h: usize, rng: &mut Rng) {
        self.w = w.max(1) as f32;
        self.h = h.max(1) as f32;
        self.dungeon.resize(w, h, rng);
    }

    fn step(&mut self, dt: f32, input: &Input, rng: &mut Rng) {
        let dt = sanitize_dt(dt);
        self.t = (self.t + dt) % 1000.0;
        self.confetti.step(dt);
        match self.phase {
            Phase::Playing => self.step_playing(dt, input, rng),
            Phase::Over(_) => {
                if self.phase.advance(dt) {
                    self.reset(rng);
                }
            }
            Phase::Cleared(t) => {
                self.phase.advance(dt);
                if let Some(m) = &mut self.monster {
                    if let Some(a) = &mut m.dying {
                        *a += dt;
                    }
                }
                for p in input.presses().iter().filter(|p| p.is_pointed()) {
                    self.confetti.burst(p.x * self.w / self.h, p.y, 30, rng);
                }
                if t < 3.0 && t >= self.next_burst {
                    self.next_burst += 0.6;
                    self.confetti.burst(rng.range_f32(0.2, self.w / self.h - 0.2), 0.6, 30, rng);
                }
            }
        }
    }

    fn render(&mut self, frame: &mut Frame, theme: &Theme) {
        self.dungeon.render(frame, theme);
        if let Some(m) = &self.monster {
            if let Some(p) = m.portal {
                self.draw_portal(frame, p);
            }
            self.draw_monster(frame, m);
        }
        if matches!(self.phase, Phase::Over(_)) {
            // Claw marks across the view.
            let (w, h) = (frame.w as f32, frame.h as f32);
            for k in 0..3 {
                let x = w * (0.35 + k as f32 * 0.12);
                frame.line((x, h * 0.15), (x + w * 0.1, h * 0.85), (h * 0.03).max(1.0), Rgb::new(140, 0, 0), 0.8);
            }
        }
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

    fn head_press(sim: &Crawler) -> Option<Input> {
        let m = sim.monster.as_ref().filter(|m| m.dying.is_none())?;
        let (_, head, _) = sim.monster_parts(m)?;
        Some(Input::press(head.0 / sim.w, head.1 / sim.h))
    }

    fn body_press(sim: &Crawler) -> Option<Input> {
        let m = sim.monster.as_ref().filter(|m| m.dying.is_none())?;
        let (_, _, (bx, by, bw, bh)) = sim.monster_parts(m)?;
        Some(Input::press((bx + bw * 0.5) / sim.w, (by + bh * 0.5) / sim.h))
    }

    fn wait_for_monster(sim: &mut Crawler, rng: &mut Rng) {
        for _ in 0..60 * 30 {
            if sim.monster.as_ref().is_some_and(|m| m.dying.is_none()) {
                return;
            }
            sim.step(DT, &Input::default(), rng);
        }
        panic!("no monster ever showed up");
    }

    #[test]
    fn one_head_click_kills() {
        let mut rng = Rng::new(1);
        let mut sim = Crawler::new(320, 96, &mut rng);
        wait_for_monster(&mut sim, &mut rng);
        let input = head_press(&sim).unwrap();
        sim.step(DT, &input, &mut rng);
        assert_eq!(sim.kills, 1);
    }

    #[test]
    fn the_body_takes_three_clicks() {
        let mut rng = Rng::new(2);
        let mut sim = Crawler::new(320, 96, &mut rng);
        wait_for_monster(&mut sim, &mut rng);
        for i in 0..BODY_HITS {
            assert_eq!(sim.kills, 0, "died after {i} body hits");
            let input = body_press(&sim).unwrap();
            sim.step(DT, &input, &mut rng);
        }
        assert_eq!(sim.kills, 1);
    }

    #[test]
    fn clicking_the_walls_does_nothing() {
        let mut rng = Rng::new(3);
        let mut sim = Crawler::new(320, 96, &mut rng);
        wait_for_monster(&mut sim, &mut rng);
        sim.step(DT, &Input::press(0.02, 0.5), &mut rng);
        sim.step(DT, &Input::press(0.98, 0.1), &mut rng);
        assert_eq!(sim.monster.as_ref().unwrap().hits_left, BODY_HITS);
    }

    #[test]
    fn an_ignored_monster_gets_you_and_the_game_restarts() {
        let mut rng = Rng::new(4);
        let mut sim = Crawler::new(320, 96, &mut rng);
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
        assert!(overs >= 3, "only {overs} game overs");
    }

    #[test]
    fn the_tenth_monster_is_the_boss_at_the_exit() {
        for seed in 1..11 {
            let mut rng = Rng::new(seed);
            let mut sim = Crawler::new(320, 96, &mut rng);
            let mut seen = 0;
            let mut since = 1.0;
            for _ in 0..60 * 120 {
                // Aim for the head, a quarter second after it appears.
                let fresh = sim.monster.as_ref().is_some_and(|m| m.dying.is_none());
                let input = if fresh && since >= 0.25 { head_press(&sim) } else { None };
                if fresh && since >= 0.25 {
                    let m = sim.monster.as_ref().unwrap();
                    seen += 1;
                    assert_eq!(m.boss, sim.kills + 1 == GOAL);
                    assert_eq!(m.portal.is_some(), m.boss);
                }
                since = if fresh { since + DT } else { 0.0 };
                if input.is_some() {
                    since = 0.0;
                }
                sim.step(DT, &input.unwrap_or_default(), &mut rng);
                assert!(!matches!(sim.phase, Phase::Over(_)), "seed {seed} fell at {} kills", sim.kills);
                if sim.cleared() {
                    break;
                }
            }
            assert!(sim.cleared(), "seed {seed} never cleared ({} kills)", sim.kills);
            assert_eq!(seen, GOAL as usize);
        }
    }

    #[test]
    fn renders_opaque_at_odd_and_tiny_sizes_with_bad_dt() {
        let mut rng = Rng::new(5);
        let mut sim = Crawler::new(320, 96, &mut rng);
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
