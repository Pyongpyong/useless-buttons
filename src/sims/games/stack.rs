//! Stack: a block slides back and forth above the tower; press to drop
//! it. It falls under gravity and keeps all of its overhang — nothing is
//! trimmed — so the tower is a stack of rigid blocks. After every landing
//! each joint is checked: if the blocks above a joint have their center of
//! mass past the edge of what's holding them up, that part of the tower
//! tips over the edge and falls. Stack ten to clear. Wait too long and the
//! block slips off the far end of its track on its own.
use super::{
    draw_cleared_flash, draw_game_over, draw_progress, draw_time_bar, fill_quad, sanitize_dt,
    unit, vgradient, Confetti, Phase, GOAL,
};
use crate::paint::{Frame, Rgb, Theme};
use crate::rng::Rng;
use crate::sims::{Input, Sim};

const BLOCK_W: f32 = 0.25;
const BLOCK_H: f32 = 0.045;
const BASE_W: f32 = 0.4;
const BASE_H: f32 = 0.06;
/// Screen height of the ground (the top of the base) with the camera at rest.
const GROUND_Y: f32 = 0.85;
/// How far above the tower the sliding block rides.
const SLIDE_GAP: f32 = 0.2;
const SLIDE_SPEED: f32 = 1.0;
const SLIDE_PER_BLOCK: f32 = 0.08;
const GRAVITY: f32 = 4.0;
/// Seconds to drop each block before it slips off on its own.
const AIM_SEC: f32 = 4.0;
/// Angular acceleration of a toppling section, rad/s².
const TOPPLE_ACCEL: f32 = 6.0;
/// Keep the top of the tower about this high above the bottom of the frame.
const CAMERA_LEAD: f32 = 0.4;

#[derive(Clone, Copy, PartialEq)]
enum Stage {
    /// The block is sliding; seconds spent aiming.
    Aim(f32),
    Falling { x: f32, h: f32, vy: f32 },
}

/// Part of the tower tipping over an edge: blocks from `from` up rotate
/// about `pivot` (world x, height), `dir` -1 to the left, +1 to the right.
#[derive(Clone, Copy)]
struct Topple {
    from: usize,
    pivot: (f32, f32),
    dir: f32,
    angle: f32,
    omega: f32,
}

pub struct Stack {
    w: f32,
    h: f32,
    /// Block centers, bottom to top; block k spans heights k*BLOCK_H..(k+1)*BLOCK_H.
    blocks: Vec<f32>,
    slider: f32,
    dir: f32,
    stage: Stage,
    topple: Option<Topple>,
    /// A block that missed everything and is falling past: x, height, vy.
    lost: Option<(f32, f32, f32)>,
    cam: f32,
    hue: f32,
    phase: Phase,
    confetti: Confetti,
    next_burst: f32,
}

impl Stack {
    pub fn new(w: usize, h: usize, rng: &mut Rng) -> Self {
        let mut s = Self {
            w: w.max(1) as f32,
            h: h.max(1) as f32,
            blocks: Vec::new(),
            slider: 0.0,
            dir: 1.0,
            stage: Stage::Aim(0.0),
            topple: None,
            lost: None,
            cam: 0.0,
            hue: 0.0,
            phase: Phase::Playing,
            confetti: Confetti::default(),
            next_burst: 0.0,
        };
        s.reset(rng);
        s
    }

    fn reset(&mut self, rng: &mut Rng) {
        self.blocks.clear();
        self.slider = BLOCK_W * 0.5;
        self.dir = 1.0;
        self.stage = Stage::Aim(0.0);
        self.topple = None;
        self.lost = None;
        self.cam = 0.0;
        self.hue = rng.range_f32(0.0, 360.0);
        self.phase = Phase::Playing;
        self.confetti.clear();
    }

    fn aspect(&self) -> f32 {
        self.w / self.h
    }

    fn base_x(&self) -> f32 {
        self.aspect() * 0.5
    }

    fn top_h(&self) -> f32 {
        self.blocks.len() as f32 * BLOCK_H
    }

    /// Center and width of what block `k` rests on.
    fn support(&self, k: usize) -> (f32, f32) {
        if k == 0 {
            (self.base_x(), BASE_W)
        } else {
            (self.blocks[k - 1], BLOCK_W)
        }
    }

    fn slide_range(&self) -> (f32, f32) {
        let half = BLOCK_W * 0.5;
        (half, (self.aspect() - half).max(half))
    }

    /// The lowest joint whose load has its center of mass past the edge
    /// of the contact beneath it, if any.
    fn unstable_joint(&self) -> Option<Topple> {
        for j in 0..self.blocks.len() {
            let (sx, sw) = self.support(j);
            let bx = self.blocks[j];
            let lo = (sx - sw * 0.5).max(bx - BLOCK_W * 0.5);
            let hi = (sx + sw * 0.5).min(bx + BLOCK_W * 0.5);
            let above = &self.blocks[j..];
            let com = above.iter().sum::<f32>() / above.len() as f32;
            let pivot_h = j as f32 * BLOCK_H;
            if com < lo {
                return Some(Topple { from: j, pivot: (lo, pivot_h), dir: -1.0, angle: 0.0, omega: 0.0 });
            }
            if com > hi {
                return Some(Topple { from: j, pivot: (hi, pivot_h), dir: 1.0, angle: 0.0, omega: 0.0 });
            }
        }
        None
    }

    fn land(&mut self, x: f32, rng: &mut Rng) {
        let (sx, sw) = self.support(self.blocks.len());
        let overlap = (sx + sw * 0.5).min(x + BLOCK_W * 0.5) - (sx - sw * 0.5).max(x - BLOCK_W * 0.5);
        if overlap <= 0.0 {
            // Missed the tower entirely: keep falling past it.
            self.lost = Some((x, self.top_h(), 0.0));
            self.phase = Phase::Over(0.0);
            return;
        }
        self.blocks.push(x);
        if let Some(t) = self.unstable_joint() {
            self.topple = Some(t);
            self.phase = Phase::Over(0.0);
            return;
        }
        if self.blocks.len() as u32 >= GOAL {
            self.phase = Phase::Cleared(0.0);
            self.confetti.burst(x, GROUND_Y - (self.top_h() - self.cam), 90, rng);
            self.next_burst = 0.5;
            return;
        }
        self.stage = Stage::Aim(0.0);
    }

    fn step_playing(&mut self, dt: f32, input: &Input, rng: &mut Rng) {
        match self.stage {
            Stage::Aim(t) => {
                let speed = SLIDE_SPEED + SLIDE_PER_BLOCK * self.blocks.len() as f32;
                let (lo, hi) = self.slide_range();
                self.slider += self.dir * speed * dt;
                if self.slider > hi {
                    self.slider = hi;
                    self.dir = -1.0;
                } else if self.slider < lo {
                    self.slider = lo;
                    self.dir = 1.0;
                }
                let start = self.top_h() + SLIDE_GAP;
                if input.clicks > 0 {
                    self.stage = Stage::Falling { x: self.slider, h: start, vy: 0.0 };
                } else if t + dt >= AIM_SEC {
                    // Slips off the end of the track farthest from the tower.
                    let top = self.blocks.last().copied().unwrap_or(self.base_x());
                    let x = if top - lo > hi - top { lo } else { hi };
                    self.slider = x;
                    self.stage = Stage::Falling { x, h: start, vy: 0.0 };
                } else {
                    self.stage = Stage::Aim(t + dt);
                }
            }
            Stage::Falling { x, h, vy } => {
                let vy = vy + GRAVITY * dt;
                let h = h - vy * dt;
                if h <= self.top_h() {
                    self.land(x, rng);
                } else {
                    self.stage = Stage::Falling { x, h, vy };
                }
            }
        }
    }

    /// World (x, height) to screen pixels.
    fn to_screen(&self, u: f32, x: f32, h: f32) -> (f32, f32) {
        (x * u, (GROUND_Y - (h - self.cam)) * u)
    }

    fn draw_block(&self, frame: &mut Frame, k: usize, x: f32, h: f32, tilt: Option<(Topple, bool)>) {
        let u = unit(frame);
        let corners = [(x - BLOCK_W * 0.5, h), (x + BLOCK_W * 0.5, h), (x + BLOCK_W * 0.5, h + BLOCK_H), (x - BLOCK_W * 0.5, h + BLOCK_H)];
        let place = |(cx, ch): (f32, f32)| -> (f32, f32) {
            match tilt {
                Some((t, true)) => {
                    // Rotate about the pivot; +dir tips clockwise on screen (to the right).
                    let a = -t.dir * t.angle;
                    let (dx, dh) = (cx - t.pivot.0, ch - t.pivot.1);
                    let (rx, rh) = (dx * a.cos() - dh * a.sin(), dx * a.sin() + dh * a.cos());
                    self.to_screen(u, t.pivot.0 + rx, t.pivot.1 + rh)
                }
                _ => self.to_screen(u, cx, ch),
            }
        };
        let pts = corners.map(place);
        let c = Rgb::from_hsv(self.hue + k as f32 * 23.0, 0.6, 0.92);
        fill_quad(frame, pts, c.lerp(Rgb::new(30, 30, 40), 0.45));
        // Inset face for a bevelled look.
        let inset = 0.012;
        let inner = [(x - BLOCK_W * 0.5 + inset, h + inset), (x + BLOCK_W * 0.5 - inset, h + inset), (x + BLOCK_W * 0.5 - inset, h + BLOCK_H - inset), (x - BLOCK_W * 0.5 + inset, h + BLOCK_H - inset)];
        fill_quad(frame, inner.map(place), c);
        let top = [(x - BLOCK_W * 0.5 + inset, h + BLOCK_H * 0.7), (x + BLOCK_W * 0.5 - inset, h + BLOCK_H * 0.7), (x + BLOCK_W * 0.5 - inset, h + BLOCK_H - inset), (x - BLOCK_W * 0.5 + inset, h + BLOCK_H - inset)];
        fill_quad(frame, top.map(place), c.lerp(Rgb::new(255, 255, 255), 0.35));
    }
}

impl Sim for Stack {
    fn resize(&mut self, w: usize, h: usize, _: &mut Rng) {
        let shift = (w.max(1) as f32 / h.max(1) as f32 - self.aspect()) * 0.5;
        self.w = w.max(1) as f32;
        self.h = h.max(1) as f32;
        // Keep the tower centered on its base when the width changes.
        for b in &mut self.blocks {
            *b += shift;
        }
        let (lo, hi) = self.slide_range();
        self.slider = (self.slider + shift).clamp(lo, hi);
    }

    fn step(&mut self, dt: f32, input: &Input, rng: &mut Rng) {
        let dt = sanitize_dt(dt);
        self.confetti.step(dt);
        let target = (self.top_h() - CAMERA_LEAD).max(0.0);
        self.cam += (target - self.cam) * (1.0 - (-dt * 4.0).exp());
        if let Some(t) = &mut self.topple {
            t.omega += TOPPLE_ACCEL * dt;
            t.angle = (t.angle + t.omega * dt).min(2.0);
        }
        if let Some((_, h, vy)) = &mut self.lost {
            *vy += GRAVITY * dt;
            *h -= *vy * dt;
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
                    self.confetti.burst(rng.range_f32(0.2, self.aspect() - 0.2), 0.5, 30, rng);
                }
            }
        }
    }

    fn render(&mut self, frame: &mut Frame, _: &Theme) {
        let u = unit(frame);
        // The sky darkens as the camera climbs.
        let k = (self.cam / 0.6).min(1.0);
        vgradient(
            frame,
            Rgb::new(250, 190, 150).lerp(Rgb::new(70, 80, 160), k),
            Rgb::new(255, 225, 190).lerp(Rgb::new(150, 150, 210), k),
        );
        for i in 0..6 {
            let x = (i as f32 * 0.71 + 0.2) % (self.aspect() + 0.3);
            let y = 0.25 + (i as f32 * 1.7).sin().abs() * 0.35 + self.cam * 0.3;
            if y < 1.2 {
                for (dx, r) in [(-0.07, 0.04), (0.0, 0.06), (0.07, 0.04)] {
                    frame.disc((x + dx) * u, y * u, r * u, Rgb::new(255, 255, 255), 0.6);
                }
            }
        }
        let (bx, by) = self.to_screen(u, self.base_x() - BASE_W * 0.5, 0.0);
        frame.rect(bx, by, BASE_W * u, BASE_H * u, Rgb::new(90, 90, 100), 1.0);
        frame.rect(bx, by, BASE_W * u, (BASE_H * 0.3 * u).max(1.0), Rgb::new(140, 140, 150), 1.0);
        let ground_y = by + BASE_H * u;
        if ground_y < frame.h as f32 {
            frame.rect(0.0, ground_y, frame.w as f32, frame.h as f32 - ground_y, Rgb::new(90, 150, 80), 1.0);
        }

        for (k, &x) in self.blocks.iter().enumerate() {
            let tilt = self.topple.map(|t| (t, k >= t.from));
            self.draw_block(frame, k, x, k as f32 * BLOCK_H, tilt);
        }
        if let Some((x, h, _)) = self.lost {
            self.draw_block(frame, self.blocks.len(), x, h, None);
        }
        if self.phase == Phase::Playing {
            let n = self.blocks.len();
            match self.stage {
                Stage::Aim(t) => {
                    let h = self.top_h() + SLIDE_GAP;
                    // A faint drop guide under the sliding block.
                    let (gx, gy) = self.to_screen(u, self.slider, h);
                    let (_, ty) = self.to_screen(u, self.slider, self.top_h());
                    frame.rect(gx - 0.5, gy, 1.0, (ty - gy).max(0.0), Rgb::new(255, 255, 255), 0.35);
                    self.draw_block(frame, n, self.slider, h, None);
                    draw_time_bar(frame, 1.0 - t / AIM_SEC);
                }
                Stage::Falling { x, h, .. } => self.draw_block(frame, n, x, h, None),
            }
        }
        self.confetti.render(frame);
        // Blocks still standing: everything below a toppling section.
        let standing = self.topple.map_or(self.blocks.len(), |t| t.from);
        draw_progress(frame, standing.min(GOAL as usize) as u32);
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

    fn drop_at(sim: &mut Stack, x: f32, rng: &mut Rng) {
        while !matches!(sim.stage, Stage::Aim(_)) || sim.phase != Phase::Playing {
            sim.step(DT, &Input::default(), rng);
        }
        sim.slider = x;
        sim.dir = 0.0;
        sim.step(DT, &Input::tap(), rng);
        while matches!(sim.stage, Stage::Falling { .. }) && sim.phase == Phase::Playing {
            sim.step(DT, &Input::default(), rng);
        }
    }

    #[test]
    fn overhang_is_kept_not_trimmed() {
        let mut rng = Rng::new(1);
        let mut sim = Stack::new(320, 96, &mut rng);
        let x = sim.base_x() + 0.15;
        drop_at(&mut sim, x, &mut rng);
        assert_eq!(sim.blocks, vec![x]);
        assert_eq!(sim.phase, Phase::Playing);
    }

    #[test]
    fn a_tower_leaning_too_far_topples_over_the_edge() {
        let mut rng = Rng::new(2);
        let mut sim = Stack::new(320, 96, &mut rng);
        // Each block 60% of a width further right: each one alone is
        // supported, but the stack's weight soon hangs past an edge.
        let mut x = sim.base_x();
        for _ in 0..6 {
            x += BLOCK_W * 0.4;
            drop_at(&mut sim, x, &mut rng);
            if sim.phase != Phase::Playing {
                break;
            }
        }
        let t = sim.topple.expect("the leaning tower never fell");
        assert_eq!(t.dir, 1.0);
        assert!(matches!(sim.phase, Phase::Over(_)));
    }

    #[test]
    fn missing_the_tower_entirely_is_a_game_over() {
        let mut rng = Rng::new(3);
        let mut sim = Stack::new(320, 96, &mut rng);
        drop_at(&mut sim, BLOCK_W * 0.5, &mut rng);
        assert!(matches!(sim.phase, Phase::Over(_)));
        assert!(sim.lost.is_some());
    }

    #[test]
    fn idle_blocks_slip_off_and_the_game_restarts_without_ever_clearing() {
        let mut rng = Rng::new(4);
        let mut sim = Stack::new(320, 96, &mut rng);
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
        assert!(overs >= 4, "only {overs} game overs");
    }

    #[test]
    fn dropping_right_over_the_top_block_stacks_ten() {
        for seed in 1..11 {
            let mut rng = Rng::new(seed);
            let mut sim = Stack::new(320, 96, &mut rng);
            for _ in 0..60 * 60 {
                let top = sim.blocks.last().copied().unwrap_or(sim.base_x());
                let press = matches!(sim.stage, Stage::Aim(_)) && (sim.slider - top).abs() < 0.03;
                sim.step(DT, &if press { Input::tap() } else { Input::default() }, &mut rng);
                assert!(!matches!(sim.phase, Phase::Over(_)), "seed {seed} fell at {} blocks", sim.blocks.len());
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
        let mut sim = Stack::new(320, 96, &mut rng);
        for (w, h) in [(321, 97), (1, 1), (0, 0), (4, 7)] {
            sim.resize(w, h, &mut rng);
            for dt in [f32::NAN, -1.0, 1000.0, DT] {
                sim.step(dt, &Input::tap(), &mut rng);
            }
            let mut frame = Frame::new(w, h);
            sim.render(&mut frame, &Theme::default());
            assert!(frame.pixels.chunks_exact(4).all(|p| p[3] == 255));
        }
    }
}
