// Selective Colour (SPEC 3.6): eight hue bands, each with its own hue shift,
// saturation and brightness.
//
// Band membership is a smooth cosine window on hue, so a pixel between two
// bands gets a share of each rather than snapping to one.

in vec2 v_uv;
out vec4 frag_color;

uniform sampler2D u_image;
uniform vec3 u_bands[8];    // x = hue shift (deg), y = saturation, z = brightness
uniform float u_centers[8]; // band hue centres in degrees

void main() {
    vec4 src = texture(u_image, v_uv);
    vec3 c = src.a > EPS ? src.rgb / src.a : vec3(0.0);
    vec3 hsl = rgb_to_hsl(clamp(linear_to_srgb(c), 0.0, 1.0));

    float hue_deg = hsl.x * 360.0;
    float dh = 0.0, ds = 0.0, dl = 0.0, total = 0.0;

    for (int i = 0; i < 8; i++) {
        float diff = abs(mod(hue_deg - u_centers[i] + 540.0, 360.0) - 180.0);
        // 60-degree half-window, cosine-tapered.
        float w = diff < 60.0 ? 0.5 + 0.5 * cos(diff / 60.0 * 3.14159265) : 0.0;
        // Unsaturated pixels have no meaningful hue, so leave them alone.
        w *= smoothstep(0.0, 0.15, hsl.y);
        dh += u_bands[i].x * w;
        ds += u_bands[i].y * w;
        dl += u_bands[i].z * w;
        total += w;
    }

    if (total > EPS) {
        hsl.x = fract(hsl.x + dh / 360.0);
        hsl.y = clamp(hsl.y * (1.0 + ds), 0.0, 1.0);
        hsl.z = clamp(hsl.z + dl * 0.25, 0.0, 1.0);
    }

    frag_color = vec4(srgb_to_linear(hsl_to_rgb(hsl)) * src.a, src.a);
}
