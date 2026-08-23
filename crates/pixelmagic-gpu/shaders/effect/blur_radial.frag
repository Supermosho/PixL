// Zoom and Spin blur: sample along the path a point would trace under a
// scale (zoom) or rotation (spin) about the effect's centre.

in vec2 v_uv;
out vec4 frag_color;

uniform sampler2D u_image;
uniform vec2 u_center;
uniform vec2 u_aspect;
uniform float u_amount;
uniform bool u_spin;

const int TAPS = 32;

void main() {
    vec2 d = (v_uv - u_center) * u_aspect;
    vec4 sum = vec4(0.0);

    for (int i = 0; i < TAPS; i++) {
        float t = float(i) / float(TAPS - 1) - 0.5;
        vec2 p;
        if (u_spin) {
            float a = t * u_amount;
            float c = cos(a), s = sin(a);
            p = vec2(d.x * c - d.y * s, d.x * s + d.y * c);
        } else {
            p = d * (1.0 + t * u_amount);
        }
        sum += texture(u_image, u_center + p / u_aspect);
    }
    frag_color = sum / float(TAPS);
}
