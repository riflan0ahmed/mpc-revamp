// Port of MPC-HC's "Denoise.hlsl" to an mpv user shader.
//!HOOK MAIN
//!BIND HOOKED
//!DESC MPC-HC: Denoise

#define width  (HOOKED_size.x)
#define height (HOOKED_size.y)
#define px (HOOKED_pt.x)
#define py (HOOKED_pt.y)
#define clock (float(frame) / 30.0)
#define PI 3.14159265358979
#define T(p) HOOKED_tex(p)

vec4 hook() {
    vec2 tex = HOOKED_pos;
    float tap = 0.1;
    vec4 acc = T(tex);
    for (int i = 0; i < 16; ++i) {
        float dx = tap / width, dy = tap / height;
        acc += (T(tex + vec2(-dx, -dy)) + T(tex + vec2(0.0, -dy)) + T(tex + vec2(-dx, 0.0)) + T(tex + vec2(dx, 0.0))
              + T(tex + vec2(0.0, dy)) + T(tex + vec2(dx, dy)) + T(tex + vec2(-dx, dy)) + T(tex + vec2(dx, -dy))) * 0.125;
        tap += 0.1;
    }
    return vec4((acc / 16.0).rgb, 1.0);
}
