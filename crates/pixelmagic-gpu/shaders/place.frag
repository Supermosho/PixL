// Draws a layer's texture into the canvas-sized accumulator, applying the
// layer's affine transform.
//
// This is an inverse map done in the fragment shader rather than a transformed
// quad. Two reasons: every pass in the pipeline is already a fullscreen
// triangle, so there is no vertex plumbing to add; and the inverse map makes
// the out-of-bounds test trivial, which is what stops a rotated layer from
// smearing its edge pixels across the canvas.

in vec2 v_uv;
out vec4 frag_color;

uniform sampler2D u_layer;
uniform mat3 u_inv_transform;   // canvas pixels -> layer pixels
uniform vec2 u_canvas_size;
uniform vec2 u_layer_size;
// Nearest-neighbour sampling, for integer translations and pixel-art work.
uniform bool u_nearest;

void main() {
    vec2 canvas_px = v_uv * u_canvas_size;
    vec3 p = u_inv_transform * vec3(canvas_px, 1.0);
    vec2 layer_px = p.xy;

    // Reject anything outside the layer, with a half-pixel margin so the
    // bilinear tap at the very edge is not clipped.
    if (any(lessThan(layer_px, vec2(-0.5))) ||
        any(greaterThan(layer_px, u_layer_size + 0.5))) {
        frag_color = vec4(0.0);
        return;
    }

    vec2 uv = layer_px / u_layer_size;
    if (u_nearest) {
        uv = (floor(layer_px) + 0.5) / u_layer_size;
    }

    // The texture is SRGB8_ALPHA8, so the fetch already returned linear light
    // with straight alpha. Premultiply here, once, on linear values — doing it
    // before upload would have multiplied encoded values, which is wrong.
    frag_color = premultiply(texture(u_layer, uv));
}
