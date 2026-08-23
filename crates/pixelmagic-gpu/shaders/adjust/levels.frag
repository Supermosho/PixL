// Levels (SPEC 3.8), evaluated through a baked lookup table so the quarter-tone
// handles and per-channel curves cost the same as a plain gamma.
//
// The LUT is a 2-D texture: x is input value, y selects the channel
// (0=RGB, 1=R, 2=G, 3=B, 4=Luminance), matching ToneChannel::index.

in vec2 v_uv;
out vec4 frag_color;

uniform sampler2D u_image;
uniform sampler2D u_lut;
uniform bool u_luminance_only;

float lut(float v, float channel) {
    return texture(u_lut, vec2(clamp(v, 0.0, 1.0), (channel + 0.5) / 5.0)).r;
}

void main() {
    vec4 src = texture(u_image, v_uv);
    vec3 c = src.a > EPS ? src.rgb / src.a : vec3(0.0);
    vec3 e = clamp(linear_to_srgb(c), 0.0, 1.0);

    if (u_luminance_only) {
        // Adjust brightness without touching saturation: scale the colour by
        // the ratio of new to old luma.
        float y = luminance(e);
        float ny = lut(y, 4.0);
        e *= safe_div(ny, y);
    } else {
        e = vec3(lut(e.r, 0.0), lut(e.g, 0.0), lut(e.b, 0.0));
        e = vec3(lut(e.r, 1.0), lut(e.g, 2.0), lut(e.b, 3.0));
    }

    frag_color = vec4(srgb_to_linear(clamp(e, 0.0, 1.0)) * src.a, src.a);
}
