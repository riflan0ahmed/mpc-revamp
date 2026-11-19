// Port of MPC-HC's "Procamp.hlsl" to an mpv user shader.
//!HOOK MAIN
//!BIND HOOKED
//!DESC MPC-HC: Procamp

#define width  (HOOKED_size.x)
#define height (HOOKED_size.y)
#define px (HOOKED_pt.x)
#define py (HOOKED_pt.y)
#define clock (float(frame) / 30.0)
#define PI 3.14159265358979
#define T(p) HOOKED_tex(p)

// Brightness: -1.0 to 1.0, default 0.0
// Contrast: 0.0 to 10.0, default 1.0
// Hue: -180.0 to +180.0, default 0.0
// Saturation: 0.0 to 10.0, default 1.0
#define Brightness 0.0
#define Contrast   1.0
#define Hue        0.0
#define Saturation 1.0
#define ymin (16.0 / 255.0)

vec4 hook() {
    vec3 c = T(HOOKED_pos).rgb;
    // RGB -> YUV (the HLSL matrices are row-major, so they're written transposed here).
    vec3 yuv = mat3(0.299, -0.147, 0.615, 0.587, -0.289, -0.515, 0.114, 0.437, -0.100) * c;
    yuv.x = Contrast * (yuv.x - ymin) + ymin + Brightness;
    float a = Hue * PI / 180.0;
    yuv.yz = mat2(cos(a), -sin(a), sin(a), cos(a)) * yuv.yz * Saturation;
    return vec4(mat3(1.0, 1.0, 1.0, 0.0, -0.394, 2.028, 1.140, -0.581, 0.0) * yuv, 1.0);
}
