// Color Controls: saturation, brightness and contrast in one pass.

in vec2 v_uv;
out vec4 frag_color;

uniform sampler2D u_image;
uniform float u_saturation;
uniform float u_brightness;
uniform float u_contrast;

void main() {
    vec4 src = texture(u_image, v_uv);
    vec3 c = src.a > EPS ? src.rgb / src.a : vec3(0.0);
    vec3 e = linear_to_srgb(c);

    e = mix(vec3(luminance(e)), e, 1.0 + u_saturation);
    e += u_brightness * 0.5;
    e = (e - 0.5) * (1.0 + u_contrast) + 0.5;

    frag_color = vec4(srgb_to_linear(e) * src.a, src.a);
}
