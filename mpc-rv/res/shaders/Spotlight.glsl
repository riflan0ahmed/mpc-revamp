// Port of MPC-HC's "Spotlight.hlsl" to an mpv user shader.
//!HOOK MAIN
//!BIND HOOKED
//!DESC MPC-HC: Spotlight

#define width  (HOOKED_size.x)
#define height (HOOKED_size.y)
#define px (HOOKED_pt.x)
#define py (HOOKED_pt.y)
#define clock (float(frame) / 30.0)
#define PI 3.14159265358979
#define T(p) HOOKED_tex(p)

vec4 hook() {
    vec2 tex = HOOKED_pos;
    vec4 c0 = T(tex);
    vec3 lightsrc = vec3(sin(clock * PI / 1.5) / 2.0 + 0.5, cos(clock * PI) / 2.0 + 0.5, 1.0);
    vec3 light = normalize(lightsrc - vec3(tex, 0.0));
    return vec4(c0.rgb * pow(dot(light, vec3(0.0, 0.0, 1.0)), 50.0), 1.0);
}
