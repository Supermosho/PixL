// Tilt-Shift and Focus: mix a sharp image with a blurred copy according to a
// geometric in-focus region.

in vec2 v_uv;
out vec4 frag_color;

uniform sampler2D u_image;
uniform sampler2D u_blur;
uniform vec2 u_center;
uniform vec2 u_aspect;
uniform float u_transition;
uniform float u_angle;   // radians; tilt-shift only
uniform bool u_radial;   // true = Focus (circular), false = Tilt-Shift (band)

void main() {
    vec2 d = (v_uv - u_center) * u_aspect;
    float dist;
    if (u_radial) {
        dist = length(d) / max(length(u_aspect) * 0.5, EPS);
    } else {
        // Distance from the band's centre line.
        vec2 n = vec2(-sin(u_angle), cos(u_angle));
        dist = abs(dot(d, n)) / max(length(u_aspect) * 0.5, EPS);
    }

    float t = clamp(u_transition, 0.01, 1.0);
    float amount = smoothstep(t * 0.5, t * 0.5 + t, dist);
    frag_color = mix(texture(u_image, v_uv), texture(u_blur, v_uv), amount);
}
