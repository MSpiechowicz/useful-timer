in vec2 v_uv;
uniform sampler2D u_texture;
uniform vec2 u_output_size;
out vec4 out_color;

void main() {
    // Exact pixel-area resolve, not a blur kernel or interpolated texture scale.
    ivec2 pixel = ivec2(v_uv * u_output_size) * 2;
    out_color = 0.25 * (texelFetch(u_texture, pixel, 0)
                     + texelFetch(u_texture, pixel + ivec2(1, 0), 0)
                     + texelFetch(u_texture, pixel + ivec2(0, 1), 0)
                     + texelFetch(u_texture, pixel + ivec2(1, 1), 0));
}
