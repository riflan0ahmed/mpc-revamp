// Port of MPC-HC's "0-255 to 16-235.hlsl" to an mpv user shader.
//!HOOK MAIN
//!BIND HOOKED
//!DESC MPC-HC: 0-255 to 16-235

#define width  (HOOKED_size.x)
#define height (HOOKED_size.y)
#define px (HOOKED_pt.x)
#define py (HOOKED_pt.y)
#define clock (float(frame) / 30.0)
#define PI 3.14159265358979
#define T(p) HOOKED_tex(p)

vec4 hook() {
    vec4 c0 = T(HOOKED_pos);
    return vec4(c0.rgb * (219.0 / 255.0) + 16.0 / 255.0, 1.0);
}
