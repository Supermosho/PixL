// Unsharp masking (SPEC 3.12). Takes a pre-blurred copy at the chosen radius.

in vec2 v_uv;
out vec4 frag_color;

uniform sampler2D u_image;
uniform sampler2D u_blur;
uniform float u_intensity;
// When set, sharpen luminance only, leaving saturation untouched.
uniform bool u_luminance_only;

void main() {
    vec4 src = texture(u_image, v_uv);
    vec4 blr = texture(u_blur, v_uv);
    vec3 c = src.a > EPS ? src.rgb / src.a : vec3(0.0);
    vec3 b = blr.a > EPS ? blr.rgb / blr.a : vec3(0.0);

    vec3 outc;
    if (u_luminance_only) {
        float detail = luminance(c) - luminance(b);
        outc = c + detail * u_intensity * 3.0;
    } else {
        outc = c + (c - b) * u_intensity * 3.0;
    }
    frag_color = vec4(max(outc, 0.0) * src.a, src.a);
}
