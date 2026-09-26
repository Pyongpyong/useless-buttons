//! Pong: you're the left paddle, a slow-footed computer is the right one.
//! Press anywhere and your paddle slides to that height. Where the ball
//! meets your paddle sets its angle, so edge hits send it steep enough to
//! beat the computer. Score ten points to clear; let one past you and
//! it's game over. Every serve comes at you.
use super::{
    draw_cleared_flash, draw_game_over, draw_progress, sanitize_dt, substeps, unit, vgradient,
    Confetti, Phase, GOAL,
};
use crate::paint::{Frame, Rgb, Theme};
use crate::rng::Rng;
use crate::sims::{Input, Sim};

const COURT_TOP: f32 = 0.16;
const COURT_BOTTOM: f32 = 0.97;
const PADDLE_H: f32 = 0.24;
const PADDLE_W: f32 = 0.035;
/// Paddle centers' distance from their own edge of the frame.
const PADDLE_INSET: f32 = 0.12;
const PLAYER_SPEED: f32 = 2.4;
/// The computer is slower than you, and only chases a ball that's
/// coming its way and past this fraction of the court.
const AI_SPEED: f32 = 0.4;
const AI_WAKE: f32 = 0.4;
const AI_IDLE_SPEED: f32 = 0.3;
const BALL_R: f32 = 0.025;
const SERVE_SPEED: f32 = 2.0;
const HIT_SPEED_UP: f32 = 1.06;
const MAX_SPEED: f32 = 3.2;
/// Steepest angle off a paddle's very edge, from horizontal.
const MAX_BOUNCE: f32 = 1.0;
const SERVE_SEC: f32 = 0.7;

pub struct Pong {
    w: f32,
    h: f32,
    player: f32,
    target: f32,
    ai: f32,
    ball: (f32, f32),
    vel: (f32, f32),
    serve: f32,
    points: u32,
    /// Seconds since the last point, for a little flash on the right edge.
    scored: f32,
    phase: Phase,
    confetti: Confetti,
    next_burst: f32,
}

impl Pong {
    pub fn new(w: usize, h: usize, _: &mut Rng) -> Self {
        let mut s = Self {
            w: w.max(1) as f32,
            h: h.max(1) as f32,
            player: 0.0,
            target: 0.0,
            ai: 0.0,
            ball: (0.0, 0.0),
            vel: (0.0, 0.0),
            serve: SERVE_SEC,
            points: 0,
            scored: 10.0,
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

    fn mid(&self) -> f32 {
        (COURT_TOP + COURT_BOTTOM) * 0.5
    }

    fn reset(&mut self) {
        self.player = self.mid();
        self.target = self.player;
        self.ai = self.mid();
        self.points = 0;
        self.phase = Phase::Playing;
        self.confetti.clear();
        self.new_serve();
    }

    fn new_serve(&mut self) {
        self.ball = (self.aspect() * 0.5, self.mid());
        self.vel = (0.0, 0.0);
        self.serve = SERVE_SEC;
    }

    fn ai_x(&self) -> f32 {
        self.aspect() - PADDLE_INSET
    }

    fn clamp_paddle(y: f32) -> f32 {
        y.clamp(COURT_TOP + PADDLE_H * 0.5, COURT_BOTTOM - PADDLE_H * 0.5)
    }

    fn bounce(&mut self, paddle_y: f32, dir: f32) {
        let offset = ((self.ball.1 - paddle_y) / (PADDLE_H * 0.5)).clamp(-1.0, 1.0);
        let angle = offset * MAX_BOUNCE;
        let speed = (self.vel.0.hypot(self.vel.1) * HIT_SPEED_UP).min(MAX_SPEED);
        self.vel = (dir * angle.cos() * speed, angle.sin() * speed);
    }

    /// One physics substep. Returns `Some(true)` when you score,
    /// `Some(false)` when the ball gets past you.
    fn physics(&mut self, h: f32, rng: &mut Rng) -> Option<bool> {
        let step = PLAYER_SPEED * h;
        self.player = Self::clamp_paddle(self.player + (self.target - self.player).clamp(-step, step));
        let chasing = self.vel.0 > 0.0 && self.ball.0 > self.aspect() * AI_WAKE;
        let (goal, speed) = if chasing { (self.ball.1, AI_SPEED) } else { (self.mid(), AI_IDLE_SPEED) };
        let step = speed * h;
        self.ai = Self::clamp_paddle(self.ai + (goal - self.ai).clamp(-step, step));

        if self.serve > 0.0 {
            self.serve -= h;
            if self.serve <= 0.0 {
                let angle = rng.range_f32(0.2, 0.5) * if rng.bool_p(0.5) { 1.0 } else { -1.0 };
                self.vel = (-angle.cos() * SERVE_SPEED, angle.sin() * SERVE_SPEED);
            }
            return None;
        }

        self.ball.0 += self.vel.0 * h;
        self.ball.1 += self.vel.1 * h;
        if self.ball.1 < COURT_TOP + BALL_R {
            self.ball.1 = COURT_TOP + BALL_R;
            self.vel.1 = self.vel.1.abs();
        } else if self.ball.1 > COURT_BOTTOM - BALL_R {
            self.ball.1 = COURT_BOTTOM - BALL_R;
            self.vel.1 = -self.vel.1.abs();
        }

        let reach = PADDLE_H * 0.5 + BALL_R;
        let face = PADDLE_W * 0.5 + BALL_R;
        if self.vel.0 < 0.0
            && self.ball.0 <= PADDLE_INSET + face
            && self.ball.0 >= PADDLE_INSET - face
            && (self.ball.1 - self.player).abs() <= reach
        {
            self.ball.0 = PADDLE_INSET + face;
            self.bounce(self.player, 1.0);
        } else if self.vel.0 > 0.0
            && self.ball.0 >= self.ai_x() - face
            && self.ball.0 <= self.ai_x() + face
            && (self.ball.1 - self.ai).abs() <= reach
        {
            self.ball.0 = self.ai_x() - face;
            self.bounce(self.ai, -1.0);
        }

        if self.ball.0 > self.aspect() + BALL_R {
            return Some(true);
        }
        if self.ball.0 < -BALL_R {
            return Some(false);
        }
        None
    }

    fn step_playing(&mut self, dt: f32, input: &Input, rng: &mut Rng) {
        if let Some(p) = input.presses().iter().rev().find(|p| p.is_pointed()) {
            self.target = Self::clamp_paddle(p.y);
        }
        let (n, h) = substeps(dt);
        for _ in 0..n {
            match self.physics(h, rng) {
                Some(true) => {
                    self.points += 1;
                    self.scored = 0.0;
                    if self.points >= GOAL {
                        self.phase = Phase::Cleared(0.0);
                        self.confetti.burst(self.aspect() * 0.5, 0.6, 90, rng);
                        self.next_burst = 0.5;
                        return;
                    }
                    self.new_serve();
                }
                Some(false) => {
                    self.phase = Phase::Over(0.0);
                    return;
                }
                None => {}
            }
        }
    }
}

impl Sim for Pong {
    fn resize(&mut self, w: usize, h: usize, _: &mut Rng) {
        self.w = w.max(1) as f32;
        self.h = h.max(1) as f32;
        self.ball.0 = self.ball.0.clamp(0.0, self.aspect());
    }

    fn step(&mut self, dt: f32, input: &Input, rng: &mut Rng) {
        let dt = sanitize_dt(dt);
        self.confetti.step(dt);
        self.scored += dt;
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
        let fw = frame.w as f32;
        vgradient(frame, Rgb::new(8, 12, 10), Rgb::new(4, 20, 12));
        let white = Rgb::new(235, 255, 235);
        let line = (0.012 * u).max(1.0);
        frame.rect(0.0, COURT_TOP * u - line, fw, line, white, 0.7);
        frame.rect(0.0, COURT_BOTTOM * u, fw, line, white, 0.7);
        // Dashed net.
        let dash = 0.05 * u;
        let mut y = COURT_TOP * u;
        while y < COURT_BOTTOM * u {
            frame.rect(fw * 0.5 - line * 0.5, y, line, dash, white, 0.5);
            y += dash * 2.0;
        }
        if self.scored < 0.4 {
            frame.rect(fw - 0.03 * u, COURT_TOP * u, 0.03 * u, (COURT_BOTTOM - COURT_TOP) * u, Rgb::new(120, 255, 140), 1.0 - self.scored / 0.4);
        }
        for (x, y, c) in [(PADDLE_INSET, self.player, Rgb::new(120, 255, 140)), (self.ai_x(), self.ai, white)] {
            frame.rect((x - PADDLE_W * 0.5) * u, (y - PADDLE_H * 0.5) * u, PADDLE_W * u, PADDLE_H * u, c, 1.0);
        }
        if !matches!(self.phase, Phase::Cleared(_)) {
            let s = BALL_R * 2.0 * u;
            frame.rect(self.ball.0 * u - s * 0.5, self.ball.1 * u - s * 0.5, s, s, white, 1.0);
        }
        self.confetti.render(frame);
        draw_progress(frame, self.points);
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

    /// Where the ball will cross `x`, folding in bounces off the court's
    /// top and bottom.
    fn predict_y(sim: &Pong, x: f32) -> f32 {
        let t = ((x - sim.ball.0) / sim.vel.0).max(0.0);
        let (lo, hi) = (COURT_TOP + BALL_R, COURT_BOTTOM - BALL_R);
        let span = hi - lo;
        let raw = (sim.ball.1 - lo + sim.vel.1 * t).rem_euclid(span * 2.0);
        lo + if raw > span { span * 2.0 - raw } else { raw }
    }

    /// Meets the ball off-center so it rebounds steeply, away from
    /// wherever the computer's paddle is.
    fn bot_input(sim: &Pong, since: f32) -> Option<Input> {
        if since < 0.1 || sim.vel.0 >= 0.0 {
            return None;
        }
        let y = predict_y(sim, PADDLE_INSET);
        let up = sim.ai > sim.mid();
        let aim = if up { y + PADDLE_H * 0.35 } else { y - PADDLE_H * 0.35 };
        Some(Input::press(0.1, aim))
    }

    #[test]
    fn paddle_slides_to_the_height_you_press() {
        let mut rng = Rng::new(1);
        let mut sim = Pong::new(320, 96, &mut rng);
        sim.step(DT, &Input::press(0.5, 0.0), &mut rng);
        for _ in 0..60 {
            sim.step(DT, &Input::default(), &mut rng);
        }
        assert!((sim.player - (COURT_TOP + PADDLE_H * 0.5)).abs() < 1e-3);
    }

    #[test]
    fn idle_player_is_passed_and_the_game_restarts_without_ever_clearing() {
        let mut rng = Rng::new(2);
        let mut sim = Pong::new(320, 96, &mut rng);
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
    fn angled_returns_beat_the_computer_ten_times() {
        for seed in 1..11 {
            let mut rng = Rng::new(seed);
            let mut sim = Pong::new(320, 96, &mut rng);
            let mut since = 1.0;
            for _ in 0..60 * 120 {
                let input = bot_input(&sim, since);
                since = if input.is_some() { 0.0 } else { since + DT };
                sim.step(DT, &input.unwrap_or_default(), &mut rng);
                assert!(!matches!(sim.phase, Phase::Over(_)), "seed {seed} lost at {} points", sim.points);
                if sim.cleared() {
                    break;
                }
            }
            assert!(sim.cleared(), "seed {seed} never cleared ({} points)", sim.points);
            for _ in 0..600 {
                sim.step(DT, &Input::press(0.5, 0.5), &mut rng);
            }
            assert!(sim.cleared(), "clearing must be permanent");
        }
    }

    #[test]
    fn renders_opaque_at_odd_and_tiny_sizes_with_bad_dt() {
        let mut rng = Rng::new(5);
        let mut sim = Pong::new(320, 96, &mut rng);
        for (w, h) in [(321, 97), (1, 1), (0, 0), (4, 7)] {
            sim.resize(w, h, &mut rng);
            for dt in [f32::NAN, -1.0, 1000.0, DT] {
                sim.step(dt, &Input::press(0.5, 0.5), &mut rng);
            }
            let mut frame = Frame::new(w, h);
            sim.render(&mut frame, &Theme::default());
            assert!(frame.pixels.chunks_exact(4).all(|p| p[3] == 255));
            assert!(sim.ball.0.is_finite() && sim.ball.1.is_finite());
        }
    }
}
