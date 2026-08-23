// Gradient Map: remap luminance through a gradient ramp texture.

in vec2 v_uv;
out vec4 frag_color;

uniform sampler2D u_image;
uniform sampler2D u_ramp;
uniform float u_opacity;

void main() {
    vec4 src = texture(u_image, v_uv);
    vec3 c = src.a > EPS ? src.rgb / src.a : vec3(0.0);
    float y = clamp(luminance(clamp(linear_to_srgb(c), 0.0, 1.0)), 0.0, 1.0);
    vec4 ramp = texture(u_ramp, vec2(y, 0.5));
    vec3 mapped = srgb_to_linear(ramp.rgb);
    frag_color = vec4(mix(c, mapped, clamp(u_opacity, 0.0, 1.0) * ramp.a) * src.a, src.a);
}
