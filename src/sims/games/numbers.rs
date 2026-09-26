//! Numbers: numbered circles are scattered across the button; press them
//! in order, 1 first. A wrong number, or running out of time, ends the
//! run. Each round adds more numbers (4 up to 8); ten rounds clear it.
use super::{
    draw_cleared_flash, draw_game_over, draw_number, draw_progress, draw_time_bar, ring,
    sanitize_dt, unit, vgradient, Confetti, Phase, GOAL,
};
use crate::paint::{Frame, Rgb, Theme};
use crate::rng::Rng;
use crate::sims::{Input, Sim};

const R: f32 = 0.1;
/// Presses count a little outside the drawn circle.
const HIT_R: f32 = R * 1.2;
const COUNT_START: usize = 4;
const COUNT_MAX: usize = 8;
/// Every this many won rounds adds one more number.
const ROUNDS_PER_EXTRA: u32 = 2;
const TIME_BASE: f32 = 1.5;
const TIME_PER_NUMBER: f32 = 0.7;
const BETWEEN_SEC: f32 = 0.5;
const Y_MIN: f32 = 0.24;
const Y_MAX: f32 = 0.86;

struct Dot {
    x: f32,
    y: f32,
    /// Seconds since it was pressed, if it has been.
    done: Option<f32>,
}

#[derive(Clone, Copy, PartialEq)]
enum Round {
    Play(f32),
    Won(f32),
}

pub struct Numbers {
    w: f32,
    h: f32,
    /// Dot `i` shows the number `i + 1`.
    dots: Vec<Dot>,
    next: usize,
    round: Round,
    limit: f32,
    wins: u32,
    /// The wrong dot pressed, shown during the game-over screen.
    wrong: Option<usize>,
    hue: f32,
    phase: Phase,
    confetti: Confetti,
    next_burst: f32,
}

impl Numbers {
    pub fn new(w: usize, h: usize, rng: &mut Rng) -> Self {
        let mut s = Self {
            w: w.max(1) as f32,
            h: h.max(1) as f32,
            dots: Vec::new(),
            next: 0,
            round: Round::Play(0.0),
            limit: 0.0,
            wins: 0,
            wrong: None,
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

    fn count(&self) -> usize {
        (COUNT_START + (self.wins / ROUNDS_PER_EXTRA) as usize).min(COUNT_MAX)
    }

    fn deal(&mut self, rng: &mut Rng) {
        let n = self.count();
        let margin = R * 1.3;
        let x_max = (self.aspect() - margin).max(margin);
        self.dots.clear();
        for _ in 0..n {
            // Best of a few tries at keeping circles apart.
            let mut best = (margin, Y_MIN, -1.0f32);
            for _ in 0..40 {
                let (x, y) = (rng.range_f32(margin, x_max), rng.range_f32(Y_MIN, Y_MAX));
                let gap = self.dots.iter().map(|d| ((d.x - x).powi(2) + (d.y - y).powi(2)).sqrt()).fold(f32::INFINITY, f32::min);
                if gap > best.2 {
                    best = (x, y, gap);
                }
                if gap > R * 2.6 {
                    break;
                }
            }
            self.dots.push(Dot { x: best.0, y: best.1, done: None });
        }
        self.next = 0;
        self.limit = TIME_BASE + TIME_PER_NUMBER * n as f32;
        self.wrong = None;
        self.hue = rng.range_f32(0.0, 360.0);
        self.round = Round::Play(0.0);
    }

    fn dot_at(&self, x: f32, y: f32) -> Option<usize> {
        // Nearest untouched dot within reach; a finished dot can't be pressed twice.
        (0..self.dots.len())
            .filter(|&i| self.dots[i].done.is_none())
            .filter(|&i| ((self.dots[i].x - x).powi(2) + (self.dots[i].y - y).powi(2)).sqrt() <= HIT_R)
            .min_by(|&a, &b| {
                let da = (self.dots[a].x - x).powi(2) + (self.dots[a].y - y).powi(2);
                let db = (self.dots[b].x - x).powi(2) + (self.dots[b].y - y).powi(2);
                da.total_cmp(&db)
            })
    }

    fn step_playing(&mut self, dt: f32, input: &Input, rng: &mut Rng) {
        match self.round {
            Round::Play(t) => {
                for p in input.presses().iter().filter(|p| p.is_pointed()) {
                    let Some(i) = self.dot_at(p.x * self.aspect(), p.y) else { continue };
                    if i != self.next {
                        self.wrong = Some(i);
                        self.phase = Phase::Over(0.0);
                        return;
                    }
                    self.dots[i].done = Some(0.0);
                    self.next += 1;
                    if self.next == self.dots.len() {
                        self.wins += 1;
                        if self.wins >= GOAL {
                            self.phase = Phase::Cleared(0.0);
                            self.confetti.burst(self.dots[i].x, self.dots[i].y, 90, rng);
                            self.next_burst = 0.5;
                        } else {
                            self.round = Round::Won(0.0);
                        }
                        return;
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
                if t + dt >= BETWEEN_SEC {
                    self.deal(rng);
                } else {
                    self.round = Round::Won(t + dt);
                }
            }
        }
    }
}

impl Sim for Numbers {
    fn resize(&mut self, w: usize, h: usize, _: &mut Rng) {
        let k = (w.max(1) as f32 / h.max(1) as f32) / self.aspect().max(1e-3);
        self.w = w.max(1) as f32;
        self.h = h.max(1) as f32;
        for d in &mut self.dots {
            d.x *= k;
        }
    }

    fn step(&mut self, dt: f32, input: &Input, rng: &mut Rng) {
        let dt = sanitize_dt(dt);
        self.confetti.step(dt);
        for d in &mut self.dots {
            if let Some(a) = &mut d.done {
                *a += dt;
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
        vgradient(frame, Rgb::new(245, 240, 228), Rgb::new(228, 220, 205));
        // Light graph-paper grid.
        let cell = 0.1 * u;
        let mut x = 0.0;
        while x < frame.w as f32 {
            frame.rect(x, 0.0, 1.0, frame.h as f32, Rgb::new(170, 190, 220), 0.25);
            x += cell;
        }
        let mut y = 0.0;
        while y < frame.h as f32 {
            frame.rect(0.0, y, frame.w as f32, 1.0, Rgb::new(170, 190, 220), 0.25);
            y += cell;
        }
        for (i, d) in self.dots.iter().enumerate() {
            let (x, y, r) = (d.x * u, d.y * u, R * u);
            let wrong = self.wrong == Some(i);
            match d.done {
                Some(age) => {
                    frame.disc(x, y, r, Rgb::new(120, 200, 130), 0.35);
                    if age < 0.3 {
                        ring(frame, x, y, r * (1.0 + age * 2.0), (u * 0.015).max(1.0), Rgb::new(90, 190, 110), 1.0 - age / 0.3);
                    }
                    draw_number(frame, i as u32 + 1, x, y, r * 0.95, Rgb::new(255, 255, 255));
                }
                None => {
                    let c = if wrong { Rgb::new(220, 50, 50) } else { Rgb::from_hsv(self.hue + i as f32 * 29.0, 0.55, 0.85) };
                    frame.disc(x + r * 0.08, y + r * 0.12, r, Rgb::new(0, 0, 0), 0.15);
                    frame.disc(x, y, r, c, 1.0);
                    frame.disc(x, y, r * 0.82, c.lerp(Rgb::new(255, 255, 255), 0.25), 1.0);
                    draw_number(frame, i as u32 + 1, x, y, r * 0.95, Rgb::new(40, 40, 60));
                }
            }
        }
        if let (Phase::Playing, Round::Play(t)) = (self.phase, self.round) {
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

    fn press(sim: &Numbers, i: usize) -> Input {
        Input::press(sim.dots[i].x / sim.aspect(), sim.dots[i].y)
    }

    /// Presses the next number, one every 0.3 s.
    fn bot_input(sim: &Numbers, since: f32) -> Option<Input> {
        if since < 0.3 || !matches!(sim.round, Round::Play(_)) || sim.next >= sim.dots.len() {
            return None;
        }
        Some(press(sim, sim.next))
    }

    #[test]
    fn rounds_grow_from_four_to_eight_numbers_inside_the_frame() {
        let mut rng = Rng::new(1);
        let mut sim = Numbers::new(320, 96, &mut rng);
        let mut counts = Vec::new();
        for wins in 0..GOAL {
            sim.wins = wins;
            sim.deal(&mut rng);
            counts.push(sim.dots.len());
            for d in &sim.dots {
                assert!(d.x - R > 0.0 && d.x + R < sim.aspect() && d.y - R > 0.1 && d.y + R < 1.0);
            }
        }
        assert_eq!(counts.first(), Some(&COUNT_START));
        assert_eq!(counts.last(), Some(&COUNT_MAX));
    }

    #[test]
    fn a_wrong_number_ends_the_run() {
        let mut rng = Rng::new(2);
        let mut sim = Numbers::new(320, 96, &mut rng);
        sim.step(DT, &press(&sim, 0), &mut rng);
        assert_eq!(sim.next, 1);
        sim.step(DT, &press(&sim, 2), &mut rng);
        assert!(matches!(sim.phase, Phase::Over(_)));
        assert_eq!(sim.wrong, Some(2));
    }

    #[test]
    fn pressing_a_done_number_or_empty_paper_does_nothing() {
        let mut rng = Rng::new(3);
        let mut sim = Numbers::new(320, 96, &mut rng);
        sim.step(DT, &press(&sim, 0), &mut rng);
        sim.step(DT, &press(&sim, 0), &mut rng);
        sim.step(DT, &Input::tap(), &mut rng);
        assert_eq!(sim.phase, Phase::Playing);
        assert_eq!(sim.next, 1);
    }

    #[test]
    fn idle_rounds_time_out_and_the_game_restarts_without_ever_clearing() {
        let mut rng = Rng::new(4);
        let mut sim = Numbers::new(320, 96, &mut rng);
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
    fn pressing_in_order_clears_ten_rounds() {
        for seed in 1..11 {
            let mut rng = Rng::new(seed);
            let mut sim = Numbers::new(320, 96, &mut rng);
            let mut since = 1.0;
            for _ in 0..60 * 90 {
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
        let mut rng = Rng::new(5);
        let mut sim = Numbers::new(320, 96, &mut rng);
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
