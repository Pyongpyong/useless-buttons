//! Django: a quick-draw shooting gallery in a western town. Every wave,
//! 3–5 targets pop up at random spots for a few seconds; shoot them all
//! (click on them) before time runs out. You get exactly one round per
//! target: a single miss ends the run. Ten clean waves clear it.
use super::{
    column_from, draw_cleared_flash, draw_game_over, draw_progress, draw_time_bar, sanitize_dt,
    unit, vgradient, Confetti, Phase, GOAL,
};
use crate::paint::{Frame, Rgb, Theme};
use crate::rng::Rng;
use crate::sims::{Input, Sim};

const TARGET_R: f32 = 0.11;
/// Clicks count as hits a little outside the drawn target.
const HIT_R: f32 = TARGET_R * 1.2;
const TARGETS_MIN: usize = 3;
const TARGETS_MAX: usize = 5;
/// Pause before each wave's targets pop up.
const READY_SEC: f32 = 0.7;
const TIME_BASE: f32 = 1.6;
const TIME_PER_TARGET: f32 = 0.5;
/// Each cleared wave shaves this fraction off the next wave's time.
const TIME_SHRINK: f32 = 0.03;
const POP_SEC: f32 = 0.15;
const SPLINTER_SEC: f32 = 0.35;
const Y_MIN: f32 = 0.28;
const Y_MAX: f32 = 0.76;
const GROUND_Y: f32 = 0.88;

struct Target {
    x: f32,
    y: f32,
    /// Seconds since this target was hit, if it was.
    hit: Option<f32>,
}

#[derive(Clone, Copy, PartialEq)]
enum Wave {
    Ready(f32),
    /// Seconds since the targets popped up.
    Live(f32),
}

struct Building {
    /// Left edge and width as fractions of the frame width.
    x: f32,
    w: f32,
    /// Height of the false front above the boardwalk, in units.
    h: f32,
    color: Rgb,
}

pub struct Django {
    w: f32,
    h: f32,
    targets: Vec<Target>,
    wave: Wave,
    limit: f32,
    ammo: u32,
    /// Bullet holes from misses: position and age.
    holes: Vec<(f32, f32, f32)>,
    wins: u32,
    phase: Phase,
    buildings: Vec<Building>,
    confetti: Confetti,
    next_burst: f32,
}

impl Django {
    pub fn new(w: usize, h: usize, rng: &mut Rng) -> Self {
        let mut buildings = Vec::new();
        let mut x = rng.range_f32(-0.02, 0.03);
        while x < 1.0 {
            let w = rng.range_f32(0.1, 0.17);
            buildings.push(Building {
                x,
                w,
                h: rng.range_f32(0.22, 0.36),
                color: Rgb::from_hsv(rng.range_f32(15.0, 40.0), rng.range_f32(0.35, 0.6), rng.range_f32(0.45, 0.7)),
            });
            x += w + rng.range_f32(0.01, 0.06);
        }
        let mut s = Self {
            w: w.max(1) as f32,
            h: h.max(1) as f32,
            targets: Vec::new(),
            wave: Wave::Ready(0.0),
            limit: 0.0,
            ammo: 0,
            holes: Vec::new(),
            wins: 0,
            phase: Phase::Playing,
            buildings,
            confetti: Confetti::default(),
            next_burst: 0.0,
        };
        s.reset();
        s
    }

    fn reset(&mut self) {
        self.targets.clear();
        self.holes.clear();
        self.wave = Wave::Ready(0.0);
        self.ammo = 0;
        self.wins = 0;
        self.phase = Phase::Playing;
        self.confetti.clear();
    }

    fn aspect(&self) -> f32 {
        self.w / self.h
    }

    fn spawn_wave(&mut self, rng: &mut Rng) {
        let n = rng.range_usize(TARGETS_MIN, TARGETS_MAX + 1);
        let margin = TARGET_R * 1.5;
        let x_max = (self.aspect() - margin).max(margin);
        self.targets.clear();
        for _ in 0..n {
            // Best of a few tries at keeping targets apart; on a very
            // narrow button they're allowed to overlap.
            let mut best = (margin, Y_MIN, -1.0f32);
            for _ in 0..30 {
                let (x, y) = (rng.range_f32(margin, x_max), rng.range_f32(Y_MIN, Y_MAX));
                let gap = self
                    .targets
                    .iter()
                    .map(|t| ((t.x - x).powi(2) + (t.y - y).powi(2)).sqrt())
                    .fold(f32::INFINITY, f32::min);
                if gap > best.2 {
                    best = (x, y, gap);
                }
                if gap > TARGET_R * 2.6 {
                    break;
                }
            }
            self.targets.push(Target { x: best.0, y: best.1, hit: None });
        }
        let shrink = (1.0 - TIME_SHRINK).powi(self.wins as i32);
        self.limit = (TIME_BASE + TIME_PER_TARGET * n as f32) * shrink;
        // One round per target, no spares.
        self.ammo = n as u32;
        self.wave = Wave::Live(0.0);
    }

    /// Fire one round. Returns `false` on a miss.
    fn shoot(&mut self, x: f32, y: f32) -> bool {
        if self.ammo == 0 {
            return true;
        }
        self.ammo -= 1;
        let hit = self
            .targets
            .iter_mut()
            .filter(|t| t.hit.is_none() && ((t.x - x).powi(2) + (t.y - y).powi(2)).sqrt() <= HIT_R)
            .min_by(|a, b| {
                let da = (a.x - x).powi(2) + (a.y - y).powi(2);
                let db = (b.x - x).powi(2) + (b.y - y).powi(2);
                da.total_cmp(&db)
            });
        match hit {
            Some(t) => {
                t.hit = Some(0.0);
                true
            }
            None => {
                if self.holes.len() >= 12 {
                    self.holes.remove(0);
                }
                self.holes.push((x, y, 0.0));
                false
            }
        }
    }

    fn remaining(&self) -> usize {
        self.targets.iter().filter(|t| t.hit.is_none()).count()
    }

    fn step_playing(&mut self, dt: f32, input: &Input, rng: &mut Rng) {
        match self.wave {
            Wave::Ready(t) => {
                let t = t + dt;
                if t >= READY_SEC {
                    self.spawn_wave(rng);
                } else {
                    self.wave = Wave::Ready(t);
                }
            }
            Wave::Live(t) => {
                // A press with no position has nowhere to aim, so it doesn't fire.
                for p in input.presses().iter().filter(|p| p.is_pointed()) {
                    if !self.shoot(p.x * self.aspect(), p.y) {
                        self.phase = Phase::Over(0.0);
                        return;
                    }
                }
                if self.remaining() == 0 {
                    self.wins += 1;
                    if self.wins >= GOAL {
                        self.phase = Phase::Cleared(0.0);
                        self.confetti.burst(self.aspect() * 0.5, 0.6, 90, rng);
                        self.next_burst = 0.5;
                    } else {
                        self.wave = Wave::Ready(0.0);
                    }
                    return;
                }
                let t = t + dt;
                if t >= self.limit || self.ammo == 0 {
                    self.phase = Phase::Over(0.0);
                    return;
                }
                self.wave = Wave::Live(t);
            }
        }
    }

    fn draw_target(&self, frame: &mut Frame, t: &Target) {
        let u = unit(frame);
        let (x, y) = (t.x * u, t.y * u);
        if let Some(age) = t.hit {
            // Splinters fly outward and fade.
            let k = age / SPLINTER_SEC;
            if k >= 1.0 {
                return;
            }
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::TAU / 8.0 + t.x * 7.0;
                let d = (0.04 + k * 0.18) * u;
                let s = (0.03 * u).max(1.0);
                let c = if i % 2 == 0 { Rgb::new(220, 40, 40) } else { Rgb::new(245, 240, 230) };
                frame.rect(x + a.cos() * d - s * 0.5, y + a.sin() * d - s * 0.5 + k * k * 0.1 * u, s, s, c, 1.0 - k);
            }
            return;
        }
        let pop = match self.wave {
            Wave::Live(age) => (age / POP_SEC).min(1.0),
            Wave::Ready(_) => 1.0,
        };
        let r = TARGET_R * u * (0.3 + 0.7 * pop);
        // Wooden stake under the target.
        frame.rect(x - r * 0.12, y + r * 0.8, r * 0.24, r * 1.1, Rgb::new(110, 70, 40), 1.0);
        frame.disc(x, y, r + (u * 0.012).max(1.0), Rgb::new(60, 30, 20), 1.0);
        for (k, c) in [(1.0, Rgb::new(245, 240, 230)), (0.72, Rgb::new(220, 40, 40)), (0.46, Rgb::new(245, 240, 230)), (0.2, Rgb::new(220, 40, 40))] {
            frame.disc(x, y, r * k, c, 1.0);
        }
    }
}

impl Sim for Django {
    fn resize(&mut self, w: usize, h: usize, _: &mut Rng) {
        self.w = w.max(1) as f32;
        self.h = h.max(1) as f32;
    }

    fn step(&mut self, dt: f32, input: &Input, rng: &mut Rng) {
        let dt = sanitize_dt(dt);
        self.confetti.step(dt);
        for t in &mut self.targets {
            if let Some(age) = &mut t.hit {
                *age += dt;
            }
        }
        for hole in &mut self.holes {
            hole.2 += dt;
        }
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
        vgradient(frame, Rgb::new(250, 140, 70), Rgb::new(255, 214, 140));
        frame.disc(fw * 0.72, 0.5 * u, 0.22 * u, Rgb::new(255, 240, 190), 0.9);

        // Flat-topped mesas on the horizon.
        for px in 0..frame.w {
            let x = px as f32 / u;
            let wave = (x * 1.1).sin() * 0.5 + (x * 2.7 + 1.3).sin() * 0.3;
            let top = if wave > 0.35 { 0.44 } else if wave > 0.0 { 0.53 } else { 0.6 };
            column_from(frame, px, top * u, Rgb::new(196, 104, 72));
        }

        // A row of false-front buildings along the street.
        let walk = 0.84;
        for b in &self.buildings {
            let x = b.x * fw;
            let w = b.w * fw;
            let top = (walk - b.h) * u;
            let dark = b.color.lerp(Rgb::new(40, 20, 10), 0.35);
            frame.rect(x, top, w, walk * u - top, b.color, 1.0);
            frame.rect(x, top, w, (0.012 * u).max(1.0), dark, 1.0);
            frame.rect(x + w * 0.12, top + 0.04 * u, w * 0.76, 0.05 * u, Rgb::new(240, 220, 170), 1.0);
            let win = 0.06 * u;
            for k in 0..2 {
                frame.rect(x + w * (0.18 + k as f32 * 0.46), top + 0.13 * u, w * 0.18, win, Rgb::new(50, 30, 20), 1.0);
            }
            frame.rect(x + w * 0.38, walk * u - 0.12 * u, w * 0.24, 0.12 * u, Rgb::new(70, 40, 25), 1.0);
        }
        frame.rect(0.0, walk * u, fw, (GROUND_Y - walk) * u, Rgb::new(140, 90, 50), 1.0);
        frame.rect(0.0, GROUND_Y * u, fw, u, Rgb::new(214, 170, 110), 1.0);

        for &(x, y, age) in &self.holes {
            let puff = (1.0 - age / 0.4).max(0.0);
            if puff > 0.0 {
                frame.disc(x * u, y * u, (0.03 + (1.0 - puff) * 0.05) * u, Rgb::new(230, 210, 170), puff * 0.8);
            }
            frame.disc(x * u, y * u, (0.018 * u).max(1.0), Rgb::new(30, 20, 15), 0.9);
        }
        if !matches!(self.phase, Phase::Cleared(_)) {
            for t in &self.targets {
                self.draw_target(frame, t);
            }
        }

        // Rounds left in the cylinder, bottom left.
        let s = (0.035 * u).max(1.5);
        for i in 0..self.targets.len() as u32 {
            let x = 0.05 * u + i as f32 * s * 1.5;
            let y = frame.h as f32 - 0.1 * u;
            let loaded = i < self.ammo;
            let brass = if loaded { Rgb::new(230, 180, 60) } else { Rgb::new(90, 70, 50) };
            frame.rect(x, y, s, s * 1.4, brass, 1.0);
            frame.disc(x + s * 0.5, y, s * 0.5, if loaded { Rgb::new(170, 110, 60) } else { brass }, 1.0);
        }

        if let (Phase::Playing, Wave::Live(t)) = (self.phase, self.wave) {
            draw_time_bar(frame, 1.0 - t / self.limit.max(1e-3));
        }
        self.confetti.render(frame);
        draw_progress(frame, self.wins);
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

    /// Shoots the first standing target dead center, at most every 0.15 s.
    fn bot_input(sim: &Django, since_shot: f32) -> Option<Input> {
        if since_shot < 0.15 || !matches!(sim.wave, Wave::Live(t) if t >= POP_SEC) {
            return None;
        }
        let t = sim.targets.iter().find(|t| t.hit.is_none())?;
        Some(Input::press(t.x / sim.aspect(), t.y))
    }

    #[test]
    fn waves_have_three_to_five_targets_inside_the_frame() {
        let mut rng = Rng::new(1);
        let mut sim = Django::new(320, 96, &mut rng);
        for _ in 0..100 {
            sim.spawn_wave(&mut rng);
            assert!((TARGETS_MIN..=TARGETS_MAX).contains(&sim.targets.len()));
            for t in &sim.targets {
                assert!(t.x - TARGET_R > 0.0 && t.x + TARGET_R < sim.aspect());
                assert!(t.y - TARGET_R > 0.0 && t.y + TARGET_R < 1.0);
            }
        }
    }

    #[test]
    fn idle_waves_time_out_and_the_game_restarts_without_ever_clearing() {
        let mut rng = Rng::new(2);
        let mut sim = Django::new(320, 96, &mut rng);
        let mut overs = 0;
        let mut was_over = false;
        for _ in 0..60 * 20 {
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
    fn a_single_miss_ends_the_run() {
        let mut rng = Rng::new(3);
        let mut sim = Django::new(320, 96, &mut rng);
        while !matches!(sim.wave, Wave::Live(_)) {
            sim.step(DT, &Input::default(), &mut rng);
        }
        // Park the targets far from where we'll shoot.
        let far = sim.aspect() - 0.2;
        for t in &mut sim.targets {
            t.x = far;
        }
        sim.step(DT, &Input::press(0.01, 0.95), &mut rng);
        assert!(matches!(sim.phase, Phase::Over(_)));
    }

    #[test]
    fn presses_without_a_position_do_not_fire() {
        let mut rng = Rng::new(4);
        let mut sim = Django::new(320, 96, &mut rng);
        while !matches!(sim.wave, Wave::Live(_)) {
            sim.step(DT, &Input::default(), &mut rng);
        }
        sim.step(DT, &Input::tap(), &mut rng);
        assert_eq!(sim.ammo, sim.targets.len() as u32);
    }

    #[test]
    fn a_good_shot_clears_ten_waves() {
        for seed in 1..11 {
            let mut rng = Rng::new(seed);
            let mut sim = Django::new(320, 96, &mut rng);
            let mut since_shot = 1.0;
            for _ in 0..60 * 60 {
                let input = bot_input(&sim, since_shot);
                since_shot = if input.is_some() { 0.0 } else { since_shot + DT };
                sim.step(DT, &input.unwrap_or_default(), &mut rng);
                assert!(!matches!(sim.phase, Phase::Over(_)), "seed {seed} failed at wave {}", sim.wins);
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
        let mut sim = Django::new(320, 96, &mut rng);
        for (w, h) in [(321, 97), (1, 1), (0, 0), (4, 7)] {
            sim.resize(w, h, &mut rng);
            for dt in [f32::NAN, -1.0, 1000.0, DT] {
                sim.step(dt, &Input::press(0.5, 0.5), &mut rng);
                sim.spawn_wave(&mut rng);
            }
            let mut frame = Frame::new(w, h);
            sim.render(&mut frame, &Theme::default());
            assert!(frame.pixels.chunks_exact(4).all(|p| p[3] == 255));
        }
    }
}
