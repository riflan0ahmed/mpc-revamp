// Port of MPC-HC's "Deinterlace (blend).hlsl" to an mpv user shader.
//!HOOK MAIN
//!BIND HOOKED
//!DESC MPC-HC: Deinterlace (blend)

#define width  (HOOKED_size.x)
#define height (HOOKED_size.y)
#define px (HOOKED_pt.x)
#define py (HOOKED_pt.y)
#define clock (float(frame) / 30.0)
#define PI 3.14159265358979
#define T(p) HOOKED_tex(p)

vec4 hook() {
    vec2 tex = HOOKED_pos;
    vec2 h = vec2(0.0, 1.0 / height);
    return (T(tex) * 2.0 + T(tex - h) + T(tex + h)) / 4.0;
}
