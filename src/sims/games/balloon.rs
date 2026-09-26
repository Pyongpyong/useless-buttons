//! Balloon: balloons drift down from the top; click each one 3–5 times to
//! pop it. Every hit also bumps the balloon back up a little. If one
//! touches the ground it's game over; pop all ten to clear.
use super::{
    draw_cleared_flash, draw_game_over, draw_progress, ring, sanitize_dt, unit, vgradient,
    Confetti, Phase, GOAL,
};
use crate::paint::{Frame, Rgb, Theme};
use crate::rng::Rng;
use crate::sims::{Input, Sim};

const R: f32 = 0.12;
/// Clicks count as hits a little outside the drawn balloon.
const HIT_R: f32 = R * 1.25;
const HP_MIN: u32 = 3;
const HP_MAX: u32 = 5;
const GROUND_Y: f32 = 0.9;
const FALL_BASE: f32 = 0.13;
/// Each later balloon falls a little faster.
const FALL_PER_BALLOON: f32 = 0.012;
/// Upward kick from a hit, units/s.
const BUMP_VY: f32 = -0.2;
const FIRST_SPAWN_SEC: f32 = 0.3;
const SPAWN_GAP_START: f32 = 1.7;
const SPAWN_GAP_STEP: f32 = 0.07;
const SPAWN_GAP_MIN: f32 = 1.0;
const POP_SEC: f32 = 0.35;

struct Balloon {
    x: f32,
    y: f32,
    vy: f32,
    fall: f32,
    hp: u32,
    hue: f32,
    sway: f32,
    /// Seconds left on the white hit flash.
    flash: f32,
}

pub struct BalloonPop {
    w: f32,
    h: f32,
    balloons: Vec<Balloon>,
    spawned: u32,
    next_spawn: f32,
    popped: u32,
    /// Pop bursts: position, hue, age.
    bursts: Vec<(f32, f32, f32, f32)>,
    t: f32,
    phase: Phase,
    confetti: Confetti,
    next_burst: f32,
}

impl BalloonPop {
    pub fn new(w: usize, h: usize, _: &mut Rng) -> Self {
        let mut s = Self {
            w: w.max(1) as f32,
            h: h.max(1) as f32,
            balloons: Vec::new(),
            spawned: 0,
            next_spawn: FIRST_SPAWN_SEC,
            popped: 0,
            bursts: Vec::new(),
            t: 0.0,
            phase: Phase::Playing,
            confetti: Confetti::default(),
            next_burst: 0.0,
        };
        s.reset();
        s
    }

    fn reset(&mut self) {
        self.balloons.clear();
        self.bursts.clear();
        self.spawned = 0;
        self.next_spawn = FIRST_SPAWN_SEC;
        self.popped = 0;
        self.phase = Phase::Playing;
        self.confetti.clear();
    }

    fn aspect(&self) -> f32 {
        self.w / self.h
    }

    fn spawn(&mut self, rng: &mut Rng) {
        let margin = R + 0.05;
        let fall = FALL_BASE + FALL_PER_BALLOON * self.spawned as f32;
        self.balloons.push(Balloon {
            x: rng.range_f32(margin, (self.aspect() - margin).max(margin)),
            y: -R * 1.3,
            vy: fall,
            fall,
            hp: rng.range_usize(HP_MIN as usize, HP_MAX as usize + 1) as u32,
            hue: rng.range_f32(0.0, 360.0),
            sway: rng.range_f32(0.0, std::f32::consts::TAU),
            flash: 0.0,
        });
        self.spawned += 1;
        let gap = (SPAWN_GAP_START - SPAWN_GAP_STEP * self.spawned as f32).max(SPAWN_GAP_MIN);
        self.next_spawn = gap;
    }

    fn balloon_x(&self, b: &Balloon) -> f32 {
        b.x + (self.t * 1.3 + b.sway).sin() * 0.03
    }

    /// Hit the front-most balloon under `(x, y)`, if any.
    fn poke(&mut self, x: f32, y: f32) {
        let hit = (0..self.balloons.len()).rev().find(|&i| {
            let b = &self.balloons[i];
            let (dx, dy) = (self.balloon_x(b) - x, b.y - y);
            (dx * dx + dy * dy).sqrt() <= HIT_R
        });
        let Some(i) = hit else { return };
        let bx = self.balloon_x(&self.balloons[i]);
        let b = &mut self.balloons[i];
        b.hp -= 1;
        b.flash = 0.12;
        b.vy = BUMP_VY;
        if b.hp == 0 {
            self.bursts.push((bx, b.y, b.hue, 0.0));
            self.balloons.remove(i);
            self.popped += 1;
        }
    }

    fn step_playing(&mut self, dt: f32, input: &Input, rng: &mut Rng) {
        for p in input.presses().iter().filter(|p| p.is_pointed()) {
            self.poke(p.x * self.aspect(), p.y);
        }
        if self.popped >= GOAL {
            self.phase = Phase::Cleared(0.0);
            self.confetti.burst(self.aspect() * 0.5, 0.6, 90, rng);
            self.next_burst = 0.5;
            return;
        }
        if self.spawned < GOAL {
            self.next_spawn -= dt;
            if self.next_spawn <= 0.0 {
                self.spawn(rng);
            }
        }
        for b in &mut self.balloons {
            // A bumped balloon eases back to its falling speed.
            b.vy += (b.fall - b.vy) * (1.0 - (-dt * 2.5).exp());
            b.y += b.vy * dt;
            b.flash = (b.flash - dt).max(0.0);
        }
        if self.balloons.iter().any(|b| b.y + R >= GROUND_Y) {
            self.phase = Phase::Over(0.0);
        }
    }
}

impl Sim for BalloonPop {
    fn resize(&mut self, w: usize, h: usize, _: &mut Rng) {
        self.w = w.max(1) as f32;
        self.h = h.max(1) as f32;
    }

    fn step(&mut self, dt: f32, input: &Input, rng: &mut Rng) {
        let dt = sanitize_dt(dt);
        self.t = (self.t + dt) % 1000.0;
        self.confetti.step(dt);
        for b in &mut self.bursts {
            b.3 += dt;
        }
        self.bursts.retain(|b| b.3 < POP_SEC);
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
                    self.confetti.burst(rng.range_f32(0.2, self.aspect() - 0.2), 0.7, 30, rng);
                }
            }
        }
    }

    fn render(&mut self, frame: &mut Frame, _: &Theme) {
        let u = unit(frame);
        let fw = frame.w as f32;
        vgradient(frame, Rgb::new(120, 190, 250), Rgb::new(210, 236, 255));
        for i in 0..5 {
            let x = (i as f32 * 0.83 + 0.3) % (fw / u + 0.4);
            let y = 0.2 + (i as f32 * 1.9).sin().abs() * 0.25;
            for (dx, r) in [(-0.08, 0.05), (0.0, 0.075), (0.08, 0.05)] {
                frame.disc((x + dx) * u, y * u, r * u, Rgb::new(255, 255, 255), 0.85);
            }
        }
        for px in 0..frame.w {
            let x = px as f32 / u;
            let top = 0.8 + (x * 2.1).sin() * 0.03;
            super::column_from(frame, px, top * u, Rgb::new(120, 190, 110));
        }
        frame.rect(0.0, GROUND_Y * u, fw, u, Rgb::new(90, 160, 70), 1.0);

        for b in &self.balloons {
            let x = self.balloon_x(b) * u;
            let y = b.y * u;
            let r = R * u;
            let body = Rgb::from_hsv(b.hue, 0.75, 0.95);
            let body = body.lerp(Rgb::new(255, 255, 255), b.flash / 0.12);
            frame.line((x, y + r), (x + (self.t * 3.0 + b.sway).sin() * r * 0.2, y + r * 2.4), (u * 0.008).max(1.0), Rgb::new(90, 90, 90), 0.8);
            frame.disc(x, y + r * 0.98, r * 0.16, body.lerp(Rgb::new(0, 0, 0), 0.25), 1.0);
            frame.disc(x, y, r, body, 1.0);
            frame.disc(x - r * 0.35, y - r * 0.4, r * 0.22, Rgb::new(255, 255, 255), 0.55);
            // Remaining hits as dots across the middle.
            let dot = (r * 0.12).max(1.0);
            for k in 0..b.hp {
                let dx = (k as f32 - (b.hp - 1) as f32 * 0.5) * dot * 2.8;
                frame.disc(x + dx, y + r * 0.2, dot, Rgb::new(255, 255, 255), 0.9);
            }
        }
        for &(x, y, hue, age) in &self.bursts {
            let k = age / POP_SEC;
            let c = Rgb::from_hsv(hue, 0.75, 0.95);
            ring(frame, x * u, y * u, (R + k * 0.12) * u, (u * 0.02).max(1.0), c, 1.0 - k);
            for i in 0..6 {
                let a = i as f32 * std::f32::consts::TAU / 6.0;
                let d = (R * 0.5 + k * 0.2) * u;
                frame.disc(x * u + a.cos() * d, y * u + a.sin() * d, (0.02 * u).max(1.0), c, 1.0 - k);
            }
        }

        self.confetti.render(frame);
        draw_progress(frame, self.popped);
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

    /// Clicks the lowest balloon, at most every `gap` seconds.
    fn bot_input(sim: &BalloonPop, since: f32, gap: f32) -> Option<Input> {
        if since < gap {
            return None;
        }
        let b = sim.balloons.iter().max_by(|a, b| a.y.total_cmp(&b.y))?;
        Some(Input::press(sim.balloon_x(b) / sim.aspect(), b.y))
    }

    #[test]
    fn ten_balloons_each_needing_three_to_five_hits() {
        let mut rng = Rng::new(1);
        let mut sim = BalloonPop::new(320, 96, &mut rng);
        for _ in 0..GOAL {
            sim.spawn(&mut rng);
        }
        assert_eq!(sim.balloons.len(), GOAL as usize);
        assert!(sim.balloons.iter().all(|b| (HP_MIN..=HP_MAX).contains(&b.hp)));
    }

    #[test]
    fn a_balloon_pops_after_exactly_its_hp_in_hits() {
        let mut rng = Rng::new(2);
        let mut sim = BalloonPop::new(320, 96, &mut rng);
        sim.spawn(&mut rng);
        let hp = sim.balloons[0].hp;
        for i in 0..hp {
            assert_eq!(sim.popped, 0, "popped early after {i} hits");
            let b = &sim.balloons[0];
            let (x, y) = (sim.balloon_x(b), b.y);
            sim.poke(x, y);
        }
        assert_eq!(sim.popped, 1);
        assert!(sim.balloons.is_empty());
    }

    #[test]
    fn a_balloon_reaching_the_ground_ends_the_run_and_it_restarts() {
        let mut rng = Rng::new(3);
        let mut sim = BalloonPop::new(320, 96, &mut rng);
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
        assert!(overs >= 2, "only {overs} game overs");
    }

    #[test]
    fn clicking_five_times_a_second_pops_all_ten() {
        for seed in 1..11 {
            let mut rng = Rng::new(seed);
            let mut sim = BalloonPop::new(320, 96, &mut rng);
            let mut since = 1.0;
            for _ in 0..60 * 60 {
                let input = bot_input(&sim, since, 0.2);
                since = if input.is_some() { 0.0 } else { since + DT };
                sim.step(DT, &input.unwrap_or_default(), &mut rng);
                assert!(!matches!(sim.phase, Phase::Over(_)), "seed {seed} lost at {} popped", sim.popped);
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
    fn clicking_once_a_second_is_not_enough() {
        let mut lost = 0;
        for seed in 1..6 {
            let mut rng = Rng::new(seed);
            let mut sim = BalloonPop::new(320, 96, &mut rng);
            let mut since = 1.0;
            for _ in 0..60 * 60 {
                let input = bot_input(&sim, since, 1.0);
                since = if input.is_some() { 0.0 } else { since + DT };
                sim.step(DT, &input.unwrap_or_default(), &mut rng);
                if matches!(sim.phase, Phase::Over(_)) {
                    lost += 1;
                    break;
                }
            }
        }
        assert_eq!(lost, 5);
    }

    #[test]
    fn renders_opaque_at_odd_and_tiny_sizes_with_bad_dt() {
        let mut rng = Rng::new(5);
        let mut sim = BalloonPop::new(320, 96, &mut rng);
        for (w, h) in [(321, 97), (1, 1), (0, 0), (4, 7)] {
            sim.resize(w, h, &mut rng);
            for dt in [f32::NAN, -1.0, 1000.0, DT] {
                sim.spawn(&mut rng);
                sim.step(dt, &Input::press(0.5, 0.5), &mut rng);
            }
            let mut frame = Frame::new(w, h);
            sim.render(&mut frame, &Theme::default());
            assert!(frame.pixels.chunks_exact(4).all(|p| p[3] == 255));
        }
    }
}
