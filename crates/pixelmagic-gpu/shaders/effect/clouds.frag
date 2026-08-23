// Clouds generator: fractal value noise between transparent and the chosen
// colour.

in vec2 v_uv;
out vec4 frag_color;

uniform sampler2D u_image;
uniform vec2 u_size_px;
uniform vec4 u_color;
uniform float u_width;
uniform float u_opacity;

void main() {
    vec4 src = texture(u_image, v_uv);
    float w = max(u_width, 1.0);
    float n = fbm(v_uv * u_size_px / w);
    n = clamp(n * 1.6 - 0.3, 0.0, 1.0);

    vec4 gen = vec4(srgb_to_linear(u_color.rgb), 1.0) * n * u_color.a * u_opacity;
    frag_color = gen + src * (1.0 - gen.a);
}
