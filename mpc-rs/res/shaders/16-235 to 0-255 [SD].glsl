// Port of MPC-HC's "16-235 to 0-255 [SD].hlsl" to an mpv user shader.
//!HOOK MAIN
//!BIND HOOKED
//!DESC MPC-HC: 16-235 to 0-255 [SD]

#define width  (HOOKED_size.x)
#define height (HOOKED_size.y)
#define px (HOOKED_pt.x)
#define py (HOOKED_pt.y)
#define clock (float(frame) / 30.0)
#define PI 3.14159265358979
#define T(p) HOOKED_tex(p)

vec4 hook() {
    vec4 c0 = T(HOOKED_pos);
    if (height >= 720.0)
        return c0;
    return vec4((c0.rgb - 16.0 / 255.0) * (255.0 / 219.0), 1.0);
}
