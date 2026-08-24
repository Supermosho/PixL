// Frosted glass behind a floating panel.
//
// GTK4 has no `backdrop-filter`. A translucent widget shows whatever is under
// it verbatim, so a panel over a photograph reads as a dirty window rather than
// frosted glass, and the text on it competes with every edge in the image.
// Blur My Shell cannot help: it blurs what is behind the *window*, and this is
// compositing inside one.
//
// So the canvas draws it. The host snapshots the framebuffer, blurs the
// snapshot, and calls this once per panel to lay the blurred copy back down
// inside that panel's rectangle, tinted, with rounded corners. The GTK panel
// widget then draws its border and contents on top of that.
//
// Coordinates are in framebuffer pixels with the GL convention (origin bottom
// left), which is what `gl_FragCoord` gives us — no flip anywhere in here.

in vec2 v_uv;
out vec4 frag_color;

// The blurred snapshot of the whole framebuffer, possibly at reduced size;
// sampling is normalised so the scale does not matter here.
uniform sampler2D u_image;
// Framebuffer size in pixels.
uniform vec2 u_resolution;
// Panel rectangle: xy = lower-left corner, zw = size, in framebuffer pixels.
uniform vec4 u_rect;
// Corner radius in pixels.
uniform float u_corner;
// Panel tint. `a` is how much of it covers the blurred backdrop.
uniform vec4 u_tint;
// Overall opacity of the whole effect, so it can be faded out.
uniform float u_opacity;

// Signed distance to a rounded box centred at the origin. Negative inside.
float rounded_box(vec2 p, vec2 half_size, float r) {
    vec2 q = abs(p) - half_size + r;
    return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - r;
}

void main() {
    vec2 p = gl_FragCoord.xy;

    vec2 half_size = u_rect.zw * 0.5;
    vec2 centre = u_rect.xy + half_size;
    float r = min(u_corner, min(half_size.x, half_size.y));
    float d = rounded_box(p - centre, half_size, r);

    // One pixel of feathering, so the corners are not stair-stepped. The panel
    // border GTK draws on top lands on the same curve.
    float coverage = 1.0 - smoothstep(-0.5, 0.5, d);
    if (coverage <= 0.0) {
        discard;
    }

    vec3 blurred = texture(u_image, p / u_resolution).rgb;
    vec3 rgb = mix(blurred, u_tint.rgb, u_tint.a);

    float a = coverage * u_opacity;
    // Premultiplied, because that is what the framebuffer blend expects.
    frag_color = vec4(rgb * a, a);
}
