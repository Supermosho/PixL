// Hue, Saturation and Vibrance (SPEC 3.4).

in vec2 v_uv;
out vec4 frag_color;

uniform sampler2D u_image;
uniform float u_hue;         // degrees
uniform float u_saturation;  // -1..1
uniform float u_vibrance;    // -1..1

void main() {
    vec4 src = texture(u_image, v_uv);
    vec3 c = src.a > EPS ? src.rgb / src.a : vec3(0.0);

    vec3 hsl = rgb_to_hsl(clamp(linear_to_srgb(c), 0.0, 1.0));
    hsl.x = fract(hsl.x + u_hue / 360.0);

    // Vibrance protects already-saturated colours: the boost falls off as
    // existing saturation rises, so skin tones move while a red car does not
    // blow out.
    if (abs(u_vibrance) > EPS) {
        float protect = 1.0 - hsl.y;
        hsl.y = clamp(hsl.y + u_vibrance * protect * protect, 0.0, 1.0);
    }
    if (abs(u_saturation) > EPS) {
        hsl.y = clamp(hsl.y * (1.0 + u_saturation), 0.0, 1.0);
    }

    vec3 outc = srgb_to_linear(hsl_to_rgb(hsl));
    frag_color = vec4(outc * src.a, src.a);
}
