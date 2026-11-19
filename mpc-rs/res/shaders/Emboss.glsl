// Port of MPC-HC's "Emboss.hlsl" to an mpv user shader.
//!HOOK MAIN
//!BIND HOOKED
//!DESC MPC-HC: Emboss

#define width  (HOOKED_size.x)
#define height (HOOKED_size.y)
#define px (HOOKED_pt.x)
#define py (HOOKED_pt.y)
#define clock (float(frame) / 30.0)
#define PI 3.14159265358979
#define T(p) HOOKED_tex(p)

vec4 hook() {
    vec2 tex = HOOKED_pos;
    float dx = 1.0 / width, dy = 1.0 / height;
    vec4 c0 = -T(tex + vec2(-dx, -dy)) - T(tex + vec2(0.0, -dy)) - T(tex + vec2(-dx, 0.0))
            + T(tex + vec2(dx, 0.0)) + T(tex + vec2(0.0, dy)) + T(tex + vec2(dx, dy));
    return vec4(vec3((c0.r + c0.g + c0.b) / 3.0 + 0.5), 1.0);
}
