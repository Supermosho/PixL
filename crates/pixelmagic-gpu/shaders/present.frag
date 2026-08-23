// Final pass: linear working space to the display, over a transparency
// checkerboard.

in vec2 v_uv;
out vec4 frag_color;

uniform sampler2D u_image;
// Checker square size in device pixels.
uniform float u_checker_size;
uniform vec2 u_viewport;
uniform bool u_show_checker;

void main() {
    // Internally v_uv.y = 0 is the *document's* top row, which lands at the
    // bottom of a GL framebuffer. Flip once, here, at the boundary with the
    // screen — rather than making every other pass think about it.
    vec2 uv = vec2(v_uv.x, 1.0 - v_uv.y);
    vec4 c = texture(u_image, uv);
    vec3 rgb = c.a > EPS ? c.rgb / c.a : vec3(0.0);
    rgb = linear_to_srgb(rgb);

    if (u_show_checker) {
        vec2 cell = floor(uv * u_viewport / max(u_checker_size, 1.0));
        float odd = mod(cell.x + cell.y, 2.0);
        vec3 checker = mix(vec3(0.80), vec3(0.68), odd);
        frag_color = vec4(mix(checker, rgb, c.a), 1.0);
    } else {
        frag_color = vec4(rgb * c.a, c.a);
    }
}
