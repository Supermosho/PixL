// Posterize: quantise to N levels per channel, in the encoded domain so the
// bands land where the eye expects them.

in vec2 v_uv;
out vec4 frag_color;

uniform sampler2D u_image;
uniform float u_levels;

void main() {
    vec4 src = texture(u_image, v_uv);
    vec3 c = src.a > EPS ? src.rgb / src.a : vec3(0.0);
    float n = max(floor(u_levels), 2.0);
    vec3 e = clamp(linear_to_srgb(c), 0.0, 1.0);
    e = floor(e * n + 0.5) / n;
    frag_color = vec4(srgb_to_linear(e) * src.a, src.a);
}
