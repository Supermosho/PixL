// Selection overlay: marching ants, and the Quick Selection hover fill.
//
// Two jobs in one pass because they are drawn together and share the boundary
// arithmetic — the hover preview gets a tinted fill *and* an outline, the
// committed selection gets an outline only.
//
// ## Why the boundary is found here rather than precomputed
//
// The alternative is an edge-detect pass into a texture, which would then be
// magnified along with the image and give you fat, blurry ants at 800% zoom.
// Selection outlines are chrome: they must stay one screen pixel wide however
// far the user has zoomed in. So the boundary test happens in screen space, at
// the resolution it will actually be drawn at, by sampling the mask at points
// one *device* pixel apart. That is why this shader needs to know the zoom.
//
// ## The dash pattern
//
// True marching ants march along the contour, which needs its arc length —
// meaning you have to trace the boundary, which a fragment shader cannot do.
// The standard substitute, and what this uses, is diagonal stripes in screen
// space: `x + y` increases by a constant along any 45° line, so thresholding
// it modulo a period gives alternating black and white segments that read as
// dashes on any boundary orientation. Animating the phase makes them crawl.
// On a boundary that happens to run exactly at 45° the dashes are stationary;
// that is the one visible artefact of the approximation and it is why the
// stripe direction is (1, -1) rather than (1, 1), which would sit still on the
// far more common top-left-to-bottom-right diagonal.

in vec2 v_uv;
out vec4 frag_color;

// Selection coverage, 0..1, at document resolution.
uniform sampler2D u_mask;
// Document size in pixels, so a one-device-pixel step can be expressed in UV.
uniform vec2 u_doc_size;
// Device pixels per document pixel. 1.0 at 100% zoom.
uniform float u_scale;
// Animation phase in device pixels; advancing this makes the ants crawl.
uniform float u_phase;
// Length of one dash *pair* in device pixels.
uniform float u_dash;
// Tint laid over the selected interior. Alpha 0 draws the outline only, which
// is what a committed selection wants; the hover preview passes yellow.
uniform vec4 u_fill;
// Coverage below this is "outside". A selection's antialiased edge ramps
// through the middle, so the boundary lands where the ramp crosses it.
uniform float u_threshold;

float coverage(vec2 uv) {
    // Same flip `present.frag` does, and for the same reason: the mask's row 0
    // is the document's *top* row, which lands at the bottom of a GL
    // framebuffer. Without this the outline is mirrored vertically — invisible
    // on a symmetric selection, obvious on any real one.
    vec2 p = vec2(uv.x, 1.0 - uv.y);
    // Clamp rather than relying on the sampler's wrap mode: a selection that
    // reaches the canvas edge must read as *inside* past that edge, or the
    // canvas border grows ants it should not have.
    return texture(u_mask, clamp(p, vec2(0.0), vec2(1.0))).r;
}

void main() {
    // One device pixel, expressed in the mask's UV space. At high zoom this is
    // a fraction of a texel, so the four taps below land inside one texel and
    // the boundary is found where the *magnified* edge falls — which is what
    // keeps the outline thin instead of stair-stepping across whole texels.
    vec2 texel = 1.0 / max(u_doc_size, vec2(1.0));
    vec2 step = texel / max(u_scale, 0.0001);

    float here = coverage(v_uv);
    bool inside = here >= u_threshold;

    // A fragment is on the boundary when it is inside and at least one of its
    // four neighbours is outside. Testing only from the inside means the
    // outline sits *within* the selection rather than straddling it, so
    // adjacent selections do not merge their outlines into a double-width
    // line.
    float l = coverage(v_uv + vec2(-step.x, 0.0));
    float r = coverage(v_uv + vec2(step.x, 0.0));
    float d = coverage(v_uv + vec2(0.0, -step.y));
    float u = coverage(v_uv + vec2(0.0, step.y));
    bool edge = inside
        && (l < u_threshold || r < u_threshold || d < u_threshold || u < u_threshold);

    vec4 result = vec4(0.0);

    // Interior tint first, so the outline draws over it.
    if (inside && u_fill.a > 0.0) {
        result = vec4(u_fill.rgb * u_fill.a, u_fill.a);
    }

    if (edge) {
        // gl_FragCoord is in device pixels, which is exactly the space the
        // dash period is defined in — so the dashes stay the same size on
        // screen no matter the zoom, like the outline itself.
        float t = gl_FragCoord.x - gl_FragCoord.y + u_phase;
        float half_period = max(u_dash, 2.0) * 0.5;
        bool light = mod(t, max(u_dash, 2.0)) < half_period;
        vec3 ant = light ? vec3(1.0) : vec3(0.0);
        result = vec4(ant, 1.0);
    }

    if (result.a <= 0.0) {
        discard;
    }
    frag_color = result;
}
