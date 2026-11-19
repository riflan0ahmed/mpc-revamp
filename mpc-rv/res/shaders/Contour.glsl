// Port of MPC-HC's "Contour.hlsl" to an mpv user shader.
//!HOOK MAIN
//!BIND HOOKED
//!DESC MPC-HC: Contour

#define width  (HOOKED_size.x)
#define height (HOOKED_size.y)
#define px (HOOKED_pt.x)
#define py (HOOKED_pt.y)
#define clock (float(frame) / 30.0)
#define PI 3.14159265358979
#define T(p) HOOKED_tex(p)

vec4 hook() {
    vec2 tex = HOOKED_pos;
    float dx = 4.0 / width, dy = 4.0 / height;
    vec4 c2 = T(tex + vec2(0.0, -dy));
    vec4 c4 = T(tex + vec2(-dx, 0.0));
    vec4 c5 = T(tex);
    vec4 c6 = T(tex + vec2(dx, 0.0));
    vec4 c8 = T(tex + vec2(0.0, dy));
    vec4 c0 = -c2 - c4 + c5 * 4.0 - c6 - c8;
    return length(c0) < 1.0 ? vec4(0.0, 0.0, 0.0, 1.0) : vec4(1.0);
}
