// Port of MPC-HC's "Letterbox.hlsl" to an mpv user shader.
//!HOOK MAIN
//!BIND HOOKED
//!DESC MPC-HC: Letterbox

#define width  (HOOKED_size.x)
#define height (HOOKED_size.y)
#define px (HOOKED_pt.x)
#define py (HOOKED_pt.y)
#define clock (float(frame) / 30.0)
#define PI 3.14159265358979
#define T(p) HOOKED_tex(p)

vec4 hook() {
    vec2 tex = HOOKED_pos;
    vec2 ar = vec2(16.0, 9.0);
    float h = (1.0 - width / height * ar.y / ar.x) / 2.0;
    return (tex.y >= h && tex.y <= 1.0 - h) ? T(tex) : vec4(0.0, 0.0, 0.0, 1.0);
}
