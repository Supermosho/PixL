// Shared helpers, prepended to every fragment shader by the program cache.
//
// Convention for the whole shader library: fragments carry **premultiplied,
// linear-light** RGBA. Premultiplied because that is the only representation in
// which "blend, then blend again" is associative; linear because compositing in
// a gamma space produces the dark fringes and muddy midtones that make an
// editor look amateurish.

#ifndef PIXELMAGIC_COMMON
#define PIXELMAGIC_COMMON

const float EPS = 1e-6;

// -- alpha ------------------------------------------------------------------

vec4 unpremultiply(vec4 c) {
    return c.a > EPS ? vec4(c.rgb / c.a, c.a) : vec4(0.0);
}

vec4 premultiply(vec4 c) {
    return vec4(c.rgb * c.a, c.a);
}

// -- transfer functions -----------------------------------------------------
// Mirrored through the origin so negative excursions from wide-gamut
// conversions survive a round trip instead of clamping to zero.

float srgb_to_linear_1(float c) {
    float s = sign(c);
    c = abs(c);
    return s * (c <= 0.04045 ? c / 12.92 : pow((c + 0.055) / 1.055, 2.4));
}

float linear_to_srgb_1(float c) {
    float s = sign(c);
    c = abs(c);
    return s * (c <= 0.0031308 ? c * 12.92 : 1.055 * pow(c, 1.0 / 2.4) - 0.055);
}

vec3 srgb_to_linear(vec3 c) {
    return vec3(srgb_to_linear_1(c.r), srgb_to_linear_1(c.g), srgb_to_linear_1(c.b));
}

vec3 linear_to_srgb(vec3 c) {
    return vec3(linear_to_srgb_1(c.r), linear_to_srgb_1(c.g), linear_to_srgb_1(c.b));
}

// -- luminance --------------------------------------------------------------

// Rec. 709, matching Rgba::luminance on the CPU side.
float luminance(vec3 c) {
    return dot(c, vec3(0.2126, 0.7152, 0.0722));
}

// Perceptual lightness, for controls that should feel even to the eye.
float lightness(vec3 c) {
    float y = luminance(c);
    return y <= 0.008856 ? y * 903.3 / 100.0 : (1.16 * pow(y, 1.0 / 3.0) - 0.16);
}

// -- HSL --------------------------------------------------------------------

vec3 rgb_to_hsl(vec3 c) {
    float mx = max(c.r, max(c.g, c.b));
    float mn = min(c.r, min(c.g, c.b));
    float d = mx - mn;
    float l = (mx + mn) * 0.5;
    if (d < EPS) return vec3(0.0, 0.0, l);

    float s = d / max(1.0 - abs(2.0 * l - 1.0), EPS);
    float h;
    if (mx == c.r)      h = mod((c.g - c.b) / d, 6.0);
    else if (mx == c.g) h = (c.b - c.r) / d + 2.0;
    else                h = (c.r - c.g) / d + 4.0;
    return vec3(h / 6.0, clamp(s, 0.0, 1.0), l);
}

vec3 hsl_to_rgb(vec3 hsl) {
    float h = fract(hsl.x) * 6.0;
    float c = (1.0 - abs(2.0 * hsl.z - 1.0)) * hsl.y;
    float x = c * (1.0 - abs(mod(h, 2.0) - 1.0));
    vec3 rgb;
    if      (h < 1.0) rgb = vec3(c, x, 0.0);
    else if (h < 2.0) rgb = vec3(x, c, 0.0);
    else if (h < 3.0) rgb = vec3(0.0, c, x);
    else if (h < 4.0) rgb = vec3(0.0, x, c);
    else if (h < 5.0) rgb = vec3(x, 0.0, c);
    else              rgb = vec3(c, 0.0, x);
    return rgb + (hsl.z - c * 0.5);
}

// -- tonal masks ------------------------------------------------------------
// Smooth weights for shadows / midtones / highlights, used by the range-based
// adjustments. They sum to 1 at every luminance, so a uniform edit across all
// three ranges is the same as a global one.

float shadow_weight(float y)    { return pow(clamp(1.0 - y, 0.0, 1.0), 2.0); }
float highlight_weight(float y) { return pow(clamp(y, 0.0, 1.0), 2.0); }
float midtone_weight(float y)   { return max(0.0, 1.0 - shadow_weight(y) - highlight_weight(y)); }

// -- noise ------------------------------------------------------------------

// Deterministic hash. Not cryptographic, but stable across frames — which is
// what matters for grain that must not shimmer while the user drags a slider.
float hash12(vec2 p) {
    vec3 p3 = fract(vec3(p.xyx) * 0.1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

vec2 hash22(vec2 p) {
    vec3 p3 = fract(vec3(p.xyx) * vec3(0.1031, 0.1030, 0.0973));
    p3 += dot(p3, p3.yxz + 33.33);
    return fract((p3.xx + p3.yz) * p3.zy);
}

float value_noise(vec2 p) {
    vec2 i = floor(p);
    vec2 f = fract(p);
    vec2 u = f * f * (3.0 - 2.0 * f);
    return mix(
        mix(hash12(i + vec2(0.0, 0.0)), hash12(i + vec2(1.0, 0.0)), u.x),
        mix(hash12(i + vec2(0.0, 1.0)), hash12(i + vec2(1.0, 1.0)), u.x),
        u.y);
}

float fbm(vec2 p) {
    float v = 0.0;
    float a = 0.5;
    for (int i = 0; i < 5; i++) {
        v += a * value_noise(p);
        p *= 2.0;
        a *= 0.5;
    }
    return v;
}

// -- misc -------------------------------------------------------------------

float safe_div(float a, float b) {
    return b > EPS ? a / b : 0.0;
}

// Blend an adjustment result back towards the original by `amount`, keeping
// alpha untouched. Every adjustment ends with this so its intensity control
// behaves identically everywhere.
vec4 mix_amount(vec4 original, vec3 adjusted, float amount) {
    return vec4(mix(original.rgb, adjusted, clamp(amount, 0.0, 1.0)), original.a);
}

#endif
