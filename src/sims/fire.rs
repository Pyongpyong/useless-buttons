//! `fire` — the classic demoscene pixel fire. A heat grid is fed from the
//! bottom row; every step each cell takes the heat of the cell below it,
//! loses a random bit, and drifts a random step sideways, so flames lick
//! upward and die out. Heat maps through black, red, orange and yellow to
//! white. Every so often the base flares and the flames leap higher.
use super::{Input, Sim};
use crate::paint::{Frame, Rgb, Theme};
use crate::rng::Rng;

/// Heat levels, 0 (cold) to `MAX_HEAT` (white hot).
const MAX_HEAT: u8 = 36;
/// Fixed simulation rate, independent of the render rate.
const STEP_DT: f32 = 1.0 / 30.0;
const MAX_STEPS: u32 = 4;
/// Grid rows the flames are tall, roughly; cells are sized so the fire
/// fills the frame whatever its height.
const ROWS: usize = 48;
const FLARE_MIN_SEC: f32 = 1.5;
const FLARE_MAX_SEC: f32 = 4.0;
const FLARE_SEC: f32 = 0.6;

pub struct Fire {
    w: usize,
    h: usize,
    cell: usize,
    cols: usize,
    rows: usize,
    heat: Vec<u8>,
    palette: [Rgb; MAX_HEAT as usize + 1],
    acc: f32,
    next_flare: f32,
    flare: f32,
}

fn palette() -> [Rgb; MAX_HEAT as usize + 1] {
    // Black -> deep red -> orange -> yellow -> white.
    let stops = [
        (0.0, Rgb::new(8, 4, 4)),
        (0.25, Rgb::new(120, 16, 8)),
        (0.5, Rgb::new(230, 70, 10)),
        (0.75, Rgb::new(255, 180, 40)),
        (1.0, Rgb::new(255, 250, 220)),
    ];
    std::array::from_fn(|i| {
        let t = i as f32 / MAX_HEAT as f32;
        let k = stops
            .windows(2)
            .position(|w| t <= w[1].0)
            .unwrap_or(stops.len() - 2);
        let (a, b) = (stops[k], stops[k + 1]);
        a.1.lerp(b.1, (t - a.0) / (b.0 - a.0))
    })
}

impl Fire {
    pub fn new(w: usize, h: usize, rng: &mut Rng) -> Self {
        let mut s = Self {
            w: 1,
            h: 1,
            cell: 1,
            cols: 1,
            rows: 1,
            heat: Vec::new(),
            palette: palette(),
            acc: 0.0,
            next_flare: rng.range_f32(FLARE_MIN_SEC, FLARE_MAX_SEC),
            flare: 0.0,
        };
        s.resize(w, h, rng);
        s
    }

    fn idx(&self, x: usize, y: usize) -> usize {
        y * self.cols + x
    }

    fn spread(&mut self, rng: &mut Rng) {
        let (cols, rows) = (self.cols, self.rows);
        // The source row: hotter while flaring, with a little flicker.
        let base = if self.flare > 0.0 {
            MAX_HEAT
        } else {
            MAX_HEAT - 4
        };
        for x in 0..cols {
            let i = self.idx(x, rows - 1);
            self.heat[i] = base.saturating_sub(rng.range_usize(0, 4) as u8);
        }
        // Each cell pulls from the one below, cooling and drifting sideways.
        for y in 0..rows - 1 {
            for x in 0..cols {
                let below = self.heat[self.idx(x, y + 1)];
                let decay = rng.range_usize(0, 3) as u8;
                let drift = rng.range_i32(-1, 2);
                let tx = (x as i32 + drift).clamp(0, cols as i32 - 1) as usize;
                let i = self.idx(tx, y);
                self.heat[i] = below.saturating_sub(decay);
            }
        }
    }
}

impl Sim for Fire {
    fn resize(&mut self, w: usize, h: usize, _: &mut Rng) {
        self.w = w.max(1);
        self.h = h.max(1);
        self.cell = (self.h / ROWS).max(2);
        self.cols = self.w.div_ceil(self.cell).max(1);
        self.rows = self.h.div_ceil(self.cell).max(2);
        self.heat = vec![0; self.cols * self.rows];
    }

    fn step(&mut self, dt: f32, _: &Input, rng: &mut Rng) {
        let dt = if dt.is_finite() {
            dt.clamp(0.0, 0.5)
        } else {
            0.0
        };
        self.next_flare -= dt;
        if self.next_flare <= 0.0 {
            self.flare = FLARE_SEC;
            self.next_flare = rng.range_f32(FLARE_MIN_SEC, FLARE_MAX_SEC);
        }
        self.flare = (self.flare - dt).max(0.0);
        self.acc += dt;
        let mut steps = 0;
        while self.acc >= STEP_DT && steps < MAX_STEPS {
            self.spread(rng);
            self.acc -= STEP_DT;
            steps += 1;
        }
        self.acc = self.acc.min(STEP_DT * MAX_STEPS as f32);
    }

    fn render(&mut self, frame: &mut Frame, _: &Theme) {
        let cell = self.cell;
        for y in 0..self.rows {
            for x in 0..self.cols {
                let c = self.palette[self.heat[self.idx(x, y)].min(MAX_HEAT) as usize];
                let (px0, py0) = (x * cell, y * cell);
                for py in py0..(py0 + cell).min(frame.h) {
                    let row = py * frame.w;
                    for px in px0..(px0 + cell).min(frame.w) {
                        let i = (row + px) * 4;
                        frame.pixels[i] = c.r;
                        frame.pixels[i + 1] = c.g;
                        frame.pixels[i + 2] = c.b;
                        frame.pixels[i + 3] = 255;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_runs_from_near_black_to_near_white() {
        let p = palette();
        let lum = |c: Rgb| c.r as u32 + c.g as u32 + c.b as u32;
        assert!(lum(p[0]) < 30);
        assert!(lum(p[MAX_HEAT as usize]) > 700);
        assert!(
            p.windows(2).all(|w| lum(w[1]) + 2 >= lum(w[0])),
            "palette should brighten with heat"
        );
    }

    #[test]
    fn flames_rise_hot_at_the_bottom_and_cool_toward_the_top() {
        let mut rng = Rng::new(1);
        let mut sim = Fire::new(320, 96, &mut rng);
        for _ in 0..120 {
            sim.step(1.0 / 60.0, &Input::default(), &mut rng);
        }
        let row_avg = |y: usize| {
            (0..sim.cols)
                .map(|x| sim.heat[sim.idx(x, y)] as u32)
                .sum::<u32>()
                / sim.cols as u32
        };
        assert!(row_avg(sim.rows - 2) > 25);
        assert!(row_avg(sim.rows / 2) > 0, "flames never rose");
        assert!(row_avg(0) < row_avg(sim.rows - 2));
    }

    #[test]
    fn renders_opaque_changing_frames_at_odd_and_tiny_sizes() {
        let mut rng = Rng::new(2);
        let mut sim = Fire::new(320, 96, &mut rng);
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
        sim.step(0.1, &Input::default(), &mut rng);
        sim.render(&mut frame, &Theme::default());
        let before = frame.pixels.clone();
        sim.step(0.1, &Input::default(), &mut rng);
        sim.render(&mut frame, &Theme::default());
        assert_ne!(before, frame.pixels);
    }
}
