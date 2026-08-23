// Exposure effect: a pure stop adjustment in linear light.

in vec2 v_uv;
out vec4 frag_color;

uniform sampler2D u_image;
uniform float u_ev;

void main() {
    vec4 src = texture(u_image, v_uv);
    frag_color = vec4(src.rgb * exp2(u_ev), src.a);
}
