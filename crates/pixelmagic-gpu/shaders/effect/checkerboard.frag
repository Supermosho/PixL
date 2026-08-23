// Checkerboard and Stripes generators.

in vec2 v_uv;
out vec4 frag_color;

uniform sampler2D u_image;
uniform vec2 u_size_px;
uniform vec4 u_color;
uniform float u_width;
uniform float u_sharpness;
uniform float u_angle;
uniform float u_opacity;
uniform bool u_stripes;

void main() {
    vec4 src = texture(u_image, v_uv);
    float w = max(u_width, 1.0);
    vec2 px = v_uv * u_size_px;
    float c = cos(u_angle), s = sin(u_angle);
    vec2 r = vec2(px.x * c - px.y * s, px.x * s + px.y * c) / w;

    float v;
    if (u_stripes) {
        v = cos(r.x * 3.14159265) * 0.5 + 0.5;
    } else {
        v = mod(floor(r.x) + floor(r.y), 2.0);
    }
    float edge = mix(0.5, 0.01, clamp(u_sharpness, 0.0, 1.0));
    v = smoothstep(0.5 - edge, 0.5 + edge, v);

    vec4 gen = vec4(srgb_to_linear(u_color.rgb), 1.0) * v * u_color.a * u_opacity;
    // Generators replace their input rather than filtering it.
    frag_color = gen + src * (1.0 - gen.a);
}
