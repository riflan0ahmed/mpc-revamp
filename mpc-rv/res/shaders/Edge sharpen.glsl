// Port of MPC-HC's "Edge sharpen.hlsl" to an mpv user shader.
//!HOOK MAIN
//!BIND HOOKED
//!DESC MPC-HC: Edge sharpen

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
    vec4 c0 = T(tex);
    vec4 c1 = T(tex + vec2(-dx, -dy)), c2 = T(tex + vec2(0.0, -dy)), c3 = T(tex + vec2(dx, -dy));
    vec4 c4 = T(tex + vec2(-dx, 0.0)), c5 = T(tex + vec2(dx, 0.0));
    vec4 c6 = T(tex + vec2(-dx, dy)), c7 = T(tex + vec2(0.0, dy)), c8 = T(tex + vec2(dx, dy));
    vec4 d1 = c6 + c4 + c1 - c3 - c5 - c8;
    vec4 d2 = c4 + c1 + c2 - c5 - c8 - c7;
    vec4 d3 = c1 + c2 + c3 - c8 - c7 - c6;
    vec4 d4 = c2 + c3 + c5 - c7 - c6 - c4;
    float value = length(abs(d1) + abs(d2) + abs(d3) + abs(d4)) / 6.0;
    if (value > 0.2)
        return vec4((c0 * 2.0 - (c1 + c2 + c3 + c4 + c5 + c6 + c7 + c8) * 0.125).rgb, 1.0);
    return c0;
}
