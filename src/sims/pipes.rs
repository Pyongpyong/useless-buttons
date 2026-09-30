//! `pipes` — the old 3D Pipes screensaver. Shiny pipes grow cell by cell
//! across a grid, turning now and then and leaving a ball joint at each
//! bend; a pipe that boxes itself in stops and a new one starts somewhere
//! free in a new color. When the screen is mostly full it's wiped and the
//! plumbing starts over. Pipes are shaded across their width like
//! cylinders so they read as 3D.
use super::{Input, Sim};
use crate::paint::{Frame, Rgb, Theme};
use crate::rng::Rng;

/// Grid rows across the frame's height.
const ROWS: usize = 6;
const PIPES: usize = 2;
/// Seconds per cell of growth.
const GROW_SEC: f32 = 0.18;
const TURN_P: f32 = 0.3;
/// Wipe and start over once this fraction of cells is taken.
const FULL: f32 = 0.7;
const PIPE_R: f32 = 0.26;
const BACKGROUND: Rgb = Rgb::new(4, 4, 10);
const DIRS: [(i32, i32); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];

struct Pipe {
    x: i32,
    y: i32,
    dir: usize,
    hue: f32,
    alive: bool,
}

pub struct Pipes {
    cols: usize,
    rows: usize,
    cell: f32,
    taken: Vec<bool>,
    canvas: Frame,
    pipes: Vec<Pipe>,
    acc: f32,
}

impl Pipes {
    pub fn new(w: usize, h: usize, rng: &mut Rng) -> Self {
        let mut s = Self { cols: 1, rows: 1, cell: 1.0, taken: Vec::new(), canvas: Frame::new(1, 1), pipes: Vec::new(), acc: 0.0 };
        s.resize(w, h, rng);
        s
    }

    fn free(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && (x as usize) < self.cols && (y as usize) < self.rows && !self.taken[y as usize * self.cols + x as usize]
    }

    fn center(&self, x: i32, y: i32) -> (f32, f32) {
        ((x as f32 + 0.5) * self.cell, (y as f32 + 0.5) * self.cell)
    }

    fn wipe(&mut self, rng: &mut Rng) {
        self.taken.iter_mut().for_each(|t| *t = false);
        self.canvas.fill(BACKGROUND);
        self.pipes.clear();
        for _ in 0..PIPES {
            let p = self.start_pipe(rng);
            self.pipes.push(p);
        }
    }

    fn start_pipe(&mut self, rng: &mut Rng) -> Pipe {
        let free: Vec<usize> = (0..self.taken.len()).filter(|&i| !self.taken[i]).collect();
        let Some(&i) = free.get(rng.range_usize(0, free.len().max(1))) else {
            return Pipe { x: 0, y: 0, dir: 0, hue: 0.0, alive: false };
        };
        self.taken[i] = true;
        let (x, y) = ((i % self.cols) as i32, (i / self.cols) as i32);
        let hue = rng.range_f32(0.0, 360.0);
        let (cx, cy) = self.center(x, y);
        joint(&mut self.canvas, cx, cy, self.cell * PIPE_R * 1.35, hue);
        Pipe { x, y, dir: rng.range_usize(0, 4), hue, alive: true }
    }

    fn grow(&mut self, rng: &mut Rng) {
        for i in 0..self.pipes.len() {
            let p = &self.pipes[i];
            if !p.alive {
                continue;
            }
            let (x, y, dir) = (p.x, p.y, p.dir);
            let open: Vec<usize> = (0..4).filter(|&d| self.free(x + DIRS[d].0, y + DIRS[d].1)).collect();
            if open.is_empty() {
                self.pipes[i] = self.start_pipe(rng);
                continue;
            }
            let straight = open.contains(&dir) && !rng.bool_p(TURN_P);
            let nd = if straight { dir } else { open[rng.range_usize(0, open.len())] };
            let (nx, ny) = (x + DIRS[nd].0, y + DIRS[nd].1);
            self.taken[ny as usize * self.cols + nx as usize] = true;
            let hue = self.pipes[i].hue;
            let (a, b) = (self.center(x, y), self.center(nx, ny));
            tube(&mut self.canvas, a, b, self.cell * PIPE_R, hue);
            if nd != dir {
                joint(&mut self.canvas, a.0, a.1, self.cell * PIPE_R * 1.35, hue);
            }
            self.pipes[i] = Pipe { x: nx, y: ny, dir: nd, hue, alive: true };
        }
    }
}

/// A straight run of pipe, shaded across its width like a lit cylinder.
fn tube(frame: &mut Frame, a: (f32, f32), b: (f32, f32), r: f32, hue: f32) {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = dx.hypot(dy).max(1e-3);
    let (nx, ny) = (-dy / len, dx / len);
    let steps = (r * 2.0).ceil().max(2.0) as i32;
    for s in 0..=steps {
        let o = -r + 2.0 * r * s as f32 / steps as f32;
        let k = o / r;
        // Lit from the upper left: brightest a third of the way across.
        let lit = (1.0 - (k + 0.35).powi(2)).max(0.0).sqrt();
        let c = Rgb::from_hsv(hue, 0.75 - 0.5 * lit.powi(6), 0.18 + 0.82 * lit);
        frame.line((a.0 + nx * o, a.1 + ny * o), (b.0 + nx * o, b.1 + ny * o), 1.5, c, 1.0);
    }
}

/// A ball joint: concentric discs, dark rim to a highlight up and left.
fn joint(frame: &mut Frame, x: f32, y: f32, r: f32, hue: f32) {
    let rings = (r.ceil() as i32).max(2);
    for i in 0..rings {
        let k = i as f32 / rings as f32;
        let rr = r * (1.0 - k);
        let off = r * 0.35 * k;
        let c = Rgb::from_hsv(hue, 0.75 - 0.55 * k.powi(3), 0.2 + 0.8 * k);
        frame.disc(x - off, y - off, rr, c, 1.0);
    }
}

impl Sim for Pipes {
    fn resize(&mut self, w: usize, h: usize, rng: &mut Rng) {
        let (w, h) = (w.max(1), h.max(1));
        self.cell = (h as f32 / ROWS as f32).max(1.0);
        self.cols = ((w as f32 / self.cell).ceil() as usize).max(1);
        self.rows = ROWS;
        self.taken = vec![false; self.cols * self.rows];
        self.canvas = Frame::new(w, h);
        self.wipe(rng);
    }

    fn step(&mut self, dt: f32, _: &Input, rng: &mut Rng) {
        let dt = if dt.is_finite() { dt.clamp(0.0, 0.1) } else { 0.0 };
        self.acc += dt;
        while self.acc >= GROW_SEC {
            self.acc -= GROW_SEC;
            self.grow(rng);
            let filled = self.taken.iter().filter(|&&t| t).count() as f32 / self.taken.len() as f32;
            if filled >= FULL {
                self.wipe(rng);
            }
        }
    }

    fn render(&mut self, frame: &mut Frame, _: &Theme) {
        if frame.pixels.len() == self.canvas.pixels.len() {
            frame.pixels.copy_from_slice(&self.canvas.pixels);
        } else {
            frame.fill(BACKGROUND);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipes_grow_without_crossing_and_the_screen_wipes_when_full() {
        let mut rng = Rng::new(1);
        let mut sim = Pipes::new(320, 96, &mut rng);
        let mut wiped = false;
        let mut last = sim.taken.iter().filter(|&&t| t).count();
        for _ in 0..60 * 30 {
            sim.step(1.0 / 60.0, &Input::default(), &mut rng);
            let now = sim.taken.iter().filter(|&&t| t).count();
            if now < last {
                wiped = true;
            }
            last = now;
            assert!((now as f32) < sim.taken.len() as f32 * FULL + PIPES as f32 * 2.0);
        }
        assert!(wiped, "never filled up and started over");
    }

    #[test]
    fn renders_opaque_changing_frames_at_odd_and_tiny_sizes() {
        let mut rng = Rng::new(2);
        let mut sim = Pipes::new(320, 96, &mut rng);
        for (w, h) in [(321, 97), (1, 1), (0, 0), (4, 7)] {
            sim.resize(w, h, &mut rng);
            for dt in [f32::NAN, -1.0, 1000.0, 0.1] {
                sim.step(dt, &Input::default(), &mut rng);
            }
            let mut frame = Frame::new(w, h);
            sim.render(&mut frame, &Theme::default());
            assert!(frame.pixels.chunks_exact(4).all(|p| p[3] == 255));
        }
        sim.resize(64, 48, &mut rng);
        let mut frame = Frame::new(64, 48);
        sim.render(&mut frame, &Theme::default());
        let before = frame.pixels.clone();
        // A pipe grows one cell every GROW_SEC.
        for _ in 0..2 {
            sim.step(0.1, &Input::default(), &mut rng);
        }
        sim.render(&mut frame, &Theme::default());
        assert_ne!(before, frame.pixels);
    }
}
