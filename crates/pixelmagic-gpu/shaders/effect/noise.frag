// Noise: uniform digital noise, colour or monochrome.

in vec2 v_uv;
out vec4 frag_color;

uniform sampler2D u_image;
uniform vec2 u_size_px;
uniform float u_amount;
uniform bool u_monochrome;

void main() {
    vec4 src = texture(u_image, v_uv);
    vec3 c = src.a > EPS ? src.rgb / src.a : vec3(0.0);
    vec2 p = v_uv * u_size_px;

    vec3 n = u_monochrome
        ? vec3(hash12(p) - 0.5)
        : vec3(hash12(p), hash12(p + 17.3), hash12(p + 91.7)) - 0.5;

    frag_color = vec4(max(c + n * u_amount, 0.0) * src.a, src.a);
}
