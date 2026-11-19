// Port of MPC-HC's "Sphere.hlsl" to an mpv user shader.
//!HOOK MAIN
//!BIND HOOKED
//!DESC MPC-HC: Sphere

#define width  (HOOKED_size.x)
#define height (HOOKED_size.y)
#define px (HOOKED_pt.x)
#define py (HOOKED_pt.y)
#define clock (float(frame) / 30.0)
#define PI 3.14159265358979
#define T(p) HOOKED_tex(p)

vec4 hook() {
    vec2 tex = HOOKED_pos;
    vec3 pl = vec3(3.0, -3.0, -4.0); // light position
    vec4 cl = vec4(0.4);             // light color
    vec3 pc = vec3(0.0, 0.0, -1.0);  // camera position
    vec3 ps = vec3(0.0, 0.0, 0.5);   // sphere position
    float r = 0.65;                  // sphere radius
    vec3 pd = normalize(vec3(tex.x - 0.5, tex.y - 0.5, 0.0) - pc);
    float B = 2.0 * dot(pd, pc - ps);
    float C = dot(pc - ps, pc - ps) - r * r;
    float D = B * B - 4.0 * C;
    if (D < 0.0)
        return vec4(0.0, 0.0, 0.0, 1.0);
    float t = (-B - sqrt(D)) / 2.0;
    vec3 p = pc + pd * t;
    vec3 n = normalize(p - ps);
    vec3 l = normalize(pl - p);
    tex = acos(-n.xy) / PI;
    tex.x = fract(tex.x + fract(clock / 10.0));
    vec4 c0 = T(tex) * dot(n, l) + cl * pow(max(dot(l, reflect(pd, n)), 0.0), 50.0);
    return vec4(c0.rgb, 1.0);
}
