//! Flappy: a bird that only rises when you click, threading ten pipe
//! pairs to reach the finish flag. Left alone it drops into the ground.
use super::{
    draw_cleared_flash, draw_game_over, draw_progress, sanitize_dt, substeps, unit, vgradient,
    Confetti, Phase, GOAL,
};
use crate::paint::{Frame, Rgb, Theme};
use crate::rng::Rng;
use crate::sims::{Input, Sim};

const GRAVITY: f32 = 5.0;
const FLAP_VY: f32 = -1.3;
const MAX_FALL_VY: f32 = 2.2;
const SPEED: f32 = 0.9;
const PIPE_W: f32 = 0.22;
const GAP: f32 = 0.44;
const PIPE_SPACING: f32 = 1.35;
const FIRST_PIPE_X: f32 = 2.2;
const BIRD_R: f32 = 0.065;
/// Collision radius is a little under the drawn one — grazing a pipe
/// with a feather shouldn't count.
const HIT_R: f32 = BIRD_R * 0.8;
const GROUND_TOP: f32 = 0.9;
/// The bird hovers in place this long after a restart before gravity
/// kicks in (a click starts it sooner).
const READY_SEC: f32 = 0.9;
const START_Y: f32 = 0.45;

struct Pipe {
    x: f32,
    gap_y: f32,
    passed: bool,
}

pub struct Flappy {
    w: f32,
    h: f32,
    pipes: Vec<Pipe>,
    goal_x: f32,
    /// World x at the frame's left edge.
    cam: f32,
    bird_y: f32,
    bird_vy: f32,
    run_t: f32,
    started: bool,
    passed: u32,
    phase: Phase,
    confetti: Confetti,
    next_burst: f32,
}

impl Flappy {
    pub fn new(w: usize, h: usize, rng: &mut Rng) -> Self {
        let mut s = Self {
            w: w.max(1) as f32,
            h: h.max(1) as f32,
            pipes: Vec::new(),
            goal_x: 0.0,
            cam: 0.0,
            bird_y: START_Y,
            bird_vy: 0.0,
            run_t: 0.0,
            started: false,
            passed: 0,
            phase: Phase::Playing,
            confetti: Confetti::default(),
            next_burst: 0.0,
        };
        s.reset(rng);
        s
    }

    fn reset(&mut self, rng: &mut Rng) {
        self.pipes.clear();
        let mut x = FIRST_PIPE_X;
        let mut gap_y = 0.5;
        for _ in 0..GOAL {
            gap_y = (gap_y + rng.range_f32(-0.28, 0.28)).clamp(0.32, 0.6);
            self.pipes.push(Pipe { x, gap_y, passed: false });
            x += PIPE_SPACING;
        }
        self.goal_x = x - PIPE_SPACING + PIPE_W + 0.9;
        self.cam = 0.0;
        self.bird_y = START_Y;
        self.bird_vy = 0.0;
        self.run_t = 0.0;
        self.started = false;
        self.passed = 0;
        self.phase = Phase::Playing;
        self.confetti.clear();
    }

    /// The bird's fixed on-screen x, in units from the left edge.
    fn bird_sx(&self) -> f32 {
        (self.w / self.h * 0.28).min(0.65)
    }

    fn bird_x(&self) -> f32 {
        self.cam + self.bird_sx()
    }

    fn crashed(&self) -> bool {
        if self.bird_y + HIT_R > GROUND_TOP {
            return true;
        }
        let bx = self.bird_x();
        self.pipes.iter().any(|p| {
            bx + HIT_R > p.x
                && bx - HIT_R < p.x + PIPE_W
                && (self.bird_y - HIT_R < p.gap_y - GAP * 0.5 || self.bird_y + HIT_R > p.gap_y + GAP * 0.5)
        })
    }

    fn step_playing(&mut self, dt: f32, mut clicks: u32, rng: &mut Rng) {
        let (n, h) = substeps(dt);
        for _ in 0..n {
            if clicks > 0 {
                clicks = 0;
                self.started = true;
                self.bird_vy = FLAP_VY;
            }
            self.run_t += h;
            self.cam += SPEED * h;
            if !self.started && self.run_t >= READY_SEC {
                self.started = true;
            }
            if self.started {
                self.bird_vy = (self.bird_vy + GRAVITY * h).min(MAX_FALL_VY);
                self.bird_y += self.bird_vy * h;
            } else {
                self.bird_y = START_Y + (self.run_t * 7.0).sin() * 0.03;
            }
            if self.bird_y < BIRD_R {
                self.bird_y = BIRD_R;
                self.bird_vy = self.bird_vy.max(0.0);
            }
            if self.crashed() {
                self.phase = Phase::Over(0.0);
                return;
            }
            let bx = self.bird_x();
            for p in &mut self.pipes {
                if !p.passed && bx - HIT_R > p.x + PIPE_W {
                    p.passed = true;
                    self.passed += 1;
                }
            }
            if bx > self.goal_x {
                self.phase = Phase::Cleared(0.0);
                self.confetti.burst(bx, self.bird_y, 80, rng);
                self.next_burst = 0.5;
                return;
            }
        }
    }

    /// After winning: the bird cruises on through an empty sky, bobbing on
    /// its own. Clicks throw confetti.
    fn step_cleared(&mut self, dt: f32, clicks: u32, rng: &mut Rng) {
        let t = match self.phase {
            Phase::Cleared(t) => t,
            _ => 0.0,
        };
        self.cam += SPEED * dt;
        let target = START_Y + (t * 2.5).sin() * 0.08;
        self.bird_y += (target - self.bird_y) * (1.0 - (-dt * 3.0).exp());
        self.bird_vy = (target - self.bird_y) * 3.0;
        let bx = self.bird_sx();
        if clicks > 0 {
            self.confetti.burst(bx, self.bird_y, 40, rng);
        }
        if t < 3.0 && t >= self.next_burst {
            self.next_burst += 0.6;
            self.confetti.burst(rng.range_f32(0.2, self.w / self.h - 0.2), 0.7, 30, rng);
        }
    }

    fn draw_bird(&self, frame: &mut Frame, x: f32, y: f32) {
        let u = unit(frame);
        let (x, y, r) = (x * u, y * u, BIRD_R * u);
        let tilt = (self.bird_vy * 0.18).clamp(-0.25, 0.35) * r;
        frame.disc(x, y, r + (u * 0.012).max(1.0), Rgb::new(90, 60, 20), 1.0);
        frame.disc(x, y, r, Rgb::new(255, 208, 48), 1.0);
        // Wing flaps up hard right after a flap, droops while falling.
        let wing_dy = if self.bird_vy < -0.4 { -r * 0.35 } else { r * 0.25 };
        frame.disc(x - r * 0.35, y + wing_dy, r * 0.5, Rgb::new(255, 244, 200), 1.0);
        frame.disc(x + r * 0.4, y - r * 0.3 + tilt * 0.5, r * 0.36, Rgb::new(255, 255, 255), 1.0);
        frame.disc(x + r * 0.52, y - r * 0.3 + tilt * 0.5, r * 0.16, Rgb::new(20, 20, 20), 1.0);
        frame.rect(x + r * 0.6, y + r * 0.05 + tilt, r * 0.75, r * 0.4, Rgb::new(240, 100, 40), 1.0);
    }
}

impl Sim for Flappy {
    fn resize(&mut self, w: usize, h: usize, _: &mut Rng) {
        self.w = w.max(1) as f32;
        self.h = h.max(1) as f32;
    }

    fn step(&mut self, dt: f32, input: &Input, rng: &mut Rng) {
        let dt = sanitize_dt(dt);
        self.confetti.step(dt);
        match self.phase {
            Phase::Playing => self.step_playing(dt, input.clicks, rng),
            Phase::Over(_) => {
                if self.phase.advance(dt) {
                    self.reset(rng);
                } else {
                    // The stunned bird drops to the ground.
                    self.bird_vy = (self.bird_vy + GRAVITY * dt).min(MAX_FALL_VY);
                    self.bird_y = (self.bird_y + self.bird_vy * dt).min(GROUND_TOP - BIRD_R);
                }
            }
            Phase::Cleared(_) => {
                self.phase.advance(dt);
                self.step_cleared(dt, input.clicks, rng);
            }
        }
    }

    fn render(&mut self, frame: &mut Frame, _: &Theme) {
        let u = unit(frame);
        let fw = frame.w as f32;
        let view_w = fw / u;
        vgradient(frame, Rgb::new(78, 192, 214), Rgb::new(206, 240, 232));

        // Clouds, far parallax.
        for i in 0..8 {
            let span = view_w + 1.2;
            let x = (i as f32 * 0.93 - self.cam * 0.2).rem_euclid(span) - 0.5;
            let y = 0.18 + (i as f32 * 1.7).sin().abs() * 0.25;
            for (dx, dy, r) in [(0.0, 0.0, 0.1), (0.1, 0.02, 0.08), (-0.1, 0.03, 0.07)] {
                frame.disc((x + dx) * u, (y + dy) * u, r * u, Rgb::new(255, 255, 255), 0.85);
            }
        }
        // Rolling bushes, mid parallax.
        for px in 0..frame.w {
            let wx = px as f32 / u + self.cam * 0.5;
            let top = 0.74 + (wx * 2.3).sin() * 0.04 + (wx * 5.1).sin() * 0.025;
            super::column_from(frame, px, top * u, Rgb::new(110, 196, 110));
        }

        // Pipes.
        let dark = Rgb::new(40, 90, 30);
        for p in &self.pipes {
            let x = (p.x - self.cam) * u;
            let pw = PIPE_W * u;
            if x > fw || x + pw < 0.0 {
                continue;
            }
            let top_end = (p.gap_y - GAP * 0.5) * u;
            let bottom_start = (p.gap_y + GAP * 0.5) * u;
            let lip = 0.06 * u;
            let edge = (u * 0.012).max(1.0);
            for (y, h) in [(0.0, top_end), (bottom_start, GROUND_TOP * u - bottom_start)] {
                frame.rect(x, y, pw, h, dark, 1.0);
                frame.rect(x + edge, y, pw - edge * 2.0, h, Rgb::new(96, 192, 60), 1.0);
                frame.rect(x + pw * 0.2, y, pw * 0.15, h, Rgb::new(170, 236, 110), 1.0);
            }
            for y in [top_end - lip, bottom_start] {
                frame.rect(x - edge * 2.0, y, pw + edge * 4.0, lip, dark, 1.0);
                frame.rect(x - edge, y + edge, pw + edge * 2.0, lip - edge * 2.0, Rgb::new(110, 206, 70), 1.0);
            }
        }

        // Finish flag.
        let gx = (self.goal_x - self.cam) * u;
        if gx > -u && gx < fw + u {
            frame.rect(gx - u * 0.01, u * 0.18, (u * 0.02).max(1.0), (GROUND_TOP - 0.18) * u, Rgb::new(60, 60, 60), 1.0);
            let cell = u * 0.045;
            for row in 0..3 {
                for col in 0..5 {
                    let c = if (row + col) % 2 == 0 { Rgb::new(20, 20, 20) } else { Rgb::new(250, 250, 250) };
                    frame.rect(gx + col as f32 * cell, u * 0.18 + row as f32 * cell, cell, cell, c, 1.0);
                }
            }
        }

        // Ground with scrolling stripes.
        frame.rect(0.0, GROUND_TOP * u, fw, u, Rgb::new(222, 200, 130), 1.0);
        frame.rect(0.0, GROUND_TOP * u, fw, (u * 0.02).max(1.0), Rgb::new(90, 170, 60), 1.0);
        let stripe = 0.12;
        let offset = (self.cam % stripe) * u;
        let mut x = -offset;
        while x < fw {
            frame.rect(x, (GROUND_TOP + 0.03) * u, stripe * u * 0.5, u * 0.04, Rgb::new(200, 176, 100), 1.0);
            x += stripe * u;
        }

        let bird_x = self.bird_sx();
        self.draw_bird(frame, bird_x, self.bird_y);
        self.confetti.render(frame);
        draw_progress(frame, self.passed);
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

    fn click() -> Input {
        Input::tap()
    }

    /// Flaps whenever the bird sinks below the gap it's heading for.
    fn bot_input(sim: &Flappy) -> Input {
        let bx = sim.bird_x();
        let target = sim
            .pipes
            .iter()
            .find(|p| p.x + PIPE_W > bx - HIT_R)
            .map_or(0.5, |p| p.gap_y);
        if sim.bird_y > target + 0.05 && sim.bird_vy >= 0.0 {
            click()
        } else {
            Input::default()
        }
    }

    #[test]
    fn idle_bird_crashes_and_the_game_restarts_without_ever_clearing() {
        let mut rng = Rng::new(1);
        let mut sim = Flappy::new(320, 96, &mut rng);
        let mut crashes = 0;
        let mut was_over = false;
        for _ in 0..60 * 20 {
            sim.step(DT, &Input::default(), &mut rng);
            let over = matches!(sim.phase, Phase::Over(_));
            if over && !was_over {
                crashes += 1;
            }
            was_over = over;
            assert!(!sim.cleared());
        }
        assert!(crashes >= 3, "only {crashes} crashes");
    }

    #[test]
    fn a_player_that_flaps_well_clears_all_ten_pipes() {
        for seed in 1..6 {
            let mut rng = Rng::new(seed);
            let mut sim = Flappy::new(320, 96, &mut rng);
            for _ in 0..60 * 30 {
                let input = bot_input(&sim);
                sim.step(DT, &input, &mut rng);
                assert!(!matches!(sim.phase, Phase::Over(_)), "seed {seed} crashed at pipe {}", sim.passed);
                if sim.cleared() {
                    break;
                }
            }
            assert!(sim.cleared(), "seed {seed} never cleared");
            assert_eq!(sim.passed, GOAL);
            for _ in 0..600 {
                sim.step(DT, &Input::default(), &mut rng);
            }
            assert!(sim.cleared(), "clearing must be permanent");
        }
    }

    #[test]
    fn renders_opaque_at_odd_and_tiny_sizes_with_bad_dt() {
        let mut rng = Rng::new(2);
        let mut sim = Flappy::new(320, 96, &mut rng);
        for (w, h) in [(321, 97), (1, 1), (0, 0), (4, 7)] {
            sim.resize(w, h, &mut rng);
            for dt in [f32::NAN, -1.0, 1000.0, DT] {
                sim.step(dt, &click(), &mut rng);
            }
            let mut frame = Frame::new(w, h);
            sim.render(&mut frame, &Theme::default());
            assert!(frame.pixels.chunks_exact(4).all(|p| p[3] == 255));
            assert!(sim.bird_y.is_finite() && sim.cam.is_finite());
        }
    }
}
