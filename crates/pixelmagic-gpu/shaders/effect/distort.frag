// Bump, Pinch and Twirl: displace the sampling coordinate within a radius.
//
// All three are inverse maps — for each output pixel we work out which input
// pixel it came from — which is what keeps the result hole-free.

in vec2 v_uv;
out vec4 frag_color;

uniform sampler2D u_image;
uniform vec2 u_center;
uniform vec2 u_size_px;
uniform float u_radius;   // pixels
uniform float u_amount;   // scale, or angle in radians for twirl
uniform int u_mode;       // 0 = bump, 1 = pinch, 2 = twirl

void main() {
    vec2 px = v_uv * u_size_px;
    vec2 cp = u_center * u_size_px;
    vec2 d = px - cp;
    float dist = length(d);
    float r = max(u_radius, 1.0);

    if (dist >= r || dist < EPS) {
        frag_color = texture(u_image, v_uv);
        return;
    }

    float t = dist / r;
    // Falls to zero at the edge of the radius, so there is no visible seam.
    float falloff = 1.0 - t;
    falloff *= falloff;

    vec2 src;
    if (u_mode == 2) {
        float a = u_amount * falloff;
        float c = cos(a), s = sin(a);
        src = cp + vec2(d.x * c - d.y * s, d.x * s + d.y * c);
    } else {
        // Bump pushes outward, pinch pulls in; a negative amount swaps them.
        float k = u_mode == 0 ? 1.0 - u_amount * falloff
                              : 1.0 + u_amount * falloff;
        src = cp + d * max(k, 0.02);
    }

    frag_color = texture(u_image, src / u_size_px);
}
