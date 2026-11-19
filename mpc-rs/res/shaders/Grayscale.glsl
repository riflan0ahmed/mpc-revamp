// Port of MPC-HC's "Grayscale.hlsl" to an mpv user shader.
//!HOOK MAIN
//!BIND HOOKED
//!DESC MPC-HC: Grayscale

#define width  (HOOKED_size.x)
#define height (HOOKED_size.y)
#define px (HOOKED_pt.x)
#define py (HOOKED_pt.y)
#define clock (float(frame) / 30.0)
#define PI 3.14159265358979
#define T(p) HOOKED_tex(p)

vec4 hook() {
    float c0 = dot(T(HOOKED_pos).rgb, vec3(0.299, 0.587, 0.114));
    return vec4(vec3(c0), 1.0);
}
