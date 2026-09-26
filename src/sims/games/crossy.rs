//! Crossy: a Crossy Road-style dash. A chicken starts on the left and has
//! to reach the finish on the right, one column per press — right half of
//! the button hops right, left half hops left. Every road column has cars
//! running up or down it; every 3–4 roads there's a grass rest column.
//! Once the chicken leaves the start (or reaches a rest column) it can't
//! go back left of it. Standing still too long brings the eagle.
use super::{
    draw_cleared_flash, draw_game_over, draw_progress, sanitize_dt, substeps, unit, vgradient,
    Confetti, Phase, GOAL,
};
use crate::paint::{Frame, Rgb, Theme};
use crate::rng::Rng;
use crate::sims::{Input, Sim};

/// Roads between rest columns: always `GOAL` roads in total.
const ROAD_GROUPS: [[usize; 3]; 3] = [[3, 4, 3], [3, 3, 4], [4, 3, 3]];
const PLAYER_Y: f32 = 0.56;
/// Half the chicken's height for collisions, a bit under the drawn size.
const PLAYER_HALF: f32 = 0.06;
const CAR_LEN_MIN: f32 = 0.26;
const CAR_LEN_MAX: f32 = 0.34;
/// A car's track wraps over this much more than the screen, so it's
/// always fully off-screen at the moment it wraps.
const LOOP_MIN: f32 = 1.0 + 2.0 * CAR_LEN_MAX + 0.1;
const LOOP_MAX: f32 = LOOP_MIN + 0.5;
/// Chance a road carries a second car, rising from the first road to
/// the last so traffic thickens as you go.
const SECOND_CAR_P_FIRST: f32 = 0.5;
const SECOND_CAR_P_LAST: f32 = 1.0;
const SPEED_MIN: f32 = 0.4;
const SPEED_PER_ROAD: f32 = 0.04;
const SPEED_JITTER: f32 = 0.15;
/// Standing still this long (any column, rest spots included) and the
/// eagle takes you.
const IDLE_LIMIT: f32 = 6.0;
/// The eagle's shadow starts growing over the chicken this long before.
const EAGLE_WARNING: f32 = 2.0;
const HOP_SEC: f32 = 0.12;

#[derive(Clone, Copy, PartialEq)]
enum Column {
    Start,
    Road(Lane),
    Rest,
    Finish,
}

#[derive(Clone, Copy, PartialEq)]
struct Lane {
    /// +1 down, -1 up.
    dir: f32,
    speed: f32,
    loop_len: f32,
    /// How many of `cars` are on the road (1 or 2).
    count: usize,
    cars: [(f32, f32); 2],
    hue: [f32; 2],
}

impl Lane {
    /// Top edge of car `i` at time `t`.
    fn car_top(&self, i: usize, t: f32) -> f32 {
        (self.cars[i].0 + self.dir * self.speed * t).rem_euclid(self.loop_len) - CAR_LEN_MAX
    }

    fn blocked(&self, t: f32) -> bool {
        (0..self.count).any(|i| {
            let top = self.car_top(i, t);
            top < PLAYER_Y + PLAYER_HALF && top + self.cars[i].1 > PLAYER_Y - PLAYER_HALF
        })
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Death {
    Car,
    Eagle,
}

pub struct Crossy {
    w: f32,
    h: f32,
    columns: Vec<Column>,
    col: usize,
    /// Leftmost column the chicken may hop back to.
    floor: usize,
    /// Column hopped from and seconds since, for the hop animation.
    hop_from: usize,
    hop_t: f32,
    idle: f32,
    /// Lane clock: car positions are a pure function of it.
    t: f32,
    death: Death,
    phase: Phase,
    confetti: Confetti,
    next_burst: f32,
}

impl Crossy {
    pub fn new(w: usize, h: usize, rng: &mut Rng) -> Self {
        let mut s = Self {
            w: w.max(1) as f32,
            h: h.max(1) as f32,
            columns: Vec::new(),
            col: 0,
            floor: 0,
            hop_from: 0,
            hop_t: HOP_SEC,
            idle: 0.0,
            t: 0.0,
            death: Death::Car,
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
        let groups = ROAD_GROUPS[rng.range_usize(0, ROAD_GROUPS.len())];
        let mut road = 0;
        for (g, &n) in groups.iter().enumerate() {
            if g > 0 {
                self.columns.push(Column::Rest);
            }
            for _ in 0..n {
                self.columns.push(Column::Road(Self::lane(road, rng)));
                road += 1;
            }
        }
        self.columns.push(Column::Finish);
        self.col = 0;
        self.floor = 0;
        self.hop_from = 0;
        self.hop_t = HOP_SEC;
        self.idle = 0.0;
        self.t = 0.0;
        self.phase = Phase::Playing;
        self.confetti.clear();
    }

    fn lane(road: usize, rng: &mut Rng) -> Lane {
        let loop_len = rng.range_f32(LOOP_MIN, LOOP_MAX);
        let first = rng.range_f32(0.0, loop_len);
        let second = first + loop_len * 0.5 + rng.range_f32(-0.15, 0.15);
        let t = road as f32 / (GOAL - 1) as f32;
        let second_p = SECOND_CAR_P_FIRST + (SECOND_CAR_P_LAST - SECOND_CAR_P_FIRST) * t;
        Lane {
            dir: if rng.bool_p(0.5) { 1.0 } else { -1.0 },
            speed: SPEED_MIN + SPEED_PER_ROAD * road as f32 + rng.range_f32(0.0, SPEED_JITTER),
            loop_len,
            count: if rng.bool_p(second_p) { 2 } else { 1 },
            cars: [
                (first, rng.range_f32(CAR_LEN_MIN, CAR_LEN_MAX)),
                (second.rem_euclid(loop_len), rng.range_f32(CAR_LEN_MIN, CAR_LEN_MAX)),
            ],
            hue: [rng.range_f32(0.0, 360.0), rng.range_f32(0.0, 360.0)],
        }
    }

    /// Road columns entirely behind the furthest column reached.
    fn passed(&self) -> u32 {
        self.columns[..self.col]
            .iter()
            .filter(|c| matches!(c, Column::Road(_)))
            .count() as u32
    }

    fn blocked(&self, col: usize, t: f32) -> bool {
        match self.columns.get(col) {
            Some(Column::Road(lane)) => lane.blocked(t),
            _ => false,
        }
    }

    fn hop(&mut self, to: usize) {
        self.hop_from = self.col;
        self.hop_t = 0.0;
        self.col = to;
        self.idle = 0.0;
        // Leaving the start, or reaching a rest spot, closes the way back.
        if self.hop_from == 0 {
            self.floor = 1;
        }
        if self.columns[to] == Column::Rest {
            self.floor = to;
        }
    }

    fn step_playing(&mut self, dt: f32, input: &Input, rng: &mut Rng) {
        let left = input.left_clicks.min(input.clicks);
        let right = input.clicks - left;
        // Opposite presses in the same tick cancel out, so a double tap
        // on both halves can't skip a column.
        if right > left && self.col + 1 < self.columns.len() {
            self.hop(self.col + 1);
        } else if left > right && self.col > self.floor {
            self.hop(self.col - 1);
        }
        let (n, h) = substeps(dt);
        for _ in 0..n {
            self.t += h;
            self.hop_t += h;
            self.idle += h;
            if self.blocked(self.col, self.t) {
                self.death = Death::Car;
                self.phase = Phase::Over(0.0);
                return;
            }
            if self.idle >= IDLE_LIMIT {
                self.death = Death::Eagle;
                self.phase = Phase::Over(0.0);
                return;
            }
        }
        if self.columns[self.col] == Column::Finish {
            self.phase = Phase::Cleared(0.0);
            self.confetti.burst(self.col_center(self.col), PLAYER_Y, 80, rng);
            self.next_burst = 0.5;
        }
    }

    fn col_width(&self) -> f32 {
        self.w / self.h / self.columns.len().max(1) as f32
    }

    fn col_center(&self, col: usize) -> f32 {
        (col as f32 + 0.5) * self.col_width()
    }

    fn draw_car(&self, frame: &mut Frame, x: f32, lane: &Lane, i: usize) {
        let u = unit(frame);
        let cw = self.col_width() * u;
        let top = lane.car_top(i, self.t) * u;
        let len = lane.cars[i].1 * u;
        let wid = cw * 0.64;
        let left = x * u - wid * 0.5;
        let body = Rgb::from_hsv(lane.hue[i], 0.7, 0.9);
        let r = wid * 0.2;
        frame.rect(left, top + r, wid, len - r * 2.0, body, 1.0);
        frame.rect(left + r, top, wid - r * 2.0, len, body, 1.0);
        for (cx, cy) in [(left + r, top + r), (left + wid - r, top + r), (left + r, top + len - r), (left + wid - r, top + len - r)] {
            frame.disc(cx, cy, r, body, 1.0);
        }
        // Windshield and headlights at the front, i.e. in the direction of travel.
        let front = if lane.dir > 0.0 { top + len * 0.62 } else { top + len * 0.14 };
        frame.rect(left + wid * 0.15, front, wid * 0.7, len * 0.22, Rgb::new(40, 50, 70), 1.0);
        let lamp_y = if lane.dir > 0.0 { top + len - r * 0.6 } else { top + r * 0.6 };
        for lx in [left + wid * 0.25, left + wid * 0.75] {
            frame.disc(lx, lamp_y, (wid * 0.1).max(0.8), Rgb::new(255, 240, 160), 1.0);
        }
    }

    fn draw_chicken(&self, frame: &mut Frame, x: f32, y: f32, squashed: bool) {
        let u = unit(frame);
        let s = (self.col_width() * 0.34).min(0.09) * u;
        let (x, y) = (x * u, y * u);
        if squashed {
            frame.rect(x - s * 1.4, y + s * 0.3, s * 2.8, s * 0.7, Rgb::new(250, 250, 250), 1.0);
            frame.rect(x + s * 0.8, y + s * 0.2, s * 0.6, s * 0.3, Rgb::new(230, 40, 40), 1.0);
            return;
        }
        frame.disc(x, y + s * 0.95, s * 0.75, Rgb::new(0, 0, 0), 0.18);
        frame.rect(x - s * 0.3, y + s * 0.55, s * 0.18, s * 0.35, Rgb::new(250, 150, 40), 1.0);
        frame.rect(x + s * 0.12, y + s * 0.55, s * 0.18, s * 0.35, Rgb::new(250, 150, 40), 1.0);
        frame.disc(x, y, s * 0.7, Rgb::new(250, 250, 250), 1.0);
        frame.rect(x - s * 0.1, y - s * 1.0, s * 0.35, s * 0.4, Rgb::new(230, 40, 40), 1.0);
        frame.rect(x + s * 0.6, y - s * 0.2, s * 0.4, s * 0.22, Rgb::new(250, 150, 40), 1.0);
        frame.disc(x + s * 0.32, y - s * 0.32, (s * 0.1).max(0.8), Rgb::new(20, 20, 20), 1.0);
        frame.rect(x - s * 0.6, y - s * 0.05, s * 0.45, s * 0.3, Rgb::new(225, 225, 225), 1.0);
    }
}

impl Sim for Crossy {
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
                    self.confetti.burst(self.col_center(self.col), PLAYER_Y, 40, rng);
                }
                if t < 3.0 && t >= self.next_burst {
                    self.next_burst += 0.6;
                    self.confetti.burst(rng.range_f32(0.2, self.w / self.h - 0.2), 0.7, 30, rng);
                }
            }
        }
    }

    fn render(&mut self, frame: &mut Frame, _: &Theme) {
        let u = unit(frame);
        let fh = frame.h as f32;
        let cw = self.col_width();
        vgradient(frame, Rgb::new(70, 70, 78), Rgb::new(60, 60, 68));

        for (i, c) in self.columns.iter().enumerate() {
            let x0 = i as f32 * cw * u;
            let w = cw * u;
            match c {
                Column::Start | Column::Rest => {
                    let (a, b) = if *c == Column::Start {
                        (Rgb::new(96, 170, 70), Rgb::new(84, 156, 62))
                    } else {
                        (Rgb::new(130, 200, 90), Rgb::new(116, 186, 80))
                    };
                    frame.rect(x0, 0.0, w, fh, a, 1.0);
                    // Checkered grass tufts so it reads as ground, not a stripe.
                    let tile = (cw * 0.5).max(0.05) * u;
                    let mut y = 0.0;
                    let mut row = 0;
                    while y < fh {
                        let off = if row % 2 == 0 { 0.0 } else { tile };
                        frame.rect(x0 + off, y, tile.min(w - off), tile, b, 1.0);
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
                Column::Road(_) => {
                    // Dashed divider on the left edge when the previous column is road too.
                    if matches!(self.columns.get(i.wrapping_sub(1)), Some(Column::Road(_))) {
                        let dash = 0.08 * u;
                        let mut y = 0.0;
                        while y < fh {
                            frame.rect(x0 - 0.5, y, (u * 0.012).max(1.0), dash, Rgb::new(230, 230, 230), 0.8);
                            y += dash * 2.0;
                        }
                    }
                }
            }
        }

        for (i, c) in self.columns.iter().enumerate() {
            if let Column::Road(lane) = c {
                for k in 0..lane.count {
                    self.draw_car(frame, self.col_center(i), lane, k);
                }
            }
        }

        // Chicken, mid-hop if it just moved.
        let hop = (self.hop_t / HOP_SEC).clamp(0.0, 1.0);
        let x = self.col_center(self.hop_from) + (self.col_center(self.col) - self.col_center(self.hop_from)) * hop;
        let lift = (hop * std::f32::consts::PI).sin() * 0.08;
        let over = matches!(self.phase, Phase::Over(_));
        let eaten = over && self.death == Death::Eagle;
        if !eaten {
            self.draw_chicken(frame, x, PLAYER_Y - lift, over && self.death == Death::Car);
        }

        // The eagle: a growing shadow as a warning, then it swoops in.
        if self.phase == Phase::Playing && self.idle > IDLE_LIMIT - EAGLE_WARNING {
            let k = (self.idle - (IDLE_LIMIT - EAGLE_WARNING)) / EAGLE_WARNING;
            frame.disc(x * u, PLAYER_Y * u, (0.05 + k * 0.2) * u, Rgb::new(0, 0, 0), 0.15 + k * 0.35);
        }
        if let (Phase::Over(t), true) = (self.phase, eaten) {
            let k = (t / 0.5).min(1.0);
            let ex = x * u + (1.0 - k) * 0.8 * u;
            let ey = (PLAYER_Y - (1.0 - k) * 0.6 - (t - 0.5).max(0.0) * 0.8) * u;
            let brown = Rgb::new(110, 70, 40);
            frame.disc(ex, ey, 0.09 * u, brown, 1.0);
            frame.line((ex - 0.28 * u, ey - 0.06 * u), (ex + 0.28 * u, ey - 0.06 * u), (0.06 * u).max(1.0), brown, 1.0);
            frame.disc(ex + 0.05 * u, ey - 0.08 * u, 0.045 * u, Rgb::new(245, 245, 245), 1.0);
            frame.rect(ex + 0.08 * u, ey - 0.08 * u, 0.05 * u, 0.025 * u, Rgb::new(250, 190, 40), 1.0);
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

    const DT: f32 = 1.0 / 60.0;
    const RIGHT: Input = Input { clicks: 1, left_clicks: 0 };
    const LEFT: Input = Input { clicks: 1, left_clicks: 1 };

    fn road_count(sim: &Crossy) -> usize {
        sim.columns.iter().filter(|c| matches!(c, Column::Road(_))).count()
    }

    /// Seconds per planning step: roughly how fast a person re-decides.
    const STEP: f32 = 0.1;
    const HORIZON: usize = 40;

    fn clear_during(sim: &Crossy, col: usize, t0: f32, t1: f32) -> bool {
        let n = ((t1 - t0) / 0.01).ceil().max(1.0) as usize;
        (0..=n).all(|k| !sim.blocked(col, t0 + (t1 - t0) * k as f32 / n as f32))
    }

    /// Plans a few seconds ahead: a breadth-first search over (column,
    /// step) with hop right / wait / hop left, keeping only moves that
    /// stay clear of cars for the whole step. Heads for the furthest
    /// column it can safely reach and returns the first move of that plan.
    fn bot_input(sim: &Crossy, since_move: f32) -> Input {
        // A hair under STEP: whole frames rarely add up to it exactly.
        if since_move < STEP - 0.02 {
            return Input::default();
        }
        let cols = sim.columns.len();
        // first[k][c]: first move (-1, 0, +1) of a safe path to column c at step k.
        let mut first = vec![vec![None::<i32>; cols]; HORIZON + 1];
        first[0][sim.col] = Some(0);
        for k in 0..HORIZON {
            let (t0, t1) = (sim.t + k as f32 * STEP, sim.t + (k + 1) as f32 * STEP);
            for c in 0..cols {
                let Some(root) = first[k][c] else { continue };
                // The floor only ever rises, so planning with today's is conservative.
                let floor = if c > 0 { sim.floor.max(1) } else { 0 };
                for d in [1i32, 0, -1] {
                    let n = c as i32 + d;
                    if n < floor as i32 || n >= cols as i32 {
                        continue;
                    }
                    let n = n as usize;
                    let root = if k == 0 { d } else { root };
                    // Several plans can reach the same cell; keep the most
                    // eager first move (hop right > wait > hop left).
                    if first[k + 1][n].is_some_and(|prev| prev >= root) || !clear_during(sim, n, t0, t1) {
                        continue;
                    }
                    first[k + 1][n] = Some(root);
                }
            }
        }
        // Only plans that survive the whole horizon count; of those, the
        // one that ends furthest right wins.
        let plan = (0..cols).rev().find_map(|c| first[HORIZON][c]).unwrap_or(0);

        match plan {
            1 => RIGHT,
            -1 => LEFT,
            _ => Input::default(),
        }
    }

    #[test]
    fn layout_is_ten_roads_with_a_rest_every_three_or_four() {
        let mut rng = Rng::new(1);
        let mut sim = Crossy::new(320, 96, &mut rng);
        for _ in 0..50 {
            sim.reset(&mut rng);
            assert_eq!(road_count(&sim), GOAL as usize);
            assert!(sim.columns[0] == Column::Start && *sim.columns.last().unwrap() == Column::Finish);
            let mut run = 0;
            for c in &sim.columns[1..sim.columns.len() - 1] {
                if matches!(c, Column::Road(_)) {
                    run += 1;
                } else {
                    assert!((3..=4).contains(&run), "rest after {run} roads");
                    run = 0;
                }
            }
            assert!((3..=4).contains(&run));
        }
    }

    #[test]
    fn cannot_go_back_to_start_or_left_of_a_rest_spot() {
        let mut rng = Rng::new(2);
        let mut sim = Crossy::new(320, 96, &mut rng);
        // Freeze traffic out of the way so only the movement rules matter.
        for c in &mut sim.columns {
            if let Column::Road(lane) = c {
                lane.speed = 0.0;
                lane.cars = [(0.0, CAR_LEN_MIN), (0.1, CAR_LEN_MIN)];
            }
        }
        sim.step(DT, &LEFT, &mut rng);
        assert_eq!(sim.col, 0);
        sim.step(DT, &RIGHT, &mut rng);
        sim.step(DT, &RIGHT, &mut rng);
        assert_eq!(sim.col, 2);
        sim.step(DT, &LEFT, &mut rng);
        assert_eq!(sim.col, 1);
        sim.step(DT, &LEFT, &mut rng);
        assert_eq!(sim.col, 1, "hopped back onto the start");
        let rest = sim.columns.iter().position(|c| *c == Column::Rest).unwrap();
        while sim.col < rest + 1 {
            sim.step(DT, &RIGHT, &mut rng);
        }
        sim.step(DT, &LEFT, &mut rng);
        assert_eq!(sim.col, rest);
        sim.step(DT, &LEFT, &mut rng);
        assert_eq!(sim.col, rest, "went left of a rest spot");
        assert_eq!(sim.phase, Phase::Playing);
    }

    #[test]
    fn a_press_on_both_halves_in_one_tick_goes_nowhere() {
        let mut rng = Rng::new(3);
        let mut sim = Crossy::new(320, 96, &mut rng);
        sim.step(DT, &Input { clicks: 2, left_clicks: 1 }, &mut rng);
        assert_eq!(sim.col, 0);
    }

    #[test]
    fn idle_chicken_is_taken_by_the_eagle_and_the_game_restarts() {
        let mut rng = Rng::new(4);
        let mut sim = Crossy::new(320, 96, &mut rng);
        let mut deaths = 0;
        let mut was_over = false;
        for _ in 0..60 * 30 {
            sim.step(DT, &Input::default(), &mut rng);
            let over = matches!(sim.phase, Phase::Over(_));
            if over && !was_over {
                deaths += 1;
                assert!(sim.death == Death::Eagle);
            }
            was_over = over;
            assert!(!sim.cleared());
        }
        assert!(deaths >= 3, "only {deaths} deaths");
    }

    #[test]
    fn running_straight_across_gets_hit() {
        let mut hits = 0;
        for seed in 1..11 {
            let mut rng = Rng::new(seed);
            let mut sim = Crossy::new(320, 96, &mut rng);
            for _ in 0..60 * 5 {
                sim.step(DT, &RIGHT, &mut rng);
                if matches!(sim.phase, Phase::Over(_)) {
                    hits += 1;
                    break;
                }
            }
        }
        assert!(hits >= 9, "mashing right survived {} of 10 runs", 10 - hits);
    }

    #[test]
    fn a_careful_player_reaches_the_finish() {
        for seed in 1..41 {
            let mut rng = Rng::new(seed);
            let mut sim = Crossy::new(320, 96, &mut rng);
            let mut since_move = 1.0;
            for _ in 0..60 * 60 {
                let input = bot_input(&sim, since_move);
                let before = sim.col;
                sim.step(DT, &input, &mut rng);
                since_move = if sim.col != before { 0.0 } else { since_move + DT };
                assert!(!matches!(sim.phase, Phase::Over(_)), "seed {seed} died at column {} ({:?})", sim.col, sim.death == Death::Eagle);
                if sim.cleared() {
                    break;
                }
            }
            assert!(sim.cleared(), "seed {seed} never finished");
            assert_eq!(sim.passed(), GOAL);
            for _ in 0..600 {
                sim.step(DT, &RIGHT, &mut rng);
            }
            assert!(sim.cleared(), "clearing must be permanent");
        }
    }

    #[test]
    fn renders_opaque_at_odd_and_tiny_sizes_with_bad_dt() {
        let mut rng = Rng::new(5);
        let mut sim = Crossy::new(320, 96, &mut rng);
        for (w, h) in [(321, 97), (1, 1), (0, 0), (4, 7)] {
            sim.resize(w, h, &mut rng);
            for dt in [f32::NAN, -1.0, 1000.0, DT] {
                sim.step(dt, &RIGHT, &mut rng);
            }
            let mut frame = Frame::new(w, h);
            sim.render(&mut frame, &Theme::default());
            assert!(frame.pixels.chunks_exact(4).all(|p| p[3] == 255));
        }
    }
}
