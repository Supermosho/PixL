// Invert (SPEC 3.15). Inverts in the encoded domain, which is what makes a
// negative look like a negative.

in vec2 v_uv;
out vec4 frag_color;

uniform sampler2D u_image;
uniform float u_intensity;

void main() {
    vec4 src = texture(u_image, v_uv);
    vec3 c = src.a > EPS ? src.rgb / src.a : vec3(0.0);
    vec3 e = linear_to_srgb(c);
    vec3 inv = srgb_to_linear(1.0 - clamp(e, 0.0, 1.0));
    frag_color = vec4(mix(c, inv, clamp(u_intensity, 0.0, 1.0)) * src.a, src.a);
}
