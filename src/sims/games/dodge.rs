//! Dodge: a five-lane highway. Your car sits on the left; traffic comes
//! at you from the right in staggered rows. Press the top half of the
//! button to move up a lane, the bottom half to move down.
//! A sign marks every kilometre; the finish is at 10 km. Every row blocks
//! the lane you're in when it's generated, so sitting still crashes, and
//! every row leaves a free lane at most one lane away from any free lane
//! of the row before, so there's always a way through.
use super::{
    draw_cleared_flash, draw_game_over, draw_number, draw_progress, sanitize_dt, substeps, unit,
    vgradient, Confetti, Phase, GOAL,
};
use crate::paint::{Frame, Rgb, Theme};
use crate::rng::Rng;
use crate::sims::{Input, Sim};

const LANES: usize = 5;
const ROAD_TOP: f32 = 0.2;
const ROAD_BOTTOM: f32 = 0.92;
const LANE_H: f32 = (ROAD_BOTTOM - ROAD_TOP) / LANES as f32;
const START_LANE: usize = 2;
/// Player car's center, in units from the left edge.
const PLAYER_X: f32 = 0.4;
const CAR_LEN: f32 = 0.3;
const CAR_W: f32 = LANE_H * 0.62;
/// Ground covered per kilometre, in units.
const KM: f32 = 6.0;
/// Traffic drives the same way, just slower: this is its own speed.
const TRAFFIC_SPEED: f32 = 0.6;
/// How fast the traffic closes in, at the start and per kilometre.
const CLOSING_BASE: f32 = 1.3;
const CLOSING_PER_KM: f32 = 0.08;
/// Distance between rows of traffic, in closing units.
const ROW_GAP_MIN: f32 = 1.4;
const ROW_GAP_MAX: f32 = 1.9;
const FIRST_ROW_AT: f32 = 2.6;
/// No new rows this close to the finish.
const FINISH_CLEAR: f32 = 1.5;
const LANE_CHANGE_SEC: f32 = 0.1;

struct Car {
    lane: usize,
    /// Position along the closing axis; on screen at `x - closed + PLAYER_X`.
    x: f32,
    hue: f32,
}

pub struct Dodge {
    w: f32,
    h: f32,
    lane: usize,
    /// Lane being left and seconds since, for the lane-change animation.
    from_lane: usize,
    change_t: f32,
    /// Distance along the ground, for kilometres and road markings.
    ground: f32,
    /// Distance the traffic has closed in by.
    closed: f32,
    cars: Vec<Car>,
    next_row: f32,
    /// Lanes the last generated row left open.
    last_free: Vec<usize>,
    phase: Phase,
    confetti: Confetti,
    next_burst: f32,
}

impl Dodge {
    pub fn new(w: usize, h: usize, _: &mut Rng) -> Self {
        let mut s = Self {
            w: w.max(1) as f32,
            h: h.max(1) as f32,
            lane: START_LANE,
            from_lane: START_LANE,
            change_t: LANE_CHANGE_SEC,
            ground: 0.0,
            closed: 0.0,
            cars: Vec::new(),
            next_row: FIRST_ROW_AT,
            last_free: (0..LANES).collect(),
            phase: Phase::Playing,
            confetti: Confetti::default(),
            next_burst: 0.0,
        };
        s.reset();
        s
    }

    fn reset(&mut self) {
        self.lane = START_LANE;
        self.from_lane = START_LANE;
        self.change_t = LANE_CHANGE_SEC;
        self.ground = 0.0;
        self.closed = 0.0;
        self.cars.clear();
        self.next_row = FIRST_ROW_AT;
        self.last_free = (0..LANES).collect();
        self.phase = Phase::Playing;
        self.confetti.clear();
    }

    fn aspect(&self) -> f32 {
        self.w / self.h
    }

    fn km(&self) -> f32 {
        self.ground / KM
    }

    fn finish_ground(&self) -> f32 {
        GOAL as f32 * KM
    }

    fn lane_y(lane: f32) -> f32 {
        ROAD_TOP + (lane + 0.5) * LANE_H
    }

    fn closing_speed(&self) -> f32 {
        CLOSING_BASE + CLOSING_PER_KM * self.km()
    }

    /// Pick the lanes a new row blocks.
    fn row(&self, rng: &mut Rng) -> Vec<usize> {
        let km = self.km();
        let blocked = if km < 4.0 {
            rng.range_usize(2, 4)
        } else if km < 7.0 || !rng.bool_p(0.3) {
            3
        } else {
            4
        };
        for _ in 0..60 {
            let mut lanes: Vec<usize> = (0..LANES).collect();
            for i in (1..LANES).rev() {
                lanes.swap(i, rng.range_usize(0, i + 1));
            }
            let mut block: Vec<usize> = lanes[..blocked].to_vec();
            // Always block the lane you're sitting in, so standing still crashes.
            if !block.contains(&self.lane) {
                block[0] = self.lane;
            }
            let free: Vec<usize> = (0..LANES).filter(|l| !block.contains(l)).collect();
            // From wherever you got through the last row, a free lane in
            // this one must be at most one lane away.
            if self.last_free.iter().all(|&p| free.iter().any(|&f| f.abs_diff(p) <= 1)) {
                return block;
            }
        }
        vec![self.lane]
    }

    fn spawn_rows(&mut self, rng: &mut Rng) {
        let horizon = self.closed + self.aspect() + CAR_LEN;
        while self.next_row < horizon {
            // Rows are placed by closing distance; map to the ground to
            // keep them clear of the finish line.
            let ahead = (self.next_row - self.closed) / self.closing_speed() * (self.closing_speed() + TRAFFIC_SPEED);
            if self.ground + ahead > self.finish_ground() - FINISH_CLEAR {
                self.next_row = f32::INFINITY;
                return;
            }
            let block = self.row(rng);
            for &lane in &block {
                self.cars.push(Car {
                    lane,
                    x: self.next_row + rng.range_f32(-0.12, 0.12),
                    hue: rng.range_f32(0.0, 360.0),
                });
            }
            self.last_free = (0..LANES).filter(|l| !block.contains(l)).collect();
            self.next_row += rng.range_f32(ROW_GAP_MIN, ROW_GAP_MAX);
        }
    }

    fn crashed(&self) -> bool {
        self.cars
            .iter()
            .any(|c| c.lane == self.lane && (c.x - self.closed).abs() < CAR_LEN * 0.9)
    }

    fn step_playing(&mut self, dt: f32, input: &Input, rng: &mut Rng) {
        for p in input.presses() {
            if !p.y.is_finite() {
                continue;
            }
            let to = if p.y < 0.5 { self.lane.saturating_sub(1) } else { (self.lane + 1).min(LANES - 1) };
            if to != self.lane {
                self.from_lane = self.lane;
                self.lane = to;
                self.change_t = 0.0;
            }
        }
        let (n, h) = substeps(dt);
        for _ in 0..n {
            let closing = self.closing_speed();
            self.closed += closing * h;
            self.ground += (closing + TRAFFIC_SPEED) * h;
            self.change_t += h;
            self.spawn_rows(rng);
            if self.crashed() {
                self.phase = Phase::Over(0.0);
                return;
            }
            if self.ground >= self.finish_ground() {
                self.phase = Phase::Cleared(0.0);
                self.confetti.burst(PLAYER_X, Self::lane_y(self.lane as f32), 90, rng);
                self.next_burst = 0.5;
                return;
            }
        }
        let behind = self.closed - PLAYER_X - CAR_LEN;
        self.cars.retain(|c| c.x > behind);
    }

    fn draw_car(frame: &mut Frame, x: f32, y: f32, body: Rgb) {
        let u = unit(frame);
        let (x, y) = (x * u, y * u);
        let (l, w) = (CAR_LEN * u, CAR_W * u);
        let r = w * 0.22;
        frame.rect(x - l * 0.5 + r, y - w * 0.5, l - r * 2.0, w, body, 1.0);
        frame.rect(x - l * 0.5, y - w * 0.5 + r, l, w - r * 2.0, body, 1.0);
        for (cx, cy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
            frame.disc(x + cx * (l * 0.5 - r), y + cy * (w * 0.5 - r), r, body, 1.0);
        }
        // Facing right: windshield toward the front, rear window behind.
        let glass = Rgb::new(40, 50, 70);
        frame.rect(x + l * 0.08, y - w * 0.36, l * 0.16, w * 0.72, glass, 1.0);
        frame.rect(x - l * 0.34, y - w * 0.32, l * 0.1, w * 0.64, glass, 1.0);
        frame.rect(x - l * 0.2, y - w * 0.3, l * 0.26, w * 0.6, body.lerp(Rgb::new(255, 255, 255), 0.25), 1.0);
        for dy in [-0.3, 0.3] {
            frame.disc(x + l * 0.47, y + dy * w, (w * 0.1).max(0.8), Rgb::new(255, 240, 170), 1.0);
        }
    }
}

impl Sim for Dodge {
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
                    self.reset();
                }
            }
            Phase::Cleared(t) => {
                self.phase.advance(dt);
                // Cruise on past the finish.
                self.ground += (CLOSING_BASE + TRAFFIC_SPEED) * dt;
                self.closed += CLOSING_BASE * dt;
                self.change_t += dt;
                if input.clicks > 0 {
                    self.confetti.burst(PLAYER_X, Self::lane_y(self.lane as f32), 30, rng);
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
        let fw = frame.w as f32;
        vgradient(frame, Rgb::new(96, 170, 80), Rgb::new(80, 150, 66));
        frame.rect(0.0, ROAD_TOP * u, fw, (ROAD_BOTTOM - ROAD_TOP) * u, Rgb::new(70, 72, 80), 1.0);
        let edge = (u * 0.015).max(1.0);
        for y in [ROAD_TOP, ROAD_BOTTOM] {
            frame.rect(0.0, y * u - edge * 0.5, fw, edge, Rgb::new(240, 240, 240), 1.0);
        }
        // Lane dashes scroll with the ground.
        let dash = 0.22;
        let offset = self.ground.rem_euclid(dash * 2.0);
        for lane in 1..LANES {
            let y = (ROAD_TOP + lane as f32 * LANE_H) * u;
            let mut x = -offset;
            while x < fw / u {
                frame.rect(x * u, y - edge * 0.4, dash * u, edge * 0.8, Rgb::new(230, 230, 230), 0.8);
                x += dash * 2.0;
            }
        }

        // Kilometre signs on the top verge, and the finish across the road.
        for k in 1..=GOAL {
            let sx = (k as f32 * KM - self.ground) + PLAYER_X;
            if sx < -0.3 || sx > fw / u + 0.3 {
                continue;
            }
            let x = sx * u;
            if k == GOAL {
                let tile = LANE_H * u / 3.0;
                let rows = ((ROAD_BOTTOM - ROAD_TOP) * u / tile).ceil() as usize;
                for r in 0..rows {
                    for c in 0..2 {
                        let color = if (r + c) % 2 == 0 { Rgb::new(245, 245, 245) } else { Rgb::new(25, 25, 25) };
                        frame.rect(x + c as f32 * tile, ROAD_TOP * u + r as f32 * tile, tile, tile, color, 1.0);
                    }
                }
            }
            let post = (0.015 * u).max(1.0);
            frame.rect(x - post * 0.5, 0.1 * u, post, (ROAD_TOP - 0.1) * u, Rgb::new(120, 120, 120), 1.0);
            let (bw, bh) = (0.22 * u, 0.1 * u);
            let board = if k == GOAL { Rgb::new(200, 40, 40) } else { Rgb::new(30, 120, 70) };
            frame.rect(x - bw * 0.5, 0.03 * u, bw, bh, Rgb::new(240, 240, 240), 1.0);
            frame.rect(x - bw * 0.5 + post, 0.03 * u + post, bw - post * 2.0, bh - post * 2.0, board, 1.0);
            draw_number(frame, k, x, 0.08 * u, bh * 0.55, Rgb::new(250, 250, 250));
        }

        for c in &self.cars {
            let x = c.x - self.closed + PLAYER_X;
            if x < -CAR_LEN || x > fw / u + CAR_LEN {
                continue;
            }
            Self::draw_car(frame, x, Self::lane_y(c.lane as f32), Rgb::from_hsv(c.hue, 0.55, 0.85));
        }
        let k = (self.change_t / LANE_CHANGE_SEC).clamp(0.0, 1.0);
        let lane = self.from_lane as f32 + (self.lane as f32 - self.from_lane as f32) * k;
        let y = Self::lane_y(lane);
        Self::draw_car(frame, PLAYER_X, y, Rgb::new(230, 40, 40));
        if let Phase::Over(t) = self.phase {
            // Smoke puffs off the wreck.
            for i in 0..4 {
                let a = t * 1.5 + i as f32 * 0.8;
                frame.disc((PLAYER_X + 0.1 + (a * 3.0).sin() * 0.04) * u, (y - 0.05 - (a % 1.0) * 0.12) * u, (0.04 + (a % 1.0) * 0.04) * u, Rgb::new(200, 200, 200), 0.6 * (1.0 - a % 1.0));
            }
        }

        self.confetti.render(frame);
        draw_progress(frame, (self.km().floor() as u32).min(GOAL));
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

    fn up() -> Input {
        Input::press(0.5, 0.1)
    }
    fn down() -> Input {
        Input::press(0.5, 0.9)
    }

    /// Looks at the nearest row still ahead and steps toward its nearest
    /// free lane, one lane at most every 0.15 s.
    fn bot_input(sim: &Dodge, since: f32) -> Option<Input> {
        if since < 0.15 {
            return None;
        }
        let row_x = sim
            .cars
            .iter()
            .map(|c| c.x)
            .filter(|&x| x > sim.closed - CAR_LEN * 0.5)
            .fold(f32::INFINITY, f32::min);
        if !row_x.is_finite() {
            return None;
        }
        let blocked: Vec<usize> = sim.cars.iter().filter(|c| (c.x - row_x).abs() < 0.3).map(|c| c.lane).collect();
        if !blocked.contains(&sim.lane) {
            return None;
        }
        let target = (0..LANES).filter(|l| !blocked.contains(l)).min_by_key(|l| l.abs_diff(sim.lane))?;
        // Only change lanes once the car beside us (if any) has passed.
        let side = if target < sim.lane { sim.lane - 1 } else { sim.lane + 1 };
        if sim.cars.iter().any(|c| c.lane == side && (c.x - sim.closed).abs() < CAR_LEN * 1.1) {
            return None;
        }
        Some(if target < sim.lane { up() } else { down() })
    }

    #[test]
    fn top_and_bottom_halves_change_lanes() {
        let mut rng = Rng::new(1);
        let mut sim = Dodge::new(320, 96, &mut rng);
        sim.step(DT, &up(), &mut rng);
        assert_eq!(sim.lane, START_LANE - 1);
        sim.step(DT, &down(), &mut rng);
        assert_eq!(sim.lane, START_LANE);
        sim.step(DT, &Input::tap(), &mut rng);
        assert_eq!(sim.lane, START_LANE, "a press with no position shouldn't steer");
        for _ in 0..6 {
            sim.step(DT, &down(), &mut rng);
        }
        assert_eq!(sim.lane, LANES - 1);
    }

    #[test]
    fn every_row_leaves_a_way_through() {
        let mut rng = Rng::new(2);
        let mut sim = Dodge::new(320, 96, &mut rng);
        for _ in 0..200 {
            let before = sim.last_free.clone();
            sim.lane = rng.range_usize(0, LANES);
            let block = sim.row(&mut rng);
            assert!(block.contains(&sim.lane));
            let free: Vec<usize> = (0..LANES).filter(|l| !block.contains(l)).collect();
            assert!(!free.is_empty());
            assert!(before.iter().all(|&p| free.iter().any(|&f| f.abs_diff(p) <= 1)));
            sim.last_free = free;
        }
    }

    #[test]
    fn idle_driver_crashes_and_the_game_restarts_without_ever_clearing() {
        let mut rng = Rng::new(3);
        let mut sim = Dodge::new(320, 96, &mut rng);
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
    fn a_careful_driver_reaches_ten_km() {
        for seed in 1..21 {
            let mut rng = Rng::new(seed);
            let mut sim = Dodge::new(320, 96, &mut rng);
            let mut since = 1.0;
            for _ in 0..60 * 90 {
                let input = bot_input(&sim, since);
                since = if input.is_some() { 0.0 } else { since + DT };
                sim.step(DT, &input.unwrap_or_default(), &mut rng);
                assert!(!matches!(sim.phase, Phase::Over(_)), "seed {seed} crashed at {:.1} km", sim.km());
                if sim.cleared() {
                    break;
                }
            }
            assert!(sim.cleared(), "seed {seed} never finished");
            for _ in 0..600 {
                sim.step(DT, &up(), &mut rng);
            }
            assert!(sim.cleared(), "clearing must be permanent");
        }
    }

    #[test]
    fn renders_opaque_at_odd_and_tiny_sizes_with_bad_dt() {
        let mut rng = Rng::new(5);
        let mut sim = Dodge::new(320, 96, &mut rng);
        for (w, h) in [(321, 97), (1, 1), (0, 0), (4, 7)] {
            sim.resize(w, h, &mut rng);
            for dt in [f32::NAN, -1.0, 1000.0, DT] {
                sim.step(dt, &down(), &mut rng);
            }
            let mut frame = Frame::new(w, h);
            sim.render(&mut frame, &Theme::default());
            assert!(frame.pixels.chunks_exact(4).all(|p| p[3] == 255));
        }
    }
}
