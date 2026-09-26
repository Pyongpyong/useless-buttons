//! Mole: whack-a-mole. Moles pop out of a grid of holes for a moment;
//! click one while it's up to whack it. Ten whacks in a row clear it;
//! let a single mole get away and it's game over. Moles come quicker, and
//! later two at a time, as you go.
use super::{
    draw_cleared_flash, draw_game_over, draw_progress, ring, sanitize_dt, unit, vgradient,
    Confetti, Phase, GOAL,
};
use crate::paint::{Frame, Rgb, Theme};
use crate::rng::Rng;
use crate::sims::{Input, Sim};

const ROW_Y: [f32; 2] = [0.46, 0.84];
const COLS_MIN: usize = 3;
const COLS_MAX: usize = 7;
/// Holes per unit of width.
const COLS_PER_UNIT: f32 = 1.4;
const HOLE_R: f32 = 0.12;
/// How far a fully raised mole's head sits above its hole.
const RISE: f32 = 0.13;
const HEAD_R: f32 = 0.085;
/// A press this close to a raised mole's head counts.
const HIT_R: f32 = 0.14;
const RISE_SEC: f32 = 0.12;
const UP_START: f32 = 1.1;
const UP_PER_WHACK: f32 = 0.05;
const UP_MIN: f32 = 0.6;
const GAP_MIN: f32 = 0.35;
const GAP_MAX: f32 = 0.75;
/// Whacks before a second mole can be up at once.
const TWO_AT_ONCE_FROM: u32 = 4;
const WHACKED_SEC: f32 = 0.35;

/// Filled axis-aligned ellipse; `lower_only` fills just the half below
/// the center line (the front rim of a hole).
fn ellipse(frame: &mut Frame, cx: f32, cy: f32, rx: f32, ry: f32, c: Rgb, lower_only: bool) {
    if rx <= 0.0 || ry <= 0.0 || !cx.is_finite() || !cy.is_finite() {
        return;
    }
    let y0 = if lower_only { cy } else { cy - ry };
    let (y0, y1) = (y0.floor().max(0.0) as usize, ((cy + ry).ceil().max(0.0) as usize).min(frame.h));
    for py in y0..y1 {
        let dy = (py as f32 + 0.5 - cy) / ry;
        if dy.abs() > 1.0 {
            continue;
        }
        let half = rx * (1.0 - dy * dy).sqrt();
        let x0 = (cx - half).round().max(0.0);
        let x1 = (cx + half).round().min(frame.w as f32);
        if x1 > x0 {
            frame.rect(x0, py as f32, x1 - x0, 1.0, c, 1.0);
        }
    }
}

const MOUND: Rgb = Rgb::new(120, 84, 52);
const OPENING: Rgb = Rgb::new(28, 18, 12);
/// Mound and opening ellipses, as multiples of `HOLE_R`.
const MOUND_RX: f32 = 1.3;
const MOUND_RY: f32 = 0.6;
const OPENING_RX: f32 = 1.0;
const OPENING_RY: f32 = 0.38;

struct Mole {
    hole: usize,
    /// Seconds since it started rising.
    t: f32,
    up_for: f32,
    /// Seconds since it was whacked, if it was.
    whacked: Option<f32>,
}

impl Mole {
    /// 0 = hidden, 1 = fully raised.
    fn raised(&self) -> f32 {
        if let Some(w) = self.whacked {
            return (1.0 - w / WHACKED_SEC).max(0.0);
        }
        let rise = (self.t / RISE_SEC).min(1.0);
        let sink = ((self.up_for + RISE_SEC - self.t) / RISE_SEC).clamp(0.0, 1.0);
        rise.min(sink)
    }

    fn gone(&self) -> bool {
        match self.whacked {
            Some(w) => w >= WHACKED_SEC,
            None => self.t >= self.up_for + RISE_SEC * 2.0,
        }
    }
}

pub struct MoleWhack {
    w: f32,
    h: f32,
    moles: Vec<Mole>,
    next_mole: f32,
    whacks: u32,
    phase: Phase,
    confetti: Confetti,
    next_burst: f32,
}

impl MoleWhack {
    pub fn new(w: usize, h: usize, _: &mut Rng) -> Self {
        let mut s = Self {
            w: w.max(1) as f32,
            h: h.max(1) as f32,
            moles: Vec::new(),
            next_mole: 0.5,
            whacks: 0,
            phase: Phase::Playing,
            confetti: Confetti::default(),
            next_burst: 0.0,
        };
        s.reset();
        s
    }

    fn reset(&mut self) {
        self.moles.clear();
        self.next_mole = 0.5;
        self.whacks = 0;
        self.phase = Phase::Playing;
        self.confetti.clear();
    }

    fn aspect(&self) -> f32 {
        self.w / self.h
    }

    fn cols(&self) -> usize {
        ((self.aspect() * COLS_PER_UNIT).round() as usize).clamp(COLS_MIN, COLS_MAX)
    }

    fn holes(&self) -> usize {
        self.cols() * ROW_Y.len()
    }

    /// Center of a hole's opening.
    fn hole_pos(&self, i: usize) -> (f32, f32) {
        let cols = self.cols();
        let (row, col) = (i / cols, i % cols);
        // Stagger the back row half a hole so the grid doesn't read as a table.
        let shift = if row == 0 { 0.25 } else { -0.25 };
        let cell = self.aspect() / cols as f32;
        (((col as f32 + 0.5 + shift) * cell).clamp(HOLE_R, (self.aspect() - HOLE_R).max(HOLE_R)), ROW_Y[row.min(ROW_Y.len() - 1)])
    }

    fn head_pos(&self, m: &Mole) -> (f32, f32) {
        let (x, y) = self.hole_pos(m.hole);
        (x, y - RISE * m.raised())
    }

    fn spawn(&mut self, rng: &mut Rng) {
        let taken: Vec<usize> = self.moles.iter().map(|m| m.hole).collect();
        let free: Vec<usize> = (0..self.holes()).filter(|h| !taken.contains(h)).collect();
        if free.is_empty() {
            return;
        }
        let up_for = (UP_START - UP_PER_WHACK * self.whacks as f32).max(UP_MIN);
        self.moles.push(Mole { hole: free[rng.range_usize(0, free.len())], t: 0.0, up_for, whacked: None });
    }

    fn step_playing(&mut self, dt: f32, input: &Input, rng: &mut Rng) {
        for p in input.presses().iter().filter(|p| p.is_pointed()) {
            let (px, py) = (p.x * self.aspect(), p.y);
            let hit = (0..self.moles.len()).find(|&i| {
                let m = &self.moles[i];
                let (hx, hy) = self.head_pos(m);
                m.whacked.is_none() && m.raised() > 0.3 && ((hx - px).powi(2) + (hy - py).powi(2)).sqrt() <= HIT_R
            });
            if let Some(i) = hit {
                self.moles[i].whacked = Some(0.0);
                self.whacks += 1;
                if self.whacks >= GOAL {
                    self.phase = Phase::Cleared(0.0);
                    let (x, y) = self.head_pos(&self.moles[i]);
                    self.confetti.burst(x, y, 90, rng);
                    self.next_burst = 0.5;
                    return;
                }
            }
        }
        for m in &mut self.moles {
            match &mut m.whacked {
                Some(w) => *w += dt,
                None => m.t += dt,
            }
        }
        // Every mole has to be whacked: one getting away ends the run.
        if self.moles.iter().any(|m| m.whacked.is_none() && m.gone()) {
            self.phase = Phase::Over(0.0);
            return;
        }
        self.moles.retain(|m| !m.gone());
        let max_up = if self.whacks >= TWO_AT_ONCE_FROM { 2 } else { 1 };
        let up = self.moles.iter().filter(|m| m.whacked.is_none()).count();
        self.next_mole -= dt;
        if self.next_mole <= 0.0 && up < max_up {
            self.spawn(rng);
            self.next_mole = rng.range_f32(GAP_MIN, GAP_MAX);
        }
    }

    fn draw_mole(&self, frame: &mut Frame, m: &Mole) {
        let u = unit(frame);
        let (x, y) = self.head_pos(m);
        let (hx, hy) = self.hole_pos(m.hole);
        let (x, y, r) = (x * u, y * u, HEAD_R * u);
        if m.raised() < 0.05 {
            return;
        }
        let fur = Rgb::new(140, 98, 66);
        // Body from the head down into the hole; the rim drawn afterwards
        // hides whatever is below the opening.
        frame.rect(x - r, y, r * 2.0, (hy * u - y).max(0.0), fur, 1.0);
        frame.disc(x, y, r, fur, 1.0);
        frame.disc(x, y + r * 0.35, r * 0.55, Rgb::new(200, 160, 120), 1.0);
        frame.disc(x, y + r * 0.2, r * 0.18, Rgb::new(230, 110, 130), 1.0);
        if m.whacked.is_some() {
            // Dizzy crossed-out eyes.
            for dx in [-0.4, 0.4] {
                let (ex, ey, s) = (x + dx * r, y - r * 0.2, r * 0.16);
                frame.line((ex - s, ey - s), (ex + s, ey + s), (u * 0.008).max(1.0), Rgb::new(20, 20, 20), 1.0);
                frame.line((ex - s, ey + s), (ex + s, ey - s), (u * 0.008).max(1.0), Rgb::new(20, 20, 20), 1.0);
            }
        } else {
            for dx in [-0.4, 0.4] {
                frame.disc(x + dx * r, y - r * 0.2, (r * 0.13).max(0.8), Rgb::new(20, 20, 20), 1.0);
            }
        }
        // The mound's front rim hides the mole's lower half.
        let hr = HOLE_R * u;
        ellipse(frame, hx * u, hy * u, hr * MOUND_RX, hr * MOUND_RY, MOUND, true);
    }
}

impl Sim for MoleWhack {
    fn resize(&mut self, w: usize, h: usize, _: &mut Rng) {
        self.w = w.max(1) as f32;
        self.h = h.max(1) as f32;
        // A different column count can put moles in holes that no longer exist.
        let holes = self.holes();
        self.moles.retain(|m| m.hole < holes);
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
                for m in &mut self.moles {
                    if let Some(w) = &mut m.whacked {
                        *w += dt;
                    } else {
                        m.whacked = Some(0.0);
                    }
                }
                self.moles.retain(|m| !m.gone());
                for p in input.presses().iter().filter(|p| p.is_pointed()) {
                    self.confetti.burst(p.x * self.aspect(), p.y, 30, rng);
                }
                if t < 3.0 && t >= self.next_burst {
                    self.next_burst += 0.6;
                    self.confetti.burst(rng.range_f32(0.2, self.aspect() - 0.2), 0.6, 30, rng);
                }
            }
        }
    }

    fn render(&mut self, frame: &mut Frame, _: &Theme) {
        let u = unit(frame);
        vgradient(frame, Rgb::new(120, 200, 90), Rgb::new(90, 165, 65));
        // Grass tufts.
        for i in 0..30u32 {
            let hx = (i.wrapping_mul(2654435761) >> 8) as f32 / 16_777_216.0;
            let hy = (i.wrapping_mul(2246822519) >> 8) as f32 / 16_777_216.0;
            let (x, y) = (hx * frame.w as f32, (0.2 + hy * 0.8) * frame.h as f32);
            frame.line((x, y), (x - 0.01 * u, y - 0.03 * u), 1.0, Rgb::new(60, 130, 50), 0.8);
            frame.line((x, y), (x + 0.01 * u, y - 0.03 * u), 1.0, Rgb::new(60, 130, 50), 0.8);
        }
        for i in 0..self.holes() {
            let (x, y) = self.hole_pos(i);
            let r = HOLE_R * u;
            ellipse(frame, x * u, y * u + r * 0.08, r * MOUND_RX, r * MOUND_RY, MOUND, false);
            ellipse(frame, x * u, y * u, r * OPENING_RX, r * OPENING_RY, OPENING, false);
        }
        for m in &self.moles {
            self.draw_mole(frame, m);
            if let Some(w) = m.whacked {
                let k = w / WHACKED_SEC;
                let (hx, hy) = self.hole_pos(m.hole);
                ring(frame, hx * u, (hy - RISE) * u, (HEAD_R + k * 0.1) * u, (u * 0.02).max(1.0), Rgb::new(255, 240, 120), 1.0 - k);
            }
        }
        self.confetti.render(frame);
        draw_progress(frame, self.whacks);
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

    /// Whacks the first mole that's up, after a human-ish 0.25 s reaction.
    fn bot_input(sim: &MoleWhack, since: f32) -> Option<Input> {
        if since < 0.12 {
            return None;
        }
        let m = sim.moles.iter().find(|m| m.whacked.is_none() && m.t > 0.25 && m.raised() > 0.5)?;
        let (x, y) = sim.head_pos(m);
        Some(Input::press(x / sim.aspect(), y))
    }

    #[test]
    fn holes_sit_inside_the_frame() {
        let mut rng = Rng::new(1);
        for (w, h) in [(320, 96), (520, 144), (96, 96), (1200, 96)] {
            let sim = MoleWhack::new(w, h, &mut rng);
            assert!((COLS_MIN..=COLS_MAX).contains(&sim.cols()));
            for i in 0..sim.holes() {
                let (x, y) = sim.hole_pos(i);
                assert!(x >= HOLE_R - 1e-4 && x <= sim.aspect() - HOLE_R + 1e-4 && y < 1.0, "{w}x{h} hole {i}");
            }
        }
    }

    #[test]
    fn clicking_an_empty_hole_or_a_sinking_mole_does_nothing() {
        let mut rng = Rng::new(2);
        let mut sim = MoleWhack::new(320, 96, &mut rng);
        sim.moles.push(Mole { hole: 0, t: 0.0, up_for: 1.0, whacked: None });
        let (x, y) = sim.hole_pos(3);
        sim.step(DT, &Input::press(x / sim.aspect(), y), &mut rng);
        assert_eq!(sim.whacks, 0);
    }

    #[test]
    fn an_escape_ends_the_run_and_it_restarts_without_ever_clearing() {
        let mut rng = Rng::new(3);
        let mut sim = MoleWhack::new(320, 96, &mut rng);
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
        assert!(overs >= 5, "only {overs} game overs");
    }

    #[test]
    fn one_miss_after_a_streak_starts_over_from_zero() {
        let mut rng = Rng::new(4);
        let mut sim = MoleWhack::new(320, 96, &mut rng);
        let mut since = 1.0;
        while sim.whacks < 5 {
            let input = bot_input(&sim, since);
            since = if input.is_some() { 0.0 } else { since + DT };
            sim.step(DT, &input.unwrap_or_default(), &mut rng);
        }
        while !matches!(sim.phase, Phase::Over(_)) {
            sim.step(DT, &Input::default(), &mut rng);
        }
        assert_eq!(sim.whacks, 5);
        while matches!(sim.phase, Phase::Over(_)) {
            sim.step(DT, &Input::default(), &mut rng);
        }
        assert_eq!(sim.whacks, 0);
    }

    #[test]
    fn a_quick_hand_whacks_ten() {
        for seed in 1..11 {
            let mut rng = Rng::new(seed);
            let mut sim = MoleWhack::new(320, 96, &mut rng);
            let mut since = 1.0;
            for _ in 0..60 * 60 {
                let input = bot_input(&sim, since);
                since = if input.is_some() { 0.0 } else { since + DT };
                sim.step(DT, &input.unwrap_or_default(), &mut rng);
                assert!(!matches!(sim.phase, Phase::Over(_)), "seed {seed} lost at {} whacks", sim.whacks);
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
        let mut sim = MoleWhack::new(320, 96, &mut rng);
        for (w, h) in [(321, 97), (1, 1), (0, 0), (4, 7)] {
            sim.resize(w, h, &mut rng);
            for dt in [f32::NAN, -1.0, 1000.0, DT] {
                sim.spawn(&mut rng);
                sim.step(dt, &Input::press(0.5, 0.5), &mut rng);
            }
            let mut frame = Frame::new(w, h);
            sim.render(&mut frame, &Theme::default());
            assert!(frame.pixels.chunks_exact(4).all(|p| p[3] == 255));
        }
    }
}
