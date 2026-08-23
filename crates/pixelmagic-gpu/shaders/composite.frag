// Composites one layer onto the accumulated backdrop.
//
// `u_blend_mode` indexes the 26 modes in the order of `BlendMode::ALL`, so the
// Rust enum's discriminant is the branch taken here. Keep the two in step.
//
// ## Which space the blend math runs in
//
// Alpha compositing is unambiguously correct in linear light, and that is what
// happens below. The *blend functions* are a different question: `Multiply`,
// `Overlay`, `Soft Light` and the rest were defined on gamma-encoded values,
// and that is how Photoshop and Core Image evaluate them by default. Running
// them on linear values is arguably more "physical" but produces visibly
// different — and to most users, wrong — results: `Overlay` pivots around a
// mid-grey that is no longer mid, and `Soft Light` barely does anything.
//
// So `u_blend_gamma` selects: 1.0 (the default) evaluates the blend function on
// encoded values and converts the result back to linear before compositing;
// 0.0 evaluates it directly in linear light. The toggle exists because this is
// a judgement call, and hard-coding either answer would hide it.

in vec2 v_uv;
out vec4 frag_color;

uniform sampler2D u_backdrop;
uniform sampler2D u_source;
uniform int u_blend_mode;
uniform float u_opacity;
uniform float u_blend_gamma;
// Coverage from the layer's mask and the document selection, already combined.
uniform sampler2D u_mask;
uniform bool u_use_mask;

// -- separable blend functions ----------------------------------------------

float b_darken(float b, float s)      { return min(b, s); }
float b_multiply(float b, float s)    { return b * s; }
float b_color_burn(float b, float s)  {
    if (b >= 1.0) return 1.0;
    if (s <= 0.0) return 0.0;
    return 1.0 - min(1.0, (1.0 - b) / s);
}
float b_linear_burn(float b, float s) { return b + s - 1.0; }
float b_lighten(float b, float s)     { return max(b, s); }
float b_screen(float b, float s)      { return b + s - b * s; }
float b_color_dodge(float b, float s) {
    if (b <= 0.0) return 0.0;
    if (s >= 1.0) return 1.0;
    return min(1.0, b / (1.0 - s));
}
float b_linear_dodge(float b, float s) { return b + s; }
float b_hard_light(float b, float s)   {
    return s <= 0.5 ? b_multiply(b, 2.0 * s) : b_screen(b, 2.0 * s - 1.0);
}
float b_overlay(float b, float s)      { return b_hard_light(s, b); }
float b_soft_light(float b, float s)   {
    // W3C compositing spec's formulation; smooth at s = 0.5, unlike the
    // simpler Photoshop approximation.
    float d = b <= 0.25 ? ((16.0 * b - 12.0) * b + 4.0) * b : sqrt(b);
    return s <= 0.5 ? b - (1.0 - 2.0 * s) * b * (1.0 - b)
                    : b + (2.0 * s - 1.0) * (d - b);
}
float b_vivid_light(float b, float s)  {
    return s <= 0.5 ? b_color_burn(b, 2.0 * s) : b_color_dodge(b, 2.0 * s - 1.0);
}
float b_linear_light(float b, float s) { return b + 2.0 * s - 1.0; }
float b_pin_light(float b, float s)    {
    return s <= 0.5 ? min(b, 2.0 * s) : max(b, 2.0 * s - 1.0);
}
float b_hard_mix(float b, float s)     { return b_vivid_light(b, s) < 0.5 ? 0.0 : 1.0; }
float b_difference(float b, float s)   { return abs(b - s); }
float b_exclusion(float b, float s)    { return b + s - 2.0 * b * s; }
float b_subtract(float b, float s)     { return b - s; }
float b_divide(float b, float s)       { return s <= 0.0 ? 1.0 : min(1.0, b / s); }

// -- non-separable blend functions ------------------------------------------
// W3C compositing spec, §"Non-separable blend modes".

float nonsep_lum(vec3 c) { return dot(c, vec3(0.3, 0.59, 0.11)); }

vec3 clip_color(vec3 c) {
    float l = nonsep_lum(c);
    float n = min(c.r, min(c.g, c.b));
    float x = max(c.r, max(c.g, c.b));
    if (n < 0.0) c = l + (c - l) * l / max(l - n, EPS);
    if (x > 1.0) c = l + (c - l) * (1.0 - l) / max(x - l, EPS);
    return c;
}

vec3 set_lum(vec3 c, float l) {
    return clip_color(c + (l - nonsep_lum(c)));
}

float sat(vec3 c) {
    return max(c.r, max(c.g, c.b)) - min(c.r, min(c.g, c.b));
}

vec3 set_sat(vec3 c, float s) {
    float mn = min(c.r, min(c.g, c.b));
    float mx = max(c.r, max(c.g, c.b));
    if (mx <= mn) return vec3(0.0);
    // Rescale so the mid component keeps its relative position.
    return (c - mn) * s / (mx - mn);
}

vec3 blend_separable(vec3 b, vec3 s, int mode) {
    vec3 r;
    for (int i = 0; i < 3; i++) {
        float bb = b[i];
        float ss = s[i];
        float v;
        if      (mode == 1)  v = b_darken(bb, ss);
        else if (mode == 2)  v = b_multiply(bb, ss);
        else if (mode == 3)  v = b_color_burn(bb, ss);
        else if (mode == 4)  v = b_linear_burn(bb, ss);
        else if (mode == 6)  v = b_lighten(bb, ss);
        else if (mode == 7)  v = b_screen(bb, ss);
        else if (mode == 8)  v = b_color_dodge(bb, ss);
        else if (mode == 9)  v = b_linear_dodge(bb, ss);
        else if (mode == 11) v = b_overlay(bb, ss);
        else if (mode == 12) v = b_soft_light(bb, ss);
        else if (mode == 13) v = b_hard_light(bb, ss);
        else if (mode == 14) v = b_vivid_light(bb, ss);
        else if (mode == 15) v = b_linear_light(bb, ss);
        else if (mode == 16) v = b_pin_light(bb, ss);
        else if (mode == 17) v = b_hard_mix(bb, ss);
        else if (mode == 18) v = b_difference(bb, ss);
        else if (mode == 19) v = b_exclusion(bb, ss);
        else if (mode == 20) v = b_subtract(bb, ss);
        else if (mode == 21) v = b_divide(bb, ss);
        else                 v = ss;
        r[i] = v;
    }
    return r;
}

vec3 blend_function(vec3 b, vec3 s, int mode) {
    // Whole-colour comparisons: pick one layer's colour outright.
    if (mode == 5)  return nonsep_lum(s) < nonsep_lum(b) ? s : b;   // Darker Color
    if (mode == 10) return nonsep_lum(s) > nonsep_lum(b) ? s : b;   // Lighter Color
    if (mode == 22) return set_lum(set_sat(s, sat(b)), nonsep_lum(b)); // Hue
    if (mode == 23) return set_lum(set_sat(b, sat(s)), nonsep_lum(b)); // Saturation
    if (mode == 24) return set_lum(s, nonsep_lum(b));                  // Color
    if (mode == 25) return set_lum(b, nonsep_lum(s));                  // Luminosity
    if (mode == 0)  return s;                                          // Normal
    return blend_separable(b, s, mode);
}

void main() {
    vec4 backdrop = texture(u_backdrop, v_uv);
    vec4 source = texture(u_source, v_uv);

    float alpha = clamp(u_opacity, 0.0, 1.0);
    if (u_use_mask) {
        alpha *= texture(u_mask, v_uv).r;
    }
    source *= alpha;

    // Blend functions are defined on un-premultiplied colour.
    vec3 cb = backdrop.a > EPS ? backdrop.rgb / backdrop.a : vec3(0.0);
    vec3 cs = source.a > EPS ? source.rgb / source.a : vec3(0.0);

    vec3 blended;
    if (u_blend_gamma > 0.5) {
        blended = srgb_to_linear(
            blend_function(linear_to_srgb(cb), linear_to_srgb(cs), u_blend_mode));
    } else {
        blended = blend_function(cb, cs, u_blend_mode);
    }

    // The blend function only applies where both layers have coverage;
    // elsewhere the source shows through unchanged.
    vec3 cr = mix(cs, blended, backdrop.a);

    float ao = source.a + backdrop.a * (1.0 - source.a);
    vec3 co = source.a * cr + backdrop.a * (1.0 - source.a) * cb;

    frag_color = vec4(co, ao);
}
