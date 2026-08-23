// Fullscreen pass vertex shader.
//
// Draws a single oversized triangle rather than two triangles. There is no
// diagonal seam to rasterise twice, and the GPU culls the off-screen part for
// free — a small win repeated once per pass, and there are a lot of passes.

out vec2 v_uv;

void main() {
    vec2 p = vec2((gl_VertexID << 1) & 2, gl_VertexID & 2);
    v_uv = p;
    gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}
