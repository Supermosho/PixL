// Replace Colour (SPEC 3.14).

in vec2 v_uv;
out vec4 frag_color;

uniform sampler2D u_image;
uniform vec3 u_source;
uniform vec3 u_target;
uniform float u_range;
uniform float u_intensity;

void main() {
    vec4 src = texture(u_image, v_uv);
    vec3 c = src.a > EPS ? src.rgb / src.a : vec3(0.0);
    vec3 e = clamp(linear_to_srgb(c), 0.0, 1.0);

    // Match on hue and saturation with luminance weighted down, so a colour is
    // replaced across its whole range of shading rather than only where it
    // matches in brightness too.
    vec3 d = e - u_source;
    float dist = length(vec3(d.r, d.g, d.b) * vec3(1.0, 1.0, 1.0));
    float w = 1.0 - smoothstep(u_range * 0.5, u_range * 1.5 + EPS, dist);

    vec3 shifted = clamp(e + (u_target - u_source), 0.0, 1.0);
    vec3 outc = mix(e, shifted, w * clamp(u_intensity, 0.0, 1.0));
    frag_color = vec4(srgb_to_linear(outc) * src.a, src.a);
}
