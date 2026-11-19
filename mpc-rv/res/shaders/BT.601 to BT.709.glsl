// Port of MPC-HC's "BT.601 to BT.709.hlsl" to an mpv user shader.
//!HOOK MAIN
//!BIND HOOKED
//!DESC MPC-HC: BT.601 to BT.709

#define width  (HOOKED_size.x)
#define height (HOOKED_size.y)
#define px (HOOKED_pt.x)
#define py (HOOKED_pt.y)
#define clock (float(frame) / 30.0)
#define PI 3.14159265358979
#define T(p) HOOKED_tex(p)

vec4 hook() {
    vec4 si = T(HOOKED_pos);
    if (width < 1120.0 && height < 630.0)
        return si; // SD video is left alone
    vec3 s1 = si.rgb;
    s1 = s1.rrr * vec3(0.299, -0.1495 / 0.886, 0.5) + s1.ggg * vec3(0.587, -0.2935 / 0.886, -0.2935 / 0.701) + s1.bbb * vec3(0.114, 0.5, -0.057 / 0.701);
    return vec4(s1.rrr + vec3(0.0, -0.1674679 / 0.894, 1.8556) * s1.ggg + vec3(1.5748, -0.4185031 / 0.894, 0.0) * s1.bbb, 1.0);
}
