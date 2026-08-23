// White balance (SPEC 3.3): temperature runs blue-to-amber, tint green-to-
// magenta. Implemented as a channel scale in linear light, which is what a
// von Kries chromatic adaptation reduces to for small shifts.

in vec2 v_uv;
out vec4 frag_color;

uniform sampler2D u_image;
uniform float u_temperature;
uniform float u_tint;

void main() {
    vec4 src = texture(u_image, v_uv);
    vec3 c = src.a > EPS ? src.rgb / src.a : vec3(0.0);

    float t = u_temperature * 0.4;
    float g = u_tint * 0.3;

    vec3 gain = vec3(1.0 + t, 1.0 - g * 0.5, 1.0 - t);
    // Renormalise so the adjustment shifts hue without changing exposure.
    gain /= max(luminance(gain), EPS);
    c *= gain;
    c.g *= 1.0 + g;

    frag_color = vec4(c * src.a, src.a);
}
