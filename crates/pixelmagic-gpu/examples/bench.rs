//! Benchmark the compute path against the fragment path.
//!
//! Run with `cargo run --release -p pixelmagic-gpu --example bench`.
//!
//! A caveat that matters: on a software rasteriser (llvmpipe) these numbers
//! mean very little. Workgroup shared memory is the whole point of the compute
//! blur, and on a CPU rasteriser "shared memory" is just more RAM — there is no
//! fast on-die scratchpad to win back. Expect the two paths to be close, or
//! compute to lose. On real hardware, where shared memory is roughly an order
//! of magnitude faster than a texture fetch that misses cache, the gap opens
//! up. Run this on the machine you care about rather than trusting a number
//! from someone else's.

use pixelmagic_core::buffer::PixelBuffer;
use pixelmagic_core::color::Rgba;
use pixelmagic_core::document::Document;
use pixelmagic_core::effect::Effect;
use pixelmagic_core::layer::{LayerId, LayerKind};
use pixelmagic_core::param::ParamValue;
use pixelmagic_gpu::headless::HeadlessContext;
use pixelmagic_gpu::Renderer;
use std::collections::HashMap;
use std::time::Instant;

fn scene(size: u32, radius: f32) -> Document {
    let mut buffer = PixelBuffer::new(size, size);
    for y in 0..size {
        for x in 0..size {
            let v = (((x / 16) + (y / 16)) % 2) as f32;
            buffer.set(x, y, Rgba::new(v, x as f32 / size as f32, y as f32 / size as f32, 1.0));
        }
    }
    let mut doc = Document::empty(size, size);
    let id = doc.layers.insert("scene", LayerKind::Pixel { buffer }, None);
    let mut effect = Effect::new("gaussian-blur").unwrap();
    effect.set("radius", ParamValue::Float(radius));
    doc.layers.get_mut(id).unwrap().effects.push(effect);
    doc
}

fn time_renders(renderer: &mut Renderer, doc: &Document, iterations: u32) -> f64 {
    let revisions: HashMap<LayerId, u64> = HashMap::new();

    // One warm-up pass: the first render compiles shaders and fills the target
    // pool, and timing that would measure start-up rather than throughput.
    let t = renderer.render_document(doc, &revisions).unwrap();
    renderer.release(t);

    let start = Instant::now();
    for _ in 0..iterations {
        let target = renderer.render_document(doc, &revisions).unwrap();
        // Force the GPU to actually finish: without a readback the driver is
        // free to queue the work and return immediately, and we would be
        // timing command submission.
        let _ = renderer.read_image(&target).unwrap();
        renderer.release(target);
    }
    start.elapsed().as_secs_f64() * 1000.0 / iterations as f64
}

fn main() {
    let ctx = match HeadlessContext::new() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("no GL context: {e}");
            std::process::exit(1);
        }
    };
    let mut renderer = Renderer::new(ctx.gl.clone(), ctx.flavor).expect("renderer");
    let caps = renderer.capabilities();

    println!("device : {}", ctx.describe());
    println!("caps   : {}", caps.describe());
    println!();

    if !caps.compute {
        println!("Compute unavailable — nothing to compare.");
        return;
    }

    println!(
        "{:>6}  {:>8}  {:>12}  {:>12}  {:>9}",
        "size", "radius", "fragment ms", "compute ms", "speed-up"
    );
    println!("{}", "-".repeat(56));

    for size in [512u32, 1024] {
        for radius in [8.0f32, 24.0, 48.0] {
            let doc = scene(size, radius);
            let iterations = if size >= 1024 { 3 } else { 6 };

            renderer.set_compute_enabled(false);
            let fragment_ms = time_renders(&mut renderer, &doc, iterations);

            renderer.set_compute_enabled(true);
            let compute_ms = time_renders(&mut renderer, &doc, iterations);

            println!(
                "{size:>6}  {radius:>8.0}  {fragment_ms:>12.1}  {compute_ms:>12.1}  {:>8.2}x",
                fragment_ms / compute_ms
            );
        }
    }

    println!();
    println!("Note: read-back is included in both timings, so the ratios understate");
    println!("the difference between the two blur implementations themselves.");
}
