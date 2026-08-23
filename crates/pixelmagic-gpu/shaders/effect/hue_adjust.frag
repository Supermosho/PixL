// Hue Adjust: rotate every hue by a fixed angle.

in vec2 v_uv;
out vec4 frag_color;

uniform sampler2D u_image;
uniform float u_angle;   // degrees

void main() {
    vec4 src = texture(u_image, v_uv);
    vec3 c = src.a > EPS ? src.rgb / src.a : vec3(0.0);
    vec3 hsl = rgb_to_hsl(clamp(linear_to_srgb(c), 0.0, 1.0));
    hsl.x = fract(hsl.x + u_angle / 360.0);
    frag_color = vec4(srgb_to_linear(hsl_to_rgb(hsl)) * src.a, src.a);
}
