// Kaleidoscope: fold the plane into `count` mirrored wedges around a centre.

in vec2 v_uv;
out vec4 frag_color;

uniform sampler2D u_image;
uniform vec2 u_center;
uniform vec2 u_aspect;
uniform float u_angle;   // radians
uniform float u_count;

void main() {
    vec2 d = (v_uv - u_center) * u_aspect;
    float r = length(d);
    float a = atan(d.y, d.x) - u_angle;

    float wedge = 6.28318531 / max(u_count, 2.0);
    a = mod(a, wedge);
    // Mirror across the wedge's bisector so adjacent segments reflect.
    a = min(a, wedge - a);
    a += u_angle;

    vec2 src = u_center + vec2(cos(a), sin(a)) * r / u_aspect;
    frag_color = texture(u_image, clamp(src, 0.0, 1.0));
}
