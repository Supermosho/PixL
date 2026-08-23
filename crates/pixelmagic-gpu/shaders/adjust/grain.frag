// Film grain (SPEC 3.11).
//
// Grain is modulated by luminance: real film shows the most grain in the
// midtones and almost none in blown highlights, and flat uniform noise reads
// as digital rather than photographic.

in vec2 v_uv;
out vec4 frag_color;

uniform sampler2D u_image;
uniform vec2 u_size_px;
uniform float u_size;
uniform float u_intensity;

void main() {
    vec4 src = texture(u_image, v_uv);
    vec3 c = src.a > EPS ? src.rgb / src.a : vec3(0.0);

    vec2 p = v_uv * u_size_px / max(u_size, 0.05);
    float n = value_noise(p) - 0.5;

    float y = clamp(luminance(c), 0.0, 1.0);
    float weight = 4.0 * y * (1.0 - y);

    c += n * u_intensity * 0.35 * weight;
    frag_color = vec4(max(c, 0.0) * src.a, src.a);
}
