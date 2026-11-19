// Port of MPC-HC's "LCD angle correction.hlsl" to an mpv user shader.
//!HOOK MAIN
//!BIND HOOKED
//!DESC MPC-HC: LCD angle correction

#define width  (HOOKED_size.x)
#define height (HOOKED_size.y)
#define px (HOOKED_pt.x)
#define py (HOOKED_pt.y)
#define clock (float(frame) / 30.0)
#define PI 3.14159265358979
#define T(p) HOOKED_tex(p)

// Brightness, contrast and gamma for the top and bottom of the screen (blended in between).
#define BrightnessTop vec3(0.0)
#define BrightnessBottom vec3(0.0)
#define ContrastTop vec3(1.0)
#define ContrastBottom vec3(1.0)
#define GammaTop vec3(0.8)
#define GammaBottom vec3(1.0)

vec4 hook() {
    vec2 tex = HOOKED_pos;
    vec3 s1 = T(tex).rgb;
    float yi = 1.0 - tex.y;
    s1 = s1 * (yi * ContrastTop + tex.y * ContrastBottom) + yi * BrightnessTop + tex.y * BrightnessBottom;
    return vec4(sign(s1) * pow(abs(s1), yi * GammaTop + tex.y * GammaBottom), 1.0);
}
