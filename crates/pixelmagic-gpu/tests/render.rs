//! Integration tests that run the real renderer against a real GL context.
//!
//! These use the surfaceless EGL context from [`pixelmagic_gpu::headless`], so
//! they work on a build machine with no display and no GPU — Mesa's llvmpipe is
//! enough. If EGL is unavailable the tests skip rather than fail, since a
//! missing driver is an environment problem, not a code problem.
//!
//! What they are actually for: shader bugs do not announce themselves. A
//! `Multiply` that silently screens, a premultiply that happens twice, a
//! blend-mode index off by one — all of those compile fine and just look
//! slightly wrong. Checking pixel values against arithmetic worked out by hand
//! is the only way to catch them.

use pixelmagic_core::adjust::{Adjustment, AdjustmentInstance, AdjustmentKind};
use pixelmagic_core::blend::BlendMode;
use pixelmagic_core::buffer::PixelBuffer;
use pixelmagic_core::color::Rgba;
use pixelmagic_core::document::Document;
use pixelmagic_core::effect::Effect;
use pixelmagic_core::layer::{LayerId, LayerKind};
use pixelmagic_core::param::ParamValue;
use pixelmagic_gpu::headless::HeadlessContext;
use pixelmagic_gpu::{Renderer, Result};
use std::collections::HashMap;

/// Bring up a context, or return `None` so the caller can skip.
fn context() -> Option<HeadlessContext> {
    match HeadlessContext::new() {
        Ok(ctx) => {
            eprintln!("GL context: {}", ctx.describe());
            Some(ctx)
        }
        Err(e) => {
            eprintln!("skipping GPU test: {e}");
            None
        }
    }
}

macro_rules! gl_test {
    ($ctx:ident) => {
        let Some($ctx) = context() else { return };
    };
}

fn solid_doc(w: u32, h: u32, colors: &[(Rgba, BlendMode, f32)]) -> Document {
    let mut doc = Document::empty(w, h);
    // `insert` puts each new layer at the front, so inserting in the caller's
    // bottom-to-top order leaves the last one front-most.
    for (color, blend, opacity) in colors.iter() {
        let buffer = PixelBuffer::filled(w, h, *color);
        let id = doc.layers.insert("layer", LayerKind::Pixel { buffer }, None);
        let layer = doc.layers.get_mut(id).unwrap();
        layer.blend_mode = *blend;
        layer.opacity = *opacity;
    }
    doc
}

fn render(renderer: &mut Renderer, doc: &Document) -> Result<Vec<u8>> {
    let revisions: HashMap<LayerId, u64> = HashMap::new();
    let target = renderer.render_document(doc, &revisions)?;
    let px = renderer.read_image(&target)?;
    renderer.release(target);
    Ok(px)
}

fn pixel_at(px: &[u8], w: u32, x: u32, y: u32) -> [u8; 4] {
    let i = (y as usize * w as usize + x as usize) * 4;
    [px[i], px[i + 1], px[i + 2], px[i + 3]]
}

/// Compare with a tolerance. Half-float intermediates, the sRGB round trip and
/// llvmpipe's rounding all contribute a little error; anything within a couple
/// of 8-bit steps is the same colour.
fn assert_close(actual: [u8; 4], expected: [u8; 4], tol: i32, what: &str) {
    for i in 0..4 {
        let d = actual[i] as i32 - expected[i] as i32;
        assert!(
            d.abs() <= tol,
            "{what}: channel {i} was {}, expected {} (±{tol})\n  actual   {actual:?}\n  expected {expected:?}",
            actual[i],
            expected[i]
        );
    }
}

#[test]
fn headless_context_starts() {
    gl_test!(ctx);
    assert!(!ctx.describe().is_empty());
}

#[test]
fn every_shader_compiles() {
    gl_test!(ctx);
    let mut renderer = Renderer::new(ctx.gl.clone(), ctx.flavor).expect("renderer");
    // The single most valuable test in the suite: a typo in any shader in the
    // library fails here rather than the first time a user opens that panel.
    let n = renderer.precompile().expect("all shaders should compile");
    assert!(n >= 30, "expected the full library, compiled {n}");
}

#[test]
fn single_opaque_layer_round_trips() {
    gl_test!(ctx);
    let mut renderer = Renderer::new(ctx.gl.clone(), ctx.flavor).unwrap();
    let doc = solid_doc(8, 8, &[(Rgba::from_u8(200, 100, 50, 255), BlendMode::Normal, 1.0)]);
    let px = render(&mut renderer, &doc).unwrap();
    assert_close(pixel_at(&px, 8, 4, 4), [200, 100, 50, 255], 2, "identity round trip");
}

#[test]
fn empty_document_is_transparent() {
    gl_test!(ctx);
    let mut renderer = Renderer::new(ctx.gl.clone(), ctx.flavor).unwrap();
    let doc = Document::empty(4, 4);
    let px = render(&mut renderer, &doc).unwrap();
    assert_eq!(pixel_at(&px, 4, 2, 2), [0, 0, 0, 0]);
}

#[test]
fn hidden_layers_do_not_render() {
    gl_test!(ctx);
    let mut renderer = Renderer::new(ctx.gl.clone(), ctx.flavor).unwrap();
    let mut doc = solid_doc(4, 4, &[(Rgba::WHITE, BlendMode::Normal, 1.0)]);
    let id = doc.layers.roots()[0];
    doc.layers.get_mut(id).unwrap().visible = false;
    let px = render(&mut renderer, &doc).unwrap();
    assert_eq!(pixel_at(&px, 4, 2, 2), [0, 0, 0, 0]);
    assert_eq!(renderer.stats.layers_drawn, 0);
    assert_eq!(renderer.stats.layers_skipped, 1);
}

#[test]
fn multiply_darkens_by_the_product() {
    gl_test!(ctx);
    let mut renderer = Renderer::new(ctx.gl.clone(), ctx.flavor).unwrap();
    // 0.5 * 0.5 = 0.25 in the encoded domain, which is where the blend runs.
    let doc = solid_doc(
        4,
        4,
        &[
            (Rgba::from_u8(128, 128, 128, 255), BlendMode::Normal, 1.0),
            (Rgba::from_u8(128, 128, 128, 255), BlendMode::Multiply, 1.0),
        ],
    );
    let px = render(&mut renderer, &doc).unwrap();
    let got = pixel_at(&px, 4, 2, 2);
    let expected = (0.502f32 * 0.502 * 255.0).round() as u8;
    assert_close(got, [expected, expected, expected, 255], 3, "multiply");
}

#[test]
fn screen_lightens_symmetrically_to_multiply() {
    gl_test!(ctx);
    let mut renderer = Renderer::new(ctx.gl.clone(), ctx.flavor).unwrap();
    let doc = solid_doc(
        4,
        4,
        &[
            (Rgba::from_u8(128, 128, 128, 255), BlendMode::Normal, 1.0),
            (Rgba::from_u8(128, 128, 128, 255), BlendMode::Screen, 1.0),
        ],
    );
    let px = render(&mut renderer, &doc).unwrap();
    // screen(a, a) = 2a - a^2
    let a = 0.502f32;
    let expected = ((2.0 * a - a * a) * 255.0).round() as u8;
    assert_close(pixel_at(&px, 4, 2, 2), [expected, expected, expected, 255], 3, "screen");
}

#[test]
fn difference_of_a_colour_with_itself_is_black() {
    gl_test!(ctx);
    let mut renderer = Renderer::new(ctx.gl.clone(), ctx.flavor).unwrap();
    let c = Rgba::from_u8(180, 90, 30, 255);
    let doc = solid_doc(4, 4, &[(c, BlendMode::Normal, 1.0), (c, BlendMode::Difference, 1.0)]);
    let px = render(&mut renderer, &doc).unwrap();
    assert_close(pixel_at(&px, 4, 2, 2), [0, 0, 0, 255], 2, "difference");
}

#[test]
fn darken_and_lighten_pick_the_right_side() {
    gl_test!(ctx);
    let mut renderer = Renderer::new(ctx.gl.clone(), ctx.flavor).unwrap();
    let dark = Rgba::from_u8(40, 40, 40, 255);
    let light = Rgba::from_u8(210, 210, 210, 255);

    let doc =
        solid_doc(4, 4, &[(dark, BlendMode::Normal, 1.0), (light, BlendMode::Darken, 1.0)]);
    let px = render(&mut renderer, &doc).unwrap();
    assert_close(pixel_at(&px, 4, 2, 2), [40, 40, 40, 255], 2, "darken keeps the darker");

    let doc =
        solid_doc(4, 4, &[(dark, BlendMode::Normal, 1.0), (light, BlendMode::Lighten, 1.0)]);
    let px = render(&mut renderer, &doc).unwrap();
    assert_close(pixel_at(&px, 4, 2, 2), [210, 210, 210, 255], 2, "lighten keeps the lighter");
}

#[test]
fn normal_blend_is_a_plain_replace() {
    gl_test!(ctx);
    let mut renderer = Renderer::new(ctx.gl.clone(), ctx.flavor).unwrap();
    let doc = solid_doc(
        4,
        4,
        &[
            (Rgba::from_u8(255, 0, 0, 255), BlendMode::Normal, 1.0),
            (Rgba::from_u8(0, 0, 255, 255), BlendMode::Normal, 1.0),
        ],
    );
    let px = render(&mut renderer, &doc).unwrap();
    assert_close(pixel_at(&px, 4, 2, 2), [0, 0, 255, 255], 2, "top layer wins");
}

#[test]
fn opacity_interpolates_in_linear_light() {
    gl_test!(ctx);
    let mut renderer = Renderer::new(ctx.gl.clone(), ctx.flavor).unwrap();
    let doc = solid_doc(
        4,
        4,
        &[(Rgba::BLACK, BlendMode::Normal, 1.0), (Rgba::WHITE, BlendMode::Normal, 0.5)],
    );
    let px = render(&mut renderer, &doc).unwrap();
    // Half-covering white over black is 0.5 in *linear* light, which encodes
    // to about 188 — not 128. Getting 128 here would mean the compositor is
    // working in the wrong space.
    let expected = (pixelmagic_core::color::linear_to_srgb(0.5) * 255.0).round() as u8;
    assert_close(pixel_at(&px, 4, 2, 2), [expected, expected, expected, 255], 3, "50% opacity");
    assert!(expected > 180, "linear-light 50% should encode near 188, got {expected}");
}

#[test]
fn zero_opacity_layer_is_skipped_entirely() {
    gl_test!(ctx);
    let mut renderer = Renderer::new(ctx.gl.clone(), ctx.flavor).unwrap();
    let doc = solid_doc(
        4,
        4,
        &[
            (Rgba::from_u8(10, 20, 30, 255), BlendMode::Normal, 1.0),
            (Rgba::WHITE, BlendMode::Normal, 0.0),
        ],
    );
    let px = render(&mut renderer, &doc).unwrap();
    assert_close(pixel_at(&px, 4, 2, 2), [10, 20, 30, 255], 2, "transparent layer");
}

#[test]
fn every_blend_mode_renders_without_error() {
    gl_test!(ctx);
    let mut renderer = Renderer::new(ctx.gl.clone(), ctx.flavor).unwrap();
    for mode in BlendMode::ALL {
        let doc = solid_doc(
            4,
            4,
            &[
                (Rgba::from_u8(120, 80, 200, 255), BlendMode::Normal, 1.0),
                (Rgba::from_u8(60, 190, 100, 255), mode, 1.0),
            ],
        );
        let px = render(&mut renderer, &doc).unwrap();
        let p = pixel_at(&px, 4, 2, 2);
        assert_eq!(p[3], 255, "{} lost opacity", mode.label());
        // Every mode must produce a finite, in-gamut result over opaque input.
        assert!(
            p[0] as u32 + p[1] as u32 + p[2] as u32 > 0 || mode == BlendMode::Difference,
            "{} produced pure black unexpectedly",
            mode.label()
        );
    }
}

#[test]
fn invert_adjustment_inverts() {
    gl_test!(ctx);
    let mut renderer = Renderer::new(ctx.gl.clone(), ctx.flavor).unwrap();
    let mut doc =
        solid_doc(4, 4, &[(Rgba::from_u8(200, 100, 50, 255), BlendMode::Normal, 1.0)]);
    let id = doc.layers.roots()[0];
    let mut inst = AdjustmentInstance::new(AdjustmentKind::Invert);
    if let Adjustment::Invert(a) = &mut inst.adjustment {
        a.intensity = 1.0;
    }
    // Nudge off the default so the pass is not treated as a no-op.
    doc.layers.get_mut(id).unwrap().adjustments.push(inst);

    let px = render(&mut renderer, &doc).unwrap();
    assert_close(pixel_at(&px, 4, 2, 2), [55, 155, 205, 255], 3, "invert");
}

#[test]
fn black_and_white_removes_colour() {
    gl_test!(ctx);
    let mut renderer = Renderer::new(ctx.gl.clone(), ctx.flavor).unwrap();
    let mut doc = solid_doc(4, 4, &[(Rgba::from_u8(255, 0, 0, 255), BlendMode::Normal, 1.0)]);
    let id = doc.layers.roots()[0];
    let mut inst = AdjustmentInstance::new(AdjustmentKind::BlackAndWhite);
    if let Adjustment::BlackAndWhite(a) = &mut inst.adjustment {
        a.tone = 0.0;
        a.intensity = 1.0;
        a.red = 1.0;
        a.green = 0.0;
        a.blue = 0.0;
    }
    doc.layers.get_mut(id).unwrap().adjustments.push(inst);

    let px = render(&mut renderer, &doc).unwrap();
    let p = pixel_at(&px, 4, 2, 2);
    assert!(p[0] == p[1] && p[1] == p[2], "expected a neutral grey, got {p:?}");
}

#[test]
fn adjustment_layer_affects_layers_below() {
    gl_test!(ctx);
    let mut renderer = Renderer::new(ctx.gl.clone(), ctx.flavor).unwrap();
    let mut doc = Document::empty(4, 4);
    doc.layers.insert(
        "base",
        LayerKind::Pixel {
            buffer: PixelBuffer::filled(4, 4, Rgba::from_u8(200, 100, 50, 255)),
        },
        None,
    );
    let adj = doc.layers.insert("adjust", LayerKind::ColorAdjustments, None);
    let mut inst = AdjustmentInstance::new(AdjustmentKind::Invert);
    if let Adjustment::Invert(a) = &mut inst.adjustment {
        a.intensity = 1.0;
    }
    doc.layers.get_mut(adj).unwrap().adjustments.push(inst);

    let px = render(&mut renderer, &doc).unwrap();
    assert_close(pixel_at(&px, 4, 2, 2), [55, 155, 205, 255], 3, "adjustment layer");
}

#[test]
fn gaussian_blur_averages_an_edge() {
    gl_test!(ctx);
    let mut renderer = Renderer::new(ctx.gl.clone(), ctx.flavor).unwrap();

    let mut buffer = PixelBuffer::new(64, 64);
    buffer.fill_rect(pixelmagic_core::geom::Rect::new(0.0, 0.0, 32.0, 64.0), Rgba::WHITE);
    buffer.fill_rect(pixelmagic_core::geom::Rect::new(32.0, 0.0, 32.0, 64.0), Rgba::BLACK);

    let mut doc = Document::empty(64, 64);
    let id = doc.layers.insert("edge", LayerKind::Pixel { buffer }, None);
    let mut effect = Effect::new("gaussian-blur").unwrap();
    effect.set("radius", ParamValue::Float(12.0));
    doc.layers.get_mut(id).unwrap().effects.push(effect);

    let px = render(&mut renderer, &doc).unwrap();
    let at_edge = pixel_at(&px, 64, 32, 32)[0];
    // The hard edge should have become a ramp: mid-grey at the boundary, and
    // still near the extremes far away from it.
    assert!((60..=200).contains(&(at_edge as i32)), "edge should be a gradient, got {at_edge}");
    assert!(pixel_at(&px, 64, 2, 32)[0] > 220, "far side should stay white");
    assert!(pixel_at(&px, 64, 61, 32)[0] < 40, "far side should stay black");
}

#[test]
fn blur_with_zero_radius_is_a_no_op() {
    gl_test!(ctx);
    let mut renderer = Renderer::new(ctx.gl.clone(), ctx.flavor).unwrap();
    let mut doc =
        solid_doc(8, 8, &[(Rgba::from_u8(90, 140, 210, 255), BlendMode::Normal, 1.0)]);
    let id = doc.layers.roots()[0];
    let mut effect = Effect::new("gaussian-blur").unwrap();
    effect.set("radius", ParamValue::Float(0.0));
    // Force it past the no-op check so the shader's own guard is what we test.
    effect.set("radius", ParamValue::Float(0.2));
    doc.layers.get_mut(id).unwrap().effects.push(effect);

    let px = render(&mut renderer, &doc).unwrap();
    assert_close(pixel_at(&px, 8, 4, 4), [90, 140, 210, 255], 2, "sub-pixel blur");
}

#[test]
fn posterize_quantises() {
    gl_test!(ctx);
    let mut renderer = Renderer::new(ctx.gl.clone(), ctx.flavor).unwrap();
    let mut doc =
        solid_doc(4, 4, &[(Rgba::from_u8(130, 130, 130, 255), BlendMode::Normal, 1.0)]);
    let id = doc.layers.roots()[0];
    let mut effect = Effect::new("posterize").unwrap();
    effect.set("levels", ParamValue::Float(2.0));
    doc.layers.get_mut(id).unwrap().effects.push(effect);

    let px = render(&mut renderer, &doc).unwrap();
    let v = pixel_at(&px, 4, 2, 2)[0];
    // With two levels the only outputs are 0, 128 and 255.
    assert!(
        v < 4 || (124..=132).contains(&(v as i32)) || v > 250,
        "posterize left an intermediate value: {v}"
    );
}

#[test]
fn groups_composite_their_children() {
    gl_test!(ctx);
    let mut renderer = Renderer::new(ctx.gl.clone(), ctx.flavor).unwrap();
    let mut doc = Document::empty(4, 4);
    let group = doc.layers.insert("group", LayerKind::Group, None);
    doc.layers.insert(
        "child",
        LayerKind::Pixel { buffer: PixelBuffer::filled(4, 4, Rgba::from_u8(0, 200, 0, 255)) },
        Some(group),
    );
    // Half-opacity on the group must apply to the composited result.
    doc.layers.get_mut(group).unwrap().opacity = 0.5;

    let px = render(&mut renderer, &doc).unwrap();
    let p = pixel_at(&px, 4, 2, 2);
    assert!(p[1] > 100, "green should show through: {p:?}");
    assert!(
        (120..=140).contains(&(p[3] as i32)),
        "group opacity should halve alpha, got {}",
        p[3]
    );
}

#[test]
fn layer_transform_places_content() {
    gl_test!(ctx);
    let mut renderer = Renderer::new(ctx.gl.clone(), ctx.flavor).unwrap();
    let mut doc = Document::empty(16, 16);
    let id = doc.layers.insert(
        "small",
        LayerKind::Pixel { buffer: PixelBuffer::filled(4, 4, Rgba::WHITE) },
        None,
    );
    doc.layers.get_mut(id).unwrap().transform =
        pixelmagic_core::geom::Transform::translate(glam::Vec2::new(8.0, 8.0));

    let px = render(&mut renderer, &doc).unwrap();
    assert_eq!(pixel_at(&px, 16, 10, 10)[3], 255, "content should be at the offset");
    assert_eq!(pixel_at(&px, 16, 2, 2)[3], 0, "origin should be empty");
}

#[test]
fn target_pool_is_reused_across_frames() {
    gl_test!(ctx);
    let mut renderer = Renderer::new(ctx.gl.clone(), ctx.flavor).unwrap();
    let doc = solid_doc(
        32,
        32,
        &[(Rgba::WHITE, BlendMode::Normal, 1.0), (Rgba::BLACK, BlendMode::Multiply, 1.0)],
    );
    let revisions = HashMap::new();

    let t = renderer.render_document(&doc, &revisions).unwrap();
    renderer.release(t);
    let first = renderer.memory_estimate();

    for _ in 0..5 {
        let t = renderer.render_document(&doc, &revisions).unwrap();
        renderer.release(t);
    }
    // Steady-state rendering must not keep allocating.
    assert_eq!(renderer.memory_estimate(), first, "pool should have reached a steady state");
}

#[test]
fn layer_textures_are_cached_between_frames() {
    gl_test!(ctx);
    let mut renderer = Renderer::new(ctx.gl.clone(), ctx.flavor).unwrap();
    let doc = solid_doc(16, 16, &[(Rgba::WHITE, BlendMode::Normal, 1.0)]);
    let mut revisions = HashMap::new();
    revisions.insert(doc.layers.roots()[0], 1u64);

    let t = renderer.render_document(&doc, &revisions).unwrap();
    renderer.release(t);
    assert_eq!(renderer.stats.uploads, 1);

    let t = renderer.render_document(&doc, &revisions).unwrap();
    renderer.release(t);
    assert_eq!(renderer.stats.uploads, 0, "unchanged layer should not re-upload");

    revisions.insert(doc.layers.roots()[0], 2u64);
    let t = renderer.render_document(&doc, &revisions).unwrap();
    renderer.release(t);
    assert_eq!(renderer.stats.uploads, 1, "bumped revision should re-upload");
}

// ---------------------------------------------------------------------------
// Compute shaders
// ---------------------------------------------------------------------------

/// Build a document with a hard vertical edge and a blur applied.
fn blurred_edge_doc(size: u32, radius: f32, effect_id: &str) -> Document {
    let mut buffer = PixelBuffer::new(size, size);
    buffer.fill_rect(
        pixelmagic_core::geom::Rect::new(0.0, 0.0, (size / 2) as f32, size as f32),
        Rgba::WHITE,
    );
    buffer.fill_rect(
        pixelmagic_core::geom::Rect::new(
            (size / 2) as f32,
            0.0,
            (size / 2) as f32,
            size as f32,
        ),
        Rgba::from_u8(20, 60, 200, 255),
    );

    let mut doc = Document::empty(size, size);
    let id = doc.layers.insert("edge", LayerKind::Pixel { buffer }, None);
    let mut effect = Effect::new(effect_id).unwrap();
    effect.set("radius", ParamValue::Float(radius));
    doc.layers.get_mut(id).unwrap().effects.push(effect);
    doc
}

#[test]
fn compute_is_available_on_this_driver() {
    gl_test!(ctx);
    let renderer = Renderer::new(ctx.gl.clone(), ctx.flavor).unwrap();
    let caps = renderer.capabilities();
    eprintln!("capabilities: {}", caps.describe());
    // Not an assertion that compute exists — that is a driver property, and the
    // fallback is meant to work. Just make the answer visible in test output.
    assert!(caps.version.0 >= 3, "expected at least GL/GLES 3.x");
}

#[test]
fn every_compute_shader_compiles() {
    gl_test!(ctx);
    let mut renderer = Renderer::new(ctx.gl.clone(), ctx.flavor).unwrap();
    if !renderer.capabilities().compute {
        eprintln!("skipping: no compute support");
        return;
    }
    // precompile() covers fragment + compute; the count proves compute was
    // included rather than silently skipped.
    let n = renderer.precompile().expect("shaders should compile");
    assert!(n >= 43, "expected fragment and compute shaders, got {n}");
}

/// The single most important test in this file.
///
/// Two implementations of the same blur will drift apart the moment someone
/// edits one and not the other, and the drift is invisible — a slightly
/// different sigma looks fine until you compare. Rendering the same scene both
/// ways and diffing is the only thing that keeps them honest.
#[test]
fn the_blur_threshold_picks_a_path() {
    gl_test!(ctx);
    let mut renderer = Renderer::new(ctx.gl.clone(), ctx.flavor).unwrap();
    if !renderer.capabilities().compute {
        return;
    }
    renderer.set_compute_blur_min_radius(20.0);

    let small = blurred_edge_doc(64, 4.0, "gaussian-blur");
    render(&mut renderer, &small).unwrap();
    assert_eq!(renderer.stats.dispatches, 0, "a small radius should stay on fragment");

    let large = blurred_edge_doc(64, 40.0, "gaussian-blur");
    render(&mut renderer, &large).unwrap();
    assert!(renderer.stats.dispatches > 0, "a large radius should dispatch compute");
}

#[test]
fn compute_blur_matches_the_fragment_blur() {
    gl_test!(ctx);
    let mut renderer = Renderer::new(ctx.gl.clone(), ctx.flavor).unwrap();
    if !renderer.capabilities().compute {
        eprintln!("skipping: no compute support");
        return;
    }

    // Radii either side of the shared-memory cutoff (48), so both the LDS path
    // and the wide fallback are covered.
    for radius in [1.0f32, 4.0, 12.0, 31.0, 47.0, 64.0, 96.0] {
        for effect in ["gaussian-blur", "box-blur"] {
            let doc = blurred_edge_doc(96, radius, effect);

            renderer.set_compute_enabled(true);
            // Force compute even at radii where the heuristic would prefer the
            // fragment path — the point here is to compare implementations.
            renderer.set_compute_blur_min_radius(0.0);
            let with_compute = render(&mut renderer, &doc).unwrap();
            assert!(
                renderer.stats.dispatches > 0,
                "{effect} r={radius}: expected the compute path to be taken"
            );

            renderer.set_compute_enabled(false);
            let with_fragment = render(&mut renderer, &doc).unwrap();
            assert_eq!(
                renderer.stats.dispatches, 0,
                "{effect} r={radius}: expected the fragment path"
            );

            let mut worst = 0i32;
            let mut worst_at = 0usize;
            for (i, (a, b)) in with_compute.iter().zip(with_fragment.iter()).enumerate() {
                let d = (*a as i32 - *b as i32).abs();
                if d > worst {
                    worst = d;
                    worst_at = i;
                }
            }
            // Both paths do the same arithmetic in a different order, so the
            // f16 accumulation can differ in the last bit or two.
            assert!(
                worst <= 2,
                "{effect} r={radius}: paths diverged by {worst} at byte {worst_at}\n  \
                 compute {:?}\n  fragment {:?}",
                &with_compute[worst_at & !3..(worst_at & !3) + 4],
                &with_fragment[worst_at & !3..(worst_at & !3) + 4],
            );
        }
    }

    renderer.set_compute_enabled(true);
}

#[test]
fn compute_blur_actually_blurs() {
    gl_test!(ctx);
    let mut renderer = Renderer::new(ctx.gl.clone(), ctx.flavor).unwrap();
    if !renderer.capabilities().compute {
        return;
    }
    let doc = blurred_edge_doc(96, 16.0, "gaussian-blur");
    renderer.set_compute_blur_min_radius(0.0);
    let px = render(&mut renderer, &doc).unwrap();

    // The edge should have become a ramp, and the far sides should be intact.
    let edge = pixel_at(&px, 96, 48, 48)[0];
    assert!((40..=220).contains(&(edge as i32)), "edge should be mid-ramp, got {edge}");
    assert!(pixel_at(&px, 96, 2, 48)[0] > 230, "left edge should stay white");
    assert!(pixel_at(&px, 96, 93, 48)[0] < 60, "right edge should stay blue");
}

#[test]
fn sub_pixel_blur_radius_is_a_no_op_on_both_paths() {
    gl_test!(ctx);
    let mut renderer = Renderer::new(ctx.gl.clone(), ctx.flavor).unwrap();
    if !renderer.capabilities().compute {
        return;
    }
    let doc = blurred_edge_doc(32, 0.2, "gaussian-blur");

    renderer.set_compute_enabled(true);
    renderer.set_compute_blur_min_radius(0.0);
    let a = render(&mut renderer, &doc).unwrap();
    renderer.set_compute_enabled(false);
    let b = render(&mut renderer, &doc).unwrap();
    renderer.set_compute_enabled(true);

    assert_eq!(a, b, "a sub-pixel radius must pass through unchanged either way");
    // And it really is unchanged: the edge is still hard.
    assert!(pixel_at(&a, 32, 15, 16)[0] > 250);
    assert!(pixel_at(&a, 32, 16, 16)[0] < 40);
}

// ---------------------------------------------------------------------------
// Histogram
// ---------------------------------------------------------------------------

fn histogram_of(
    renderer: &mut Renderer,
    doc: &Document,
) -> pixelmagic_gpu::renderer::Histogram {
    let revisions: HashMap<LayerId, u64> = HashMap::new();
    let target = renderer.render_document(doc, &revisions).unwrap();
    let h = renderer.histogram(&target).unwrap();
    renderer.release(target);
    h
}

#[test]
fn histogram_of_a_flat_colour_is_a_single_spike() {
    gl_test!(ctx);
    let mut renderer = Renderer::new(ctx.gl.clone(), ctx.flavor).unwrap();
    use pixelmagic_gpu::renderer::Histogram;

    let doc = solid_doc(64, 64, &[(Rgba::from_u8(200, 100, 50, 255), BlendMode::Normal, 1.0)]);
    let h = histogram_of(&mut renderer, &doc);

    assert_eq!(h.total, 64 * 64, "every opaque pixel should be counted");
    // Allow a bin either side: the f16 round trip can nudge a value across a
    // bin boundary.
    let red_mass: u32 = h.bins[Histogram::RED][199..=201].iter().sum();
    assert_eq!(red_mass, 64 * 64, "red should be one spike at ~200");
    let green_mass: u32 = h.bins[Histogram::GREEN][99..=101].iter().sum();
    assert_eq!(green_mass, 64 * 64);
    let blue_mass: u32 = h.bins[Histogram::BLUE][49..=51].iter().sum();
    assert_eq!(blue_mass, 64 * 64);
}

#[test]
fn histogram_ignores_transparent_pixels() {
    gl_test!(ctx);
    let mut renderer = Renderer::new(ctx.gl.clone(), ctx.flavor).unwrap();

    // A 64×64 canvas with only the left half painted.
    let mut buffer = PixelBuffer::new(64, 64);
    buffer.fill_rect(
        pixelmagic_core::geom::Rect::new(0.0, 0.0, 32.0, 64.0),
        Rgba::from_u8(128, 128, 128, 255),
    );
    let mut doc = Document::empty(64, 64);
    doc.layers.insert("half", LayerKind::Pixel { buffer }, None);

    let h = histogram_of(&mut renderer, &doc);
    assert_eq!(h.total, 32 * 64, "transparent half must not be binned");
}

#[test]
fn histogram_of_a_gradient_is_broad() {
    gl_test!(ctx);
    let mut renderer = Renderer::new(ctx.gl.clone(), ctx.flavor).unwrap();
    use pixelmagic_gpu::renderer::Histogram;

    let mut buffer = PixelBuffer::new(256, 8);
    for y in 0..8 {
        for x in 0..256 {
            buffer.set(x, y, Rgba::from_u8(x as u8, x as u8, x as u8, 255));
        }
    }
    let mut doc = Document::empty(256, 8);
    doc.layers.insert("ramp", LayerKind::Pixel { buffer }, None);

    let h = histogram_of(&mut renderer, &doc);
    assert_eq!(h.total, 256 * 8);
    let occupied = h.bins[Histogram::LUMA].iter().filter(|&&v| v > 0).count();
    assert!(occupied > 200, "a full ramp should occupy most bins, got {occupied}");
    // Evenly spread: no bin should dominate.
    assert!(h.peak(Histogram::LUMA) <= 16, "unexpected spike: {}", h.peak(Histogram::LUMA));
}

#[test]
fn compute_and_cpu_histograms_agree() {
    gl_test!(ctx);
    use pixelmagic_gpu::renderer::Histogram;
    let mut renderer = Renderer::new(ctx.gl.clone(), ctx.flavor).unwrap();
    if !renderer.capabilities().compute {
        eprintln!("skipping: no compute support");
        return;
    }

    let mut buffer = PixelBuffer::new(97, 61); // deliberately not a round number
    for y in 0..61 {
        for x in 0..97 {
            buffer.set(
                x,
                y,
                Rgba::from_u8((x * 2) as u8, (y * 4) as u8, ((x + y) * 3) as u8, 255),
            );
        }
    }
    let mut doc = Document::empty(97, 61);
    doc.layers.insert("noise", LayerKind::Pixel { buffer }, None);

    renderer.set_compute_enabled(true);
    let gpu = histogram_of(&mut renderer, &doc);
    renderer.set_compute_enabled(false);
    let cpu = histogram_of(&mut renderer, &doc);
    renderer.set_compute_enabled(true);

    assert_eq!(gpu.total, cpu.total, "pixel counts must match");
    assert_eq!(gpu.total, 97 * 61);

    // Red, green and blue must agree bin for bin: both paths quantise the same
    // encoded value with the same rounding.
    for channel in [Histogram::RED, Histogram::GREEN, Histogram::BLUE] {
        for bin in 0..256 {
            let (a, b) = (gpu.bins[channel][bin], cpu.bins[channel][bin]);
            assert_eq!(a, b, "channel {channel} bin {bin}: gpu {a} vs cpu {b}");
        }
    }

    // Luminance cannot agree exactly, and the GPU is the more accurate of the
    // two: it weights full-precision channels, whereas the CPU fallback can
    // only weight the 8-bit values it read back, so its inputs are already
    // quantised. That shifts a handful of pixels into a neighbouring bin.
    //
    // Comparing cumulative distributions instead of raw bins is the right test:
    // it is insensitive to a pixel moving one bin, but would still catch a real
    // disagreement about the shape of the histogram.
    let mut gpu_run = 0u32;
    let mut cpu_run = 0u32;
    let tolerance = (gpu.total as f64 * 0.02).ceil() as u32;
    for bin in 0..256 {
        gpu_run += gpu.bins[Histogram::LUMA][bin];
        cpu_run += cpu.bins[Histogram::LUMA][bin];
        assert!(
            gpu_run.abs_diff(cpu_run) <= tolerance,
            "luma cumulative diverged at bin {bin}: gpu {gpu_run} vs cpu {cpu_run} \
             (tolerance {tolerance})"
        );
    }
    assert_eq!(gpu_run, cpu_run, "both must account for every pixel");
}

#[test]
fn histogram_of_an_empty_document_is_empty() {
    gl_test!(ctx);
    let mut renderer = Renderer::new(ctx.gl.clone(), ctx.flavor).unwrap();
    let doc = Document::empty(16, 16);
    let h = histogram_of(&mut renderer, &doc);
    assert!(h.is_empty());
    assert_eq!(h.peak(0), 0);
}

// ---------------------------------------------------------------------------
// Frosted-glass backdrop
// ---------------------------------------------------------------------------

/// The backdrop pass has to blur what is *already in the framebuffer*, which
/// means a framebuffer-to-framebuffer blit and an alpha-blended draw — two
/// things the rest of the renderer never does, and two things drivers disagree
/// about. Checking it by eye in the running app catches "it looks frosted";
/// only this catches "the frosting samples the wrong region" or "the blend
/// leaves the panel fully opaque".
#[test]
fn backdrop_blurs_only_inside_its_rectangle() {
    use pixelmagic_gpu::renderer::{BackdropRect, BackdropStyle};
    gl_test!(ctx);
    let mut renderer = Renderer::new(ctx.gl.clone(), ctx.flavor).unwrap();

    // A hard black/white split down the middle. Blurring it smears grey across
    // the seam, which is trivially distinguishable from not blurring it.
    let (w, h) = (128u32, 128u32);
    let mut buffer = PixelBuffer::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let v = if x < w / 2 { 0.0 } else { 1.0 };
            buffer.set(x, y, Rgba::new(v, v, v, 1.0));
        }
    }
    let mut doc = Document::empty(w, h);
    doc.layers.insert("split", LayerKind::Pixel { buffer }, None);

    let revisions: HashMap<LayerId, u64> = HashMap::new();
    let image = renderer.render_document(&doc, &revisions).unwrap();

    // Present into an 8-bit scratch target, which stands in for the widget's
    // framebuffer, then frost the left half of it.
    let scratch = renderer.acquire_rgba8(w, h).unwrap();
    scratch.bind();
    renderer
        .present(&image, (0, 0, w as i32, h as i32), false, Some(scratch.framebuffer()))
        .unwrap();
    renderer.release(image);

    let rect = BackdropRect { x: 0.0, y: 0.0, width: (w / 2) as f32, height: h as f32 };
    let style = BackdropStyle {
        radius: 24.0,
        scale: 1,
        corner: 0.0,
        // No tint, so the test measures the blur and nothing else.
        tint: [0.0, 0.0, 0.0, 0.0],
        opacity: 1.0,
    };
    renderer
        .blur_backdrop((w as i32, h as i32), &[rect], style, Some(scratch.framebuffer()))
        .unwrap();

    let px = scratch.read_rgba8().unwrap();
    renderer.release(scratch);

    let mid = h / 2;
    // Just inside the frosted rectangle, hard by the black/white seam: the
    // blur must have pulled white across, so this is no longer black.
    let inside = pixel_at(&px, w, w / 2 - 2, mid);
    assert!(
        inside[0] > 40,
        "expected the blur to lighten the edge of the frosted region, got {inside:?}"
    );

    // Well inside the frosted rectangle and far from the seam, still black:
    // the blur is finite, so it must not have washed out the whole panel.
    let deep = pixel_at(&px, w, 4, mid);
    assert!(deep[0] < 40, "the blur reached far past its radius: {deep:?}");

    // Outside the rectangle, untouched — the same distance from the seam as
    // `inside`, so any smearing there would be the pass ignoring its bounds.
    let outside = pixel_at(&px, w, w / 2 + 2, mid);
    assert!(outside[0] > 200, "the backdrop pass drew outside its rectangle, got {outside:?}");
}

/// An empty rectangle list must be a no-op, not a blank frame. The app calls
/// this on every render, including the ones before the panels have been laid
/// out and measured.
#[test]
fn backdrop_with_no_rectangles_leaves_the_frame_alone() {
    use pixelmagic_gpu::renderer::BackdropStyle;
    gl_test!(ctx);
    let mut renderer = Renderer::new(ctx.gl.clone(), ctx.flavor).unwrap();

    let (w, h) = (32u32, 32u32);
    let doc = solid_doc(w, h, &[(Rgba::new(1.0, 0.0, 0.0, 1.0), BlendMode::Normal, 1.0)]);
    let revisions: HashMap<LayerId, u64> = HashMap::new();
    let image = renderer.render_document(&doc, &revisions).unwrap();

    let scratch = renderer.acquire_rgba8(w, h).unwrap();
    scratch.bind();
    renderer
        .present(&image, (0, 0, w as i32, h as i32), false, Some(scratch.framebuffer()))
        .unwrap();
    renderer.release(image);

    renderer
        .blur_backdrop(
            (w as i32, h as i32),
            &[],
            BackdropStyle::default(),
            Some(scratch.framebuffer()),
        )
        .unwrap();

    let px = scratch.read_rgba8().unwrap();
    renderer.release(scratch);
    let c = pixel_at(&px, w, w / 2, h / 2);
    assert!(c[0] > 200 && c[1] < 40, "the no-op path changed the frame: {c:?}");
}

// ---------------------------------------------------------------------------
// Selection overlay
// ---------------------------------------------------------------------------

/// Build a single-channel mask texture with a solid rectangle in it.
fn mask_texture(
    ctx: &HeadlessContext,
    w: u32,
    h: u32,
    rect: (u32, u32, u32, u32),
) -> pixelmagic_gpu::texture::Texture {
    use pixelmagic_gpu::texture::{Filter, Format, Texture, Wrap};
    let mut data = vec![0u8; (w * h) as usize];
    let (rx, ry, rw, rh) = rect;
    for y in ry..(ry + rh).min(h) {
        for x in rx..(rx + rw).min(w) {
            data[(y * w + x) as usize] = 255;
        }
    }
    let tex =
        Texture::new(ctx.gl.clone(), w, h, Format::R8, Filter::Nearest, Wrap::Clamp).unwrap();
    tex.upload_raw(&data).unwrap();
    tex
}

/// The overlay must mark the selection's *boundary* and leave both the
/// interior and the exterior alone. Getting this backwards — filling the
/// whole selection with ants — is the obvious failure mode and looks fine in
/// a thumbnail, so it is worth an explicit test.
#[test]
fn selection_overlay_draws_the_boundary_and_not_the_interior() {
    use pixelmagic_gpu::renderer::SelectionOverlayStyle;
    gl_test!(ctx);
    let mut renderer = Renderer::new(ctx.gl.clone(), ctx.flavor).unwrap();

    let (w, h) = (64u32, 64u32);
    // Deliberately **not** vertically centred. A symmetric rectangle looks
    // identical whether or not the shader applies the same top-row flip that
    // `present` does, so it cannot catch a mirrored overlay — which is exactly
    // the bug this test failed to catch the first time round.
    let mask = mask_texture(&ctx, w, h, (16, 8, 32, 20));

    let scratch = renderer.acquire_rgba8(w, h).unwrap();
    scratch.clear();

    // Phase 0 with a long dash so the sampled boundary points are all in the
    // *light* half of the pattern — otherwise a dark ant is indistinguishable
    // from the cleared background and the test is checking nothing.
    let style = SelectionOverlayStyle { dash: 4096.0, phase: 0.0, ..Default::default() };
    renderer
        .draw_selection_overlay(
            &mask,
            (0, 0, w as i32, h as i32),
            style,
            Some(scratch.framebuffer()),
        )
        .unwrap();

    let px = scratch.read_rgba8().unwrap();
    renderer.release(scratch);

    // Read in the same top-left-origin space the mask was written in.
    let at = |x: u32, y: u32| pixel_at(&px, w, x, h - 1 - y);

    // Mask rows 8..28, columns 16..48.
    let interior = at(32, 18);
    assert!(interior[3] < 16, "the middle of the selection must stay clear, got {interior:?}");

    let exterior = at(4, 4);
    assert!(exterior[3] < 16, "outside the selection must stay clear, got {exterior:?}");

    // Top edge, just inside. If the overlay were mirrored this would land in
    // empty space and read as transparent.
    let top = at(32, 8);
    assert!(top[3] > 200, "the top boundary must be drawn, got {top:?}");

    // Bottom edge, and the row just past it, which pins the vertical
    // placement from both sides.
    let bottom = at(32, 27);
    assert!(bottom[3] > 200, "the bottom boundary must be drawn, got {bottom:?}");
    let below = at(32, 29);
    assert!(below[3] < 16, "below the selection must stay clear, got {below:?}");

    // The left edge too, so this is not passing on one orientation only.
    let left_edge = at(16, 18);
    assert!(left_edge[3] > 200, "the left boundary must be drawn, got {left_edge:?}");
}

/// The hover preview tints the interior as well as outlining it — that is the
/// whole difference between it and a committed selection, and it is what the
/// user sees when deciding whether to click.
#[test]
fn preview_style_tints_the_interior() {
    use pixelmagic_gpu::renderer::SelectionOverlayStyle;
    gl_test!(ctx);
    let mut renderer = Renderer::new(ctx.gl.clone(), ctx.flavor).unwrap();

    let (w, h) = (64u32, 64u32);
    let mask = mask_texture(&ctx, w, h, (16, 8, 32, 20));

    let scratch = renderer.acquire_rgba8(w, h).unwrap();
    scratch.clear();
    renderer
        .draw_selection_overlay(
            &mask,
            (0, 0, w as i32, h as i32),
            SelectionOverlayStyle::preview(0.0),
            Some(scratch.framebuffer()),
        )
        .unwrap();

    let px = scratch.read_rgba8().unwrap();
    renderer.release(scratch);
    let at = |x: u32, y: u32| pixel_at(&px, w, x, h - 1 - y);

    let interior = at(32, 18);
    assert!(interior[3] > 40, "the preview must tint its interior, got {interior:?}");
    assert!(
        interior[0] > interior[2],
        "the tint is yellow, so red must exceed blue: {interior:?}"
    );

    let exterior = at(4, 4);
    assert!(exterior[3] < 16, "the tint must not escape the region, got {exterior:?}");
}

/// Advancing the phase must change which parts of the boundary are light and
/// which are dark — otherwise the ants are painted on and do not march.
#[test]
fn advancing_the_phase_moves_the_dashes() {
    use pixelmagic_gpu::renderer::SelectionOverlayStyle;
    gl_test!(ctx);
    let mut renderer = Renderer::new(ctx.gl.clone(), ctx.flavor).unwrap();

    let (w, h) = (64u32, 64u32);
    let mask = mask_texture(&ctx, w, h, (16, 16, 32, 32));

    let sample = |renderer: &mut Renderer, phase: f32| -> Vec<u8> {
        let scratch = renderer.acquire_rgba8(w, h).unwrap();
        scratch.clear();
        renderer
            .draw_selection_overlay(
                &mask,
                (0, 0, w as i32, h as i32),
                SelectionOverlayStyle::ants(phase),
                Some(scratch.framebuffer()),
            )
            .unwrap();
        let px = scratch.read_rgba8().unwrap();
        renderer.release(scratch);
        px
    };

    let a = sample(&mut renderer, 0.0);
    // Half a dash period: every light segment should now be dark and vice
    // versa, which is the largest possible difference.
    let b = sample(&mut renderer, 4.0);

    let differing = a.iter().zip(b.iter()).filter(|(x, y)| x != y).count();
    assert!(
        differing > 100,
        "the dash pattern did not move with the phase ({differing} bytes differ)"
    );
}
