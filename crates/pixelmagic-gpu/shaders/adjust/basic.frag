// The eight-slider Basic section (SPEC 3.5).
//
// Order matters and follows a raw-developer pipeline: exposure scales linear
// light first, tonal recovery reshapes the ends, then contrast and the local
// controls act on what is left.

in vec2 v_uv;
out vec4 frag_color;

uniform sampler2D u_image;
// Blurred copy of the same image, used as the local-contrast reference for
// clarity and texture. Radius differs per control, so two are supplied.
uniform sampler2D u_blur_coarse;
uniform sampler2D u_blur_fine;

uniform float u_exposure;
uniform float u_highlights;
uniform float u_shadows;
uniform float u_brightness;
uniform float u_contrast;
uniform float u_black_point;
uniform float u_texture;
uniform float u_clarity;

void main() {
    vec4 src = texture(u_image, v_uv);
    vec3 c = src.a > EPS ? src.rgb / src.a : vec3(0.0);

    // Exposure in stops, applied in linear light where it is a pure scale.
    c *= exp2(u_exposure * 2.0);

    float y = clamp(luminance(c), 0.0, 4.0);

    // Highlight recovery and shadow lift, each weighted to its own end of the
    // range so they do not fight over the midtones.
    if (abs(u_highlights) > EPS) {
        c *= 1.0 + u_highlights * highlight_weight(min(y, 1.0));
    }
    if (abs(u_shadows) > EPS) {
        c += u_shadows * shadow_weight(min(y, 1.0)) * 0.25;
    }

    // Black point pulls the floor up or down before contrast pivots.
    if (abs(u_black_point) > EPS) {
        float bp = u_black_point * 0.25;
        c = (c - bp) / max(1.0 - bp, 0.05);
    }

    // Brightness and contrast are perceptual, so evaluate them encoded.
    vec3 e = linear_to_srgb(c);
    e += u_brightness * 0.5;
    if (abs(u_contrast) > EPS) {
        float k = u_contrast > 0.0 ? 1.0 + u_contrast * 2.0
                                   : 1.0 + u_contrast * 0.95;
        e = (e - 0.5) * k + 0.5;
    }

    // Local contrast: add back the difference between the image and a blurred
    // copy of itself. Coarse radius reads as clarity, fine radius as texture.
    if (abs(u_clarity) > EPS) {
        vec4 b = texture(u_blur_coarse, v_uv);
        vec3 bc = linear_to_srgb(b.a > EPS ? b.rgb / b.a : vec3(0.0));
        e += (e - bc) * u_clarity;
    }
    if (abs(u_texture) > EPS) {
        vec4 b = texture(u_blur_fine, v_uv);
        vec3 bc = linear_to_srgb(b.a > EPS ? b.rgb / b.a : vec3(0.0));
        e += (e - bc) * u_texture;
    }

    frag_color = vec4(srgb_to_linear(e) * src.a, src.a);
}
