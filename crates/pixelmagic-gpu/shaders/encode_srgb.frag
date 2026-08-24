// Converts the working format (premultiplied, linear light) to straight-alpha
// sRGB for readback.
//
// This exists so that reading an image back never depends on floating-point
// ReadPixels. GLES guarantees exactly one readable format — RGBA8 — plus one
// implementation-defined pair, so asking a half-float target for GL_FLOAT
// works on desktop GL and is a coin toss elsewhere. Encoding in a shader first
// and reading bytes works the same everywhere, and loses nothing: the result
// was going to be 8-bit regardless.

in vec2 v_uv;
out vec4 frag_color;

uniform sampler2D u_image;

void main() {
    vec4 c = texture(u_image, v_uv);
    if (c.a <= EPS) {
        frag_color = vec4(0.0);
        return;
    }
    frag_color = vec4(linear_to_srgb(c.rgb / c.a), c.a);
}
