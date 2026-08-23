// Multiplies a layer's alpha by a coverage mask.
//
// Kept as its own pass rather than folded into `composite` so that a mask can
// be applied *before* the layer's effects run — which is what makes masking a
// glow or a drop shadow behave the way people expect.

in vec2 v_uv;
out vec4 frag_color;

uniform sampler2D u_image;
uniform sampler2D u_mask;
uniform mat3 u_inv_transform;   // canvas pixels -> mask pixels
uniform vec2 u_canvas_size;
uniform vec2 u_mask_size;
uniform bool u_inverted;
uniform float u_opacity;
uniform float u_density;

void main() {
    vec4 src = texture(u_image, v_uv);

    vec3 p = u_inv_transform * vec3(v_uv * u_canvas_size, 1.0);
    vec2 uv = p.xy / u_mask_size;

    // Outside the mask's own bounds, treat coverage as fully hidden — a mask
    // smaller than the canvas should crop, not tile.
    float m = (any(lessThan(uv, vec2(0.0))) || any(greaterThan(uv, vec2(1.0))))
        ? 0.0
        : texture(u_mask, uv).r;

    if (u_inverted) m = 1.0 - m;

    // Density compresses the mask towards fully-revealed, which is how the
    // Adjust Mask control behaves: at density 0 the mask stops hiding anything.
    m = mix(1.0, m, clamp(u_density, 0.0, 1.0));
    m *= clamp(u_opacity, 0.0, 1.0);

    frag_color = src * m;
}
