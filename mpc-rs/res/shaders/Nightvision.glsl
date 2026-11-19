// Port of MPC-HC's "Nightvision.hlsl" to an mpv user shader.
//!HOOK MAIN
//!BIND HOOKED
//!DESC MPC-HC: Nightvision

#define width  (HOOKED_size.x)
#define height (HOOKED_size.y)
#define px (HOOKED_pt.x)
#define py (HOOKED_pt.y)
#define clock (float(frame) / 30.0)
#define PI 3.14159265358979
#define T(p) HOOKED_tex(p)

vec4 hook() {
    vec4 s = T(HOOKED_pos);
    float c = dot(s, vec4(0.2, 0.6, 0.1, 0.1));
    return vec4(0.0, c, 0.0, 1.0);
}
