//! Frog: a Frogger-style river crossing, in the same left-to-right layout
//! as `crossy`. Every river column has lily pads drifting up or down it;
//! hop onto one (right half of the button hops right, left half left) and
//! ride it. Land in the water, or ride a pad off the edge of the screen,
//! and it's game over. There's a grassy bank to rest on every 3–4 rivers,
//! and once you leave the start or reach a bank you can't go back past
//! it; on dry land the frog wanders back to the middle. Sit still for too long and the heron gets you. Cross ten rivers.
use super::{
    draw_cleared_flash, draw_game_over, draw_progress, sanitize_dt, substeps, unit, vgradient,
    Confetti, Phase,
};
#[cfg(test)]
use super::GOAL;
use crate::paint::{Frame, Rgb, Theme};
use crate::rng::Rng;
use crate::sims::{Input, Sim};

const RIVER_GROUPS: [[usize; 3]; 3] = [[3, 4, 3], [3, 3, 4], [4, 3, 3]];
const START_Y: f32 = 0.56;
/// The frog is in the water, not on the pad, when its center is within
/// this of the pad's end.
const FOOTING: f32 = 0.02;
/// Riding off the screen past these lines is a game over.
const RIVER_TOP: f32 = 0.1;
const RIVER_BOTTOM: f32 = 0.98;
const PAD_MIN: f32 = 0.3;
const PAD_MAX: f32 = 0.42;
const LOOP_MIN: f32 = 1.0 + 2.0 * PAD_MAX + 0.2;
const LOOP_MAX: f32 = LOOP_MIN + 0.5;
const SPEED_MIN: f32 = 0.13;
const SPEED_PER_RIVER: f32 = 0.02;
const SPEED_JITTER: f32 = 0.08;
/// Long enough to wait out the slowest gap in the pads; standing still
/// longer than this brings the heron.
const IDLE_LIMIT: f32 = 12.0;
const HERON_WARNING: f32 = 3.0;
const HOP_SEC: f32 = 0.12;
/// On dry land the frog wanders back to the middle at this speed, so a
/// ride that ended near the top or bottom edge can't strand it there.
const LAND_WALK: f32 = 0.5;

#[derive(Clone, Copy, PartialEq)]
struct River {
    /// +1 down, -1 up.
    dir: f32,
    speed: f32,
    loop_len: f32,
    count: usize,
    pads: [(f32, f32); 3],
}

impl River {
    /// Top edge of pad `i` at time `t`.
    fn pad_top(&self, i: usize, t: f32) -> f32 {
        (self.pads[i].0 + self.dir * self.speed * t).rem_euclid(self.loop_len) - PAD_MAX
    }

    /// The pad under height `y` at time `t`, if any.
    fn pad_at(&self, y: f32, t: f32) -> Option<usize> {
        (0..self.count).find(|&i| {
            let top = self.pad_top(i, t);
            y > top + FOOTING && y < top + self.pads[i].1 - FOOTING
        })
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Column {
    Start,
    River(River),
    Bank,
    Finish,
}

#[derive(Clone, Copy, PartialEq)]
enum Death {
    Splash,
    Heron,
}

pub struct Frog {
    w: f32,
    h: f32,
    columns: Vec<Column>,
    col: usize,
    floor: usize,
    y: f32,
    hop_from: usize,
    hop_t: f32,
    idle: f32,
    t: f32,
    death: Death,
    phase: Phase,
    confetti: Confetti,
    next_burst: f32,
}

impl Frog {
    pub fn new(w: usize, h: usize, rng: &mut Rng) -> Self {
        let mut s = Self {
            w: w.max(1) as f32,
            h: h.max(1) as f32,
            columns: Vec::new(),
            col: 0,
            floor: 0,
            y: START_Y,
            hop_from: 0,
            hop_t: HOP_SEC,
            idle: 0.0,
            t: 0.0,
            death: Death::Splash,
            phase: Phase::Playing,
            confetti: Confetti::default(),
            next_burst: 0.0,
        };
        s.reset(rng);
        s
    }

    fn reset(&mut self, rng: &mut Rng) {
        self.columns.clear();
        self.columns.push(Column::Start);
        let groups = RIVER_GROUPS[rng.range_usize(0, RIVER_GROUPS.len())];
        let mut river = 0;
        for (g, &n) in groups.iter().enumerate() {
            if g > 0 {
                self.columns.push(Column::Bank);
            }
            for _ in 0..n {
                self.columns.push(Column::River(Self::river(river, rng)));
                river += 1;
            }
        }
        self.columns.push(Column::Finish);
        self.col = 0;
        self.floor = 0;
        self.y = START_Y;
        self.hop_from = 0;
        self.hop_t = HOP_SEC;
        self.idle = 0.0;
        self.t = 0.0;
        self.phase = Phase::Playing;
        self.confetti.clear();
    }

    fn river(index: usize, rng: &mut Rng) -> River {
        let loop_len = rng.range_f32(LOOP_MIN, LOOP_MAX);
        let count = 3;
        let first = rng.range_f32(0.0, loop_len);
        let mut pads = [(0.0, 0.0); 3];
        for (i, pad) in pads.iter_mut().enumerate().take(count) {
            let at = first + loop_len * i as f32 / count as f32 + rng.range_f32(-0.08, 0.08);
            *pad = (at.rem_euclid(loop_len), rng.range_f32(PAD_MIN, PAD_MAX));
        }
        River {
            dir: if rng.bool_p(0.5) { 1.0 } else { -1.0 },
            speed: SPEED_MIN + SPEED_PER_RIVER * index as f32 + rng.range_f32(0.0, SPEED_JITTER),
            loop_len,
            count,
            pads,
        }
    }

    fn aspect(&self) -> f32 {
        self.w / self.h
    }

    fn col_width(&self) -> f32 {
        self.aspect() / self.columns.len().max(1) as f32
    }

    fn col_center(&self, col: usize) -> f32 {
        (col as f32 + 0.5) * self.col_width()
    }

    fn passed(&self) -> u32 {
        self.columns[..self.col].iter().filter(|c| matches!(c, Column::River(_))).count() as u32
    }

    /// Where a frog at `y` in column `col` is carried to after `dt`, or
    /// `None` if it's in the water or has ridden off the screen.
    fn ride(&self, col: usize, y: f32, t: f32, dt: f32) -> Option<f32> {
        match self.columns.get(col)? {
            Column::River(r) => {
                r.pad_at(y, t)?;
                let y = y + r.dir * r.speed * dt;
                (RIVER_TOP..=RIVER_BOTTOM).contains(&y).then_some(y)
            }
            _ => {
                let step = LAND_WALK * dt;
                Some(y + (START_Y - y).clamp(-step, step))
            }
        }
    }

    fn hop(&mut self, to: usize) {
        self.hop_from = self.col;
        self.hop_t = 0.0;
        self.col = to;
        self.idle = 0.0;
        if self.hop_from == 0 {
            self.floor = 1;
        }
        if self.columns[to] == Column::Bank {
            self.floor = to;
        }
    }

    fn step_playing(&mut self, dt: f32, input: &Input, rng: &mut Rng) {
        let (mut left, mut right) = (0, 0);
        for p in input.presses().iter().filter(|p| p.x.is_finite()) {
            if p.x < 0.5 {
                left += 1;
            } else {
                right += 1;
            }
        }
        if right > left && self.col + 1 < self.columns.len() {
            self.hop(self.col + 1);
        } else if left > right && self.col > self.floor {
            self.hop(self.col - 1);
        }
        let (n, h) = substeps(dt);
        for _ in 0..n {
            match self.ride(self.col, self.y, self.t, h) {
                Some(y) => self.y = y,
                None => {
                    self.death = Death::Splash;
                    self.phase = Phase::Over(0.0);
                    return;
                }
            }
            self.t += h;
            self.hop_t += h;
            self.idle += h;
            if self.idle >= IDLE_LIMIT {
                self.death = Death::Heron;
                self.phase = Phase::Over(0.0);
                return;
            }
        }
        if self.columns[self.col] == Column::Finish {
            self.phase = Phase::Cleared(0.0);
            self.confetti.burst(self.col_center(self.col), self.y, 80, rng);
            self.next_burst = 0.5;
        }
    }

    fn draw_frog(&self, frame: &mut Frame, x: f32, y: f32, lift: f32) {
        let u = unit(frame);
        let s = (self.col_width() * 0.32).min(0.08) * u;
        let (x, y) = (x * u, (y - lift) * u);
        let green = Rgb::new(70, 190, 70);
        let dark = Rgb::new(40, 120, 40);
        for (dx, dy) in [(-0.8, 0.6), (0.8, 0.6), (-0.7, -0.5), (0.7, -0.5)] {
            frame.disc(x + dx * s, y + dy * s, s * 0.35, dark, 1.0);
        }
        frame.disc(x, y, s * 0.75, green, 1.0);
        for dx in [-0.35, 0.35] {
            frame.disc(x + dx * s + s * 0.25, y - s * 0.55, s * 0.28, Rgb::new(250, 250, 250), 1.0);
            frame.disc(x + dx * s + s * 0.3, y - s * 0.55, s * 0.14, Rgb::new(20, 20, 20), 1.0);
        }
    }
}

impl Sim for Frog {
    fn resize(&mut self, w: usize, h: usize, _: &mut Rng) {
        self.w = w.max(1) as f32;
        self.h = h.max(1) as f32;
    }

    fn step(&mut self, dt: f32, input: &Input, rng: &mut Rng) {
        let dt = sanitize_dt(dt);
        self.confetti.step(dt);
        match self.phase {
            Phase::Playing => self.step_playing(dt, input, rng),
            Phase::Over(_) => {
                if self.phase.advance(dt) {
                    self.reset(rng);
                } else {
                    self.t += dt;
                }
            }
            Phase::Cleared(t) => {
                self.phase.advance(dt);
                self.t += dt;
                self.hop_t += dt;
                if input.clicks > 0 {
                    self.confetti.burst(self.col_center(self.col), self.y, 40, rng);
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
        let fh = frame.h as f32;
        let cw = self.col_width();
        vgradient(frame, Rgb::new(40, 110, 190), Rgb::new(30, 90, 170));
        for (i, c) in self.columns.iter().enumerate() {
            let x0 = i as f32 * cw * u;
            let w = cw * u;
            match c {
                Column::Start | Column::Bank => {
                    frame.rect(x0, 0.0, w, fh, Rgb::new(110, 180, 80), 1.0);
                    let tile = (cw * 0.5).max(0.05) * u;
                    let mut y = 0.0;
                    let mut row = 0;
                    while y < fh {
                        let off = if row % 2 == 0 { 0.0 } else { tile };
                        frame.rect(x0 + off, y, tile.min(w - off), tile, Rgb::new(96, 164, 70), 1.0);
                        y += tile;
                        row += 1;
                    }
                }
                Column::Finish => {
                    let tile = (cw * 0.5).max(0.05) * u;
                    let mut y = 0.0;
                    let mut row = 0;
                    while y < fh {
                        for k in 0..2 {
                            let c = if (row + k) % 2 == 0 { Rgb::new(245, 245, 245) } else { Rgb::new(25, 25, 25) };
                            frame.rect(x0 + k as f32 * tile, y, tile, tile, c, 1.0);
                        }
                        y += tile;
                        row += 1;
                    }
                }
                Column::River(r) => {
                    // Ripples streaming in the direction of the current.
                    let step = 0.14;
                    let off = (r.dir * r.speed * self.t).rem_euclid(step);
                    let mut y = off - step;
                    let mut k = 0;
                    while y < 1.0 {
                        let dx = if k % 2 == 0 { 0.25 } else { 0.55 };
                        frame.rect(x0 + w * dx, y * u, w * 0.2, (0.012 * u).max(1.0), Rgb::new(140, 200, 250), 0.5);
                        y += step;
                        k += 1;
                    }
                    for p in 0..r.count {
                        let top = r.pad_top(p, self.t) * u;
                        let len = r.pads[p].1 * u;
                        let pw = w * 0.78;
                        let cx = x0 + w * 0.5;
                        let rad = pw * 0.5;
                        frame.rect(cx - rad, top + rad, pw, (len - rad * 2.0).max(0.0), Rgb::new(40, 140, 60), 1.0);
                        frame.disc(cx, top + rad, rad, Rgb::new(40, 140, 60), 1.0);
                        frame.disc(cx, top + len - rad, rad, Rgb::new(40, 140, 60), 1.0);
                        frame.rect(cx - rad * 0.8, top + rad, pw * 0.8, (len - rad * 2.0).max(0.0), Rgb::new(70, 175, 80), 1.0);
                        frame.disc(cx, top + len * 0.5, rad * 0.25, Rgb::new(250, 170, 200), 1.0);
                    }
                }
            }
        }

        let hop = (self.hop_t / HOP_SEC).clamp(0.0, 1.0);
        let x = self.col_center(self.hop_from) + (self.col_center(self.col) - self.col_center(self.hop_from)) * hop;
        let lift = (hop * std::f32::consts::PI).sin() * 0.06;
        let over = matches!(self.phase, Phase::Over(_));
        if over && self.death == Death::Splash {
            let (sx, sy) = (x * u, self.y * u);
            for k in 0..3 {
                super::ring(frame, sx, sy, (0.03 + k as f32 * 0.03) * u, (u * 0.012).max(1.0), Rgb::new(220, 240, 255), 0.8);
            }
        } else if !(over && self.death == Death::Heron) {
            self.draw_frog(frame, x, self.y, lift);
        }
        if self.phase == Phase::Playing && self.idle > IDLE_LIMIT - HERON_WARNING {
            let k = (self.idle - (IDLE_LIMIT - HERON_WARNING)) / HERON_WARNING;
            frame.disc(x * u, self.y * u, (0.05 + k * 0.2) * u, Rgb::new(0, 0, 0), 0.15 + k * 0.35);
        }
        self.confetti.render(frame);
        draw_progress(frame, self.passed());
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
    use std::collections::HashMap;

    const DT: f32 = 1.0 / 60.0;
    const STEP: f32 = 0.1;
    const HORIZON: usize = 60;

    fn right() -> Input {
        Input::press(0.9, 0.5)
    }
    fn left() -> Input {
        Input::press(0.1, 0.5)
    }

    /// Frog position after holding still in `col` from `t0` to `t1`, if it
    /// survives the whole stretch. A frog on a pad moves with it, so it
    /// stays on as long as it started on one; only the screen edges can
    /// end the ride, and the motion is straight, so checking the end is
    /// enough.
    fn survive(sim: &Frog, col: usize, y: f32, t0: f32, t1: f32) -> Option<f32> {
        sim.ride(col, y, t0, t1 - t0)
    }

    /// Breadth-first search over (column, height) in 0.1 s steps with
    /// hop right / wait / hop left, keeping only moves that stay dry and
    /// on screen. Returns the first move of the plan that ends furthest
    /// right after the whole horizon.
    fn bot_input(sim: &Frog, since: f32) -> Option<Input> {
        if since < STEP - 0.02 {
            return None;
        }
        let key = |c: usize, y: f32| (c, (y * 200.0) as i32);
        let mut frontier: HashMap<(usize, i32), (usize, f32, i32)> = HashMap::new();
        frontier.insert(key(sim.col, sim.y), (sim.col, sim.y, 0));
        for k in 0..HORIZON {
            let (t0, t1) = (sim.t + k as f32 * STEP, sim.t + (k + 1) as f32 * STEP);
            let mut next: HashMap<(usize, i32), (usize, f32, i32)> = HashMap::new();
            for &(c, y, root) in frontier.values() {
                let floor = if c > 0 { sim.floor.max(1) } else { 0 };
                for d in [1i32, 0, -1] {
                    let n = c as i32 + d;
                    if n < floor as i32 || n >= sim.columns.len() as i32 {
                        continue;
                    }
                    let Some(ny) = survive(sim, n as usize, y, t0, t1) else { continue };
                    let root = if k == 0 { d } else { root };
                    let entry = next.entry(key(n as usize, ny)).or_insert((n as usize, ny, root));
                    if root > entry.2 {
                        entry.2 = root;
                    }
                }
            }
            frontier = next;
        }
        let best = frontier.values().max_by(|a, b| a.0.cmp(&b.0).then(a.2.cmp(&b.2)))?;
        match best.2 {
            1 => Some(right()),
            -1 => Some(left()),
            _ => None,
        }
    }

    #[test]
    fn ten_rivers_with_a_bank_every_three_or_four() {
        let mut rng = Rng::new(1);
        let mut sim = Frog::new(320, 96, &mut rng);
        for _ in 0..50 {
            sim.reset(&mut rng);
            let rivers = sim.columns.iter().filter(|c| matches!(c, Column::River(_))).count();
            assert_eq!(rivers, GOAL as usize);
            let mut run = 0;
            for c in &sim.columns[1..sim.columns.len() - 1] {
                if matches!(c, Column::River(_)) {
                    run += 1;
                } else {
                    assert!((3..=4).contains(&run));
                    run = 0;
                }
            }
        }
    }

    #[test]
    fn hopping_into_open_water_is_a_splash() {
        let mut rng = Rng::new(2);
        let mut sim = Frog::new(320, 96, &mut rng);
        // Wait for a moment when there's no pad at the frog's height next door.
        while matches!(sim.columns[1], Column::River(r) if r.pad_at(sim.y, sim.t).is_some()) {
            sim.step(DT, &Input::default(), &mut rng);
        }
        sim.step(DT, &right(), &mut rng);
        assert!(matches!(sim.phase, Phase::Over(_)));
        assert!(sim.death == Death::Splash);
    }

    #[test]
    fn a_pad_carries_the_frog_along() {
        let mut rng = Rng::new(3);
        let mut sim = Frog::new(320, 96, &mut rng);
        while !matches!(sim.columns[1], Column::River(r) if r.pad_at(sim.y, sim.t).is_some_and(|i| {
            let top = r.pad_top(i, sim.t);
            sim.y > top + 0.1 && sim.y < top + r.pads[i].1 - 0.1
        })) {
            sim.step(DT, &Input::default(), &mut rng);
        }
        sim.step(DT, &right(), &mut rng);
        let y0 = sim.y;
        for _ in 0..10 {
            sim.step(DT, &Input::default(), &mut rng);
        }
        assert_eq!(sim.phase, Phase::Playing);
        assert!((sim.y - y0).abs() > 1e-3, "rode nowhere");
    }

    #[test]
    fn on_land_the_frog_walks_back_to_the_middle() {
        let mut rng = Rng::new(6);
        let mut sim = Frog::new(320, 96, &mut rng);
        sim.y = 0.15;
        for _ in 0..90 {
            sim.step(DT, &Input::default(), &mut rng);
        }
        assert!((sim.y - START_Y).abs() < 1e-3);
    }

    #[test]
    fn idle_frog_is_taken_by_the_heron_and_the_game_restarts() {
        let mut rng = Rng::new(4);
        let mut sim = Frog::new(320, 96, &mut rng);
        let mut overs = 0;
        let mut was_over = false;
        for _ in 0..60 * 60 {
            sim.step(DT, &Input::default(), &mut rng);
            let over = matches!(sim.phase, Phase::Over(_));
            if over && !was_over {
                overs += 1;
                assert!(sim.death == Death::Heron);
            }
            was_over = over;
            assert!(!sim.cleared());
        }
        assert!(overs >= 3, "only {overs} game overs");
    }

    #[test]
    fn a_careful_frog_crosses_ten_rivers() {
        for seed in 1..11 {
            let mut rng = Rng::new(seed);
            let mut sim = Frog::new(320, 96, &mut rng);
            let mut since = 1.0;
            for frame in 0..60 * 90 {
                // Re-plan every tenth of a second, like a person deciding.
                let input = if frame % 6 == 0 { bot_input(&sim, since) } else { None };
                let before = sim.col;
                sim.step(DT, &input.unwrap_or_default(), &mut rng);
                since = if sim.col != before { 0.0 } else { since + DT };
                assert!(!matches!(sim.phase, Phase::Over(_)), "seed {seed} died at column {} ({})", sim.col, if sim.death == Death::Heron { "heron" } else { "splash" });
                if sim.cleared() {
                    break;
                }
            }
            assert!(sim.cleared(), "seed {seed} never finished");
            assert_eq!(sim.passed(), GOAL);
        }
    }

    #[test]
    fn renders_opaque_at_odd_and_tiny_sizes_with_bad_dt() {
        let mut rng = Rng::new(5);
        let mut sim = Frog::new(320, 96, &mut rng);
        for (w, h) in [(321, 97), (1, 1), (0, 0), (4, 7)] {
            sim.resize(w, h, &mut rng);
            for dt in [f32::NAN, -1.0, 1000.0, DT] {
                sim.step(dt, &right(), &mut rng);
            }
            let mut frame = Frame::new(w, h);
            sim.render(&mut frame, &Theme::default());
            assert!(frame.pixels.chunks_exact(4).all(|p| p[3] == 255));
        }
    }
}
