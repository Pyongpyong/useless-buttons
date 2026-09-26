//! Rhythm: an osu!-style hit-circle game. Each screen brings 3–5
//! numbered circles on a beat; a ring shrinks onto each one, and you
//! press the circle as the ring closes. Too early, too late, or not at
//! all is a miss and ends the run. Clear ten screens.
use super::{
    draw_cleared_flash, draw_game_over, draw_number, draw_progress, ring, sanitize_dt, unit,
    vgradient, Confetti, Phase, GOAL,
};
use crate::paint::{Frame, Rgb, Theme};
use crate::rng::Rng;
use crate::sims::{Input, Sim};

const R: f32 = 0.11;
/// Presses count a little outside the drawn circle.
const HIT_R: f32 = R * 1.25;
/// The approach ring starts this many radii out and closes onto the circle.
const RING_START: f32 = 3.0;
const APPROACH: f32 = 0.9;
/// A press within this many seconds of the beat is a hit...
const WINDOW: f32 = 0.15;
/// ...and within this many, a perfect one.
const PERFECT: f32 = 0.06;
const CIRCLES_MIN: usize = 3;
const CIRCLES_MAX: usize = 5;
const BEAT_START: f32 = 0.6;
const BEAT_STEP: f32 = 0.02;
const BEAT_MIN: f32 = 0.42;
/// Quiet moment before a screen's first circle starts approaching.
const LEAD_IN: f32 = 0.2;
const BETWEEN_SEC: f32 = 0.6;
const Y_MIN: f32 = 0.24;
const Y_MAX: f32 = 0.84;
const HIT_FX_SEC: f32 = 0.35;

struct Note {
    x: f32,
    y: f32,
    /// When the ring meets the circle, in seconds into the screen.
    beat: f32,
    /// Hit: how far off the beat it was, and seconds since.
    hit: Option<(f32, f32)>,
}

#[derive(Clone, Copy, PartialEq)]
enum Screen {
    Play(f32),
    Won(f32),
}

pub struct Rhythm {
    w: f32,
    h: f32,
    notes: Vec<Note>,
    screen: Screen,
    wins: u32,
    /// The note that was missed, shown during the game-over screen.
    missed: Option<usize>,
    hue: f32,
    phase: Phase,
    confetti: Confetti,
    next_burst: f32,
}

impl Rhythm {
    pub fn new(w: usize, h: usize, rng: &mut Rng) -> Self {
        let mut s = Self {
            w: w.max(1) as f32,
            h: h.max(1) as f32,
            notes: Vec::new(),
            screen: Screen::Play(0.0),
            wins: 0,
            missed: None,
            hue: 0.0,
            phase: Phase::Playing,
            confetti: Confetti::default(),
            next_burst: 0.0,
        };
        s.reset(rng);
        s
    }

    fn reset(&mut self, rng: &mut Rng) {
        self.wins = 0;
        self.phase = Phase::Playing;
        self.confetti.clear();
        self.deal(rng);
    }

    fn aspect(&self) -> f32 {
        self.w / self.h
    }

    fn beat_gap(&self) -> f32 {
        (BEAT_START - BEAT_STEP * self.wins as f32).max(BEAT_MIN)
    }

    fn deal(&mut self, rng: &mut Rng) {
        let n = rng.range_usize(CIRCLES_MIN, CIRCLES_MAX + 1);
        let gap = self.beat_gap();
        let margin = R * 1.4;
        let x_max = (self.aspect() - margin).max(margin);
        self.notes.clear();
        for i in 0..n {
            // Keep each circle clear of the ones on screen with it.
            let mut best = (margin, Y_MIN, -1.0f32);
            for _ in 0..40 {
                let (x, y) = (rng.range_f32(margin, x_max), rng.range_f32(Y_MIN, Y_MAX));
                let d = self.notes.iter().map(|o| ((o.x - x).powi(2) + (o.y - y).powi(2)).sqrt()).fold(f32::INFINITY, f32::min);
                if d > best.2 {
                    best = (x, y, d);
                }
                if d > R * 3.0 {
                    break;
                }
            }
            self.notes.push(Note { x: best.0, y: best.1, beat: LEAD_IN + APPROACH + gap * i as f32, hit: None });
        }
        self.missed = None;
        self.hue = rng.range_f32(0.0, 360.0);
        self.screen = Screen::Play(0.0);
    }

    fn visible(&self, n: &Note, t: f32) -> bool {
        n.hit.is_none() && t >= n.beat - APPROACH
    }

    fn step_playing(&mut self, dt: f32, input: &Input, rng: &mut Rng) {
        match self.screen {
            Screen::Play(t) => {
                for p in input.presses().iter().filter(|p| p.is_pointed()) {
                    let (px, py) = (p.x * self.aspect(), p.y);
                    let under = (0..self.notes.len()).find(|&i| {
                        let n = &self.notes[i];
                        self.visible(n, t) && ((n.x - px).powi(2) + (n.y - py).powi(2)).sqrt() <= HIT_R
                    });
                    let Some(i) = under else { continue };
                    let off = t - self.notes[i].beat;
                    if off.abs() > WINDOW {
                        self.missed = Some(i);
                        self.phase = Phase::Over(0.0);
                        return;
                    }
                    self.notes[i].hit = Some((off, 0.0));
                }
                if self.notes.iter().all(|n| n.hit.is_some()) {
                    self.wins += 1;
                    if self.wins >= GOAL {
                        self.phase = Phase::Cleared(0.0);
                        let last = self.notes.last().map_or((self.aspect() * 0.5, 0.5), |n| (n.x, n.y));
                        self.confetti.burst(last.0, last.1, 90, rng);
                        self.next_burst = 0.5;
                    } else {
                        self.screen = Screen::Won(0.0);
                    }
                    return;
                }
                let t = t + dt;
                if let Some(i) = self.notes.iter().position(|n| n.hit.is_none() && t > n.beat + WINDOW) {
                    self.missed = Some(i);
                    self.phase = Phase::Over(0.0);
                    return;
                }
                self.screen = Screen::Play(t);
            }
            Screen::Won(t) => {
                if t + dt >= BETWEEN_SEC {
                    self.deal(rng);
                } else {
                    self.screen = Screen::Won(t + dt);
                }
            }
        }
    }

    fn clock(&self) -> f32 {
        match self.screen {
            Screen::Play(t) => t,
            Screen::Won(_) => f32::INFINITY,
        }
    }
}

impl Sim for Rhythm {
    fn resize(&mut self, w: usize, h: usize, _: &mut Rng) {
        let k = (w.max(1) as f32 / h.max(1) as f32) / self.aspect().max(1e-3);
        self.w = w.max(1) as f32;
        self.h = h.max(1) as f32;
        for n in &mut self.notes {
            n.x *= k;
        }
    }

    fn step(&mut self, dt: f32, input: &Input, rng: &mut Rng) {
        let dt = sanitize_dt(dt);
        self.confetti.step(dt);
        for n in &mut self.notes {
            if let Some((_, age)) = &mut n.hit {
                *age += dt;
            }
        }
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
        let t = self.clock();
        // The background pulses on every beat of the screen.
        let pulse = self
            .notes
            .iter()
            .map(|n| (1.0 - (t - n.beat).abs() / 0.15).max(0.0))
            .fold(0.0f32, f32::max);
        vgradient(
            frame,
            Rgb::new(24, 10, 48).lerp(Rgb::new(60, 20, 100), pulse * 0.6),
            Rgb::new(10, 6, 28).lerp(Rgb::new(40, 12, 70), pulse * 0.6),
        );
        // Follow points: faint dots from each circle to the next.
        for w in self.notes.windows(2) {
            if t < w[1].beat - APPROACH || w[1].hit.is_some() {
                continue;
            }
            let steps = 8;
            for k in 1..steps {
                let f = k as f32 / steps as f32;
                let (x, y) = (w[0].x + (w[1].x - w[0].x) * f, w[0].y + (w[1].y - w[0].y) * f);
                frame.disc(x * u, y * u, (0.008 * u).max(1.0), Rgb::new(200, 180, 255), 0.4);
            }
        }
        // Later circles underneath, so the next one to hit is on top.
        for (i, n) in self.notes.iter().enumerate().rev() {
            let (x, y, r) = (n.x * u, n.y * u, R * u);
            let color = Rgb::from_hsv(self.hue + i as f32 * 50.0, 0.7, 1.0);
            if let Some((off, age)) = n.hit {
                if age < HIT_FX_SEC {
                    let k = age / HIT_FX_SEC;
                    let c = if off.abs() <= PERFECT { Rgb::new(255, 220, 90) } else { Rgb::new(120, 220, 255) };
                    ring(frame, x, y, r * (1.0 + k * 0.8), (u * 0.02).max(1.0), c, 1.0 - k);
                    frame.disc(x, y, r * (1.0 - k * 0.3), c, 0.5 * (1.0 - k));
                }
                continue;
            }
            if self.missed == Some(i) {
                let red = Rgb::new(255, 60, 60);
                let (s, th) = (r * 0.8, (u * 0.03).max(1.0));
                frame.line((x - s, y - s), (x + s, y + s), th, red, 1.0);
                frame.line((x - s, y + s), (x + s, y - s), th, red, 1.0);
                continue;
            }
            let lead = n.beat - t;
            if lead > APPROACH {
                continue;
            }
            let fade = ((APPROACH - lead) / 0.2).clamp(0.0, 1.0);
            frame.disc(x, y, r * 1.08, Rgb::new(255, 255, 255), fade);
            frame.disc(x, y, r, color, fade);
            draw_number(frame, i as u32 + 1, x, y, r * 0.9, Rgb::new(255, 255, 255));
            let k = (lead / APPROACH).clamp(0.0, 1.0);
            ring(frame, x, y, r * (1.0 + (RING_START - 1.0) * k), (u * 0.012).max(1.0), Rgb::new(255, 255, 255), fade * 0.9);
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

    fn press(sim: &Rhythm, i: usize) -> Input {
        Input::press(sim.notes[i].x / sim.aspect(), sim.notes[i].y)
    }

    /// Presses each circle on the frame closest to its beat.
    fn bot_input(sim: &Rhythm) -> Option<Input> {
        let t = sim.clock();
        let i = sim.notes.iter().position(|n| n.hit.is_none())?;
        ((sim.notes[i].beat - t).abs() <= DT * 0.6).then(|| press(sim, i))
    }

    fn run_until(sim: &mut Rhythm, rng: &mut Rng, until: f32) {
        while sim.clock() < until {
            sim.step(DT, &Input::default(), rng);
        }
    }

    #[test]
    fn screens_have_three_to_five_circles_on_a_steady_beat() {
        let mut rng = Rng::new(1);
        let mut sim = Rhythm::new(320, 96, &mut rng);
        for _ in 0..50 {
            sim.deal(&mut rng);
            assert!((CIRCLES_MIN..=CIRCLES_MAX).contains(&sim.notes.len()));
            for w in sim.notes.windows(2) {
                assert!((w[1].beat - w[0].beat - sim.beat_gap()).abs() < 1e-5);
            }
        }
    }

    #[test]
    fn pressing_too_early_is_a_miss() {
        let mut rng = Rng::new(2);
        let mut sim = Rhythm::new(320, 96, &mut rng);
        let beat = sim.notes[0].beat;
        run_until(&mut sim, &mut rng, beat - WINDOW - 0.1);
        sim.step(DT, &press(&sim, 0), &mut rng);
        assert!(matches!(sim.phase, Phase::Over(_)));
        assert_eq!(sim.missed, Some(0));
    }

    #[test]
    fn pressing_on_the_beat_is_a_hit_and_empty_space_does_nothing() {
        let mut rng = Rng::new(3);
        let mut sim = Rhythm::new(320, 96, &mut rng);
        let beat = sim.notes[0].beat;
        run_until(&mut sim, &mut rng, beat - 0.02);
        sim.step(DT, &Input::press(0.0, 0.0), &mut rng);
        sim.step(DT, &press(&sim, 0), &mut rng);
        assert_eq!(sim.phase, Phase::Playing);
        assert!(sim.notes[0].hit.is_some());
    }

    #[test]
    fn idle_circles_are_missed_and_the_game_restarts_without_ever_clearing() {
        let mut rng = Rng::new(4);
        let mut sim = Rhythm::new(320, 96, &mut rng);
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
        assert!(overs >= 5, "only {overs} game overs");
    }

    #[test]
    fn hitting_every_beat_clears_ten_screens() {
        for seed in 1..11 {
            let mut rng = Rng::new(seed);
            let mut sim = Rhythm::new(320, 96, &mut rng);
            for _ in 0..60 * 60 {
                let input = bot_input(&sim);
                sim.step(DT, &input.unwrap_or_default(), &mut rng);
                assert!(!matches!(sim.phase, Phase::Over(_)), "seed {seed} missed at screen {}", sim.wins);
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
        let mut sim = Rhythm::new(320, 96, &mut rng);
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
