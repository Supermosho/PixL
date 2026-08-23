// Crystallize: a Voronoi cellularisation. Each output pixel takes the colour
// at its nearest jittered cell centre.

in vec2 v_uv;
out vec4 frag_color;

uniform sampler2D u_image;
uniform vec2 u_size_px;
uniform float u_radius;

void main() {
    float s = max(u_radius, 1.0);
    vec2 p = v_uv * u_size_px / s;
    vec2 cell = floor(p);

    float best = 1e9;
    vec2 best_point = p;
    for (int y = -1; y <= 1; y++) {
        for (int x = -1; x <= 1; x++) {
            vec2 c = cell + vec2(x, y);
            vec2 site = c + hash22(c);
            float d = distance(p, site);
            if (d < best) {
                best = d;
                best_point = site;
            }
        }
    }
    frag_color = texture(u_image, clamp(best_point * s / u_size_px, 0.0, 1.0));
}
