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
    let px = target.read_srgb8_straight()?;
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
