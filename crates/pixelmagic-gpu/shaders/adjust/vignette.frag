// Vignette (SPEC 3.11). Radius is measured against the shorter edge so the
// falloff stays circular on non-square canvases.

in vec2 v_uv;
out vec4 frag_color;

uniform sampler2D u_image;
uniform float u_exposure;
uniform float u_black_point;
uniform float u_softness;
uniform vec2 u_aspect;

void main() {
    vec4 src = texture(u_image, v_uv);
    vec3 c = src.a > EPS ? src.rgb / src.a : vec3(0.0);

    vec2 d = (v_uv - 0.5) * 2.0 * u_aspect;
    float r = length(d) / max(length(u_aspect), EPS);

    float soft = mix(0.05, 1.0, clamp(u_softness, 0.0, 1.0));
    float v = smoothstep(1.0 - soft, 1.0, r);

    c *= exp2(-u_exposure * 3.0 * v);
    c -= u_black_point * 0.3 * v;

    frag_color = vec4(max(c, 0.0) * src.a, src.a);
}
