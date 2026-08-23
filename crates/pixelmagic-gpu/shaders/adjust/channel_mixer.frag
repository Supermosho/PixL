// Channel Mixer (SPEC 3.10): a 3x3 matrix plus a per-output constant.

in vec2 v_uv;
out vec4 frag_color;

uniform sampler2D u_image;
uniform mat3 u_matrix;      // columns are the R, G, B output rows
uniform vec3 u_constant;

void main() {
    vec4 src = texture(u_image, v_uv);
    vec3 c = src.a > EPS ? src.rgb / src.a : vec3(0.0);
    vec3 e = clamp(linear_to_srgb(c), 0.0, 1.0);
    vec3 mixed = clamp(u_matrix * e + u_constant, 0.0, 1.0);
    frag_color = vec4(srgb_to_linear(mixed) * src.a, src.a);
}
