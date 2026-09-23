//! `sand` — a falling-sand cellular automaton.
//!
//! Five device pixels make one cell — chunky, clearly-visible grains
//! rather than a fine dust. Cells are processed bottom row first (so a
//! grain that falls into a lower row this pass is never re-processed in
//! the same pass — no "double fall" per frame), and each row alternates
//! which diagonal it checks first so the pile doesn't drift consistently
//! to one side. Grains slide diagonally whenever a neighboring spot is
//! open, same as any falling-sand toy, which is what gives the pile its
//! natural sloped silhouette instead of a jagged "bar chart" of
//! independent columns.
//!
//! Sand rains in continuously; interaction adds extra grains. Once every
//! cell is occupied, hold the complete pile briefly, then clear it and
//! start a new pile in the next color.

use super::{Input, Sim};
use crate::paint::{Frame, Rgb, Theme};
use crate::rng::Rng;

const CELL_PX: usize = 5;
/// Fixed simulation step; frame dt is accumulated into multiples of this
/// so the automaton's fall speed doesn't depend on render frame rate.
const STEP_DT: f32 = 1.0 / 60.0;
/// Never run more than this many fixed steps from one `step()` call, even
/// if a huge dt (backgrounded tab) is handed to us.
const MAX_SUBSTEPS: u32 = 6;
/// Keep a completely filled pile visible even across multiple substeps.
const FULL_HOLD_STEPS: u32 = 30;
/// Extra grains spawned near the cursor per step while pressed, on top of
/// the autonomous rain below.
const SPAWN_PER_STEP: u32 = 2;
const CLICK_BURST: u32 = 40;
const BAND_COUNT: usize = 4;

/// Autonomous "rain": sand keeps piling up on its own, no interaction
/// required, so the button never goes visually static while idle.
const AUTO_RAIN_ATTEMPTS: u32 = 2;
const AUTO_RAIN_P: f32 = 0.8;
pub struct Sand {
    cols: usize,
    rows: usize,
    /// True canvas size in device px — `cols * CELL_PX` / `rows * CELL_PX`
    /// generally fall a few pixels short of these (integer division), and
    /// `render` stretches the last row/column of cells to cover that
    /// remainder so the pile can visibly reach every true edge of the
    /// canvas instead of leaving a permanent gap.
    canvas_w: usize,
    canvas_h: usize,
    /// 0 = empty, otherwise `1 + band_index`.
    cells: Vec<u8>,
    count: usize,
    acc: f32,
    row_toggle: bool,
    /// Which color band newly-spawned grains currently use. Advances
    /// every time the pile reaches "full".
    fill_cycle: u32,
    full_steps: u32,
    /// One random hue per band, drawn from the full spectrum at
    /// construction and then fixed for the sim's lifetime — chosen once
    /// rather than derived from a single accent color, so different runs
    /// (different seeds) get genuinely different-colored sand instead of
    /// everyone's pile always being a shade of the same accent.
    palette: [Rgb; BAND_COUNT],
}

impl Sand {
    pub fn new(w: usize, h: usize, rng: &mut Rng) -> Self {
        let (cols, rows) = grid_dims(w, h);
        // Evenly space the bands around the hue wheel (with a little jitter
        // per band) rather than drawing each independently at random --
        // BAND_COUNT independent draws can easily cluster into one corner
        // of the wheel by chance for a given seed, which would look like
        // "one color" instead of the intended variety.
        let base_hue = rng.range_f32(0.0, 360.0);
        let step = 360.0 / BAND_COUNT as f32;
        let palette = std::array::from_fn(|i| {
            let jitter = rng.range_f32(-step * 0.2, step * 0.2);
            Rgb::from_hsv(base_hue + step * i as f32 + jitter, 0.6, 0.62)
        });
        Sand {
            cols,
            rows,
            canvas_w: w.max(1),
            canvas_h: h.max(1),
            cells: vec![0u8; cols * rows],
            count: 0,
            acc: 0.0,
            row_toggle: false,
            fill_cycle: 0,
            full_steps: 0,
            palette,
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn grain_count(&self) -> usize {
        self.count
    }

    #[cfg(test)]
    pub(crate) fn current_band(&self) -> u8 {
        self.band_for_current_cycle()
    }

    #[inline]
    fn idx(&self, x: usize, y: usize) -> usize {
        y * self.cols + x
    }

    fn cap(&self) -> usize {
        self.cells.len()
    }

    fn band_for_current_cycle(&self) -> u8 {
        1 + (self.fill_cycle as usize % BAND_COUNT) as u8
    }

    fn spawn_at(&mut self, cx: i64, cy: i64, n: u32, rng: &mut Rng) {
        let cap = self.cap();
        for _ in 0..n {
            if self.count >= cap {
                return;
            }
            let ox = rng.range_i32(-3, 4) as i64;
            let x = cx + ox;
            let y = cy;
            if x < 0 || y < 0 || x as usize >= self.cols || y as usize >= self.rows {
                continue;
            }
            let idx = self.idx(x as usize, y as usize);
            if self.cells[idx] == 0 {
                self.cells[idx] = self.band_for_current_cycle();
                self.count += 1;
            }
        }
    }

    /// Spawns a trickle of sand at random columns along the very top row,
    /// independent of any pointer interaction.
    fn auto_rain(&mut self, rng: &mut Rng) {
        if self.cols == 0 || self.rows == 0 {
            return;
        }
        let cap = self.cap();
        for _ in 0..AUTO_RAIN_ATTEMPTS {
            if self.count >= cap {
                return;
            }
            if !rng.bool_p(AUTO_RAIN_P) {
                continue;
            }
            // Start at a random column, but search all columns so even
            // the final vacancy receives rain without repeated misses.
            let start = rng.range_usize(0, self.cols);
            for offset in 0..self.cols {
                let x = (start + offset) % self.cols;
                let idx = self.idx(x, 0);
                if self.cells[idx] == 0 {
                    self.cells[idx] = self.band_for_current_cycle();
                    self.count += 1;
                    break;
                }
            }
        }
    }

    fn pass(&mut self, rng: &mut Rng) {
        if self.rows == 0 || self.cols == 0 {
            return;
        }
        if self.count == self.cap() {
            self.full_steps += 1;
            if self.full_steps >= FULL_HOLD_STEPS {
                self.cells.fill(0);
                self.count = 0;
                self.full_steps = 0;
                self.fill_cycle = self.fill_cycle.wrapping_add(1);
            }
            return;
        }
        self.auto_rain(rng);
        for y in (0..self.rows).rev() {
            let left_first = (y % 2 == 0) == self.row_toggle;
            if y + 1 >= self.rows {
                continue;
            }
            if left_first {
                for x in 0..self.cols {
                    self.try_fall(x, y);
                }
            } else {
                for x in (0..self.cols).rev() {
                    self.try_fall(x, y);
                }
            }
        }
        self.row_toggle = !self.row_toggle;
    }

    fn try_fall(&mut self, x: usize, y: usize) {
        let idx = self.idx(x, y);
        if self.cells[idx] == 0 {
            return;
        }
        let below = self.idx(x, y + 1);
        if self.cells[below] == 0 {
            self.cells.swap(idx, below);
            return;
        }
        let can_left = x > 0;
        let can_right = x + 1 < self.cols;
        let left_idx = if can_left { Some(self.idx(x - 1, y + 1)) } else { None };
        let right_idx = if can_right { Some(self.idx(x + 1, y + 1)) } else { None };

        let left_free = left_idx.map(|i| self.cells[i] == 0).unwrap_or(false);
        let right_free = right_idx.map(|i| self.cells[i] == 0).unwrap_or(false);

        // Row-local left/right priority, recomputed from the same parity
        // used to pick the scan direction above so the bias stays
        // consistent within a pass.
        let left_first = (y % 2 == 0) == self.row_toggle;
        if left_first {
            if left_free {
                self.cells.swap(idx, left_idx.unwrap());
            } else if right_free {
                self.cells.swap(idx, right_idx.unwrap());
            }
        } else if right_free {
            self.cells.swap(idx, right_idx.unwrap());
        } else if left_free {
            self.cells.swap(idx, left_idx.unwrap());
        }
    }
}

fn grid_dims(w: usize, h: usize) -> (usize, usize) {
    ((w / CELL_PX).max(1), (h / CELL_PX).max(1))
}

impl Sim for Sand {
    fn resize(&mut self, w: usize, h: usize, rng: &mut Rng) {
        let w = w.max(1);
        let h = h.max(1);
        self.canvas_w = w;
        self.canvas_h = h;
        let (cols, rows) = grid_dims(w, h);
        if cols == self.cols && rows == self.rows {
            return;
        }
        // Resizing loses the existing pile rather than trying to remap
        // coordinates into a differently-shaped grid — simplest thing
        // that can't panic, and visually irrelevant for a decorative
        // button background.
        self.cols = cols;
        self.rows = rows;
        self.cells = vec![0u8; cols * rows];
        self.count = 0;
        self.full_steps = 0;
        let _ = rng;
    }

    fn step(&mut self, dt: f32, input: &Input, rng: &mut Rng) {
        let dt = if dt.is_finite() { dt.clamp(0.0, 0.5) } else { 0.0 };
        self.acc += dt;
        let mut steps = 0;
        while self.acc >= STEP_DT && steps < MAX_SUBSTEPS {
            self.pass(rng);
            self.acc -= STEP_DT;
            steps += 1;
        }
        // Don't let a starved accumulator (huge dt beyond MAX_SUBSTEPS
        // capacity) build up unbounded backlog.
        if self.acc > STEP_DT * MAX_SUBSTEPS as f32 {
            self.acc = 0.0;
        }

        let cx = (input.x as i64) / CELL_PX as i64;
        let cy = (input.y as i64) / CELL_PX as i64;
        if input.down {
            self.spawn_at(cx, cy, SPAWN_PER_STEP, rng);
        }
        if input.clicks > 0 {
            self.spawn_at(cx, cy, CLICK_BURST, rng);
        }
    }

    fn render(&mut self, frame: &mut Frame, theme: &Theme) {
        frame.fill(theme.paper);
        for y in 0..self.rows {
            // `cols`/`rows` are floor(canvas / CELL_PX), so the grid
            // generally falls a few px short of the true canvas size.
            // Stretch the very last row/column to cover that remainder —
            // otherwise there's a permanent strip along the bottom/right
            // edge that sand can never visibly reach no matter how full
            // the pile gets.
            let cell_h = if y + 1 == self.rows {
                self.canvas_h.saturating_sub(y * CELL_PX).max(1) as f32
            } else {
                CELL_PX as f32
            };
            for x in 0..self.cols {
                let cell = self.cells[self.idx(x, y)];
                if cell == 0 {
                    continue;
                }
                let cell_w = if x + 1 == self.cols {
                    self.canvas_w.saturating_sub(x * CELL_PX).max(1) as f32
                } else {
                    CELL_PX as f32
                };
                let color = self.palette[(cell as usize - 1) % BAND_COUNT];
                frame.rect((x * CELL_PX) as f32, (y * CELL_PX) as f32, cell_w, cell_h, color, 1.0);
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
        assert_eq!(grid_dims(1, 1), (1, 1));
        assert_eq!(grid_dims(320, 96), (64, 19));
    }

    #[test]
    fn never_exceeds_cap_with_rain_and_interaction() {
        let mut rng = Rng::new(1);
        let mut sim = Sand::new(64, 64, &mut rng); // cols=12, rows=12
        let input = Input { x: 32.0, y: 32.0, hover: true, down: true, clicks: 0 };
        let cap = 12 * 12;

        let mut max_seen = 0usize;
        for _ in 0..3000 {
            sim.step(1.0 / 60.0, &input, &mut rng);
            max_seen = max_seen.max(sim.grain_count());
            assert!(sim.grain_count() <= cap, "grain count exceeded cap");
        }
        assert!(max_seen > 0, "expected some grains to spawn");
    }

    #[test]
    fn autonomous_rain_grows_pile_without_interaction() {
        let mut rng = Rng::new(10);
        let mut sim = Sand::new(64, 64, &mut rng);
        let idle = Input::default();
        for _ in 0..300 {
            sim.step(1.0 / 60.0, &idle, &mut rng);
        }
        assert!(
            sim.grain_count() > 0,
            "expected autonomous rain to spawn grains with zero interaction"
        );
    }

    #[test]
    fn fill_cycle_advances_band_once_pile_reaches_the_top() {
        let mut rng = Rng::new(11);
        let mut sim = Sand::new(50, 150, &mut rng); // cols=10, rows=30
        let idle = Input::default();
        let start_band = sim.current_band();
        let mut advanced = false;
        for _ in 0..20_000 {
            sim.step(1.0 / 60.0, &idle, &mut rng);
            if sim.current_band() != start_band {
                advanced = true;
                break;
            }
        }
        assert!(advanced, "expected the color band to change once the pile reached the top");
    }

    #[test]
    fn click_burst_spawns_grains() {
        let mut rng = Rng::new(2);
        let mut sim = Sand::new(64, 64, &mut rng);
        let input = Input { x: 32.0, y: 5.0, hover: true, down: false, clicks: 1 };
        sim.step(1.0 / 60.0, &input, &mut rng);
        assert!(sim.grain_count() > 0);
    }

    #[test]
    fn fills_every_cell_before_resetting_and_repeats() {
        for (w, h) in [(1, 1), (33, 47), (320, 96), (640, 240)] {
            for seed in [1, 11, 60] {
                let mut rng = Rng::new(seed);
                let mut sim = Sand::new(w, h, &mut rng);
                let mut full_frames = 0;
                let mut cycles = 0;
                for _ in 0..(sim.cap() * 6 + 1000) {
                    let band = sim.current_band();
                    sim.step(STEP_DT, &Input::default(), &mut rng);
                    assert_eq!(sim.count, sim.cells.iter().filter(|&&c| c != 0).count());
                    if sim.count == sim.cap() {
                        assert!(sim.cells.iter().all(|&c| c != 0));
                        full_frames += 1;
                    }
                    if sim.current_band() != band {
                        assert!(full_frames >= FULL_HOLD_STEPS);
                        assert_eq!(sim.count, 0);
                        full_frames = 0;
                        cycles += 1;
                        if cycles == 2 { break; }
                    }
                }
                assert_eq!(cycles, 2, "{w}x{h}, seed {seed}: must fill and reset twice");
            }
        }
    }

    #[test]
    fn final_top_row_gap_fills_without_draining() {
        let mut rng = Rng::new(50);
        let mut sim = Sand::new(320, 96, &mut rng);
        sim.cells.fill(1);
        sim.cells[17] = 0;
        sim.count = sim.cap() - 1;
        for _ in 0..10 {
            sim.step(STEP_DT, &Input::default(), &mut rng);
            if sim.count == sim.cap() { break; }
        }
        assert_eq!(sim.count, sim.cap());
        for _ in 0..4 {
            sim.step(STEP_DT * MAX_SUBSTEPS as f32, &Input::default(), &mut rng);
            assert!(sim.cells.iter().all(|&c| c != 0));
        }
    }

    #[test]
    fn render_stretches_edge_cells_to_cover_non_multiple_canvas_size() {
        let mut rng = Rng::new(40);
        // 33 and 47 are not multiples of CELL_PX (5) — cols/rows
        // floor-divide with a remainder, which used to leave a permanent
        // uncovered strip along the bottom/right edge no matter how full
        // the pile got.
        let mut sim = Sand::new(33, 47, &mut rng);
        for c in sim.cells.iter_mut() {
            *c = 1;
        }
        sim.count = sim.cells.len();
        let mut frame = Frame::new(33, 47);
        let theme = Theme::default();
        sim.render(&mut frame, &theme);
        let idx = ((frame.h - 1) * frame.w + (frame.w - 1)) * 4;
        let px = [frame.pixels[idx], frame.pixels[idx + 1], frame.pixels[idx + 2]];
        assert_ne!(
            px,
            [theme.paper.r, theme.paper.g, theme.paper.b],
            "expected the bottom-right corner to be covered by a stretched edge cell, not left as bare paper"
        );
    }

    #[test]
    fn resize_to_tiny_does_not_panic() {
        let mut rng = Rng::new(3);
        let mut sim = Sand::new(320, 96, &mut rng);
        let input = Input { x: 10.0, y: 10.0, hover: true, down: true, clicks: 1 };
        for _ in 0..20 {
            sim.step(1.0 / 60.0, &input, &mut rng);
        }
        sim.resize(4, 4, &mut rng);
        for _ in 0..20 {
            sim.step(1.0 / 60.0, &input, &mut rng);
        }
        let mut frame = Frame::new(4, 4);
        sim.render(&mut frame, &Theme::default());
    }

    #[test]
    fn large_dt_does_not_explode_or_panic() {
        let mut rng = Rng::new(4);
        let mut sim = Sand::new(64, 64, &mut rng);
        let input = Input { x: 32.0, y: 32.0, hover: true, down: true, clicks: 1 };
        for _ in 0..10 {
            sim.step(2.5, &input, &mut rng);
        }
        assert!(sim.grain_count() <= sim.cap());
    }

    #[test]
    fn out_of_bounds_pointer_does_not_panic() {
        let mut rng = Rng::new(5);
        let mut sim = Sand::new(64, 64, &mut rng);
        let input = Input { x: -500.0, y: 99999.0, hover: true, down: true, clicks: 1 };
        for _ in 0..30 {
            sim.step(1.0 / 60.0, &input, &mut rng);
        }
    }
}
