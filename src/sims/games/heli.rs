//! Heli: the helicopter-in-a-cave game. Hold the button to climb, let go
//! to sink. The cave winds up and down and narrows as you go; brushing a
//! wall is game over. Fly through ten checkpoint gates to clear.
use super::{
    draw_cleared_flash, draw_game_over, draw_progress, sanitize_dt, substeps, unit, vgradient,
    Confetti, Phase, GOAL,
};
use crate::paint::{Frame, Rgb, Theme};
use crate::rng::Rng;
use crate::sims::{Input, Sim};

const HELI_SX: f32 = 0.6;
const HELI_W: f32 = 0.16;
const HELI_H: f32 = 0.07;
const SPEED: f32 = 1.1;
const GRAVITY: f32 = 1.6;
/// Upward push while held; net of gravity it climbs at the same rate it falls.
const THRUST: f32 = 3.2;
const MAX_VY: f32 = 0.6;
/// Air drag on vertical speed, per second, so momentum from a press fades
/// quickly instead of carrying the helicopter into a wall: a quick tap
/// nudges it, only a long hold makes it climb hard.
const DRAG: f32 = 2.0;
const GATE_GAP: f32 = 2.6;
const FIRST_GATE: f32 = 2.4;
const GAP_START: f32 = 0.62;
const GAP_END: f32 = 0.44;
/// Nothing above this line: it's where the progress pips live.
const CEILING_MIN: f32 = 0.13;
const FLOOR_MAX: f32 = 0.98;
/// Hover in place this long after a (re)start; holding starts sooner.
const READY_SEC: f32 = 0.8;

pub struct Heli {
    w: f32,
    h: f32,
    /// Distance flown, in units.
    dist: f32,
    y: f32,
    vy: f32,
    run_t: f32,
    started: bool,
    phases: (f32, f32),
    passed: u32,
    /// Crash sparks: age since the crash.
    crash: Option<f32>,
    phase: Phase,
    confetti: Confetti,
    next_burst: f32,
}

impl Heli {
    pub fn new(w: usize, h: usize, rng: &mut Rng) -> Self {
        let mut s = Self {
            w: w.max(1) as f32,
            h: h.max(1) as f32,
            dist: 0.0,
            y: 0.55,
            vy: 0.0,
            run_t: 0.0,
            started: false,
            phases: (0.0, 0.0),
            passed: 0,
            crash: None,
            phase: Phase::Playing,
            confetti: Confetti::default(),
            next_burst: 0.0,
        };
        s.reset(rng);
        s
    }

    fn reset(&mut self, rng: &mut Rng) {
        self.dist = 0.0;
        self.vy = 0.0;
        self.run_t = 0.0;
        self.started = false;
        self.phases = (rng.range_f32(0.0, 6.3), rng.range_f32(0.0, 6.3));
        self.passed = 0;
        self.crash = None;
        self.phase = Phase::Playing;
        self.confetti.clear();
        self.y = self.center(HELI_SX);
    }

    fn aspect(&self) -> f32 {
        self.w / self.h
    }

    fn finish(&self) -> f32 {
        FIRST_GATE + GATE_GAP * (GOAL - 1) as f32
    }

    fn gap(&self, wx: f32) -> f32 {
        let k = (wx / self.finish()).clamp(0.0, 1.0);
        GAP_START + (GAP_END - GAP_START) * k
    }

    /// Middle of the cave at world x. Flat at the very start so the first
    /// second is fair, then winding.
    fn center(&self, wx: f32) -> f32 {
        let ease = (wx / 1.5).clamp(0.0, 1.0);
        let wave = (wx * 0.9 + self.phases.0).sin() * 0.16 + (wx * 2.1 + self.phases.1).sin() * 0.06;
        let mid = (CEILING_MIN + FLOOR_MAX) * 0.5;
        let half = self.gap(wx) * 0.5;
        (mid + wave * ease).clamp(CEILING_MIN + half, FLOOR_MAX - half)
    }

    fn walls(&self, wx: f32) -> (f32, f32) {
        let (c, half) = (self.center(wx), self.gap(wx) * 0.5);
        (c - half, c + half)
    }

    fn crashed(&self) -> bool {
        let x = self.dist + HELI_SX;
        [-0.5, 0.0, 0.5].iter().any(|&k| {
            let (top, bottom) = self.walls(x + k * HELI_W);
            self.y - HELI_H * 0.5 < top || self.y + HELI_H * 0.5 > bottom
        })
    }

    fn step_playing(&mut self, dt: f32, input: &Input, rng: &mut Rng) {
        let (n, h) = substeps(dt);
        for _ in 0..n {
            self.run_t += h;
            if !self.started && (input.held || self.run_t >= READY_SEC) {
                self.started = true;
            }
            if !self.started {
                self.y = self.center(HELI_SX) + (self.run_t * 6.0).sin() * 0.02;
                continue;
            }
            self.dist += SPEED * h;
            let accel = if input.held { GRAVITY - THRUST } else { GRAVITY };
            self.vy = ((self.vy + accel * h) * (-DRAG * h).exp()).clamp(-MAX_VY, MAX_VY);
            self.y += self.vy * h;
            if self.crashed() {
                self.crash = Some(0.0);
                self.phase = Phase::Over(0.0);
                return;
            }
            let x = self.dist + HELI_SX;
            let gates = if x < FIRST_GATE { 0 } else { (((x - FIRST_GATE) / GATE_GAP) as u32 + 1).min(GOAL) };
            self.passed = gates;
            if gates >= GOAL {
                self.phase = Phase::Cleared(0.0);
                self.confetti.burst(HELI_SX, self.y, 90, rng);
                self.next_burst = 0.5;
                return;
            }
        }
    }
}

impl Sim for Heli {
    fn resize(&mut self, w: usize, h: usize, _: &mut Rng) {
        self.w = w.max(1) as f32;
        self.h = h.max(1) as f32;
    }

    fn step(&mut self, dt: f32, input: &Input, rng: &mut Rng) {
        let dt = sanitize_dt(dt);
        self.confetti.step(dt);
        if let Some(c) = &mut self.crash {
            *c += dt;
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
                // Fly on through the open cave, steadied on its center line.
                self.dist += SPEED * dt;
                self.run_t += dt;
                let target = self.center(self.dist + HELI_SX);
                self.y += (target - self.y) * (1.0 - (-dt * 3.0).exp());
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
        vgradient(frame, Rgb::new(20, 30, 60), Rgb::new(40, 60, 100));
        let rock = Rgb::new(96, 70, 50);
        let edge = Rgb::new(150, 110, 70);
        for px in 0..frame.w {
            let wx = self.dist + (px as f32 + 0.5) / u;
            let (top, bottom) = self.walls(wx);
            // Rock texture: bands that scroll with the cave.
            let band = ((wx * 7.0).sin() * 0.5 + 0.5) * 0.15;
            let c = rock.lerp(Rgb::new(60, 45, 35), band);
            frame.rect(px as f32, 0.0, 1.0, top * u, c, 1.0);
            frame.rect(px as f32, bottom * u, 1.0, (1.0 - bottom) * u + 1.0, c, 1.0);
            frame.rect(px as f32, top * u - 2.0, 1.0, 2.0, edge, 1.0);
            frame.rect(px as f32, bottom * u, 1.0, 2.0, edge, 1.0);
        }
        // Checkpoint gates: glowing beams across the cave.
        for g in 0..GOAL {
            let gx = FIRST_GATE + GATE_GAP * g as f32;
            let sx = gx - self.dist;
            if sx < -0.1 || sx > self.aspect() + 0.1 {
                continue;
            }
            let (top, bottom) = self.walls(gx);
            let passed = g < self.passed;
            let c = if passed { Rgb::new(120, 255, 150) } else { Rgb::new(255, 220, 90) };
            frame.rect(sx * u - 1.0, top * u, 2.0, (bottom - top) * u, c, if passed { 0.35 } else { 0.7 });
            frame.disc(sx * u, top * u, (0.02 * u).max(1.5), c, 1.0);
            frame.disc(sx * u, bottom * u, (0.02 * u).max(1.5), c, 1.0);
        }

        // The helicopter: bubble cabin, tail boom, skids and a blurred rotor.
        let (x, y) = (HELI_SX * u, self.y * u);
        let body = Rgb::new(240, 70, 60);
        let tilt = (self.vy * 0.05).clamp(-0.03, 0.03) * u;
        frame.rect(x - HELI_W * 0.55 * u, y - 0.012 * u - tilt, HELI_W * 0.5 * u, 0.02 * u, body, 1.0);
        frame.rect(x - HELI_W * 0.6 * u, y - 0.03 * u - tilt, 0.02 * u, 0.04 * u, body, 1.0);
        frame.disc(x + 0.01 * u, y, HELI_H * 0.5 * u, body, 1.0);
        frame.disc(x + 0.025 * u, y - 0.008 * u, HELI_H * 0.25 * u, Rgb::new(170, 220, 255), 1.0);
        frame.rect(x - 0.04 * u, y + HELI_H * 0.5 * u, 0.1 * u, (0.008 * u).max(1.0), Rgb::new(60, 60, 60), 1.0);
        let spin = (self.run_t * 40.0).sin().abs();
        let rotor = HELI_W * 0.5 * (0.4 + 0.6 * spin);
        frame.rect(x + 0.01 * u - rotor * u, y - HELI_H * 0.62 * u, rotor * 2.0 * u, (0.008 * u).max(1.0), Rgb::new(220, 220, 220), 0.9);
        if let Some(age) = self.crash {
            for i in 0..8 {
                let a = i as f32 * 0.8 + age * 2.0;
                let d = (0.03 + age * 0.15) * u;
                frame.disc(x + a.cos() * d, y + a.sin() * d, (0.02 * u).max(1.0), Rgb::new(255, 200, 80), (1.0 - age).max(0.0));
            }
        }
        self.confetti.render(frame);
        draw_progress(frame, self.passed);
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

    /// Holds whenever it's below (or sinking toward) the cave's center a
    /// little way ahead.
    fn bot_input(sim: &Heli) -> Input {
        let target = sim.center(sim.dist + HELI_SX + 0.25);
        if sim.y - target + sim.vy * 0.25 > 0.0 {
            Input::hold()
        } else {
            Input::default()
        }
    }

    #[test]
    fn holding_climbs_and_letting_go_sinks() {
        let mut rng = Rng::new(1);
        let mut sim = Heli::new(320, 96, &mut rng);
        sim.step(DT, &Input::hold(), &mut rng);
        let y0 = sim.y;
        for _ in 0..10 {
            sim.step(DT, &Input::hold(), &mut rng);
        }
        assert!(sim.y < y0);
        // Upward momentum carries it a moment after letting go, then it sinks.
        let y1 = sim.y;
        for _ in 0..40 {
            sim.step(DT, &Input::default(), &mut rng);
        }
        assert!(sim.y > y1);
    }

    #[test]
    fn a_single_tap_only_nudges_it_up() {
        let mut rng = Rng::new(6);
        let mut sim = Heli::new(320, 96, &mut rng);
        let y0 = sim.y;
        // A quick click: held for about a tenth of a second.
        for _ in 0..6 {
            sim.step(DT, &Input::hold(), &mut rng);
        }
        let mut top = sim.y;
        for _ in 0..60 {
            sim.step(DT, &Input::default(), &mut rng);
            top = top.min(sim.y);
        }
        assert!(y0 - top < 0.04, "a tap lifted it {:.3}", y0 - top);
        assert!(top < y0, "a tap should lift it a little");
    }

    #[test]
    fn the_cave_always_leaves_room_to_fly() {
        let mut rng = Rng::new(2);
        let sim = Heli::new(320, 96, &mut rng);
        let mut x = 0.0;
        while x < sim.finish() + 5.0 {
            let (top, bottom) = sim.walls(x);
            assert!(top >= CEILING_MIN - 1e-4 && bottom <= FLOOR_MAX + 1e-4);
            assert!(bottom - top >= GAP_END - 1e-4);
            x += 0.05;
        }
    }

    #[test]
    fn idle_heli_sinks_into_the_floor_and_the_game_restarts() {
        let mut rng = Rng::new(3);
        let mut sim = Heli::new(320, 96, &mut rng);
        let mut overs = 0;
        let mut was_over = false;
        for _ in 0..60 * 20 {
            sim.step(DT, &Input::default(), &mut rng);
            let over = matches!(sim.phase, Phase::Over(_));
            if over && !was_over {
                overs += 1;
            }
            was_over = over;
            assert!(!sim.cleared());
        }
        assert!(overs >= 5, "only {overs} crashes");
    }

    #[test]
    fn holding_the_whole_time_hits_the_ceiling() {
        let mut rng = Rng::new(4);
        let mut sim = Heli::new(320, 96, &mut rng);
        for _ in 0..60 * 3 {
            sim.step(DT, &Input::hold(), &mut rng);
            if matches!(sim.phase, Phase::Over(_)) {
                return;
            }
        }
        panic!("held forever without crashing");
    }

    #[test]
    fn a_steady_pilot_flies_through_all_ten_gates() {
        for seed in 1..11 {
            let mut rng = Rng::new(seed);
            let mut sim = Heli::new(320, 96, &mut rng);
            for _ in 0..60 * 60 {
                let input = bot_input(&sim);
                sim.step(DT, &input, &mut rng);
                assert!(!matches!(sim.phase, Phase::Over(_)), "seed {seed} crashed at gate {}", sim.passed);
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
        let mut sim = Heli::new(320, 96, &mut rng);
        for (w, h) in [(321, 97), (1, 1), (0, 0), (4, 7)] {
            sim.resize(w, h, &mut rng);
            for dt in [f32::NAN, -1.0, 1000.0, DT] {
                sim.step(dt, &Input::hold(), &mut rng);
            }
            let mut frame = Frame::new(w, h);
            sim.render(&mut frame, &Theme::default());
            assert!(frame.pixels.chunks_exact(4).all(|p| p[3] == 255));
        }
    }
}
