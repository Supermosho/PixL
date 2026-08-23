// Bloom and Gloom: add or subtract a blurred copy of the bright areas.

in vec2 v_uv;
out vec4 frag_color;

uniform sampler2D u_image;
uniform sampler2D u_blur;
uniform float u_intensity;
uniform bool u_gloom;

void main() {
    vec4 src = texture(u_image, v_uv);
    vec4 blr = texture(u_blur, v_uv);
    vec3 c = src.a > EPS ? src.rgb / src.a : vec3(0.0);
    vec3 b = blr.a > EPS ? blr.rgb / blr.a : vec3(0.0);

    vec3 outc = u_gloom
        ? min(c, mix(c, b, u_intensity))          // dull the highlights
        : c + max(b - 0.5, 0.0) * u_intensity * 2.0;  // glow from bright areas

    frag_color = vec4(max(outc, 0.0) * src.a, src.a);
}
