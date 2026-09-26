//! Breakout: bounce the ball off your paddle to break all ten bricks.
//! Press anywhere and the paddle slides toward that spot; where the ball
//! lands on the paddle sets its rebound angle. Missing the ball ends the
//! run.
use super::{
    draw_cleared_flash, draw_game_over, draw_progress, sanitize_dt, substeps, unit, vgradient,
    Confetti, Phase,
};
#[cfg(test)]
use super::GOAL;
use crate::paint::{Frame, Rgb, Theme};
use crate::rng::Rng;
use crate::sims::{Input, Sim};

const PADDLE_Y: f32 = 0.88;
const PADDLE_W: f32 = 0.5;
const PADDLE_H: f32 = 0.045;
/// The paddle slides toward the last press at this speed rather than
/// jumping there, so it's a race, not a teleport.
const PADDLE_SPEED: f32 = 3.0;
const BALL_R: f32 = 0.03;
const BALL_SPEED: f32 = 1.3;
/// Each broken brick speeds the ball up by this factor.
const SPEED_UP: f32 = 1.04;
/// Steepest rebound off the paddle's very edge, from vertical.
const MAX_BOUNCE: f32 = 1.05;
/// Even a dead-center hit leaves at least this angle, keeping the ball's
/// sideways drift, so it can't get stuck bouncing straight up and down.
const MIN_BOUNCE: f32 = 0.15;
const SERVE_SEC: f32 = 0.8;
const BRICK_COLS: usize = 5;
const BRICK_ROWS: usize = 2;
const BRICK_TOP: f32 = 0.18;
const BRICK_H: f32 = 0.08;
const BRICK_GAP: f32 = 0.03;
const SIDE_MARGIN: f32 = 0.2;

struct Brick {
    x: f32,
    y: f32,
    w: f32,
    hue: f32,
}

pub struct Breakout {
    w: f32,
    h: f32,
    paddle: f32,
    target: f32,
    ball: (f32, f32),
    vel: (f32, f32),
    /// Seconds left with the ball resting on the paddle before it launches.
    serve: f32,
    bricks: Vec<Brick>,
    broken: u32,
    /// Broken-brick flashes: rect and age.
    debris: Vec<(f32, f32, f32, f32, f32)>,
    phase: Phase,
    confetti: Confetti,
    next_burst: f32,
}

impl Breakout {
    pub fn new(w: usize, h: usize, rng: &mut Rng) -> Self {
        let mut s = Self {
            w: w.max(1) as f32,
            h: h.max(1) as f32,
            paddle: 0.0,
            target: 0.0,
            ball: (0.0, 0.0),
            vel: (0.0, 0.0),
            serve: SERVE_SEC,
            bricks: Vec::new(),
            broken: 0,
            debris: Vec::new(),
            phase: Phase::Playing,
            confetti: Confetti::default(),
            next_burst: 0.0,
        };
        s.reset(rng);
        s
    }

    fn aspect(&self) -> f32 {
        self.w / self.h
    }

    fn reset(&mut self, rng: &mut Rng) {
        self.layout_bricks(rng);
        self.broken = 0;
        self.paddle = self.aspect() * 0.5;
        self.target = self.paddle;
        self.serve = SERVE_SEC;
        self.vel = (0.0, 0.0);
        self.debris.clear();
        self.phase = Phase::Playing;
        self.confetti.clear();
        self.rest_ball_on_paddle();
    }

    fn layout_bricks(&mut self, rng: &mut Rng) {
        self.bricks.clear();
        let span = (self.aspect() - SIDE_MARGIN * 2.0).max(0.5);
        let w = (span - BRICK_GAP * (BRICK_COLS - 1) as f32) / BRICK_COLS as f32;
        let base = rng.range_f32(0.0, 360.0);
        for row in 0..BRICK_ROWS {
            for col in 0..BRICK_COLS {
                self.bricks.push(Brick {
                    x: SIDE_MARGIN + col as f32 * (w + BRICK_GAP),
                    y: BRICK_TOP + row as f32 * (BRICK_H + BRICK_GAP),
                    w,
                    hue: base + row as f32 * 40.0 + col as f32 * 12.0,
                });
            }
        }
    }

    fn rest_ball_on_paddle(&mut self) {
        self.ball = (self.paddle, PADDLE_Y - BALL_R - 0.005);
    }

    fn speed(&self) -> f32 {
        BALL_SPEED * SPEED_UP.powi(self.broken as i32)
    }

    fn clamp_paddle(&self, x: f32) -> f32 {
        let half = PADDLE_W * 0.5;
        x.clamp(half, (self.aspect() - half).max(half))
    }

    fn launch(&mut self, rng: &mut Rng) {
        let side = if rng.bool_p(0.5) { 1.0 } else { -1.0 };
        let angle = side * rng.range_f32(0.35, 0.8);
        let speed = self.speed();
        self.vel = (angle.sin() * speed, -angle.cos() * speed);
    }

    /// One physics substep. Returns `true` if the ball got past the paddle.
    fn physics(&mut self, h: f32) -> bool {
        self.ball.0 += self.vel.0 * h;
        self.ball.1 += self.vel.1 * h;
        let aspect = self.aspect();
        if self.ball.0 < BALL_R {
            self.ball.0 = BALL_R;
            self.vel.0 = self.vel.0.abs();
        } else if self.ball.0 > aspect - BALL_R {
            self.ball.0 = aspect - BALL_R;
            self.vel.0 = -self.vel.0.abs();
        }
        if self.ball.1 < BALL_R {
            self.ball.1 = BALL_R;
            self.vel.1 = self.vel.1.abs();
        }

        // Bricks: at most one per substep, reflecting on the axis of least overlap.
        let (bx, by) = self.ball;
        if let Some(i) = self.bricks.iter().position(|b| {
            let cx = bx.clamp(b.x, b.x + b.w);
            let cy = by.clamp(b.y, b.y + BRICK_H);
            (bx - cx).powi(2) + (by - cy).powi(2) < BALL_R * BALL_R
        }) {
            let b = self.bricks.remove(i);
            let cx = bx.clamp(b.x, b.x + b.w);
            let cy = by.clamp(b.y, b.y + BRICK_H);
            if (bx - cx).abs() > (by - cy).abs() {
                self.vel.0 = if bx < cx { -self.vel.0.abs() } else { self.vel.0.abs() };
            } else {
                self.vel.1 = if by < cy { -self.vel.1.abs() } else { self.vel.1.abs() };
            }
            self.broken += 1;
            self.debris.push((b.x, b.y, b.w, b.hue, 0.0));
            let scale = self.speed() / (self.vel.0.hypot(self.vel.1)).max(1e-3);
            self.vel = (self.vel.0 * scale, self.vel.1 * scale);
        }

        // Paddle: the rebound angle follows where on the paddle it hit.
        let half = PADDLE_W * 0.5;
        if self.vel.1 > 0.0
            && self.ball.1 + BALL_R >= PADDLE_Y
            && self.ball.1 < PADDLE_Y + PADDLE_H
            && (self.ball.0 - self.paddle).abs() <= half + BALL_R
        {
            let offset = ((self.ball.0 - self.paddle) / half).clamp(-1.0, 1.0);
            let mut angle = offset * MAX_BOUNCE;
            if angle.abs() < MIN_BOUNCE {
                angle = MIN_BOUNCE.copysign(if angle == 0.0 { self.vel.0 } else { angle });
            }
            let speed = self.speed();
            self.vel = (angle.sin() * speed, -angle.cos() * speed);
            self.ball.1 = PADDLE_Y - BALL_R;
        }
        self.ball.1 - BALL_R > 1.0
    }

    fn step_playing(&mut self, dt: f32, input: &Input, rng: &mut Rng) {
        if let Some(p) = input.presses().iter().rev().find(|p| p.is_pointed()) {
            self.target = self.clamp_paddle(p.x * self.aspect());
        }
        let (n, h) = substeps(dt);
        for _ in 0..n {
            let step = PADDLE_SPEED * h;
            let d = self.target - self.paddle;
            self.paddle = self.clamp_paddle(self.paddle + d.clamp(-step, step));
            if self.serve > 0.0 {
                self.serve -= h;
                self.rest_ball_on_paddle();
                if self.serve <= 0.0 {
                    self.launch(rng);
                }
                continue;
            }
            if self.physics(h) {
                self.phase = Phase::Over(0.0);
                return;
            }
            if self.bricks.is_empty() {
                self.phase = Phase::Cleared(0.0);
                self.confetti.burst(self.ball.0, self.ball.1, 90, rng);
                self.next_burst = 0.5;
                return;
            }
        }
    }
}

impl Sim for Breakout {
    fn resize(&mut self, w: usize, h: usize, rng: &mut Rng) {
        let old = self.aspect();
        self.w = w.max(1) as f32;
        self.h = h.max(1) as f32;
        // Keep everything on the new width: rescale positions horizontally.
        let k = self.aspect() / old.max(1e-3);
        for b in &mut self.bricks {
            b.x *= k;
            b.w *= k;
        }
        self.paddle = self.clamp_paddle(self.paddle * k);
        self.target = self.clamp_paddle(self.target * k);
        self.ball.0 = (self.ball.0 * k).clamp(BALL_R, (self.aspect() - BALL_R).max(BALL_R));
        let _ = rng;
    }

    fn step(&mut self, dt: f32, input: &Input, rng: &mut Rng) {
        let dt = sanitize_dt(dt);
        self.confetti.step(dt);
        for d in &mut self.debris {
            d.4 += dt;
        }
        self.debris.retain(|d| d.4 < 0.3);
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
        vgradient(frame, Rgb::new(10, 10, 30), Rgb::new(24, 10, 44));
        // Faint scanlines for the arcade-monitor look.
        let mut y = 0.0;
        while y < frame.h as f32 {
            frame.rect(0.0, y, frame.w as f32, 1.0, Rgb::new(0, 0, 0), 0.25);
            y += 3.0;
        }
        for b in &self.bricks {
            let c = Rgb::from_hsv(b.hue, 0.75, 0.95);
            frame.rect(b.x * u, b.y * u, b.w * u, BRICK_H * u, c, 1.0);
            frame.rect(b.x * u, b.y * u, b.w * u, (BRICK_H * 0.25 * u).max(1.0), Rgb::new(255, 255, 255), 0.35);
            frame.rect(b.x * u, (b.y + BRICK_H * 0.8) * u, b.w * u, (BRICK_H * 0.2 * u).max(1.0), Rgb::new(0, 0, 0), 0.3);
        }
        for &(x, y, w, hue, age) in &self.debris {
            let k = age / 0.3;
            let grow = k * 0.05;
            frame.rect((x - grow) * u, (y - grow) * u, (w + grow * 2.0) * u, (BRICK_H + grow * 2.0) * u, Rgb::from_hsv(hue, 0.4, 1.0), 0.8 * (1.0 - k));
        }
        let half = PADDLE_W * 0.5;
        frame.rect((self.paddle - half) * u, PADDLE_Y * u, PADDLE_W * u, PADDLE_H * u, Rgb::new(120, 220, 255), 1.0);
        frame.rect((self.paddle - half) * u, PADDLE_Y * u, PADDLE_W * u, (PADDLE_H * 0.35 * u).max(1.0), Rgb::new(230, 250, 255), 1.0);
        if !matches!(self.phase, Phase::Cleared(_)) {
            frame.disc(self.ball.0 * u, self.ball.1 * u, BALL_R * u * 2.2, Rgb::new(255, 255, 255), 0.15);
            frame.disc(self.ball.0 * u, self.ball.1 * u, BALL_R * u, Rgb::new(255, 255, 255), 1.0);
        }
        self.confetti.render(frame);
        draw_progress(frame, self.broken);
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

    /// Keeps pressing where the ball is, a few times a second — offset
    /// a little so it comes off the paddle angled toward the nearest
    /// remaining brick.
    fn bot_input(sim: &Breakout, since: f32) -> Option<Input> {
        if since < 0.1 {
            return None;
        }
        let aim = sim
            .bricks
            .iter()
            .map(|b| b.x + b.w * 0.5)
            .min_by(|a, b| (a - sim.ball.0).abs().total_cmp(&(b - sim.ball.0).abs()))
            .unwrap_or(sim.ball.0);
        let lean = ((aim - sim.ball.0) / sim.aspect()).clamp(-0.5, 0.5) * PADDLE_W;
        Some(Input::press((sim.ball.0 - lean) / sim.aspect(), 0.5))
    }

    #[test]
    fn ten_bricks_inside_the_frame() {
        let mut rng = Rng::new(1);
        let sim = Breakout::new(320, 96, &mut rng);
        assert_eq!(sim.bricks.len(), GOAL as usize);
        for b in &sim.bricks {
            assert!(b.x >= 0.0 && b.x + b.w <= sim.aspect());
        }
    }

    #[test]
    fn paddle_slides_toward_a_press_instead_of_jumping() {
        let mut rng = Rng::new(2);
        let mut sim = Breakout::new(320, 96, &mut rng);
        let start = sim.paddle;
        sim.step(DT, &Input::press(0.05, 0.5), &mut rng);
        assert!(sim.paddle < start && sim.paddle > start - 0.1);
        for _ in 0..120 {
            sim.step(DT, &Input::default(), &mut rng);
        }
        assert!((sim.paddle - PADDLE_W * 0.5).abs() < 1e-3);
    }

    #[test]
    fn idle_paddle_misses_and_the_game_restarts_without_ever_clearing() {
        for seed in 1..6 {
            let mut rng = Rng::new(seed);
            let mut sim = Breakout::new(320, 96, &mut rng);
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
            assert!(overs >= 2, "seed {seed}: only {overs} game overs");
        }
    }

    #[test]
    fn chasing_the_ball_breaks_all_ten_bricks() {
        for seed in 1..11 {
            let mut rng = Rng::new(seed);
            let mut sim = Breakout::new(320, 96, &mut rng);
            let mut since = 1.0;
            for _ in 0..60 * 90 {
                let input = bot_input(&sim, since);
                since = if input.is_some() { 0.0 } else { since + DT };
                sim.step(DT, &input.unwrap_or_default(), &mut rng);
                assert!(!matches!(sim.phase, Phase::Over(_)), "seed {seed} missed at {} bricks", sim.broken);
                if sim.cleared() {
                    break;
                }
            }
            assert!(sim.cleared(), "seed {seed} never cleared ({} bricks)", sim.broken);
            for _ in 0..600 {
                sim.step(DT, &Input::press(0.5, 0.5), &mut rng);
            }
            assert!(sim.cleared(), "clearing must be permanent");
        }
    }

    #[test]
    fn renders_opaque_at_odd_and_tiny_sizes_with_bad_dt() {
        let mut rng = Rng::new(5);
        let mut sim = Breakout::new(320, 96, &mut rng);
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
