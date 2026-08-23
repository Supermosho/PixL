// Mask to Alpha: convert to greyscale and make dark areas transparent.

in vec2 v_uv;
out vec4 frag_color;

uniform sampler2D u_image;

void main() {
    vec4 src = texture(u_image, v_uv);
    vec3 c = src.a > EPS ? src.rgb / src.a : vec3(0.0);
    float y = clamp(luminance(clamp(linear_to_srgb(c), 0.0, 1.0)), 0.0, 1.0);
    frag_color = vec4(vec3(1.0) * y * src.a, y * src.a);
}
