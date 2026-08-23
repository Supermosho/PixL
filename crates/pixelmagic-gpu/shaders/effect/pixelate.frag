// Pixelate: snap sampling to a coarse grid.

in vec2 v_uv;
out vec4 frag_color;

uniform sampler2D u_image;
uniform vec2 u_size_px;
uniform float u_scale;   // block size in pixels

void main() {
    float s = max(u_scale, 1.0);
    vec2 px = v_uv * u_size_px;
    // Sample the block's centre, not its corner, or the result shifts by half
    // a block.
    vec2 snapped = (floor(px / s) + 0.5) * s;
    frag_color = texture(u_image, snapped / u_size_px);
}
