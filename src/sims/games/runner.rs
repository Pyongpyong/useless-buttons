//! Runner: a side-scrolling platformer in the spirit of Wonder Boy. The
//! kid runs on their own; click to jump each of ten chasms and reach the
//! flag. Left alone they run straight into the first pit.
use super::{
    column_from, draw_cleared_flash, draw_game_over, draw_progress, sanitize_dt, substeps, unit,
    vgradient, Confetti, Phase, GOAL,
};
use crate::paint::{Frame, Rgb, Theme};
use crate::rng::Rng;
use crate::sims::{Input, Sim};

const SPEED: f32 = 1.35;
const GRAVITY: f32 = 7.5;
/// Apex ≈ 0.37 units, airtime ≈ 0.63 s ≈ 0.85 units of ground covered.
const JUMP_VY: f32 = -2.35;
/// Top of the ground, from the top of the frame.
const GROUND_Y: f32 = 0.8;
const FIRST_GAP_X: f32 = 2.6;
const GAP_MIN: f32 = 0.26;
const GAP_MAX: f32 = 0.44;
const LEDGE_MIN: f32 = 1.0;
const LEDGE_MAX: f32 = 1.7;
/// Some ledges between chasms are barely wider than a landing: jump too
/// early over the chasm before one and you sail past it into the next,
/// and once down there's only a moment before the next jump.
const SHORT_LEDGE_MIN: f32 = 0.2;
const SHORT_LEDGE_MAX: f32 = 0.4;
/// How many of the `GOAL - 1` ledges between chasms are short, per run.
/// Never two in a row: back-to-back short ledges leave a takeoff window
/// of a few hundredths of a second, which is luck rather than skill.
const SHORT_LEDGES_MIN: usize = 3;
const SHORT_LEDGES_MAX: usize = 4;
/// The kid stays up while either foot, this far either side of center,
/// still has ground under it.
const FOOT: f32 = 0.03;
/// A click this long before landing still jumps on touchdown.
const JUMP_BUFFER_SEC: f32 = 0.12;
/// A jump this long after running off an edge still counts.
const COYOTE_SEC: f32 = 0.08;
/// How deep into a pit the kid can sink before it's game over.
const FALL_LIMIT: f32 = 0.06;

struct Chasm {
    x0: f32,
    x1: f32,
    passed: bool,
}

pub struct Runner {
    w: f32,
    h: f32,
    chasms: Vec<Chasm>,
    goal_x: f32,
    cam: f32,
    /// Feet position.
    y: f32,
    vy: f32,
    grounded: bool,
    coyote: f32,
    buffer: f32,
    stride: f32,
    passed: u32,
    phase: Phase,
    confetti: Confetti,
    next_burst: f32,
}

impl Runner {
    pub fn new(w: usize, h: usize, rng: &mut Rng) -> Self {
        let mut s = Self {
            w: w.max(1) as f32,
            h: h.max(1) as f32,
            chasms: Vec::new(),
            goal_x: 0.0,
            cam: 0.0,
            y: GROUND_Y,
            vy: 0.0,
            grounded: true,
            coyote: 0.0,
            buffer: 0.0,
            stride: 0.0,
            passed: 0,
            phase: Phase::Playing,
            confetti: Confetti::default(),
            next_burst: 0.0,
        };
        s.reset(rng);
        s
    }

    fn reset(&mut self, rng: &mut Rng) {
        self.chasms.clear();
        let mut x = FIRST_GAP_X;
        let short = short_ledges(rng);
        for i in 0..GOAL as usize {
            let width = rng.range_f32(GAP_MIN, GAP_MAX);
            self.chasms.push(Chasm { x0: x, x1: x + width, passed: false });
            let ledge = if short.get(i) == Some(&true) {
                rng.range_f32(SHORT_LEDGE_MIN, SHORT_LEDGE_MAX)
            } else {
                rng.range_f32(LEDGE_MIN, LEDGE_MAX)
            };
            x += width + ledge;
        }
        self.goal_x = self.chasms.last().map_or(x, |c| c.x1) + 1.0;
        self.cam = 0.0;
        self.y = GROUND_Y;
        self.vy = 0.0;
        self.grounded = true;
        self.coyote = 0.0;
        self.buffer = 0.0;
        self.passed = 0;
        self.phase = Phase::Playing;
        self.confetti.clear();
    }

    fn kid_sx(&self) -> f32 {
        (self.w / self.h * 0.25).min(0.7)
    }

    fn kid_x(&self) -> f32 {
        self.cam + self.kid_sx()
    }

    fn solid(&self, wx: f32) -> bool {
        !self.chasms.iter().any(|c| wx > c.x0 && wx < c.x1)
    }

    fn supported(&self, wx: f32) -> bool {
        self.solid(wx - FOOT) || self.solid(wx + FOOT)
    }

    /// One physics substep. Returns `true` if the kid fell into a pit.
    fn physics(&mut self, h: f32) -> bool {
        self.cam += SPEED * h;
        self.buffer -= h;
        let wx = self.kid_x();
        if self.grounded {
            self.stride += h;
            if !self.supported(wx) {
                self.grounded = false;
                self.coyote = COYOTE_SEC;
            }
        } else {
            self.coyote -= h;
        }
        if self.buffer > 0.0 && (self.grounded || self.coyote > 0.0) {
            self.vy = JUMP_VY;
            self.grounded = false;
            self.coyote = 0.0;
            self.buffer = 0.0;
        }
        if !self.grounded {
            let prev = self.y;
            self.vy += GRAVITY * h;
            self.y += self.vy * h;
            // Only land by coming down onto the surface from above — once
            // below it, the pit walls don't catch you.
            if self.vy >= 0.0 && prev <= GROUND_Y + 1e-3 && self.y >= GROUND_Y && self.supported(wx) {
                self.y = GROUND_Y;
                self.vy = 0.0;
                self.grounded = true;
            }
            if self.y > GROUND_Y + FALL_LIMIT {
                return true;
            }
        }
        false
    }

    fn step_playing(&mut self, dt: f32, clicks: u32, rng: &mut Rng) {
        if clicks > 0 {
            self.buffer = JUMP_BUFFER_SEC;
        }
        let (n, h) = substeps(dt);
        for _ in 0..n {
            if self.physics(h) {
                self.phase = Phase::Over(0.0);
                return;
            }
            let wx = self.kid_x();
            for c in &mut self.chasms {
                if !c.passed && wx - FOOT > c.x1 {
                    c.passed = true;
                    self.passed += 1;
                }
            }
            if wx > self.goal_x {
                self.phase = Phase::Cleared(0.0);
                self.confetti.burst(self.kid_sx(), self.y - 0.2, 80, rng);
                self.next_burst = 0.5;
                return;
            }
        }
    }

    fn step_cleared(&mut self, dt: f32, clicks: u32, rng: &mut Rng) {
        // Every chasm is behind the kid now, so the same physics can't
        // fail; clicks become victory hops with confetti.
        if clicks > 0 {
            self.buffer = JUMP_BUFFER_SEC;
            self.confetti.burst(self.kid_sx(), self.y - 0.2, 30, rng);
        }
        let (n, h) = substeps(dt);
        for _ in 0..n {
            self.physics(h);
        }
        let t = match self.phase {
            Phase::Cleared(t) => t,
            _ => 0.0,
        };
        if t < 3.0 && t >= self.next_burst {
            self.next_burst += 0.6;
            self.confetti.burst(rng.range_f32(0.2, self.w / self.h - 0.2), 0.6, 30, rng);
        }
    }

    fn draw_kid(&self, frame: &mut Frame) {
        let u = unit(frame);
        let x = self.kid_sx() * u;
        let feet = self.y * u;
        let s = u * 0.01; // one "pixel" of the sprite
        let skin = Rgb::new(255, 204, 160);
        // Legs scissor while running, tuck while airborne.
        let swing = if self.grounded { (self.stride * 16.0).sin() * 3.0 * s } else { 0.0 };
        let leg_h = if self.grounded { 6.0 * s } else { 4.0 * s };
        frame.rect(x - 4.0 * s + swing, feet - leg_h, 3.0 * s, leg_h, skin, 1.0);
        frame.rect(x + 1.0 * s - swing, feet - leg_h, 3.0 * s, leg_h, skin, 1.0);
        let hip = feet - 6.0 * s;
        // Grass skirt, bare torso.
        frame.rect(x - 5.0 * s, hip - 3.0 * s, 10.0 * s, 4.0 * s, Rgb::new(70, 160, 60), 1.0);
        frame.rect(x - 4.0 * s, hip - 9.0 * s, 8.0 * s, 6.0 * s, skin, 1.0);
        let arm = if self.grounded { -swing } else { -3.0 * s };
        frame.rect(x - 6.0 * s + arm * 0.5, hip - 8.0 * s, 2.0 * s, 4.0 * s, skin, 1.0);
        frame.rect(x + 4.0 * s - arm * 0.5, hip - 8.0 * s, 2.0 * s, 4.0 * s, skin, 1.0);
        // Head, hair, eye.
        let head_y = hip - 13.0 * s;
        frame.disc(x + 0.5 * s, head_y, 4.5 * s, skin, 1.0);
        frame.rect(x - 4.5 * s, head_y - 5.0 * s, 9.0 * s, 3.0 * s, Rgb::new(250, 200, 40), 1.0);
        frame.rect(x - 5.0 * s, head_y - 3.0 * s, 3.0 * s, 4.0 * s, Rgb::new(250, 200, 40), 1.0);
        frame.rect(x + 2.0 * s, head_y - 1.0 * s, 1.5 * s, 2.0 * s, Rgb::new(20, 20, 20), 1.0);
    }
}

/// Which of the `GOAL - 1` ledges between chasms are short: a random
/// choice of `SHORT_LEDGES_MIN..=SHORT_LEDGES_MAX` of them, no two
/// adjacent. Picking `k` distinct slots out of `n - k + 1` and spreading
/// the j-th one by `j` maps uniformly onto exactly those layouts.
fn short_ledges(rng: &mut Rng) -> Vec<bool> {
    let n = GOAL as usize - 1;
    let k = rng.range_usize(SHORT_LEDGES_MIN, SHORT_LEDGES_MAX + 1).min(n.div_ceil(2));
    let slots = n - k + 1;
    let mut short = vec![false; n];
    let mut need = k;
    let mut j = 0;
    for slot in 0..slots {
        // Selection sampling: exactly `need` of the remaining slots.
        if need > 0 && rng.next_f32() * ((slots - slot) as f32) < need as f32 {
            short[slot + j] = true;
            j += 1;
            need -= 1;
        }
    }
    short
}

impl Sim for Runner {
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
                    // Keep dropping down the pit while the world stands still.
                    self.vy += GRAVITY * dt;
                    self.y = (self.y + self.vy * dt).min(2.0);
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
        vgradient(frame, Rgb::new(92, 148, 252), Rgb::new(184, 214, 255));

        for i in 0..6 {
            let span = view_w + 1.4;
            let x = (i as f32 * 1.21 - self.cam * 0.15).rem_euclid(span) - 0.6;
            let y = 0.14 + (i as f32 * 2.3).sin().abs() * 0.2;
            for (dx, r) in [(-0.09, 0.06), (0.0, 0.085), (0.09, 0.06)] {
                frame.disc((x + dx) * u, y * u, r * u, Rgb::new(255, 255, 255), 0.9);
            }
        }
        for px in 0..frame.w {
            let wx = px as f32 / u;
            let far = wx + self.cam * 0.3;
            let top = 0.5 + ((far * 1.3).sin() * 0.5 + 0.5).powf(2.0) * -0.18 + 0.12;
            column_from(frame, px, top * u, Rgb::new(96, 170, 150));
            let near = wx + self.cam * 0.6;
            let top = 0.66 + (near * 3.1).sin() * 0.035 + (near * 7.3).sin() * 0.02;
            column_from(frame, px, top * u, Rgb::new(60, 150, 70));
        }

        // Ground, per column: grass lip over brick-patterned dirt, or a pit.
        let grass = Rgb::new(120, 210, 70);
        let dirt = Rgb::new(186, 110, 50);
        let mortar = Rgb::new(130, 70, 30);
        let ground_px = GROUND_Y * u;
        let brick = 0.08;
        for px in 0..frame.w {
            let wx = self.cam + (px as f32 + 0.5) / u;
            let start = ground_px.max(0.0).ceil() as usize;
            if self.solid(wx) {
                for py in start..frame.h {
                    let depth = (py as f32 + 0.5) / u - GROUND_Y;
                    let c = if depth < 0.035 {
                        grass
                    } else {
                        let row = (depth / brick).floor();
                        let bx = (wx / (brick * 2.0) + row * 0.5).fract();
                        let by = (depth / brick).fract();
                        if bx < 0.06 || by < 0.12 { mortar } else { dirt }
                    };
                    frame.blend_pixel(px as i64, py as i64, c, 1.0);
                }
            } else {
                let denom = (frame.h as f32 - ground_px).max(1.0);
                for py in start..frame.h {
                    let t = (py as f32 - ground_px) / denom;
                    frame.blend_pixel(px as i64, py as i64, Rgb::new(10, 10, 30), 0.5 + t * 0.5);
                }
            }
        }

        // Goal flag.
        let gx = (self.goal_x - self.cam) * u;
        if gx > -u && gx < fw + u {
            let top = 0.25 * u;
            frame.rect(gx - u * 0.01, top, (u * 0.02).max(1.0), ground_px - top, Rgb::new(240, 240, 240), 1.0);
            frame.disc(gx, top, u * 0.025, Rgb::new(255, 214, 64), 1.0);
            let fh = 0.14 * u;
            let rows = fh.ceil().max(1.0) as usize;
            for r in 0..rows {
                let t = r as f32 / rows as f32;
                let len = (1.0 - (t - 0.5).abs() * 2.0) * 0.2 * u;
                frame.rect(gx, top + r as f32, len, 1.0, Rgb::new(230, 50, 50), 1.0);
            }
        }

        self.draw_kid(frame);
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

    /// Ground covered during one full jump.
    const AIR: f32 = SPEED * 2.0 * -JUMP_VY / GRAVITY;

    /// Takes off in the middle of the window that both clears the next
    /// chasm and still lands before the one after it.
    fn bot_input(sim: &Runner) -> Input {
        let wx = sim.kid_x();
        let Some(i) = sim.chasms.iter().position(|c| c.x1 > wx) else {
            return Input::default();
        };
        let c = &sim.chasms[i];
        let earliest = c.x1 + FOOT - AIR;
        let mut latest = c.x0 + FOOT;
        if let Some(next) = sim.chasms.get(i + 1) {
            latest = latest.min(next.x0 - AIR);
        }
        let target = (earliest + latest) * 0.5;
        let jump = wx >= target && wx < latest;
        Input { clicks: jump as u32, ..Input::default() }
    }

    #[test]
    fn every_run_mixes_in_a_few_short_ledges_never_back_to_back() {
        let mut rng = Rng::new(4);
        let mut sim = Runner::new(320, 96, &mut rng);
        for _ in 0..200 {
            sim.reset(&mut rng);
            let short: Vec<bool> = sim.chasms.windows(2).map(|w| w[1].x0 - w[0].x1 < LEDGE_MIN).collect();
            let n = short.iter().filter(|&&s| s).count();
            assert!((SHORT_LEDGES_MIN..=SHORT_LEDGES_MAX).contains(&n), "{n} short ledges");
            assert!(!short.windows(2).any(|w| w[0] && w[1]), "adjacent short ledges: {short:?}");
        }
    }

    #[test]
    fn jumping_at_the_first_sight_of_the_edge_overshoots_a_short_ledge() {
        // The old habit — jump a little before every edge — no longer
        // works: over a short ledge it lands straight in the next chasm.
        let mut fell = 0;
        for seed in 1..11 {
            let mut rng = Rng::new(seed);
            let mut sim = Runner::new(320, 96, &mut rng);
            for _ in 0..60 * 40 {
                let wx = sim.kid_x();
                let early = sim.chasms.iter().find(|c| c.x1 > wx).is_some_and(|c| (0.1..0.2).contains(&(c.x0 - wx)));
                sim.step(DT, &Input { clicks: early as u32, ..Input::default() }, &mut rng);
                if matches!(sim.phase, Phase::Over(_)) {
                    fell += 1;
                    break;
                }
            }
        }
        assert!(fell >= 8, "early jumper survived {} of 10 runs", 10 - fell);
    }

    #[test]
    fn idle_runner_falls_in_and_the_game_restarts_without_ever_clearing() {
        let mut rng = Rng::new(1);
        let mut sim = Runner::new(320, 96, &mut rng);
        let mut falls = 0;
        let mut was_over = false;
        for _ in 0..60 * 20 {
            sim.step(DT, &Input::default(), &mut rng);
            let over = matches!(sim.phase, Phase::Over(_));
            if over && !was_over {
                falls += 1;
            }
            was_over = over;
            assert!(!sim.cleared());
        }
        assert!(falls >= 3, "only {falls} falls");
    }

    #[test]
    fn a_player_that_jumps_in_time_clears_all_ten_chasms() {
        for seed in 1..21 {
            let mut rng = Rng::new(seed);
            let mut sim = Runner::new(320, 96, &mut rng);
            for _ in 0..60 * 40 {
                let input = bot_input(&sim);
                sim.step(DT, &input, &mut rng);
                assert!(!matches!(sim.phase, Phase::Over(_)), "seed {seed} fell at chasm {}", sim.passed);
                if sim.cleared() {
                    break;
                }
            }
            assert!(sim.cleared(), "seed {seed} never cleared");
            assert_eq!(sim.passed, GOAL);
            for _ in 0..600 {
                sim.step(DT, &Input { clicks: 1, ..Input::default() }, &mut rng);
            }
            assert!(sim.cleared(), "clearing must be permanent");
        }
    }

    #[test]
    fn jumping_too_early_lands_in_the_pit() {
        let mut rng = Rng::new(3);
        let mut sim = Runner::new(320, 96, &mut rng);
        // Jump a full airtime before the first chasm: the kid comes down
        // on the near ledge and runs on into the gap.
        while sim.chasms[0].x0 - sim.kid_x() > 1.2 {
            sim.step(DT, &Input::default(), &mut rng);
        }
        sim.step(DT, &Input { clicks: 1, ..Input::default() }, &mut rng);
        for _ in 0..120 {
            sim.step(DT, &Input::default(), &mut rng);
        }
        assert!(matches!(sim.phase, Phase::Over(_)));
    }

    #[test]
    fn renders_opaque_at_odd_and_tiny_sizes_with_bad_dt() {
        let mut rng = Rng::new(2);
        let mut sim = Runner::new(320, 96, &mut rng);
        for (w, h) in [(321, 97), (1, 1), (0, 0), (4, 7)] {
            sim.resize(w, h, &mut rng);
            for dt in [f32::NAN, -1.0, 1000.0, DT] {
                sim.step(dt, &Input { clicks: 1, ..Input::default() }, &mut rng);
            }
            let mut frame = Frame::new(w, h);
            sim.render(&mut frame, &Theme::default());
            assert!(frame.pixels.chunks_exact(4).all(|p| p[3] == 255));
            assert!(sim.y.is_finite() && sim.cam.is_finite());
        }
    }
}
