// Sepia Tone, Color Monochrome and False Color: all three map luminance onto a
// colour ramp, so one shader with two endpoints covers them.

in vec2 v_uv;
out vec4 frag_color;

uniform sampler2D u_image;
uniform vec3 u_shadow_color;
uniform vec3 u_highlight_color;
uniform float u_intensity;

void main() {
    vec4 src = texture(u_image, v_uv);
    vec3 c = src.a > EPS ? src.rgb / src.a : vec3(0.0);
    float y = clamp(luminance(clamp(linear_to_srgb(c), 0.0, 1.0)), 0.0, 1.0);
    vec3 toned = srgb_to_linear(mix(u_shadow_color, u_highlight_color, y));
    frag_color = vec4(mix(c, toned, clamp(u_intensity, 0.0, 1.0)) * src.a, src.a);
}
