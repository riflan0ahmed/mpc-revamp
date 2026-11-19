// Port of MPC-HC's "Wave.hlsl" to an mpv user shader.
//!HOOK MAIN
//!BIND HOOKED
//!DESC MPC-HC: Wave

#define width  (HOOKED_size.x)
#define height (HOOKED_size.y)
#define px (HOOKED_pt.x)
#define py (HOOKED_pt.y)
#define clock (float(frame) / 30.0)
#define PI 3.14159265358979
#define T(p) HOOKED_tex(p)

vec4 hook() {
    vec2 tex = HOOKED_pos;
    tex.x += sin(tex.x + clock / 0.3) / 20.0;
    tex.y += sin(tex.x + clock / 0.3) / 20.0;
    if (tex.x >= 0.0 && tex.x <= 1.0 && tex.y >= 0.0 && tex.y <= 1.0)
        return T(tex);
    return vec4(0.0, 0.0, 0.0, 1.0);
}
