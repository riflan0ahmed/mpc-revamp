// Port of MPC-HC's "YV12 chroma upsampling.hlsl" to an mpv user shader.
//!HOOK MAIN
//!BIND HOOKED
//!DESC MPC-HC: YV12 chroma upsampling

#define width  (HOOKED_size.x)
#define height (HOOKED_size.y)
#define px (HOOKED_pt.x)
#define py (HOOKED_pt.y)
#define clock (float(frame) / 30.0)
#define PI 3.14159265358979
#define T(p) HOOKED_tex(p)

vec3 rgb2yuv(vec3 c) { return mat3(0.299, -0.147, 0.615, 0.587, -0.289, -0.515, 0.114, 0.436, -0.100) * c; }
vec3 yuv2rgb(vec3 c) { return mat3(1.0, 1.0, 1.0, 0.0, -0.395, 2.032, 1.140, -0.581, 0.0) * c; }

vec4 hook() {
    vec2 tex = HOOKED_pos;
    float dx = 1.0 / width, dy = 1.0 / height;
    vec3 yuv11 = rgb2yuv(T(tex).rgb);
    vec3 yuv = (rgb2yuv(T(tex + vec2(-dx, -dy)).rgb) + 2.0 * rgb2yuv(T(tex + vec2(-dx, 0.0)).rgb) + rgb2yuv(T(tex + vec2(-dx, dy)).rgb)
              + 2.0 * rgb2yuv(T(tex + vec2(0.0, -dy)).rgb) + 4.0 * yuv11 + 2.0 * rgb2yuv(T(tex + vec2(0.0, dy)).rgb)
              + rgb2yuv(T(tex + vec2(dx, -dy)).rgb) + 2.0 * rgb2yuv(T(tex + vec2(dx, 0.0)).rgb) + rgb2yuv(T(tex + vec2(dx, dy)).rgb)) / 16.0;
    yuv.x = yuv11.x;
    return vec4(yuv2rgb(yuv), 1.0);
}
