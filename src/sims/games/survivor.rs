//! Survivor: a Vampire Survivors-style arena. Your hero fires a ring of
//! shots in every direction on its own; press anywhere and the hero
//! walks there. Bats swarm in from every edge, more and faster over time,
//! and one touch is game over. Gems appear one at a time — collect ten.
//! Stand still for too long and the Reaper comes for you: it shrugs off
//! shots, and only leaves once you move again.
use super::{
    draw_cleared_flash, draw_game_over, draw_progress, fill_quad, sanitize_dt, substeps, unit,
    vgradient, Confetti, Phase, GOAL,
};
use crate::paint::{Frame, Rgb, Theme};
use crate::rng::Rng;
use crate::sims::{Input, Sim};
use std::f32::consts::TAU;

const HERO_R: f32 = 0.035;
const HERO_SPEED: f32 = 0.9;
const BAT_R: f32 = 0.035;
const BAT_SPEED: f32 = 0.28;
/// Bats get this much faster every second, up to `BAT_SPEED_MAX`.
const BAT_SPEED_RAMP: f32 = 0.008;
const BAT_SPEED_MAX: f32 = 0.42;
const SPAWN_START: f32 = 0.8;
const SPAWN_END: f32 = 0.4;
/// Seconds for the spawn interval to shrink from start to end.
const SPAWN_RAMP_SEC: f32 = 20.0;
/// After this long, some bats take two hits.
const TOUGH_AFTER: f32 = 15.0;
const VOLLEY_SEC: f32 = 0.3;
const SHOTS: usize = 8;
/// Each volley's spokes turn by this much, so the gaps sweep around.
const VOLLEY_TURN: f32 = 0.3;
const SHOT_SPEED: f32 = 1.8;
const SHOT_LIFE: f32 = 0.5;
const GEM_R: f32 = 0.04;
/// A new gem lands at least this far from the hero.
const GEM_MIN_DIST: f32 = 0.7;
const TOP: f32 = 0.14;
const MAX_BATS: usize = 60;
/// Seconds without a press before the Reaper appears.
const REAPER_AFTER: f32 = 8.0;
const REAPER_SPEED: f32 = 0.6;
const REAPER_R: f32 = 0.06;

struct Bat {
    x: f32,
    y: f32,
    hp: u32,
    flap: f32,
}

pub struct Survivor {
    w: f32,
    h: f32,
    hero: (f32, f32),
    target: (f32, f32),
    shots: Vec<(f32, f32, f32, f32, f32)>,
    bats: Vec<Bat>,
    gem: (f32, f32),
    gems: u32,
    t: f32,
    /// Seconds since the last press.
    idle: f32,
    reaper: Option<(f32, f32)>,
    next_volley: f32,
    volley_angle: f32,
    next_bat: f32,
    /// Bat deaths: position and age.
    poofs: Vec<(f32, f32, f32)>,
    phase: Phase,
    confetti: Confetti,
    next_burst: f32,
}

impl Survivor {
    pub fn new(w: usize, h: usize, rng: &mut Rng) -> Self {
        let mut s = Self {
            w: w.max(1) as f32,
            h: h.max(1) as f32,
            hero: (0.0, 0.0),
            target: (0.0, 0.0),
            shots: Vec::new(),
            bats: Vec::new(),
            gem: (0.0, 0.0),
            gems: 0,
            t: 0.0,
            idle: 0.0,
            reaper: None,
            next_volley: VOLLEY_SEC,
            volley_angle: 0.0,
            next_bat: 0.5,
            poofs: Vec::new(),
            phase: Phase::Playing,
            confetti: Confetti::default(),
            next_burst: 0.0,
        };
        s.reset(rng);
        s
    }

    fn reset(&mut self, rng: &mut Rng) {
        self.hero = (self.aspect() * 0.5, (TOP + 1.0) * 0.5);
        self.target = self.hero;
        self.shots.clear();
        self.bats.clear();
        self.poofs.clear();
        self.gems = 0;
        self.t = 0.0;
        self.idle = 0.0;
        self.reaper = None;
        self.next_volley = VOLLEY_SEC;
        self.next_bat = 0.5;
        self.phase = Phase::Playing;
        self.confetti.clear();
        self.place_gem(rng);
    }

    fn aspect(&self) -> f32 {
        self.w / self.h
    }

    fn clamp_to_field(&self, (x, y): (f32, f32)) -> (f32, f32) {
        let m = HERO_R * 1.5;
        (x.clamp(m, (self.aspect() - m).max(m)), y.clamp(TOP + m, 1.0 - m))
    }

    fn place_gem(&mut self, rng: &mut Rng) {
        let m = GEM_R * 2.0;
        let mut best = (m, TOP + m, -1.0f32);
        for _ in 0..30 {
            let p = (rng.range_f32(m, (self.aspect() - m).max(m)), rng.range_f32(TOP + m, 1.0 - m));
            let d = (p.0 - self.hero.0).hypot(p.1 - self.hero.1);
            if d > best.2 {
                best = (p.0, p.1, d);
            }
            if d >= GEM_MIN_DIST {
                break;
            }
        }
        self.gem = (best.0, best.1);
    }

    fn spawn_bat(&mut self, rng: &mut Rng) {
        if self.bats.len() >= MAX_BATS {
            return;
        }
        // Just outside a random edge.
        let (a, w) = (rng.next_f32(), self.aspect());
        let perimeter = 2.0 * (w + (1.0 - TOP));
        let d = a * perimeter;
        let (x, y) = if d < w {
            (d, TOP - BAT_R * 2.0)
        } else if d < w + (1.0 - TOP) {
            (w + BAT_R * 2.0, TOP + d - w)
        } else if d < 2.0 * w + (1.0 - TOP) {
            (d - w - (1.0 - TOP), 1.0 + BAT_R * 2.0)
        } else {
            (-BAT_R * 2.0, TOP + d - 2.0 * w - (1.0 - TOP))
        };
        let tough = self.t > TOUGH_AFTER && rng.bool_p(0.35);
        self.bats.push(Bat { x, y, hp: if tough { 2 } else { 1 }, flap: rng.range_f32(0.0, TAU) });
    }

    fn step_playing(&mut self, dt: f32, input: &Input, rng: &mut Rng) {
        if let Some(p) = input.presses().iter().rev().find(|p| p.is_pointed()) {
            self.target = self.clamp_to_field((p.x * self.aspect(), p.y));
            self.idle = 0.0;
            self.reaper = None;
        }
        let (n, h) = substeps(dt);
        for _ in 0..n {
            self.t += h;
            let (dx, dy) = (self.target.0 - self.hero.0, self.target.1 - self.hero.1);
            let d = dx.hypot(dy);
            let step = HERO_SPEED * h;
            if d > step {
                self.hero.0 += dx / d * step;
                self.hero.1 += dy / d * step;
            } else {
                self.hero = self.target;
            }

            self.next_volley -= h;
            if self.next_volley <= 0.0 {
                self.next_volley += VOLLEY_SEC;
                self.volley_angle = (self.volley_angle + VOLLEY_TURN) % TAU;
                for k in 0..SHOTS {
                    let a = self.volley_angle + k as f32 * TAU / SHOTS as f32;
                    self.shots.push((self.hero.0, self.hero.1, a.cos() * SHOT_SPEED, a.sin() * SHOT_SPEED, SHOT_LIFE));
                }
            }
            for s in &mut self.shots {
                s.0 += s.2 * h;
                s.1 += s.3 * h;
                s.4 -= h;
            }

            let ramp = (self.t / SPAWN_RAMP_SEC).min(1.0);
            self.next_bat -= h;
            if self.next_bat <= 0.0 {
                self.spawn_bat(rng);
                self.next_bat = SPAWN_START + (SPAWN_END - SPAWN_START) * ramp;
            }
            let speed = (BAT_SPEED + BAT_SPEED_RAMP * self.t).min(BAT_SPEED_MAX);
            let hero = self.hero;
            self.idle += h;
            if self.idle >= REAPER_AFTER && self.reaper.is_none() {
                // Rises out of the far corner.
                let x = if hero.0 > self.aspect() * 0.5 { REAPER_R } else { self.aspect() - REAPER_R };
                let y = if hero.1 > (TOP + 1.0) * 0.5 { TOP + REAPER_R } else { 1.0 - REAPER_R };
                self.reaper = Some((x, y));
            }
            if let Some(r) = &mut self.reaper {
                let (dx, dy) = (hero.0 - r.0, hero.1 - r.1);
                let d = dx.hypot(dy).max(1e-4);
                r.0 += dx / d * REAPER_SPEED * h;
                r.1 += dy / d * REAPER_SPEED * h;
                if d < HERO_R + REAPER_R * 0.8 {
                    self.phase = Phase::Over(0.0);
                    return;
                }
            }
            for b in &mut self.bats {
                let (dx, dy) = (hero.0 - b.x, hero.1 - b.y);
                let d = dx.hypot(dy).max(1e-4);
                b.x += dx / d * speed * h;
                b.y += dy / d * speed * h;
                b.flap += h * 14.0;
            }

            // Shots vs bats: each shot hits one bat.
            for s in &mut self.shots {
                if s.4 <= 0.0 {
                    continue;
                }
                if let Some(b) = self.bats.iter_mut().find(|b| b.hp > 0 && (b.x - s.0).hypot(b.y - s.1) < BAT_R * 1.3) {
                    b.hp -= 1;
                    s.4 = 0.0;
                    if b.hp == 0 {
                        self.poofs.push((b.x, b.y, 0.0));
                    }
                }
            }
            self.shots.retain(|s| s.4 > 0.0);
            self.bats.retain(|b| b.hp > 0);

            if self.bats.iter().any(|b| (b.x - hero.0).hypot(b.y - hero.1) < HERO_R + BAT_R * 0.8) {
                self.phase = Phase::Over(0.0);
                return;
            }
            if (self.gem.0 - hero.0).hypot(self.gem.1 - hero.1) < HERO_R + GEM_R {
                self.gems += 1;
                if self.gems >= GOAL {
                    self.phase = Phase::Cleared(0.0);
                    self.bats.clear();
                    self.confetti.burst(hero.0, hero.1, 90, rng);
                    self.next_burst = 0.5;
                    return;
                }
                self.place_gem(rng);
            }
        }
    }
}

impl Sim for Survivor {
    fn resize(&mut self, w: usize, h: usize, _: &mut Rng) {
        self.w = w.max(1) as f32;
        self.h = h.max(1) as f32;
        self.hero = self.clamp_to_field(self.hero);
        self.target = self.clamp_to_field(self.target);
        self.gem = self.clamp_to_field(self.gem);
    }

    fn step(&mut self, dt: f32, input: &Input, rng: &mut Rng) {
        let dt = sanitize_dt(dt);
        self.confetti.step(dt);
        for p in &mut self.poofs {
            p.2 += dt;
        }
        self.poofs.retain(|p| p.2 < 0.3);
        match self.phase {
            Phase::Playing => self.step_playing(dt, input, rng),
            Phase::Over(_) => {
                if self.phase.advance(dt) {
                    self.reset(rng);
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
        vgradient(frame, Rgb::new(30, 44, 30), Rgb::new(22, 34, 24));
        // Checkered turf.
        let tile = 0.1 * u;
        let mut y = 0.0;
        let mut row = 0;
        while y < frame.h as f32 {
            let mut x = if row % 2 == 0 { 0.0 } else { tile };
            while x < frame.w as f32 {
                frame.rect(x, y, tile, tile, Rgb::new(44, 62, 42), 0.5);
                x += tile * 2.0;
            }
            y += tile;
            row += 1;
        }
        // Gem: a spinning cyan diamond with a glow.
        let (gx, gy) = (self.gem.0 * u, self.gem.1 * u);
        let spin = (self.t * 3.0).sin().abs() * 0.5 + 0.5;
        frame.disc(gx, gy, GEM_R * 2.2 * u, Rgb::new(80, 220, 255), 0.2);
        let (rx, ry) = (GEM_R * u * spin, GEM_R * 1.3 * u);
        fill_quad(frame, [(gx, gy - ry), (gx + rx, gy), (gx, gy + ry), (gx - rx, gy)], Rgb::new(110, 235, 255));
        // Where the hero is walking to.
        if (self.target.0 - self.hero.0).hypot(self.target.1 - self.hero.1) > 0.01 && self.phase == Phase::Playing {
            let (tx, ty, s) = (self.target.0 * u, self.target.1 * u, 0.02 * u);
            frame.line((tx - s, ty - s), (tx + s, ty + s), 1.0, Rgb::new(255, 255, 255), 0.5);
            frame.line((tx - s, ty + s), (tx + s, ty - s), 1.0, Rgb::new(255, 255, 255), 0.5);
        }
        for s in &self.shots {
            frame.disc(s.0 * u, s.1 * u, (0.012 * u).max(1.0), Rgb::new(255, 240, 150), 1.0);
        }
        for b in &self.bats {
            let (x, y, r) = (b.x * u, b.y * u, BAT_R * u);
            let body = if b.hp > 1 { Rgb::new(200, 60, 90) } else { Rgb::new(130, 70, 170) };
            let wing = b.flap.sin() * r * 0.6;
            frame.line((x - r * 0.4, y), (x - r * 1.8, y - wing), r * 0.5, body, 1.0);
            frame.line((x + r * 0.4, y), (x + r * 1.8, y - wing), r * 0.5, body, 1.0);
            frame.disc(x, y, r, body, 1.0);
            for dx in [-0.35, 0.35] {
                frame.disc(x + dx * r, y - r * 0.15, (r * 0.2).max(0.8), Rgb::new(255, 60, 60), 1.0);
            }
        }
        if let Some((rx, ry)) = self.reaper {
            // The Reaper: a hooded shadow with a scythe.
            let (x, y, r) = (rx * u, ry * u, REAPER_R * u);
            fill_quad(frame, [(x - r, y + r * 1.1), (x - r * 0.45, y - r * 0.6), (x + r * 0.45, y - r * 0.6), (x + r, y + r * 1.1)], Rgb::new(20, 16, 30));
            frame.disc(x, y - r * 0.55, r * 0.55, Rgb::new(20, 16, 30), 1.0);
            frame.disc(x, y - r * 0.5, r * 0.3, Rgb::new(230, 225, 210), 1.0);
            for dx in [-0.12, 0.12] {
                frame.disc(x + dx * r, y - r * 0.55, (r * 0.07).max(0.8), Rgb::new(255, 40, 40), 1.0);
            }
            frame.line((x + r * 0.9, y + r * 1.1), (x + r * 1.1, y - r * 1.2), (r * 0.12).max(1.0), Rgb::new(120, 90, 60), 1.0);
            frame.line((x + r * 1.1, y - r * 1.2), (x + r * 0.2, y - r * 1.0), (r * 0.12).max(1.0), Rgb::new(210, 210, 220), 1.0);
        }
        for &(x, y, age) in &self.poofs {
            frame.disc(x * u, y * u, (BAT_R + age * 0.15) * u, Rgb::new(220, 200, 255), 0.6 * (1.0 - age / 0.3));
        }
        if !matches!(self.phase, Phase::Over(_)) {
            // Hero: a hooded figure with a cape.
            let (x, y, r) = (self.hero.0 * u, self.hero.1 * u, HERO_R * u);
            fill_quad(frame, [(x - r * 1.1, y + r * 1.2), (x - r * 0.5, y - r * 0.2), (x + r * 0.5, y - r * 0.2), (x + r * 1.1, y + r * 1.2)], Rgb::new(170, 30, 40));
            frame.disc(x, y - r * 0.3, r * 0.75, Rgb::new(240, 210, 170), 1.0);
            frame.rect(x - r * 0.75, y - r * 1.05, r * 1.5, r * 0.45, Rgb::new(60, 50, 80), 1.0);
        }
        self.confetti.render(frame);
        draw_progress(frame, self.gems);
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

    /// Heads for the gem, steering away from bats that get close.
    fn bot_input(sim: &Survivor, since: f32) -> Option<Input> {
        if since < 0.12 {
            return None;
        }
        let (hx, hy) = sim.hero;
        let (gx, gy) = (sim.gem.0 - hx, sim.gem.1 - hy);
        let gd = gx.hypot(gy).max(1e-4);
        let (mut vx, mut vy) = (gx / gd, gy / gd);
        for b in &sim.bats {
            let (dx, dy) = (hx - b.x, hy - b.y);
            let d = dx.hypot(dy).max(1e-3);
            if d < 0.45 {
                let push = 0.1 / (d * d);
                vx += dx / d * push;
                vy += dy / d * push;
            }
        }
        let v = vx.hypot(vy).max(1e-4);
        let (tx, ty) = sim.clamp_to_field((hx + vx / v * 0.25, hy + vy / v * 0.25));
        Some(Input::press(tx / sim.aspect(), ty))
    }

    #[test]
    fn a_press_walks_the_hero_there() {
        let mut rng = Rng::new(1);
        let mut sim = Survivor::new(320, 96, &mut rng);
        sim.step(DT, &Input::press(0.1, 0.8), &mut rng);
        for _ in 0..240 {
            sim.step(DT, &Input::default(), &mut rng);
        }
        let want = sim.clamp_to_field((0.1 * sim.aspect(), 0.8));
        assert!((sim.hero.0 - want.0).abs() < 1e-3 && (sim.hero.1 - want.1).abs() < 1e-3);
    }

    #[test]
    fn volleys_fire_in_every_direction() {
        let mut rng = Rng::new(2);
        let mut sim = Survivor::new(320, 96, &mut rng);
        for _ in 0..20 {
            sim.step(DT, &Input::default(), &mut rng);
        }
        assert!(sim.shots.len() >= SHOTS);
        let right = sim.shots.iter().any(|s| s.2 > 0.5);
        let left = sim.shots.iter().any(|s| s.2 < -0.5);
        let up = sim.shots.iter().any(|s| s.3 < -0.5);
        let down = sim.shots.iter().any(|s| s.3 > 0.5);
        assert!(right && left && up && down);
    }

    #[test]
    fn a_standing_hero_is_overrun_and_the_game_restarts_without_ever_clearing() {
        let mut rng = Rng::new(3);
        let mut sim = Survivor::new(320, 96, &mut rng);
        let mut overs = 0;
        let mut was_over = false;
        for _ in 0..60 * 90 {
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
    fn the_reaper_comes_for_a_hero_who_stands_still_and_leaves_on_a_press() {
        let mut rng = Rng::new(4);
        let mut sim = Survivor::new(320, 96, &mut rng);
        sim.next_bat = 1e9;
        for _ in 0..(60.0 * (REAPER_AFTER + 0.1)) as usize {
            sim.step(DT, &Input::default(), &mut rng);
            sim.next_bat = 1e9;
        }
        assert!(sim.reaper.is_some());
        sim.step(DT, &Input::press(0.5, 0.5), &mut rng);
        assert!(sim.reaper.is_none());
        for _ in 0..60 * 20 {
            sim.step(DT, &Input::default(), &mut rng);
            sim.next_bat = 1e9;
            if matches!(sim.phase, Phase::Over(_)) {
                return;
            }
        }
        panic!("the reaper never caught a hero standing still");
    }

    #[test]
    fn a_nimble_hero_collects_ten_gems() {
        for seed in 1..11 {
            let mut rng = Rng::new(seed);
            let mut sim = Survivor::new(320, 96, &mut rng);
            let mut since = 1.0;
            for _ in 0..60 * 90 {
                let input = bot_input(&sim, since);
                since = if input.is_some() { 0.0 } else { since + DT };
                sim.step(DT, &input.unwrap_or_default(), &mut rng);
                assert!(!matches!(sim.phase, Phase::Over(_)), "seed {seed} died at {} gems", sim.gems);
                if sim.cleared() {
                    break;
                }
            }
            assert!(sim.cleared(), "seed {seed} never cleared");
            for _ in 0..600 {
                sim.step(DT, &Input::press(0.5, 0.5), &mut rng);
            }
            assert!(sim.cleared(), "clearing must be permanent");
        }
    }

    #[test]
    fn renders_opaque_at_odd_and_tiny_sizes_with_bad_dt() {
        let mut rng = Rng::new(5);
        let mut sim = Survivor::new(320, 96, &mut rng);
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
