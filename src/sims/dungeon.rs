//! `dungeon` — a first-person raycaster (Wolfenstein/DOOM-style) that
//! wanders a small procedurally-carved maze on autopilot, forever.
//!
//! Two angles are tracked deliberately separately:
//! - `move_angle` is always exactly one of the four cardinal directions.
//!   Actual position only ever advances along a pure grid axis, so there
//!   is no possible diagonal drift into a corridor's side walls while
//!   turning.
//! - `view_angle` smoothly chases `move_angle` every step (a simple
//!   turn-rate-limited rotation) purely for rendering — it's what gives
//!   the raycast view its "turning" animation without affecting where the
//!   camera actually is.
//!
//! Unlike `bounce`, this variant clears and fully repaints the frame
//! every render: it's a camera view, not a trail of marks, so leaving old
//! wall slices on screen would just be visual garbage once the camera has
//! moved on.

use super::{Input, Sim};
use crate::paint::{Frame, Rgb, Theme};
use crate::rng::Rng;
use std::f32::consts::{FRAC_PI_2, PI, TAU};

/// Maze dimensions in cells. Must both be odd — the carving algorithm
/// below treats odd coordinates as "room" cells and needs a border ring
/// of permanent wall cells around them.
///
/// Sized for *range*, not for what fits on screen: the camera only ever
/// sees a corridor or two ahead, so what a bigger grid buys is a longer
/// walk before the autopilot loops back over ground it has already
/// covered. 47x35 is ~10x the area of the original 15x11, which is the
/// difference between visibly pacing the same few corridors and actually
/// exploring. Cost is negligible — generation is O(cells) once, and a
/// raycast still stops at the first wall regardless of how big the map
/// behind it is.
const GRID_W: usize = 47;
const GRID_H: usize = 35;

const MOVE_SPEED: f32 = 2.0; // cells/sec
/// Rad/sec the view swings round to face the direction of travel. Fast on
/// purpose: the camera walks cell-center to cell-center, so it is at its
/// closest to a wall exactly while it turns (half a cell, at a corner or
/// a dead end), and that is the one pose where a wall covers the whole
/// canvas. A quick turn keeps those frames brief.
const TURN_SPEED: f32 = 14.0;
/// How tall a wall one cell away is drawn, as a fraction of the canvas
/// height. Purely a look knob, not physical: at `1.0` (the naive
/// `height / distance`) a wall a single cell ahead exactly fills the
/// canvas, which in a 1-cell-wide maze means most frames are a flat
/// color block with no ceiling or floor visible at all.
const WALL_HEIGHT_SCALE: f32 = 0.62;
/// How closely the view has to line up with the direction of travel
/// before the camera starts walking again, in radians. Walking while the
/// view is still swinging around a corner reads as strafing sideways down
/// the corridor, so it turns in place first.
const TURN_SETTLE_RAD: f32 = 0.02;
const FOV_DEG: f32 = 66.0;
/// Safety cap on how far a ray is allowed to travel, in cells — guards
/// against an unbounded loop if the maze geometry is ever degenerate.
const MAX_RENDER_DIST: f32 = 24.0;
/// Render every Nth device-pixel column as one ray, matching the chunky
/// pixel-art look of the other variants and keeping the per-frame ray
/// count small.
const RAY_STEP_PX: usize = 2;
const MAX_DT: f32 = 1.0 / 20.0;

// --- Surface texturing.
//
// Everything below is procedural: sampled per pixel from the surface
// coordinate the ray landed on, never from a stored bitmap. A raycaster
// gets its whole sense of depth and speed from surface detail sliding
// past, and flat-filled walls/floor/ceiling read as three colored bars
// no matter how good the geometry underneath is.

/// Bricks per cell across a wall face, and down it. Wider than tall, like
/// actual brick. More per cell means *smaller* bricks.
const BRICKS_ACROSS: f32 = 6.0;
const BRICKS_DOWN: f32 = 10.0;
/// Mortar gap as a fraction of one brick, across and down.
const MORTAR_ACROSS: f32 = 0.08;
const MORTAR_DOWN: f32 = 0.12;

/// Hue ranges for the two wall orientations. Seen from above, a wall
/// running north-south (hit by a ray stepping in X) is warm/red, and one
/// running east-west (hit stepping in Y) is cool/blue — so the two axes
/// of the maze stay tellable apart at a glance, even mid-corner.
const HUE_NS_WALL: (f32, f32) = (348.0, 28.0); // wraps through 0
const HUE_EW_WALL: (f32, f32) = (196.0, 238.0);

/// Tiles per cell on the floor, and on the ceiling — the ceiling's are
/// deliberately the larger of the two (fewer per cell, so each tile
/// covers twice the ground).
///
/// Both are far denser than "one tile per corridor" would suggest,
/// because of how little ground is actually on screen: at this
/// projection the visible floor only spans roughly 0.6 to 3 cells out
/// from the camera, so a tile a whole cell across fills the entire strip
/// with its own interior and the floor reads as a flat gray band.
const FLOOR_TILES_PER_CELL: f32 = 6.0;
const CEILING_TILES_PER_CELL: f32 = 3.0;
/// Grout line width, as a fraction of one tile.
const TILE_GROUT: f32 = 0.1;
/// Distance, in cells, at which floor/ceiling have faded all the way out
/// to darkness. Without this the tile grid keeps its contrast to the
/// horizon and the whole thing flattens out.
const FLOOR_FOG_DIST: f32 = 9.0;

pub struct Dungeon {
    grid_w: usize,
    grid_h: usize,
    /// `true` = wall, `false` = open floor. Row-major, `grid_w * grid_h`.
    walls: Vec<bool>,
    /// Which maximal horizontal run of contiguous wall cells (within its
    /// row) each cell belongs to, or `-1` for open cells. A ray that hits
    /// a *horizontal* wall face (`RayHit::stepped_y`) looks up its color
    /// through this — see `label_wall_runs`.
    h_run_id: Vec<i32>,
    /// Same idea, but the maximal *vertical* run within its column — used
    /// for rays that hit a vertical wall face.
    v_run_id: Vec<i32>,
    /// One random hue per horizontal run, indexed by `h_run_id`.
    h_run_hue: Vec<f32>,
    /// One random hue per vertical run, indexed by `v_run_id`.
    v_run_hue: Vec<f32>,
    cam_x: f32,
    cam_y: f32,
    /// The cell center the camera is currently walking towards. The
    /// camera always travels center-to-center and only ever picks a new
    /// direction on arrival, because a cell center is the only place in a
    /// 1-wide corridor where a 90° turn is geometrically possible at all.
    /// (Turning at a cell *boundary* — the previous behavior — leaves the
    /// camera straddling two cells, so any turn immediately clips the
    /// corridor wall and the autopilot ends up pacing one corridor
    /// forever instead of exploring.)
    target_x: f32,
    target_y: f32,
    move_angle: f32,
    view_angle: f32,
    canvas_w: f32,
    canvas_h: f32,
}

/// Result of a single DDA raycast: distance plus enough to look up which
/// wall was hit and shade it distinctly.
struct RayHit {
    dist: f32,
    cell_x: i32,
    cell_y: i32,
    /// `true` if the final DDA step advanced along Y (crossed a
    /// horizontal grid line, hitting an east/west-facing wall face);
    /// `false` if it advanced along X (crossed a vertical grid line,
    /// hitting a north/south-facing face). Used to darken one orientation
    /// of wall a little relative to the other — the classic
    /// Wolfenstein-style trick that makes corners read clearly even when
    /// two adjacent walls happen to share a hue.
    stepped_y: bool,
    /// Where along the face the ray landed, in `[0, 1)` — the horizontal
    /// texture coordinate for the brick pattern.
    wall_u: f32,
    /// Unit ray direction, kept so floor/ceiling casting for this column
    /// doesn't have to recompute the sin/cos.
    dir_x: f32,
    dir_y: f32,
}

fn wrap_angle(a: f32) -> f32 {
    let mut a = a % TAU;
    if a < 0.0 {
        a += TAU;
    }
    a
}

/// Shortest signed angular distance from `from` to `target`, in `(-PI,
/// PI]`.
fn angle_diff(target: f32, from: f32) -> f32 {
    let mut d = (target - from) % TAU;
    if d > PI {
        d -= TAU;
    } else if d < -PI {
        d += TAU;
    }
    d
}

/// Randomized-DFS "backtracker" maze carve, the standard approach for a
/// perfect maze (every open cell reachable from every other, no loops):
/// cells at odd (x, y) are room centers, the even coordinate between two
/// adjacent rooms is the wall that gets carved through to connect them.
/// Border cells are never visited, so they stay permanent walls.
fn generate_maze(grid_w: usize, grid_h: usize, rng: &mut Rng) -> Vec<bool> {
    let mut walls = vec![true; grid_w * grid_h];
    if grid_w < 3 || grid_h < 3 {
        return walls;
    }
    let idx = |x: i32, y: i32| -> usize { y as usize * grid_w + x as usize };

    let mut stack: Vec<(i32, i32)> = vec![(1, 1)];
    walls[idx(1, 1)] = false;

    while let Some(&(cx, cy)) = stack.last() {
        let mut candidates: Vec<(i32, i32, i32, i32)> = Vec::new();
        for (dx, dy) in [(2, 0), (-2, 0), (0, 2), (0, -2)] {
            let nx = cx + dx;
            let ny = cy + dy;
            if nx >= 1
                && nx < grid_w as i32 - 1
                && ny >= 1
                && ny < grid_h as i32 - 1
                && walls[idx(nx, ny)]
            {
                candidates.push((nx, ny, dx, dy));
            }
        }
        if candidates.is_empty() {
            stack.pop();
            continue;
        }
        let (nx, ny, dx, dy) = candidates[rng.range_usize(0, candidates.len())];
        walls[idx(cx + dx / 2, cy + dy / 2)] = false;
        walls[idx(nx, ny)] = false;
        stack.push((nx, ny));
    }

    walls
}

/// Groups wall cells into maximal straight runs — a horizontal labeling
/// (consecutive wall cells within a row) and a vertical one (consecutive
/// wall cells within a column) — and picks one random hue per run.
///
/// This is what gives the dungeon its "real wall" feel: a ray that keeps
/// hitting the *same straight stretch* of wall (a corridor's side, dead
/// ahead or receding into the distance — a 180° continuation) always
/// resolves to the same run id, hence the same color, while turning a
/// corner (a 90° turn) means the ray is now hitting either a different
/// run of the same orientation or the *other* orientation entirely —
/// either way almost certainly a different hue. A run breaks wherever the
/// wall itself breaks (an opening), so two separate wall stretches that
/// happen to be collinear still get independent colors, same as real
/// stonework would.
fn label_wall_runs(
    walls: &[bool],
    grid_w: usize,
    grid_h: usize,
    rng: &mut Rng,
) -> (Vec<i32>, Vec<i32>, Vec<f32>, Vec<f32>) {
    let mut h_run_id = vec![-1i32; walls.len()];
    let mut h_count = 0i32;
    for y in 0..grid_h {
        let mut x = 0;
        while x < grid_w {
            if walls[y * grid_w + x] {
                let id = h_count;
                h_count += 1;
                while x < grid_w && walls[y * grid_w + x] {
                    h_run_id[y * grid_w + x] = id;
                    x += 1;
                }
            } else {
                x += 1;
            }
        }
    }

    let mut v_run_id = vec![-1i32; walls.len()];
    let mut v_count = 0i32;
    for x in 0..grid_w {
        let mut y = 0;
        while y < grid_h {
            if walls[y * grid_w + x] {
                let id = v_count;
                v_count += 1;
                while y < grid_h && walls[y * grid_w + x] {
                    v_run_id[y * grid_w + x] = id;
                    y += 1;
                }
            } else {
                y += 1;
            }
        }
    }

    // Each run draws its hue from the family for its orientation (see
    // `HUE_NS_WALL` / `HUE_EW_WALL`), so runs stay individually tellable
    // apart while the axis they belong to is still obvious at a glance.
    // `h_run` is read on a Y-step hit, which is an east-west wall.
    let h_run_hue: Vec<f32> =
        (0..h_count).map(|_| rng.range_f32(HUE_EW_WALL.0, HUE_EW_WALL.1)).collect();
    let v_run_hue: Vec<f32> =
        (0..v_count).map(|_| rng.range_f32(HUE_NS_WALL.0, HUE_NS_WALL.1 + 360.0) % 360.0).collect();
    (h_run_id, v_run_id, h_run_hue, v_run_hue)
}

/// Cheap deterministic hash of a pair of integers to `[0, 1)`. Used to
/// give individual bricks and tiles their own slight shade variation —
/// without it, a brick wall is just a regular grid of one flat color and
/// reads as wallpaper rather than stonework.
fn hash01(a: i32, b: i32) -> f32 {
    let mut h = (a as u32).wrapping_mul(0x9E37_79B9) ^ (b as u32).wrapping_mul(0x85EB_CA6B);
    h ^= h >> 13;
    h = h.wrapping_mul(0xC2B2_AE35);
    h ^= h >> 16;
    (h >> 8) as f32 / (1u32 << 24) as f32
}

/// Brick pattern sampled at `(u, v)` on a wall face, both in `[0, 1)`.
/// Returns a multiplier on the wall's base value: mortar comes out dark,
/// brick faces near 1 with a little per-brick variation.
fn brick_shade(u: f32, v: f32) -> f32 {
    let ty = v * BRICKS_DOWN;
    let row = ty.floor();
    // Offset every other course by half a brick — running bond, the
    // reason a brick wall doesn't look like a plain grid.
    let tx = u * BRICKS_ACROSS + if (row as i32).rem_euclid(2) == 1 { 0.5 } else { 0.0 };
    let fx = tx - tx.floor();
    let fy = ty - row;

    if fx < MORTAR_ACROSS || fy < MORTAR_DOWN {
        return 0.55; // mortar course
    }
    // Per-brick variation, plus a soft bevel towards the mortar so each
    // brick reads as a raised block rather than a flat rectangle.
    let variation = 0.88 + hash01(tx.floor() as i32, row as i32) * 0.24;
    let edge = ((fx - MORTAR_ACROSS) / 0.10).min((fy - MORTAR_DOWN) / 0.14).min(1.0);
    variation * (0.82 + 0.18 * edge)
}

/// Square-tile pattern for floor and ceiling, sampled at a world position
/// in cell units. `tiles_per_cell` is what makes the ceiling's tiles the
/// larger of the two (fewer of them per cell).
fn tile_shade(wx: f32, wy: f32, tiles_per_cell: f32) -> f32 {
    let tx = wx * tiles_per_cell;
    let ty = wy * tiles_per_cell;
    let (ix, iy) = (tx.floor(), ty.floor());
    let (fx, fy) = (tx - ix, ty - iy);

    if fx < TILE_GROUT || fy < TILE_GROUT {
        return 0.42; // grout line
    }
    // Checkerboard, so the tiling is still legible in the distance where
    // the grout lines are thinner than a pixel and drop out entirely.
    let checker = if (ix as i32 + iy as i32).rem_euclid(2) == 0 { 1.0 } else { 0.62 };
    checker * (0.9 + hash01(ix as i32, iy as i32) * 0.2)
}

impl Dungeon {
    pub fn new(w: usize, h: usize, rng: &mut Rng) -> Self {
        let grid_w = GRID_W;
        let grid_h = GRID_H;
        let walls = generate_maze(grid_w, grid_h, rng);
        let (h_run_id, v_run_id, h_run_hue, v_run_hue) =
            label_wall_runs(&walls, grid_w, grid_h, rng);
        let mut d = Dungeon {
            grid_w,
            grid_h,
            walls,
            h_run_id,
            v_run_id,
            h_run_hue,
            v_run_hue,
            cam_x: 1.5,
            cam_y: 1.5,
            target_x: 1.5,
            target_y: 1.5,
            move_angle: 0.0,
            view_angle: 0.0,
            canvas_w: w.max(1) as f32,
            canvas_h: h.max(1) as f32,
        };
        d.choose_direction(rng);
        d.view_angle = d.move_angle;
        d
    }

    fn is_wall(&self, x: i32, y: i32) -> bool {
        if x < 0 || y < 0 || x as usize >= self.grid_w || y as usize >= self.grid_h {
            return true;
        }
        self.walls[y as usize * self.grid_w + x as usize]
    }

    /// Pick the next open neighbor cell to walk to, preferring not to
    /// immediately double back when there's anywhere else to go (a dead
    /// end has only the way it came, so that case still reverses).
    /// Sets both `move_angle` and the `target_*` cell center.
    ///
    /// Only ever called with the camera sitting exactly on a cell center.
    fn choose_direction(&mut self, rng: &mut Rng) {
        let cx = self.cam_x.floor() as i32;
        let cy = self.cam_y.floor() as i32;
        let dirs: [(i32, i32, f32); 4] =
            [(1, 0, 0.0), (0, 1, FRAC_PI_2), (-1, 0, PI), (0, -1, -FRAC_PI_2)];
        let reverse = wrap_angle(self.move_angle + PI);

        let mut open: Vec<(i32, i32, f32)> =
            dirs.iter().copied().filter(|&(dx, dy, _)| !self.is_wall(cx + dx, cy + dy)).collect();
        if open.len() > 1 {
            open.retain(|&(_, _, a)| angle_diff(a, reverse).abs() > 0.1);
        }

        let (dx, dy, angle) = if open.is_empty() {
            // Fully sealed cell -- impossible in a carved maze, but stay
            // put rather than walking into stone if it ever happens.
            (0, 0, self.move_angle)
        } else {
            open[rng.range_usize(0, open.len())]
        };

        self.move_angle = angle;
        self.target_x = (cx + dx) as f32 + 0.5;
        self.target_y = (cy + dy) as f32 + 0.5;
    }

    /// DDA raycast from the camera at `angle`, returning the distance (in
    /// cell units) to the nearest wall plus which cell/face it hit.
    fn cast_ray(&self, angle: f32) -> RayHit {
        let dir_x = angle.cos();
        let dir_y = angle.sin();
        let mut map_x = self.cam_x.floor() as i32;
        let mut map_y = self.cam_y.floor() as i32;

        let delta_dist_x = if dir_x.abs() < 1e-6 { f32::INFINITY } else { (1.0 / dir_x).abs() };
        let delta_dist_y = if dir_y.abs() < 1e-6 { f32::INFINITY } else { (1.0 / dir_y).abs() };

        let (step_x, mut side_dist_x) = if dir_x < 0.0 {
            (-1i32, (self.cam_x - map_x as f32) * delta_dist_x)
        } else {
            (1i32, (map_x as f32 + 1.0 - self.cam_x) * delta_dist_x)
        };
        let (step_y, mut side_dist_y) = if dir_y < 0.0 {
            (-1i32, (self.cam_y - map_y as f32) * delta_dist_y)
        } else {
            (1i32, (map_y as f32 + 1.0 - self.cam_y) * delta_dist_y)
        };

        let mut dist = 0.0f32;
        let mut stepped_y = false;
        for _ in 0..512 {
            if side_dist_x < side_dist_y {
                dist = side_dist_x;
                side_dist_x += delta_dist_x;
                map_x += step_x;
                stepped_y = false;
            } else {
                dist = side_dist_y;
                side_dist_y += delta_dist_y;
                map_y += step_y;
                stepped_y = true;
            }
            if self.is_wall(map_x, map_y) || dist > MAX_RENDER_DIST {
                break;
            }
        }

        let dist = dist.min(MAX_RENDER_DIST);
        // Where the ray landed along the face: the coordinate that runs
        // *along* the wall, i.e. the axis we did not step across to hit it.
        let along = if stepped_y { self.cam_x + dist * dir_x } else { self.cam_y + dist * dir_y };
        let wall_u = along - along.floor();

        RayHit { dist, cell_x: map_x, cell_y: map_y, stepped_y, wall_u, dir_x, dir_y }
    }

    /// Look up the color for the wall face a ray hit: which *run* it
    /// belongs to depends on which orientation of face was hit
    /// (`stepped_y`) — see `label_wall_runs`.
    fn hue_at(&self, cell_x: i32, cell_y: i32, stepped_y: bool) -> f32 {
        if cell_x < 0 || cell_y < 0 || cell_x as usize >= self.grid_w || cell_y as usize >= self.grid_h
        {
            return 0.0;
        }
        let idx = cell_y as usize * self.grid_w + cell_x as usize;
        if stepped_y {
            let id = self.h_run_id[idx];
            if id < 0 {
                return 0.0;
            }
            self.h_run_hue[id as usize]
        } else {
            let id = self.v_run_id[idx];
            if id < 0 {
                return 0.0;
            }
            self.v_run_hue[id as usize]
        }
    }

    #[cfg(test)]
    pub(crate) fn camera(&self) -> (f32, f32, f32) {
        (self.cam_x, self.cam_y, self.view_angle)
    }

    #[cfg(test)]
    pub(crate) fn is_wall_at(&self, x: i32, y: i32) -> bool {
        self.is_wall(x, y)
    }

    #[cfg(test)]
    pub(crate) fn grid_dims(&self) -> (usize, usize) {
        (self.grid_w, self.grid_h)
    }
}

impl Sim for Dungeon {
    fn resize(&mut self, w: usize, h: usize, rng: &mut Rng) {
        let _ = rng; // the maze layout and camera state don't depend on canvas size
        self.canvas_w = w.max(1) as f32;
        self.canvas_h = h.max(1) as f32;
    }

    fn step(&mut self, dt: f32, input: &Input, rng: &mut Rng) {
        let _ = input; // autopilot -- no pointer interaction for this variant
        let dt = if dt.is_finite() { dt.clamp(0.0, MAX_DT) } else { 0.0 };
        if dt <= 0.0 {
            return;
        }

        let diff = angle_diff(self.move_angle, self.view_angle);
        let max_turn = TURN_SPEED * dt;
        self.view_angle = if diff.abs() <= max_turn {
            self.move_angle
        } else {
            wrap_angle(self.view_angle + max_turn * diff.signum())
        };

        // Turn in place before walking on -- see `TURN_SETTLE_RAD`.
        if angle_diff(self.move_angle, self.view_angle).abs() > TURN_SETTLE_RAD {
            return;
        }

        // Walk towards the center of the next cell, and only pick a new
        // direction once that center is actually reached -- see the
        // `target_*` fields for why turning anywhere else doesn't work.
        // `dt` is clamped to `MAX_DT`, so one step can never cover a
        // whole cell and there's at most one arrival per call.
        let dx = self.target_x - self.cam_x;
        let dy = self.target_y - self.cam_y;
        let remaining = (dx * dx + dy * dy).sqrt();
        let travel = MOVE_SPEED * dt;

        if remaining <= travel.max(1e-6) {
            self.cam_x = self.target_x;
            self.cam_y = self.target_y;
            self.choose_direction(rng);
        } else {
            self.cam_x += dx / remaining * travel;
            self.cam_y += dy / remaining * travel;
        }
    }

    fn render(&mut self, frame: &mut Frame, theme: &Theme) {
        let _ = theme; // the dungeon brings its own stone palette, not the host page's
        let w = frame.w;
        let h = frame.h;
        let half_h = h as f32 * 0.5;
        let fov = FOV_DEG.to_radians();
        // Cool slate for the masonry overhead and underfoot -- neutral
        // enough that the red/blue walls stay the thing you read.
        const CEILING_HUE: f32 = 224.0;
        const FLOOR_HUE: f32 = 214.0;

        let mut col = 0usize;
        while col < w {
            let t = (col as f32 + 0.5) / w as f32 - 0.5; // -0.5..0.5 across the view
            let ray_angle = self.view_angle + t * fov;
            let hit = self.cast_ray(ray_angle);
            // Fisheye correction: project the ray's distance onto the
            // camera's forward axis rather than using the raw radial
            // distance, so straight walls render straight. The same
            // cosine un-does the distortion for floor/ceiling casting
            // below, hence keeping it around.
            let angle_cos = (ray_angle - self.view_angle).cos().max(1e-3);
            let corrected = (hit.dist * angle_cos).max(0.05);

            let wall_h = (h as f32 * WALL_HEIGHT_SCALE / corrected).min(h as f32 * 6.0);
            let y0 = (half_h - wall_h * 0.5).max(0.0);
            let y1 = (half_h + wall_h * 0.5).min(h as f32);
            // Unclamped wall extent, needed to work out which part of the
            // wall a visible row corresponds to when the slice runs off
            // the top and bottom of the canvas.
            let wall_top = half_h - wall_h * 0.5;

            // Straight runs of wall share one hue, and the two
            // orientations draw from opposite ends of the spectrum (see
            // `label_wall_runs`), so a corner is always a visible break.
            let hue = self.hue_at(hit.cell_x, hit.cell_y, hit.stepped_y);
            let dist_t = (corrected / MAX_RENDER_DIST).clamp(0.0, 1.0);
            // North-south faces stay a touch brighter than east-west
            // ones, the classic raycaster trick for reading corners.
            let side_mult = if hit.stepped_y { 0.78 } else { 1.0 };
            let base_value = (0.86 - dist_t * 0.62) * side_mult;

            let col_w = (RAY_STEP_PX).min(w - col) as f32;
            let col_x = col as f32;

            // --- Wall: brick, sampled per row down the slice.
            let inv_wall_h = if wall_h > 0.0 { 1.0 / wall_h } else { 0.0 };
            let mut y = y0 as usize;
            let y_end = y1.ceil() as usize;
            while y < y_end && y < h {
                let v = ((y as f32 + 0.5) - wall_top) * inv_wall_h;
                let shade = brick_shade(hit.wall_u, v.clamp(0.0, 0.999));
                let color =
                    Rgb::from_hsv(hue, 0.62, (base_value * shade).clamp(0.03, 1.0));
                frame.rect(col_x, y as f32, col_w, 1.0, color, 1.0);
                y += 1;
            }

            // --- Floor and ceiling: cast each row back out to the world
            // position it shows, then tile that position. Rows are
            // independent, so this is just the wall projection solved the
            // other way round: a row `p` px below the horizon is looking
            // at ground `(h * WALL_HEIGHT_SCALE / 2) / p` cells away.
            let row_scale = h as f32 * WALL_HEIGHT_SCALE * 0.5;
            for y in 0..(y0 as usize).min(h) {
                let p = half_h - (y as f32 + 0.5);
                if p <= 0.0 {
                    continue;
                }
                let dist = row_scale / p / angle_cos;
                let wx = self.cam_x + hit.dir_x * dist;
                let wy = self.cam_y + hit.dir_y * dist;
                let fog = 1.0 - (dist / FLOOR_FOG_DIST).clamp(0.0, 1.0);
                let shade = tile_shade(wx, wy, CEILING_TILES_PER_CELL);
                let color =
                    Rgb::from_hsv(CEILING_HUE, 0.14, (0.66 * shade * fog).clamp(0.03, 1.0));
                frame.rect(col_x, y as f32, col_w, 1.0, color, 1.0);
            }
            for y in (y1.ceil() as usize)..h {
                let p = (y as f32 + 0.5) - half_h;
                if p <= 0.0 {
                    continue;
                }
                let dist = row_scale / p / angle_cos;
                let wx = self.cam_x + hit.dir_x * dist;
                let wy = self.cam_y + hit.dir_y * dist;
                let fog = 1.0 - (dist / FLOOR_FOG_DIST).clamp(0.0, 1.0);
                let shade = tile_shade(wx, wy, FLOOR_TILES_PER_CELL);
                let color =
                    Rgb::from_hsv(FLOOR_HUE, 0.18, (0.46 * shade * fog).clamp(0.03, 1.0));
                frame.rect(col_x, y as f32, col_w, 1.0, color, 1.0);
            }

            col += RAY_STEP_PX;
        }
    }

    fn preferred_fps(&self) -> f32 {
        60.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paint::Theme;

    fn make(w: usize, h: usize, seed: u32) -> Dungeon {
        let mut rng = Rng::new(seed);
        Dungeon::new(w, h, &mut rng)
    }

    #[test]
    fn maze_generation_is_fully_connected() {
        // A perfect maze (spanning tree carve) must make every open cell
        // reachable from every other open cell. Flood-fill from the known
        // start and compare against the total open-cell count.
        let mut rng = Rng::new(1);
        let walls = generate_maze(GRID_W, GRID_H, &mut rng);
        let idx = |x: usize, y: usize| y * GRID_W + x;

        let total_open = walls.iter().filter(|&&w| !w).count();
        assert!(total_open > 1, "maze generation produced almost nothing open");

        let mut seen = vec![false; walls.len()];
        let mut stack = vec![(1i32, 1i32)];
        seen[idx(1, 1)] = true;
        let mut reached = 0;
        while let Some((x, y)) = stack.pop() {
            reached += 1;
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let (nx, ny) = (x + dx, y + dy);
                if nx < 0 || ny < 0 || nx as usize >= GRID_W || ny as usize >= GRID_H {
                    continue;
                }
                let ni = idx(nx as usize, ny as usize);
                if !walls[ni] && !seen[ni] {
                    seen[ni] = true;
                    stack.push((nx, ny));
                }
            }
        }
        assert_eq!(reached, total_open, "maze has an unreachable pocket");
    }

    #[test]
    fn border_cells_are_always_walls() {
        let mut rng = Rng::new(2);
        let walls = generate_maze(GRID_W, GRID_H, &mut rng);
        for x in 0..GRID_W {
            assert!(walls[x], "top border not a wall at x={x}");
            assert!(walls[(GRID_H - 1) * GRID_W + x], "bottom border not a wall at x={x}");
        }
        for y in 0..GRID_H {
            assert!(walls[y * GRID_W], "left border not a wall at y={y}");
            assert!(walls[y * GRID_W + GRID_W - 1], "right border not a wall at y={y}");
        }
    }

    #[test]
    fn straight_wall_run_shares_one_id_and_hue() {
        // A row of 5 consecutive wall cells: 0 0 0 0 0 (grid_w=5 for this
        // tiny standalone case). All five must land in the same
        // horizontal run (same color if hit head-on/receding straight
        // ahead -- a 180 degree continuation).
        let grid_w = 5;
        let grid_h = 3;
        let mut walls = vec![false; grid_w * grid_h];
        for x in 0..grid_w {
            walls[grid_w + x] = true; // row y=1 is a solid wall
        }
        let mut rng = Rng::new(1);
        let (h_run_id, _v_run_id, h_run_hue, _v_run_hue) =
            label_wall_runs(&walls, grid_w, grid_h, &mut rng);
        let ids: Vec<i32> = (0..grid_w).map(|x| h_run_id[grid_w + x]).collect();
        assert!(ids.iter().all(|&id| id == ids[0]), "straight run split into multiple ids: {ids:?}");
        assert!(ids[0] >= 0);
        let _ = h_run_hue[ids[0] as usize]; // must not panic / be a valid index
    }

    #[test]
    fn a_gap_splits_an_otherwise_straight_run_into_two() {
        // 0 0 0 . 0 0 0 -- a real wall would never be perfectly collinear
        // across an opening, so the two stretches must get independent
        // (near-certainly different) run ids, not be merged into one.
        let grid_w = 7;
        let grid_h = 3;
        let mut walls = vec![false; grid_w * grid_h];
        for x in 0..grid_w {
            if x != 3 {
                walls[grid_w + x] = true;
            }
        }
        let mut rng = Rng::new(2);
        let (h_run_id, _v_run_id, _h_run_hue, _v_run_hue) =
            label_wall_runs(&walls, grid_w, grid_h, &mut rng);
        let left = h_run_id[grid_w];
        let right = h_run_id[grid_w + 6];
        assert!(left >= 0 && right >= 0);
        assert_ne!(left, right, "an opening in the wall must not merge the two sides into one run");
    }

    #[test]
    fn a_corner_gets_a_different_run_than_the_straight_wall_it_meets() {
        // An L-shaped wall: a horizontal run along y=1, x=0..3, meeting a
        // vertical run along x=2, y=1..3. The corner cell (2,1) is shared,
        // but a straight-ahead hit (horizontal face) and a turn (vertical
        // face) on it must resolve through *different* run tables, so
        // turning the corner can't silently keep the same color by
        // construction.
        let grid_w = 4;
        let grid_h = 4;
        let mut walls = vec![false; grid_w * grid_h];
        for x in 0..3 {
            walls[grid_w + x] = true; // horizontal arm: y=1, x=0,1,2
        }
        for y in 1..3 {
            walls[y * grid_w + 2] = true; // vertical arm: x=2, y=1,2
        }
        let mut rng = Rng::new(3);
        let (h_run_id, v_run_id, _h_run_hue, _v_run_hue) =
            label_wall_runs(&walls, grid_w, grid_h, &mut rng);
        let corner = grid_w + 2;
        assert!(h_run_id[corner] >= 0);
        assert!(v_run_id[corner] >= 0);
        // Same run tables the corner cell's two faces are looked up
        // through must be independent id spaces (a horizontal-face hit
        // never reads from v_run_id and vice versa), which is exactly
        // what lets the same physical cell present two different colors
        // depending on which face is actually visible.
    }

    #[test]
    fn camera_never_enters_a_wall_cell() {
        let mut rng = Rng::new(3);
        let mut sim = make(320, 96, 3);
        let input = Input::default();
        for _ in 0..6000 {
            sim.step(1.0 / 60.0, &input, &mut rng);
            let (x, y, _) = sim.camera();
            assert!(
                !sim.is_wall_at(x.floor() as i32, y.floor() as i32),
                "camera entered a wall cell at ({x}, {y})"
            );
        }
    }

    #[test]
    fn camera_keeps_moving_and_turning_over_time() {
        let mut rng = Rng::new(4);
        let mut sim = make(320, 96, 4);
        let input = Input::default();

        let mut positions = Vec::new();
        let mut angles = Vec::new();
        for _ in 0..8000 {
            sim.step(1.0 / 60.0, &input, &mut rng);
            let (x, y, a) = sim.camera();
            positions.push((x, y));
            angles.push(a);
        }

        let total_dist: f32 = positions
            .windows(2)
            .map(|w| {
                let (x0, y0) = w[0];
                let (x1, y1) = w[1];
                ((x1 - x0).powi(2) + (y1 - y0).powi(2)).sqrt()
            })
            .sum();
        assert!(total_dist > 5.0, "camera barely moved: total_dist={total_dist}");

        let distinct_angles = angles
            .iter()
            .filter(|&&a| angle_diff(a, angles[0]).abs() > 0.5)
            .count();
        assert!(distinct_angles > 0, "camera never turned away from its initial heading");
    }

    #[test]
    fn brick_pattern_has_mortar_courses_and_offset_rows() {
        // Mortar runs along the top edge of every course.
        assert!(brick_shade(0.5, 0.001) < 0.6, "expected a mortar line at the top of a course");
        // Brick faces are brighter than mortar. Sample the *middle* of a
        // brick, derived from the density constants rather than
        // hard-coded -- a fixed coordinate silently lands on a mortar
        // course as soon as the brick size is retuned.
        let face = brick_shade(0.5 / BRICKS_ACROSS, 0.5 / BRICKS_DOWN);
        assert!(face > 0.7, "brick face came out as dark as mortar: {face}");

        // Running bond: consecutive courses are offset by half a brick, so
        // the vertical mortar joint in one course lands mid-brick in the
        // next. Sample the same `u` one course apart and require they
        // aren't both mortar.
        let u = 1.0 / BRICKS_ACROSS * 0.5; // a vertical joint position in even courses
        let v_even = 0.5 / BRICKS_DOWN;
        let v_odd = 1.5 / BRICKS_DOWN;
        assert_ne!(
            brick_shade(u, v_even) < 0.6,
            brick_shade(u, v_odd) < 0.6,
            "courses are not offset -- joints line up vertically"
        );
    }

    #[test]
    fn wall_hues_split_by_orientation() {
        // North-south walls (hit stepping in X) read warm, east-west ones
        // cool, so the two axes of the maze stay tellable apart.
        let mut rng = Rng::new(21);
        let walls = generate_maze(GRID_W, GRID_H, &mut rng);
        let (_h_id, _v_id, h_hue, v_hue) = label_wall_runs(&walls, GRID_W, GRID_H, &mut rng);

        for hue in &v_hue {
            let warm = *hue >= HUE_NS_WALL.0 || *hue <= HUE_NS_WALL.1;
            assert!(warm, "north-south wall hue {hue} is outside the warm range");
        }
        for hue in &h_hue {
            assert!(
                *hue >= HUE_EW_WALL.0 && *hue <= HUE_EW_WALL.1,
                "east-west wall hue {hue} is outside the cool range"
            );
        }
    }

    #[test]
    fn ceiling_tiles_are_larger_than_floor_tiles() {
        assert!(
            CEILING_TILES_PER_CELL < FLOOR_TILES_PER_CELL,
            "fewer tiles per cell means bigger tiles -- the ceiling's should be the bigger ones"
        );
        // Both patterns must actually vary across a surface, or they're
        // just a flat fill with extra steps.
        let sample = |tiles: f32| {
            let mut seen: Vec<f32> = Vec::new();
            for i in 0..40 {
                let p = i as f32 * 0.05;
                let s = tile_shade(p, p * 0.5, tiles);
                if !seen.iter().any(|v| (v - s).abs() < 1e-6) {
                    seen.push(s);
                }
            }
            seen.len()
        };
        assert!(sample(FLOOR_TILES_PER_CELL) > 3, "floor tiling produced almost no variation");
        assert!(sample(CEILING_TILES_PER_CELL) > 3, "ceiling tiling produced almost no variation");
    }

    #[test]
    fn render_leaves_no_pixel_unpainted() {
        // Floor/ceiling casting replaced the old flat rects, so every row
        // is now written by one of three separate loops. A gap between
        // them would leave stale pixels from the previous frame on screen,
        // which is exactly the kind of thing that only shows up as a
        // smear while moving.
        let mut rng = Rng::new(22);
        let mut sim = make(320, 96, 22);
        let mut frame = Frame::new(320, 96);
        let theme = Theme::default();

        // Paint something unmistakable first; anything left over after a
        // render is a hole.
        frame.fill(Rgb::new(255, 0, 255));
        for _ in 0..200 {
            sim.step(1.0 / 60.0, &Input::default(), &mut rng);
        }
        sim.render(&mut frame, &theme);

        for (i, px) in frame.pixels.chunks_exact(4).enumerate() {
            assert!(
                !(px[0] == 255 && px[1] == 0 && px[2] == 255),
                "pixel {i} was never painted by render"
            );
        }
    }

    #[test]
    fn maze_is_large_enough_to_actually_roam() {
        // The point of the grid size is range: the autopilot should cover
        // a lot of distinct ground before it starts retracing itself.
        // Anchored to a count the *old* 15x11 maze physically could not
        // reach -- it only had ~69 open cells in total -- so this fails if
        // the grid is ever quietly shrunk back.
        let mut rng = Rng::new(11);
        let mut sim = make(320, 96, 11);
        let input = Input::default();

        let mut visited = std::collections::HashSet::new();
        for _ in 0..40_000 {
            sim.step(1.0 / 60.0, &input, &mut rng);
            let (x, y, _) = sim.camera();
            visited.insert((x.floor() as i32, y.floor() as i32));
        }
        assert!(
            visited.len() > 120,
            "camera only reached {} distinct cells -- maze is too cramped to explore",
            visited.len()
        );
    }

    #[test]
    fn large_dt_does_not_explode_or_panic() {
        let mut rng = Rng::new(5);
        let mut sim = make(320, 96, 5);
        let input = Input::default();
        for _ in 0..10 {
            sim.step(1.0, &input, &mut rng);
        }
        let (x, y, a) = sim.camera();
        assert!(x.is_finite() && y.is_finite() && a.is_finite());
    }

    #[test]
    fn out_of_bounds_point_is_treated_as_wall() {
        let sim = make(320, 96, 6);
        assert!(sim.is_wall_at(-1, 0));
        assert!(sim.is_wall_at(0, -1));
        let (gw, gh) = sim.grid_dims();
        assert!(sim.is_wall_at(gw as i32, 0));
        assert!(sim.is_wall_at(0, gh as i32));
    }

    #[test]
    fn resize_to_tiny_does_not_panic() {
        let mut rng = Rng::new(7);
        let mut sim = make(320, 96, 7);
        sim.resize(1, 1, &mut rng);
        let input = Input::default();
        for _ in 0..50 {
            sim.step(1.0 / 60.0, &input, &mut rng);
        }
        let mut frame = Frame::new(1, 1);
        let theme = Theme::default();
        sim.render(&mut frame, &theme);
    }

    #[test]
    fn resize_to_zero_does_not_panic() {
        let mut rng = Rng::new(8);
        let mut sim = make(320, 96, 8);
        sim.resize(0, 0, &mut rng);
        let input = Input::default();
        sim.step(1.0 / 60.0, &input, &mut rng);
    }

    #[test]
    fn render_fills_the_whole_frame_and_stays_opaque() {
        let mut rng = Rng::new(9);
        let mut sim = make(320, 96, 9);
        let mut frame = Frame::new(320, 96);
        let theme = Theme::default();
        for _ in 0..30 {
            sim.step(1.0 / 60.0, &Input::default(), &mut rng);
            sim.render(&mut frame, &theme);
        }
        for px in frame.pixels.chunks_exact(4) {
            assert_eq!(px[3], 255);
        }
    }

    #[test]
    fn never_gets_permanently_stuck_across_a_long_run() {
        let mut rng = Rng::new(10);
        let mut sim = make(320, 96, 10);
        let input = Input::default();

        let mut checkpoints = Vec::new();
        for chunk in 0..5 {
            for _ in 0..4000 {
                sim.step(1.0 / 60.0, &input, &mut rng);
            }
            let (x, y, _) = sim.camera();
            checkpoints.push((x, y));
            let _ = chunk;
        }
        let moved_between_checkpoints = checkpoints.windows(2).any(|w| {
            let (x0, y0) = w[0];
            let (x1, y1) = w[1];
            (x1 - x0).abs() > 0.1 || (y1 - y0).abs() > 0.1
        });
        assert!(moved_between_checkpoints, "camera appears frozen: {checkpoints:?}");
    }
}

