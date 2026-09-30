in vec3 v_normal;
in float v_material;
in float v_shade;
flat in float v_two_sided;
uniform vec2 u_elevation;
out vec4 out_color;

void main() {
    vec3 view = vec3(0.0, u_elevation);
    const vec3 light = vec3(-0.42, 0.43, 0.80);
    vec3 n = normalize(v_normal);
    float facing = dot(n, view);
    // Solid hardware keeps its outward normal through grazing angles. Flipping
    // there produces a discontinuous highlight along the shallow plinth bevel.
    if (v_two_sided > 0.5 && facing < 0.0) n = -n;
    float diffuse = max(dot(n, light), 0.0);
    float highlight = max(dot(n, normalize(view + light)), 0.0);
    float gold = clamp(v_material, 0.0, 1.0);
    vec3 base = mix(vec3(0.969, 0.937, 0.894), vec3(0.871, 0.745, 0.486), gold);
    float strength = mix(0.49 + 0.49 * diffuse, 0.30 + 0.65 * diffuse, gold);
    float gloss = mix(0.15 * pow(highlight, 32.0), 0.42 * pow(highlight, 48.0), gold);
    if (v_material > 1.5) {
        base = vec3(0.173, 0.192, 0.208);
        strength = 0.35 + 0.55 * diffuse;
        gloss = 0.12 * pow(highlight, 24.0);
    }
    float inner = facing < 0.0 && v_material < 0.5 ? 0.94 : 1.0;
    out_color = vec4(clamp(base * strength * v_shade * inner + vec3(gloss), 0.0, 1.0), 1.0);
}
