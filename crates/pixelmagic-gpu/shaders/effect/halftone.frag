// Halftone screens: Dot, Line, Hatched and Circular.
//
// Each compares local luminance against a periodic threshold pattern; the
// pattern's geometry is what distinguishes the four.

in vec2 v_uv;
out vec4 frag_color;

uniform sampler2D u_image;
uniform vec2 u_size_px;
uniform vec2 u_center;
uniform float u_width;      // pattern period in pixels
uniform float u_sharpness;
uniform float u_angle;      // radians
uniform int u_mode;         // 0 = dot, 1 = line, 2 = hatched, 3 = circular

float screen_value(vec2 px) {
    float w = max(u_width, 1.0);
    float c = cos(u_angle), s = sin(u_angle);
    vec2 r = vec2(px.x * c - px.y * s, px.x * s + px.y * c) / w;

    if (u_mode == 0) {
        vec2 f = fract(r) - 0.5;
        return 1.0 - length(f) * 2.0;
    } else if (u_mode == 1) {
        return cos(r.y * 6.28318531) * 0.5 + 0.5;
    } else if (u_mode == 2) {
        float a = cos(r.y * 6.28318531) * 0.5 + 0.5;
        float b = cos(r.x * 6.28318531) * 0.5 + 0.5;
        return max(a, b);
    } else {
        vec2 d = (px - u_center * u_size_px) / w;
        return cos(length(d) * 6.28318531) * 0.5 + 0.5;
    }
}

void main() {
    vec4 src = texture(u_image, v_uv);
    vec3 c = src.a > EPS ? src.rgb / src.a : vec3(0.0);
    float y = clamp(luminance(clamp(linear_to_srgb(c), 0.0, 1.0)), 0.0, 1.0);

    float pattern = screen_value(v_uv * u_size_px);
    // Sharpness controls how abrupt the transition is: at 1 it is a hard
    // threshold, at 0 the screen fades into a smooth gradient.
    float edge = mix(0.5, 0.02, clamp(u_sharpness, 0.0, 1.0));
    float v = smoothstep(-edge, edge, y - (1.0 - pattern));

    frag_color = vec4(vec3(v) * src.a, src.a);
}
