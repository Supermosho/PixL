// Curves (SPEC 3.9). Shares the Levels LUT layout: the composite RGB curve is
// applied first, then the per-channel curves.

in vec2 v_uv;
out vec4 frag_color;

uniform sampler2D u_image;
uniform sampler2D u_lut;

float lut(float v, float channel) {
    return texture(u_lut, vec2(clamp(v, 0.0, 1.0), (channel + 0.5) / 5.0)).r;
}

void main() {
    vec4 src = texture(u_image, v_uv);
    vec3 c = src.a > EPS ? src.rgb / src.a : vec3(0.0);
    vec3 e = clamp(linear_to_srgb(c), 0.0, 1.0);

    e = vec3(lut(e.r, 0.0), lut(e.g, 0.0), lut(e.b, 0.0));
    e = vec3(lut(e.r, 1.0), lut(e.g, 2.0), lut(e.b, 3.0));

    frag_color = vec4(srgb_to_linear(clamp(e, 0.0, 1.0)) * src.a, src.a);
}
