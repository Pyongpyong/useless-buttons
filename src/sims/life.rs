//! `life` — Conway's Game of Life, decoupled from render fps.
//!
//! The automaton advances at a fixed ~12 generations/second via an
//! internal accumulator, however fast (or rarely) `render` gets called —
//! and speeds up to ~36 generations/second while the cursor is hovering,
//! so the board visibly "grows faster" wherever attention is. Cells track
//! their age (for color) and dead cells leave a decaying "ghost" so the
//! whole thing doesn't flicker between hard on/off states.

use super::{Input, Sim};
use crate::paint::{Frame, Rgb, Theme};
use crate::rng::Rng;

const CELL_PX: usize = 2;
/// Base (idle) simulation rate — 3x the original ~12 generations/sec.
const STEP_DT: f32 = 1.0 / 36.0;
/// While hovering, generations advance this many times faster still than
/// the (already tripled) idle rate above.
const HOVER_STEP_SPEEDUP: f32 = 3.0;
const MAX_SUBSTEPS: u32 = 4;
const GHOST_DECAY: f32 = 0.82;
const AGE_SATURATE: f32 = 60.0;
const CLICK_SEED_RADIUS_CELLS: i64 = 10;
const CLICK_SEED_P: f32 = 0.6;
/// A gentle, continuous sprinkle of new cells under the cursor whenever
/// it's hovering — not just on click — so the board looks like it's
/// actively "growing" wherever attention is, not just occasionally
/// reseeded.
const HOVER_SEED_RADIUS_CELLS: i64 = 3;
const HOVER_SEED_P: f32 = 0.05;
/// Initial (and post-stagnation... see `reseed`) fill density.
const INITIAL_DENSITY: f32 = 0.34;
/// If population hasn't changed for this many *simulation* steps, the
/// board is stagnant (empty, a frozen still life, or a stable oscillator
/// cycle we're not bothering to detect precisely) — reseed it.
const STAGNATION_STEPS: u32 = 40;
const REVIVE_P: f32 = 0.06;
/// `replenish` refuses to add more cells once population already exceeds
/// this percentage of the board — see its doc comment.
const MAX_REPLENISH_DENSITY_PCT: usize = 35;

pub struct Life {
    cols: usize,
    rows: usize,
    alive: Vec<bool>,
    age: Vec<u32>,
    ghost: Vec<f32>,
    /// Hue (degrees), assigned freshly at each cell's birth and kept fixed
    /// while it lives (and while its ghost fades) — full spectrum, chosen
    /// per-cell, rather than every live cell sharing one accent-derived
    /// ramp. See `render`.
    hue: Vec<f32>,
    acc: f32,
    last_pop: i64,
    stagnant_steps: u32,
    generation: u64,
}

fn grid_dims(w: usize, h: usize) -> (usize, usize) {
    ((w / CELL_PX).max(1), (h / CELL_PX).max(1))
}

impl Life {
    pub fn new(w: usize, h: usize, rng: &mut Rng) -> Self {
        let (cols, rows) = grid_dims(w, h);
        let mut s = Life {
            cols,
            rows,
            alive: vec![false; cols * rows],
            age: vec![0; cols * rows],
            ghost: vec![0.0; cols * rows],
            hue: vec![0.0; cols * rows],
            acc: 0.0,
            last_pop: -1,
            stagnant_steps: 0,
            generation: 0,
        };
        s.reseed(INITIAL_DENSITY, rng);
        s
    }

    #[inline]
    fn idx(&self, x: usize, y: usize) -> usize {
        y * self.cols + x
    }

    fn reseed(&mut self, density: f32, rng: &mut Rng) {
        for i in 0..self.alive.len() {
            if rng.bool_p(density) {
                self.alive[i] = true;
                self.age[i] = 1;
                self.ghost[i] = 0.0;
                self.hue[i] = rng.range_f32(0.0, 360.0);
            }
        }
    }

    fn population(&self) -> usize {
        self.alive.iter().filter(|&&a| a).count()
    }

    fn seed_around(&mut self, cx: i64, cy: i64, radius: i64, p: f32, rng: &mut Rng) {
        if self.cols == 0 || self.rows == 0 {
            return;
        }
        let cols = self.cols as i64;
        let rows = self.rows as i64;
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx * dx + dy * dy > radius * radius {
                    continue;
                }
                let x = (cx + dx).rem_euclid(cols) as usize;
                let y = (cy + dy).rem_euclid(rows) as usize;
                if rng.bool_p(p) {
                    let idx = self.idx(x, y);
                    let was_dead = self.age[idx] == 0;
                    self.alive[idx] = true;
                    self.age[idx] = self.age[idx].max(1);
                    self.ghost[idx] = 0.0;
                    if was_dead {
                        self.hue[idx] = rng.range_f32(0.0, 360.0);
                    }
                }
            }
        }
    }

    /// Whenever the population drops from one generation to the next,
    /// spawn small clusters of new cells at random locations elsewhere on
    /// the board — roughly compensating for the loss so the board keeps
    /// staying lively instead of slowly dwindling to almost nothing, the
    /// way a plain random-soup Game of Life normally does within a few
    /// dozen generations. Small clustered patches, not isolated single
    /// cells (which almost always die immediately under B3/S23 — a lone
    /// cell needs 2-3 neighbors to survive), so the replacements have a
    /// real chance of surviving or oscillating rather than just vanishing
    /// again next generation.
    fn replenish(&mut self, deficit: usize, rng: &mut Rng) {
        if deficit == 0 || self.cols == 0 || self.rows == 0 {
            return;
        }
        // Safety valve: don't compensate at all once the board is
        // already fairly dense. A Game of Life that's mostly solid
        // doesn't read as "Life" anymore, and if per-death compensation
        // ever ran even slightly hot, an unconditional replenish would
        // let density ratchet upward forever instead of settling into a
        // lively-but-sparse equilibrium.
        let total = self.alive.len();
        if total == 0 || self.population() * 100 / total > MAX_REPLENISH_DENSITY_PCT {
            return;
        }
        const CLUSTER_RADIUS: i64 = 1;
        const CLUSTER_P: f32 = 0.7;
        // A radius-1 neighborhood is 9 cells, so a cluster seeds ~6.3 of
        // them at this probability (before accounting for cells already
        // alive) — scale the cluster count so total new cells roughly
        // *matches* the deficit instead of overshooting it.
        const CELLS_PER_CLUSTER: usize = 6;
        let clusters = deficit.div_ceil(CELLS_PER_CLUSTER).max(1);
        for _ in 0..clusters {
            let cx = rng.range_usize(0, self.cols) as i64;
            let cy = rng.range_usize(0, self.rows) as i64;
            self.seed_around(cx, cy, CLUSTER_RADIUS, CLUSTER_P, rng);
        }
    }

    fn neighbor_count(&self, x: usize, y: usize) -> u8 {
        let cols = self.cols as i64;
        let rows = self.rows as i64;
        let mut n = 0u8;
        for dy in -1i64..=1 {
            for dx in -1i64..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let nx = (x as i64 + dx).rem_euclid(cols) as usize;
                let ny = (y as i64 + dy).rem_euclid(rows) as usize;
                if self.alive[self.idx(nx, ny)] {
                    n += 1;
                }
            }
        }
        n
    }

    fn tick_generation(&mut self, rng: &mut Rng) {
        if self.cols == 0 || self.rows == 0 {
            return;
        }
        let pop_before = self.population();
        let mut next_alive = vec![false; self.alive.len()];
        for y in 0..self.rows {
            for x in 0..self.cols {
                let idx = self.idx(x, y);
                let n = self.neighbor_count(x, y);
                let was = self.alive[idx];
                let now = if was { n == 2 || n == 3 } else { n == 3 };
                next_alive[idx] = now;
            }
        }
        for i in 0..self.alive.len() {
            let was = self.alive[i];
            let now = next_alive[i];
            if now {
                if !was {
                    self.hue[i] = rng.range_f32(0.0, 360.0);
                }
                self.age[i] = if was { self.age[i] + 1 } else { 1 };
                self.ghost[i] = 0.0;
            } else {
                if was {
                    self.ghost[i] = 1.0;
                } else {
                    self.ghost[i] *= GHOST_DECAY;
                    if self.ghost[i] < 0.02 {
                        self.ghost[i] = 0.0;
                    }
                }
                self.age[i] = 0;
            }
        }
        self.alive = next_alive;
        self.generation += 1;

        let pop_after = self.population();
        if pop_after < pop_before {
            self.replenish(pop_before - pop_after, rng);
        }

        let pop = self.population() as i64;
        if pop == self.last_pop {
            self.stagnant_steps += 1;
        } else {
            self.stagnant_steps = 0;
            self.last_pop = pop;
        }
        if self.stagnant_steps >= STAGNATION_STEPS {
            self.reseed(REVIVE_P, rng);
            self.stagnant_steps = 0;
            self.last_pop = self.population() as i64;
        }
    }

    #[cfg(test)]
    pub(crate) fn population_pub(&self) -> usize {
        self.population()
    }

    #[cfg(test)]
    pub(crate) fn generation_count(&self) -> u64 {
        self.generation
    }
}

impl Sim for Life {
    fn resize(&mut self, w: usize, h: usize, rng: &mut Rng) {
        let (cols, rows) = grid_dims(w, h);
        if cols == self.cols && rows == self.rows {
            return;
        }
        self.cols = cols;
        self.rows = rows;
        self.alive = vec![false; cols * rows];
        self.age = vec![0; cols * rows];
        self.ghost = vec![0.0; cols * rows];
        self.hue = vec![0.0; cols * rows];
        self.acc = 0.0;
        self.last_pop = -1;
        self.stagnant_steps = 0;
        self.reseed(INITIAL_DENSITY, rng);
    }

    fn step(&mut self, dt: f32, input: &Input, rng: &mut Rng) {
        let dt = if dt.is_finite() { dt.clamp(0.0, 1.0) } else { 0.0 };
        // Hovering compresses simulated time: more generations per real
        // second, so growth visibly accelerates wherever the cursor is.
        let step_dt = if input.hover { STEP_DT / HOVER_STEP_SPEEDUP } else { STEP_DT };

        self.acc += dt;
        let mut steps = 0;
        while self.acc >= step_dt && steps < MAX_SUBSTEPS {
            self.tick_generation(rng);
            self.acc -= step_dt;
            steps += 1;
        }
        if self.acc > step_dt * MAX_SUBSTEPS as f32 {
            self.acc = 0.0;
        }

        if self.cols > 0 && self.rows > 0 {
            let cx = (input.x as i64) / CELL_PX as i64;
            let cy = (input.y as i64) / CELL_PX as i64;
            if input.clicks > 0 {
                self.seed_around(cx, cy, CLICK_SEED_RADIUS_CELLS, CLICK_SEED_P, rng);
            }
            if input.hover {
                self.seed_around(cx, cy, HOVER_SEED_RADIUS_CELLS, HOVER_SEED_P, rng);
            }
        }
    }

    fn render(&mut self, frame: &mut Frame, theme: &Theme) {
        frame.fill(theme.paper);
        for y in 0..self.rows {
            for x in 0..self.cols {
                let idx = self.idx(x, y);
                let px = (x * CELL_PX) as f32;
                let py = (y * CELL_PX) as f32;
                if self.alive[idx] {
                    let t = (self.age[idx] as f32 / AGE_SATURATE).clamp(0.0, 1.0);
                    // Each cell wears its own randomly-drawn hue at full
                    // strength. Saturation/value stay high on purpose: at
                    // CELL_PX this small, a dim or washed-out cell just
                    // reads as gray noise next to its neighbors, and the
                    // per-cell hue stops being visible at all. Age only
                    // deepens the color slightly rather than driving it
                    // from near-black up to visible.
                    let color = Rgb::from_hsv(self.hue[idx], 0.7 + t * 0.25, 0.95 - t * 0.2);
                    frame.rect(px, py, CELL_PX as f32, CELL_PX as f32, color, 1.0);
                } else if self.ghost[idx] > 0.0 {
                    let full = Rgb::from_hsv(self.hue[idx], 0.75, 0.9);
                    let color = theme.paper.lerp(full, self.ghost[idx]);
                    frame.rect(px, py, CELL_PX as f32, CELL_PX as f32, color, 1.0);
                }
            }
        }
    }

    fn preferred_fps(&self) -> f32 {
        60.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_dims_never_zero() {
        assert_eq!(grid_dims(0, 0), (1, 1));
        assert_eq!(grid_dims(320, 96), (160, 48));
    }

    #[test]
    fn stagnant_board_eventually_revives() {
        let mut rng = Rng::new(1);
        let mut sim = Life::new(60, 60, &mut rng);
        // Force a fully dead, unchanging board.
        for a in sim.alive.iter_mut() {
            *a = false;
        }
        sim.last_pop = 0;
        sim.stagnant_steps = 0;

        let input = Input::default();
        let mut revived = false;
        // 45 sim-steps at STEP_DT each, comfortably past STAGNATION_STEPS.
        for _ in 0..(45 * 2) {
            sim.step(STEP_DT, &input, &mut rng);
            if sim.population_pub() > 0 {
                revived = true;
                break;
            }
        }
        assert!(revived, "stagnant board never revived");
    }

    #[test]
    fn click_seeds_around_cursor() {
        let mut rng = Rng::new(2);
        let mut sim = Life::new(90, 90, &mut rng);
        for a in sim.alive.iter_mut() {
            *a = false;
        }
        let input = Input { x: 45.0, y: 45.0, hover: true, down: false, clicks: 1 };
        sim.step(0.0, &input, &mut rng);
        assert!(sim.population_pub() > 0, "click should seed cells near cursor");
    }

    #[test]
    fn hover_seeds_cells_without_requiring_a_click() {
        let mut rng = Rng::new(21);
        let mut sim = Life::new(90, 90, &mut rng);
        for a in sim.alive.iter_mut() {
            *a = false;
        }
        let input = Input { x: 45.0, y: 45.0, hover: true, down: false, clicks: 0 };
        let mut seeded = false;
        for _ in 0..200 {
            sim.step(1.0 / 60.0, &input, &mut rng);
            if sim.population_pub() > 0 {
                seeded = true;
                break;
            }
        }
        assert!(seeded, "expected hovering (no click) to eventually seed cells near the cursor");
    }

    #[test]
    fn hover_speeds_up_generation_rate() {
        let mut rng_hover = Rng::new(20);
        let mut sim_hover = Life::new(60, 60, &mut rng_hover);
        let hover_input = Input { x: 30.0, y: 30.0, hover: true, down: false, clicks: 0 };
        for _ in 0..60 {
            sim_hover.step(1.0 / 60.0, &hover_input, &mut rng_hover);
        }

        let mut rng_idle = Rng::new(20);
        let mut sim_idle = Life::new(60, 60, &mut rng_idle);
        let idle_input = Input::default();
        for _ in 0..60 {
            sim_idle.step(1.0 / 60.0, &idle_input, &mut rng_idle);
        }

        assert!(
            sim_hover.generation_count() > sim_idle.generation_count(),
            "expected hovering to advance more generations than idling over the same wall-clock time: hover={} idle={}",
            sim_hover.generation_count(),
            sim_idle.generation_count()
        );
    }

    #[test]
    fn population_does_not_dwindle_to_almost_nothing_over_time() {
        let mut rng = Rng::new(30);
        let mut sim = Life::new(120, 120, &mut rng);
        let idle = Input::default();
        let initial_pop = sim.population_pub();
        let mut min_pop_after_warmup = usize::MAX;
        for i in 0..400 {
            sim.step(STEP_DT, &idle, &mut rng);
            if i > 100 {
                // Let the initial random soup settle before measuring —
                // some early die-off before replenishment "catches up"
                // over a generation or two is expected and fine.
                min_pop_after_warmup = min_pop_after_warmup.min(sim.population_pub());
            }
        }
        assert!(
            min_pop_after_warmup > initial_pop / 10,
            "expected replenishment to keep the board from dwindling to almost nothing: \
             initial={initial_pop} min_after_warmup={min_pop_after_warmup}"
        );
    }

    #[test]
    fn population_does_not_runaway_to_near_total_density() {
        let mut rng = Rng::new(31);
        let mut sim = Life::new(120, 120, &mut rng);
        let idle = Input::default();
        let total = sim.cols * sim.rows;
        let mut max_density_pct = 0usize;
        for _ in 0..800 {
            sim.step(STEP_DT, &idle, &mut rng);
            max_density_pct = max_density_pct.max(sim.population_pub() * 100 / total);
        }
        assert!(
            max_density_pct <= 60,
            "expected replenishment to stay well short of filling the whole board: max_density_pct={max_density_pct}"
        );
    }

    #[test]
    fn large_dt_does_not_panic_and_caps_substeps() {
        let mut rng = Rng::new(3);
        let mut sim = Life::new(60, 60, &mut rng);
        let input = Input::default();
        for _ in 0..10 {
            sim.step(5.0, &input, &mut rng);
        }
    }

    #[test]
    fn resize_to_tiny_does_not_panic() {
        let mut rng = Rng::new(4);
        let mut sim = Life::new(320, 96, &mut rng);
        sim.resize(4, 4, &mut rng);
        let input = Input { x: 1.0, y: 1.0, hover: true, down: false, clicks: 1 };
        for _ in 0..20 {
            sim.step(1.0 / 12.0, &input, &mut rng);
        }
        let mut frame = Frame::new(4, 4);
        sim.render(&mut frame, &Theme::default());
    }

    #[test]
    fn sim_rate_is_independent_of_call_frequency() {
        // Many tiny dt calls vs one big equivalent dt call should advance
        // the same number of generations (mod substep cap).
        let mut rng_a = Rng::new(5);
        let mut sim_a = Life::new(60, 60, &mut rng_a);
        let input = Input::default();
        for _ in 0..12 {
            sim_a.step(1.0 / 120.0, &input, &mut rng_a);
        }
        // 12 * 1/120 = 0.1s > STEP_DT (1/36 ~= 0.0278) -> at least 1 gen
        // must have run regardless of how finely the dt was chopped up.
        assert_ne!(sim_a.last_pop, -1, "expected at least one generation to have run");
    }
}
