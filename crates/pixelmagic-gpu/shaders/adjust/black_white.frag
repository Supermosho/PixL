// Black & White (SPEC 3.13).

in vec2 v_uv;
out vec4 frag_color;

uniform sampler2D u_image;
uniform float u_red;
uniform float u_green;
uniform float u_blue;
uniform float u_tone;
uniform float u_intensity;

void main() {
    vec4 src = texture(u_image, v_uv);
    vec3 c = src.a > EPS ? src.rgb / src.a : vec3(0.0);

    float grey = dot(c, vec3(u_red, u_green, u_blue));

    // `Tone` lifts brightness where the original was saturated, so coloured
    // areas do not all collapse to the same grey.
    if (u_tone > EPS) {
        float s = rgb_to_hsl(clamp(linear_to_srgb(c), 0.0, 1.0)).y;
        grey *= 1.0 + u_tone * s;
    }

    frag_color = vec4(mix(c, vec3(grey), clamp(u_intensity, 0.0, 1.0)) * src.a, src.a);
}
