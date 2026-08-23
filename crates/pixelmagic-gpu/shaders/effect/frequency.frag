// High Pass and Low Pass.
//
// Low Pass is simply the blurred image. High Pass is the difference between
// the image and its blur, re-centred on neutral grey — the classic retouching
// primitive for separating detail from tone.

in vec2 v_uv;
out vec4 frag_color;

uniform sampler2D u_image;
uniform sampler2D u_blur;
uniform float u_opacity;
uniform bool u_high;

void main() {
    vec4 src = texture(u_image, v_uv);
    vec4 blr = texture(u_blur, v_uv);
    vec3 c = src.a > EPS ? src.rgb / src.a : vec3(0.0);
    vec3 b = blr.a > EPS ? blr.rgb / blr.a : vec3(0.0);

    vec3 outc;
    if (u_high) {
        vec3 detail = linear_to_srgb(c) - linear_to_srgb(b);
        outc = srgb_to_linear(clamp(detail + 0.5, 0.0, 1.0));
    } else {
        outc = b;
    }
    frag_color = vec4(mix(c, outc, clamp(u_opacity, 0.0, 1.0)) * src.a, src.a);
}
