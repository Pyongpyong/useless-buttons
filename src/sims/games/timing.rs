//! Timing: a ring sits on a line and a ball sweeps back and forth along
//! it. Click while the ball is inside the ring to score — the ring then
//! jumps somewhere else. Ten hits clear it. Clicking while the ball is
//! outside is a miss, and letting the ball sweep through the ring
//! `PASSES` times without clicking also ends the run.
use super::{
    draw_cleared_flash, draw_game_over, draw_progress, ring, sanitize_dt, substeps, unit,
    vgradient, Confetti, Phase, GOAL,
};
use crate::paint::{Frame, Rgb, Theme};
use crate::rng::Rng;
use crate::sims::{Input, Sim};

const LINE_Y: f32 = 0.56;
const TARGET_R: f32 = 0.17;
const BALL_R: f32 = 0.1;
/// The ball counts as "in" the ring while its center is within this
/// distance of the ring's center.
const HIT_TOLERANCE: f32 = TARGET_R;
/// Horizontal margin, in units, kept clear at both ends of the track.
const MARGIN: f32 = 0.25;
const BASE_SPEED: f32 = 1.3;
/// Extra speed per hit, so the last few are the hardest.
const SPEED_PER_HIT: f32 = 0.09;
/// Sweeps through the ring allowed before the run times out.
const PASSES: u32 = 3;
/// A new ring must land at least this far (in units) from the ball.
const RESPAWN_MIN_DIST: f32 = 0.55;

struct Pulse {
    x: f32,
    age: f32,
}

pub struct Timing {
    w: f32,
    h: f32,
    /// Ball and ring positions as fractions of the track, so a resize
    /// keeps both on it.
    ball_f: f32,
    dir: f32,
    target_f: f32,
    hits: u32,
    passes_left: u32,
    inside: bool,
    miss_at: Option<f32>,
    pulses: Vec<Pulse>,
    phase: Phase,
    confetti: Confetti,
    next_burst: f32,
}

impl Timing {
    pub fn new(w: usize, h: usize, rng: &mut Rng) -> Self {
        let mut s = Self {
            w: w.max(1) as f32,
            h: h.max(1) as f32,
            ball_f: 0.0,
            dir: 1.0,
            target_f: 0.5,
            hits: 0,
            passes_left: PASSES,
            inside: false,
            miss_at: None,
            pulses: Vec::new(),
            phase: Phase::Playing,
            confetti: Confetti::default(),
            next_burst: 0.0,
        };
        s.reset(rng);
        s
    }

    fn reset(&mut self, rng: &mut Rng) {
        self.ball_f = 0.0;
        self.dir = 1.0;
        self.hits = 0;
        self.miss_at = None;
        self.pulses.clear();
        self.phase = Phase::Playing;
        self.confetti.clear();
        self.place_target(rng);
    }

    fn span(&self) -> f32 {
        (self.w / self.h - MARGIN * 2.0).max(0.1)
    }

    fn to_x(&self, f: f32) -> f32 {
        MARGIN + f * self.span()
    }

    fn ball_x(&self) -> f32 {
        self.to_x(self.ball_f)
    }

    fn target_x(&self) -> f32 {
        self.to_x(self.target_f)
    }

    fn ball_inside(&self) -> bool {
        (self.ball_x() - self.target_x()).abs() <= HIT_TOLERANCE
    }

    fn place_target(&mut self, rng: &mut Rng) {
        let ball = self.ball_x();
        let mut best = (0.5, -1.0f32);
        for _ in 0..16 {
            let f = rng.range_f32(0.08, 0.92);
            let dist = (self.to_x(f) - ball).abs();
            if dist >= RESPAWN_MIN_DIST {
                best = (f, dist);
                break;
            }
            if dist > best.1 {
                best = (f, dist);
            }
        }
        self.target_f = best.0;
        self.passes_left = PASSES;
        self.inside = self.ball_inside();
    }

    fn step_playing(&mut self, dt: f32, clicks: u32, rng: &mut Rng) {
        if clicks > 0 {
            if self.ball_inside() {
                self.pulses.push(Pulse { x: self.target_x(), age: 0.0 });
                self.hits += 1;
                if self.hits >= GOAL {
                    self.phase = Phase::Cleared(0.0);
                    self.confetti.burst(self.ball_x(), LINE_Y, 90, rng);
                    self.next_burst = 0.5;
                    return;
                }
                self.place_target(rng);
            } else {
                self.miss_at = Some(self.ball_f);
                self.phase = Phase::Over(0.0);
                return;
            }
        }
        let speed = BASE_SPEED + SPEED_PER_HIT * self.hits as f32;
        let (n, h) = substeps(dt);
        for _ in 0..n {
            self.sweep(speed * h);
            let now = self.ball_inside();
            if self.inside && !now {
                self.passes_left = self.passes_left.saturating_sub(1);
                if self.passes_left == 0 {
                    self.phase = Phase::Over(0.0);
                    return;
                }
            }
            self.inside = now;
        }
    }

    /// Move the ball `dist` units along the track, bouncing at the ends.
    fn sweep(&mut self, dist: f32) {
        self.ball_f += self.dir * dist / self.span();
        if self.ball_f > 1.0 {
            self.ball_f = (2.0 - self.ball_f).max(0.0);
            self.dir = -1.0;
        } else if self.ball_f < 0.0 {
            self.ball_f = (-self.ball_f).min(1.0);
            self.dir = 1.0;
        }
    }

    fn step_cleared(&mut self, dt: f32, clicks: u32, rng: &mut Rng) {
        let t = match self.phase {
            Phase::Cleared(t) => t,
            _ => 0.0,
        };
        let (n, h) = substeps(dt);
        for _ in 0..n {
            self.sweep(BASE_SPEED * 0.6 * h);
        }
        if clicks > 0 {
            self.confetti.burst(self.ball_x(), LINE_Y, 40, rng);
        }
        if t < 3.0 && t >= self.next_burst {
            self.next_burst += 0.6;
            self.confetti.burst(self.to_x(rng.next_f32()), LINE_Y + 0.2, 30, rng);
        }
    }
}

impl Sim for Timing {
    fn resize(&mut self, w: usize, h: usize, _: &mut Rng) {
        self.w = w.max(1) as f32;
        self.h = h.max(1) as f32;
    }

    fn step(&mut self, dt: f32, input: &Input, rng: &mut Rng) {
        let dt = sanitize_dt(dt);
        self.confetti.step(dt);
        for p in &mut self.pulses {
            p.age += dt;
        }
        self.pulses.retain(|p| p.age < 0.6);
        match self.phase {
            Phase::Playing => self.step_playing(dt, input.clicks, rng),
            Phase::Over(_) => {
                if self.phase.advance(dt) {
                    self.reset(rng);
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
        vgradient(frame, Rgb::new(16, 18, 46), Rgb::new(44, 18, 64));

        // Faint drifting grid, just so the background isn't dead flat.
        let cell = 0.2 * u;
        let drift = (self.ball_f * 0.3 * u) % cell;
        let mut x = -drift;
        while x < fw {
            frame.rect(x, 0.0, 1.0, frame.h as f32, Rgb::new(120, 110, 200), 0.12);
            x += cell;
        }

        let y = LINE_Y * u;
        let track0 = self.to_x(0.0) * u;
        let track1 = self.to_x(1.0) * u;
        frame.line((track0, y), (track1, y), (u * 0.02).max(1.0), Rgb::new(190, 190, 230), 0.7);
        for end in [track0, track1] {
            frame.disc(end, y, (u * 0.025).max(1.0), Rgb::new(190, 190, 230), 0.9);
        }

        let tx = self.target_x() * u;
        let ball_in = self.ball_inside();
        if !matches!(self.phase, Phase::Cleared(_)) {
            let cyan = Rgb::new(80, 230, 255);
            frame.disc(tx, y, TARGET_R * u, cyan, if ball_in { 0.4 } else { 0.12 });
            ring(frame, tx, y, TARGET_R * u, (u * 0.03).max(1.0), cyan, 1.0);
            // Remaining sweeps before a timeout, under the ring.
            let dot = (u * 0.018).max(1.0);
            for i in 0..PASSES {
                let dx = (i as f32 - (PASSES - 1) as f32 * 0.5) * dot * 3.0;
                let lit = i < self.passes_left;
                frame.disc(tx + dx, y + (TARGET_R + 0.07) * u, dot, cyan, if lit { 0.9 } else { 0.2 });
            }
        }
        for p in &self.pulses {
            let t = p.age / 0.6;
            ring(frame, p.x * u, y, (TARGET_R + t * 0.35) * u, (u * 0.025).max(1.0), Rgb::new(255, 255, 255), 1.0 - t);
        }

        let bx = self.ball_x() * u;
        let hot = Rgb::new(255, 120, 80);
        frame.disc(bx, y, BALL_R * u * 1.8, hot, 0.18);
        frame.disc(bx, y, BALL_R * u, hot, 1.0);
        frame.disc(bx - BALL_R * u * 0.3, y - BALL_R * u * 0.3, BALL_R * u * 0.35, Rgb::new(255, 220, 200), 0.9);

        if let Some(f) = self.miss_at {
            let mx = self.to_x(f) * u;
            let r = BALL_R * u * 1.6;
            let red = Rgb::new(255, 60, 60);
            let t = (u * 0.03).max(1.0);
            frame.line((mx - r, y - r), (mx + r, y + r), t, red, 1.0);
            frame.line((mx - r, y + r), (mx + r, y - r), t, red, 1.0);
        }

        self.confetti.render(frame);
        draw_progress(frame, self.hits);
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
        Input { clicks: 1, ..Input::default() }
    }

    #[test]
    fn idle_play_times_out_and_restarts_without_ever_clearing() {
        let mut rng = Rng::new(1);
        let mut sim = Timing::new(320, 96, &mut rng);
        let mut timeouts = 0;
        let mut was_over = false;
        for _ in 0..60 * 40 {
            sim.step(DT, &Input::default(), &mut rng);
            let over = matches!(sim.phase, Phase::Over(_));
            if over && !was_over {
                timeouts += 1;
            }
            was_over = over;
            assert!(!sim.cleared());
        }
        assert!(timeouts >= 2, "only {timeouts} timeouts");
    }

    #[test]
    fn clicking_while_outside_the_ring_is_a_miss() {
        let mut rng = Rng::new(2);
        let mut sim = Timing::new(320, 96, &mut rng);
        while sim.ball_inside() {
            sim.step(DT, &Input::default(), &mut rng);
        }
        sim.step(DT, &click(), &mut rng);
        assert!(matches!(sim.phase, Phase::Over(_)));
        assert_eq!(sim.hits, 0);
    }

    #[test]
    fn ten_well_timed_clicks_clear_it_and_move_the_ring_each_time() {
        for seed in 1..6 {
            let mut rng = Rng::new(seed);
            let mut sim = Timing::new(320, 96, &mut rng);
            for _ in 0..60 * 60 {
                let centered = (sim.ball_x() - sim.target_x()).abs() < HIT_TOLERANCE * 0.5;
                let before = sim.target_f;
                let hits = sim.hits;
                let input = if centered { click() } else { Input::default() };
                sim.step(DT, &input, &mut rng);
                assert!(!matches!(sim.phase, Phase::Over(_)), "seed {seed} failed at hit {}", sim.hits);
                if sim.hits > hits && !sim.cleared() {
                    assert_ne!(sim.target_f, before);
                }
                if sim.cleared() {
                    break;
                }
            }
            assert!(sim.cleared(), "seed {seed} never cleared");
            for _ in 0..600 {
                sim.step(DT, &click(), &mut rng);
            }
            assert!(sim.cleared(), "clearing must be permanent");
        }
    }

    #[test]
    fn renders_opaque_at_odd_and_tiny_sizes_with_bad_dt() {
        let mut rng = Rng::new(3);
        let mut sim = Timing::new(320, 96, &mut rng);
        for (w, h) in [(321, 97), (1, 1), (0, 0), (4, 7)] {
            sim.resize(w, h, &mut rng);
            for dt in [f32::NAN, -1.0, 1000.0, DT] {
                sim.step(dt, &click(), &mut rng);
            }
            let mut frame = Frame::new(w, h);
            sim.render(&mut frame, &Theme::default());
            assert!(frame.pixels.chunks_exact(4).all(|p| p[3] == 255));
            assert!((0.0..=1.0).contains(&sim.ball_f));
        }
    }
}
