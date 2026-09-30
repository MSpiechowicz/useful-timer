in vec3 a_position;
in vec3 a_normal;
in float a_material;
in float a_shade;
in float a_petal;
uniform vec3 u_root[18];
uniform vec2 u_radial[18];
uniform float u_angle[18];
uniform vec2 u_elevation;
out vec3 v_normal;
out float v_material;
out float v_shade;
flat out float v_two_sided;

vec3 articulate(vec3 p, vec2 radial, float s, float c) {
    float outward = p.x * c + p.z * s;
    return vec3(outward * radial.x - p.y * radial.y,
                outward * radial.y + p.y * radial.x,
                p.z * c - p.x * s);
}

void main() {
    vec3 p = a_position;
    vec3 n = a_normal;
    if (a_petal >= 0.0) {
        int petal = int(a_petal);
        float s = sin(u_angle[petal]);
        float c = cos(u_angle[petal]);
        p = u_root[petal] + articulate(p, u_radial[petal], s, c);
        n = articulate(n, u_radial[petal], s, c);
    }
    float depth = dot(p.yz, u_elevation);
    float distance = 900.0 - depth;
    float screen_y = p.y * u_elevation.y - p.z * u_elevation.x;
    // Perspective and depth use the same camera. Per-fragment depth testing
    // resolves overlap instead of guessing an order from triangle centers.
    gl_Position = vec4(p.x * 900.0 / 160.0,
                       (-39.0 * distance - screen_y * 900.0) / 160.0,
                       3.0 * distance - 2400.0,
                       distance);
    v_normal = n;
    v_material = a_material;
    v_shade = a_shade;
    v_two_sided = a_petal >= 0.0 ? 1.0 : 0.0;
}
