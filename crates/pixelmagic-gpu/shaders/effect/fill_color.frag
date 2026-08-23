// Colour and gradient fills.

in vec2 v_uv;
out vec4 frag_color;

uniform sampler2D u_image;
uniform sampler2D u_ramp;
uniform vec4 u_color;
uniform float u_opacity;
uniform float u_angle;
uniform float u_scale;
uniform int u_type;   // 0 = solid, 1 = linear gradient, 2 = radial, 3 = angle

void main() {
    vec4 src = texture(u_image, v_uv);
    vec4 fill;

    if (u_type == 0) {
        fill = vec4(srgb_to_linear(u_color.rgb), 1.0) * u_color.a;
    } else {
        float t;
        vec2 d = v_uv - 0.5;
        if (u_type == 1) {
            vec2 dir = vec2(cos(u_angle), sin(u_angle));
            t = dot(d, dir) / max(u_scale, EPS) + 0.5;
        } else if (u_type == 2) {
            t = length(d) * 2.0 / max(u_scale, EPS);
        } else {
            t = fract((atan(d.y, d.x) - u_angle) / 6.28318531);
        }
        vec4 ramp = texture(u_ramp, vec2(clamp(t, 0.0, 1.0), 0.5));
        fill = vec4(srgb_to_linear(ramp.rgb), 1.0) * ramp.a;
    }

    fill *= clamp(u_opacity, 0.0, 1.0);
    frag_color = fill + src * (1.0 - fill.a);
}
