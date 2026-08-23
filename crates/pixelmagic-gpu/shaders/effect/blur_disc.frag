// Disc blur: a flat circular kernel, which is what an ideal camera aperture
// produces. Sampled on a spiral rather than a grid so the tap count stays
// affordable and the residual error looks like noise rather than banding.

in vec2 v_uv;
out vec4 frag_color;

uniform sampler2D u_image;
uniform vec2 u_texel;
uniform float u_radius;

const int TAPS = 48;
const float GOLDEN = 2.39996323;

void main() {
    float r = max(u_radius, 0.0);
    if (r < 0.5) {
        frag_color = texture(u_image, v_uv);
        return;
    }
    vec4 sum = texture(u_image, v_uv);
    for (int i = 1; i <= TAPS; i++) {
        float t = float(i) / float(TAPS);
        float a = float(i) * GOLDEN;
        // sqrt spacing keeps the samples uniformly dense over the disc's area.
        vec2 off = vec2(cos(a), sin(a)) * sqrt(t) * r;
        sum += texture(u_image, v_uv + off * u_texel);
    }
    frag_color = sum / float(TAPS + 1);
}
