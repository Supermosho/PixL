// Threshold: hard black-and-white split at a luminance cutoff.

in vec2 v_uv;
out vec4 frag_color;

uniform sampler2D u_image;
uniform float u_threshold;

void main() {
    vec4 src = texture(u_image, v_uv);
    vec3 c = src.a > EPS ? src.rgb / src.a : vec3(0.0);
    float y = luminance(clamp(linear_to_srgb(c), 0.0, 1.0));
    float v = y >= u_threshold ? 1.0 : 0.0;
    frag_color = vec4(vec3(v) * src.a, src.a);
}
