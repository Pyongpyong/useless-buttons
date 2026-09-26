//! Memory: 6 or 8 numbered cards, in pairs, are dealt face up for a
//! moment and then turned face down. Click cards to turn them over; every
//! two in a row must be a matching pair. Turn every pair up to win the
//! round; a mismatch, or running out of time, ends the run. Ten rounds
//! clear it.
use super::{
    draw_cleared_flash, draw_game_over, draw_number, draw_progress, draw_time_bar, sanitize_dt,
    unit, vgradient, Confetti, Phase, GOAL,
};
use crate::paint::{Frame, Rgb, Theme};
use crate::rng::Rng;
use crate::sims::{Input, Sim};

/// Rounds (0-based) before 8-card deals can appear, and from which on
/// every deal has 8.
const EIGHT_FROM: u32 = 3;
const ALWAYS_EIGHT_FROM: u32 = 7;
const PREVIEW_BASE: f32 = 1.2;
const PREVIEW_PER_CARD: f32 = 0.2;
/// Each won round shortens the next preview by this much, down to the base.
const PREVIEW_SHRINK: f32 = 0.08;
const PLAY_BASE: f32 = 2.0;
const PLAY_PER_CARD: f32 = 0.9;
const FLIP_SEC: f32 = 0.15;
/// Beat between a won round and the next deal.
const BETWEEN_SEC: f32 = 0.6;
const CARD_Y: f32 = 0.56;

struct Card {
    value: u32,
    /// Turned face up by the player (or matched).
    up: bool,
    matched: bool,
    /// Animated turn: 0 = face down, 1 = face up.
    flip: f32,
}

#[derive(Clone, Copy, PartialEq)]
enum Round {
    /// Cards dealt face up; the time is how long they stay up.
    Preview(f32),
    Play(f32),
    Won(f32),
}

pub struct Memory {
    w: f32,
    h: f32,
    cards: Vec<Card>,
    round: Round,
    preview: f32,
    limit: f32,
    pick: Option<usize>,
    wins: u32,
    phase: Phase,
    confetti: Confetti,
    next_burst: f32,
}

impl Memory {
    pub fn new(w: usize, h: usize, rng: &mut Rng) -> Self {
        let mut s = Self {
            w: w.max(1) as f32,
            h: h.max(1) as f32,
            cards: Vec::new(),
            round: Round::Preview(0.0),
            preview: 0.0,
            limit: 0.0,
            pick: None,
            wins: 0,
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

    fn deal(&mut self, rng: &mut Rng) {
        let n = if self.wins >= ALWAYS_EIGHT_FROM || (self.wins >= EIGHT_FROM && rng.bool_p(0.5)) { 8 } else { 6 };
        let mut values: Vec<u32> = (1..=n as u32 / 2).flat_map(|v| [v, v]).collect();
        for i in (1..values.len()).rev() {
            values.swap(i, rng.range_usize(0, i + 1));
        }
        self.cards = values.into_iter().map(|value| Card { value, up: true, matched: false, flip: 1.0 }).collect();
        self.preview = (PREVIEW_BASE + PREVIEW_PER_CARD * n as f32 - PREVIEW_SHRINK * self.wins as f32).max(PREVIEW_BASE);
        self.limit = PLAY_BASE + PLAY_PER_CARD * n as f32;
        self.pick = None;
        self.round = Round::Preview(0.0);
    }

    fn aspect(&self) -> f32 {
        self.w / self.h
    }

    /// Card width and height, in units.
    fn card_size(&self) -> (f32, f32) {
        let cell = self.aspect() / self.cards.len().max(1) as f32;
        let w = (cell * 0.8).min(0.42);
        let h = (w * 1.45).min(0.66);
        (w, h)
    }

    fn card_x(&self, i: usize) -> f32 {
        let cell = self.aspect() / self.cards.len().max(1) as f32;
        (i as f32 + 0.5) * cell
    }

    fn card_at(&self, x: f32, y: f32) -> Option<usize> {
        let (w, h) = self.card_size();
        (0..self.cards.len()).find(|&i| (x - self.card_x(i)).abs() <= w * 0.5 && (y - CARD_Y).abs() <= h * 0.5)
    }

    fn turn(&mut self, i: usize, rng: &mut Rng) {
        if self.cards[i].up {
            return;
        }
        self.cards[i].up = true;
        match self.pick.take() {
            None => self.pick = Some(i),
            Some(j) if self.cards[j].value == self.cards[i].value => {
                self.cards[i].matched = true;
                self.cards[j].matched = true;
                if self.cards.iter().all(|c| c.matched) {
                    self.wins += 1;
                    if self.wins >= GOAL {
                        self.phase = Phase::Cleared(0.0);
                        self.confetti.burst(self.aspect() * 0.5, 0.6, 90, rng);
                        self.next_burst = 0.5;
                    } else {
                        self.round = Round::Won(0.0);
                    }
                }
            }
            Some(_) => self.phase = Phase::Over(0.0),
        }
    }

    fn step_playing(&mut self, dt: f32, input: &Input, rng: &mut Rng) {
        match self.round {
            Round::Preview(t) => {
                let t = t + dt;
                if t >= self.preview {
                    for c in &mut self.cards {
                        c.up = false;
                    }
                    self.round = Round::Play(0.0);
                } else {
                    self.round = Round::Preview(t);
                }
            }
            Round::Play(t) => {
                for p in input.presses().iter().filter(|p| p.is_pointed()) {
                    if let Some(i) = self.card_at(p.x * self.aspect(), p.y) {
                        self.turn(i, rng);
                        if self.phase != Phase::Playing || self.round != Round::Play(t) {
                            return;
                        }
                    }
                }
                let t = t + dt;
                if t >= self.limit {
                    self.phase = Phase::Over(0.0);
                } else {
                    self.round = Round::Play(t);
                }
            }
            Round::Won(t) => {
                let t = t + dt;
                if t >= BETWEEN_SEC {
                    self.deal(rng);
                } else {
                    self.round = Round::Won(t);
                }
            }
        }
    }

    fn draw_card(&self, frame: &mut Frame, i: usize) {
        let u = unit(frame);
        let c = &self.cards[i];
        let (w, h) = self.card_size();
        let (w, h) = (w * u, h * u);
        let x = self.card_x(i) * u;
        let y = CARD_Y * u;
        // Turning: squeeze to zero width at the halfway point.
        let squeeze = (c.flip * std::f32::consts::PI).cos().abs().max(0.04);
        let face_up = c.flip > 0.5;
        let cw = w * squeeze;
        let left = x - cw * 0.5;
        let top = y - h * 0.5;
        let edge = (u * 0.012).max(1.0);
        let outline = if c.matched { Rgb::new(80, 200, 110) } else { Rgb::new(20, 25, 40) };
        frame.rect(left - edge, top - edge, cw + edge * 2.0, h + edge * 2.0, outline, 1.0);
        if face_up {
            frame.rect(left, top, cw, h, Rgb::new(250, 246, 234), 1.0);
            let ink = Rgb::from_hsv(c.value as f32 * 47.0 + 200.0, 0.75, 0.75);
            if squeeze > 0.5 {
                draw_number(frame, c.value, x, y, h * 0.42, ink);
            }
        } else {
            // Every back is identical: blue with a diagonal lattice.
            frame.rect(left, top, cw, h, Rgb::new(50, 80, 170), 1.0);
            let inset = edge * 2.0;
            let step = (0.05 * u).max(3.0);
            let x0 = (left + inset).max(0.0) as usize;
            let x1 = ((left + cw - inset).max(0.0) as usize).min(frame.w);
            let y0 = (top + inset).max(0.0) as usize;
            let y1 = ((top + h - inset).max(0.0) as usize).min(frame.h);
            for py in y0..y1 {
                for px in x0..x1 {
                    let (a, b) = (((px + py) as f32) % step, ((px + frame.h * 4 - py) as f32) % step);
                    if a < 1.0 || b < 1.0 {
                        frame.blend_pixel(px as i64, py as i64, Rgb::new(120, 150, 230), 0.7);
                    }
                }
            }
        }
    }
}

impl Sim for Memory {
    fn resize(&mut self, w: usize, h: usize, _: &mut Rng) {
        self.w = w.max(1) as f32;
        self.h = h.max(1) as f32;
    }

    fn step(&mut self, dt: f32, input: &Input, rng: &mut Rng) {
        let dt = sanitize_dt(dt);
        self.confetti.step(dt);
        for c in &mut self.cards {
            let target = if c.up { 1.0 } else { 0.0 };
            let step = dt / FLIP_SEC;
            c.flip = if c.flip < target { (c.flip + step).min(target) } else { (c.flip - step).max(target) };
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
                    self.confetti.burst(rng.range_f32(0.2, self.aspect() - 0.2), 0.7, 30, rng);
                }
            }
        }
    }

    fn render(&mut self, frame: &mut Frame, _: &Theme) {
        vgradient(frame, Rgb::new(30, 110, 70), Rgb::new(18, 80, 50));
        for i in 0..self.cards.len() {
            self.draw_card(frame, i);
        }
        match (self.phase, self.round) {
            (Phase::Playing, Round::Play(t)) => draw_time_bar(frame, 1.0 - t / self.limit.max(1e-3)),
            (Phase::Playing, Round::Preview(t)) => {
                // The preview counts down too, in white, so you know when
                // the cards are about to turn.
                let u = unit(frame);
                let left = (1.0 - t / self.preview.max(1e-3)).clamp(0.0, 1.0);
                let h = (u * 0.035).max(1.0);
                frame.rect(0.0, frame.h as f32 - h, frame.w as f32 * left, h, Rgb::new(240, 240, 240), 0.8);
            }
            _ => {}
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

    fn click(sim: &Memory, i: usize) -> Input {
        Input::press(sim.card_x(i) / sim.aspect(), CARD_Y)
    }

    fn play_until(sim: &mut Memory, rng: &mut Rng) {
        while !matches!(sim.round, Round::Play(_)) {
            sim.step(DT, &Input::default(), rng);
        }
    }

    /// Remembers every card and turns the pairs over in order, one card
    /// every 0.25 s.
    fn bot_input(sim: &Memory, since: f32) -> Option<Input> {
        if since < 0.25 || !matches!(sim.round, Round::Play(_)) {
            return None;
        }
        let next = match sim.pick {
            Some(j) => (0..sim.cards.len()).find(|&i| i != j && !sim.cards[i].up && sim.cards[i].value == sim.cards[j].value),
            None => (0..sim.cards.len()).find(|&i| !sim.cards[i].up),
        }?;
        Some(click(sim, next))
    }

    #[test]
    fn deals_six_or_eight_cards_in_pairs() {
        let mut rng = Rng::new(1);
        let mut sim = Memory::new(320, 96, &mut rng);
        for wins in 0..GOAL {
            sim.wins = wins;
            sim.deal(&mut rng);
            let n = sim.cards.len();
            assert!(n == 6 || n == 8);
            if wins < EIGHT_FROM {
                assert_eq!(n, 6);
            }
            for v in 1..=n as u32 / 2 {
                assert_eq!(sim.cards.iter().filter(|c| c.value == v).count(), 2);
            }
        }
    }

    #[test]
    fn cards_show_then_hide_and_clicks_during_the_preview_do_nothing() {
        let mut rng = Rng::new(2);
        let mut sim = Memory::new(320, 96, &mut rng);
        assert!(sim.cards.iter().all(|c| c.up));
        let input = click(&sim, 0);
        sim.step(DT, &input, &mut rng);
        assert!(matches!(sim.round, Round::Preview(_)) && sim.pick.is_none());
        play_until(&mut sim, &mut rng);
        assert!(sim.cards.iter().all(|c| !c.up));
    }

    #[test]
    fn a_mismatch_ends_the_run() {
        let mut rng = Rng::new(3);
        let mut sim = Memory::new(320, 96, &mut rng);
        play_until(&mut sim, &mut rng);
        let other = (1..sim.cards.len()).find(|&i| sim.cards[i].value != sim.cards[0].value).unwrap();
        sim.step(DT, &click(&sim, 0), &mut rng);
        sim.step(DT, &click(&sim, other), &mut rng);
        assert!(matches!(sim.phase, Phase::Over(_)));
    }

    #[test]
    fn idle_rounds_time_out_and_the_game_restarts_without_ever_clearing() {
        let mut rng = Rng::new(4);
        let mut sim = Memory::new(320, 96, &mut rng);
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
    fn presses_without_a_position_turn_nothing() {
        let mut rng = Rng::new(5);
        let mut sim = Memory::new(320, 96, &mut rng);
        play_until(&mut sim, &mut rng);
        sim.step(DT, &Input::tap(), &mut rng);
        assert!(sim.cards.iter().all(|c| !c.up));
    }

    #[test]
    fn a_perfect_memory_clears_ten_rounds() {
        for seed in 1..11 {
            let mut rng = Rng::new(seed);
            let mut sim = Memory::new(320, 96, &mut rng);
            let mut since = 1.0;
            for _ in 0..60 * 120 {
                let input = bot_input(&sim, since);
                since = if input.is_some() { 0.0 } else { since + DT };
                sim.step(DT, &input.unwrap_or_default(), &mut rng);
                assert!(!matches!(sim.phase, Phase::Over(_)), "seed {seed} failed at round {}", sim.wins);
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
        let mut rng = Rng::new(6);
        let mut sim = Memory::new(320, 96, &mut rng);
        for (w, h) in [(321, 97), (1, 1), (0, 0), (4, 7)] {
            sim.resize(w, h, &mut rng);
            for dt in [f32::NAN, -1.0, 1000.0, DT] {
                sim.step(dt, &Input::press(0.1, 0.5), &mut rng);
            }
            let mut frame = Frame::new(w, h);
            sim.render(&mut frame, &Theme::default());
            assert!(frame.pixels.chunks_exact(4).all(|p| p[3] == 255));
        }
    }
}
