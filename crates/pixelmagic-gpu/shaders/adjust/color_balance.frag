// Colour Balance (SPEC 3.6), Master and 3-Way.
//
// Each tonal range contributes a lift weighted by the smooth shadow/midtone/
// highlight masks from common.glsl, so adjacent ranges cross-fade instead of
// banding at their boundaries.

in vec2 v_uv;
out vec4 frag_color;

uniform sampler2D u_image;
// Per range: xyz = RGB lift from the wheel and the complementary sliders,
// w = brightness. Index 0 = shadows, 1 = midtones, 2 = highlights.
uniform vec4 u_lift[3];
uniform vec3 u_saturation;
uniform bool u_master;

void main() {
    vec4 src = texture(u_image, v_uv);
    vec3 c = src.a > EPS ? src.rgb / src.a : vec3(0.0);
    float y = clamp(luminance(c), 0.0, 1.0);

    vec3 w = u_master
        ? vec3(1.0, 0.0, 0.0)
        : vec3(shadow_weight(y), midtone_weight(y), highlight_weight(y));

    vec3 lift = vec3(0.0);
    float bright = 0.0;
    float satk = 0.0;
    for (int i = 0; i < 3; i++) {
        lift += u_lift[i].xyz * w[i];
        bright += u_lift[i].w * w[i];
        satk += u_saturation[i] * w[i];
    }

    c += lift * 0.25;
    c *= exp2(bright);

    if (abs(satk) > EPS) {
        float l = luminance(c);
        c = mix(vec3(l), c, 1.0 + satk);
    }

    frag_color = vec4(max(c, 0.0) * src.a, src.a);
}
