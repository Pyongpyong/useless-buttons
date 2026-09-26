//! Renders a short animated GIF of each variant to `preview/`, driving the
//! exact same `Sim` trait objects the wasm build uses (no browser, no
//! wasm — just the native library). Handy for eyeballing a variant before
//! spending a build cycle on it.
//!
//! Run with `cargo run --release --example preview`.

use gif::{Encoder, Frame as GifFrame, Repeat};
use std::fs::{self, File};
use std::path::Path;

use useless_buttons_core::paint::{Frame, Theme};
use useless_buttons_core::rng::Rng;
use useless_buttons_core::sims::{Input, Variant};

const W: usize = 320;
const H: usize = 96;
const FPS: f32 = 24.0;
const SECONDS: f32 = 4.0;

fn main() {
    let out_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("preview");
    fs::create_dir_all(&out_dir).expect("create preview/ dir");

    for variant in [
        Variant::Swarm,
        Variant::Sand,
        Variant::Life,
        Variant::Fractal,
        Variant::Bounce,
        Variant::Dungeon,
        Variant::Starry,
        Variant::Voronoi,
        Variant::Hyperdrive,
        Variant::Tunnel,
        Variant::Matrix,
        Variant::Blackhole,
        Variant::Chrome,
        Variant::Plasma,
        Variant::StainedGlass,
        Variant::Aurora,
        Variant::Ripple,
        Variant::Hologram,
        Variant::Supernova,
        Variant::Flappy,
        Variant::Runner,
        Variant::Timing,
        Variant::Crossy,
        Variant::Django,
        Variant::Balloon,
        Variant::Memory,
        Variant::Dodge,
    ] {
        if std::env::args().nth(1).map_or(true, |name| name == variant.as_str()) {
            render_variant(variant, &out_dir);
        }
    }

    println!("Wrote previews to {}", out_dir.display());
}

fn render_variant(variant: Variant, out_dir: &Path) {
    let name = variant.as_str();
    println!("rendering {name}...");

    let mut rng = Rng::new(20260922);
    let mut sim = variant.make(W, H, &mut rng);
    let mut frame = Frame::new(W, H);
    let theme = Theme::default();

    let path = out_dir.join(format!("{name}.gif"));
    let mut file = File::create(&path).unwrap_or_else(|e| panic!("create {path:?}: {e}"));
    let mut encoder = Encoder::new(&mut file, W as u16, H as u16, &[])
        .unwrap_or_else(|e| panic!("gif encoder for {name}: {e}"));
    encoder.set_repeat(Repeat::Infinite).ok();

    let dt = 1.0 / FPS;
    let total_frames = (SECONDS * FPS) as usize;

    // Warm up off-screen (not written to the gif) so the first captured
    // frame isn't the initial blank/high-contrast-noise transient — e.g.
    // swarm's trail-fade hasn't converged yet, life's random reseed
    // hasn't settled into recognizable patterns yet.
    let input = Input::default();
    for _ in 0..90 {
        sim.step(dt, &input, &mut rng);
        sim.render(&mut frame, &theme);
    }

    for _ in 0..total_frames {
        sim.step(dt, &input, &mut rng);
        sim.render(&mut frame, &theme);

        let mut rgba = frame.pixels.clone();
        let mut gif_frame = GifFrame::from_rgba_speed(W as u16, H as u16, &mut rgba, 10);
        gif_frame.delay = (100.0 / FPS) as u16; // gif delay unit = 1/100s
        encoder
            .write_frame(&gif_frame)
            .unwrap_or_else(|e| panic!("write frame for {name}: {e}"));
    }

    println!("  -> {}", path.display());
}
