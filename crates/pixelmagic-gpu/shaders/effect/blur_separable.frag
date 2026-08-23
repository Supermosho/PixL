// Separable blur: Gaussian, Box and Motion.
//
// A 2-D Gaussian of radius r costs O(r^2) samples per pixel; separating it into
// a horizontal and a vertical pass costs O(r). At radius 100 that is the
// difference between 40,000 taps and 400. Motion blur is the same kernel run
// once along an arbitrary direction instead of twice along the axes.

in vec2 v_uv;
out vec4 frag_color;

uniform sampler2D u_image;
uniform vec2 u_texel;      // 1 / texture size
uniform vec2 u_direction;  // unit vector for this pass
uniform float u_radius;    // in pixels
uniform int u_kernel;      // 0 = gaussian, 1 = box

// Hard cap on taps. Beyond this the step is widened instead, trading a little
// accuracy for a bounded loop — GLSL needs a constant bound, and an unbounded
// blur radius would otherwise be a hang waiting to happen.
const int MAX_TAPS = 64;

void main() {
    float r = max(u_radius, 0.0);
    if (r < 0.5) {
        frag_color = texture(u_image, v_uv);
        return;
    }

    int taps = int(min(ceil(r), float(MAX_TAPS)));
    float step_scale = r / float(taps);
    float sigma = max(r * 0.5, 0.3);
    float inv_two_sigma_sq = 1.0 / (2.0 * sigma * sigma);

    vec4 sum = vec4(0.0);
    float wsum = 0.0;

    for (int i = -MAX_TAPS; i <= MAX_TAPS; i++) {
        if (i < -taps || i > taps) continue;
        float d = float(i) * step_scale;
        float w = u_kernel == 0 ? exp(-d * d * inv_two_sigma_sq) : 1.0;
        sum += texture(u_image, v_uv + u_direction * u_texel * d) * w;
        wsum += w;
    }

    frag_color = sum / max(wsum, EPS);
}
