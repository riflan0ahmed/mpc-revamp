// Port of MPC-HC's "Sharpen complex 2.hlsl" to an mpv user shader.
//!HOOK MAIN
//!BIND HOOKED
//!DESC MPC-HC: Sharpen complex 2

#define width  (HOOKED_size.x)
#define height (HOOKED_size.y)
#define px (HOOKED_pt.x)
#define py (HOOKED_pt.y)
#define clock (float(frame) / 30.0)
#define PI 3.14159265358979
#define T(p) HOOKED_tex(p)

#define CoefBlur 2.0
#define CoefOrig (1.0 + CoefBlur)
#define SharpenEdge 0.2
#define Sharpen_val0 2.0
#define Sharpen_val1 ((Sharpen_val0 - 1.0) / 8.0)

vec4 hook() {
    vec2 tex = HOOKED_pos;
    float dx = 0.6 * px, dy = 0.6 * py;
    vec4 orig = T(tex);
    vec4 c1 = T(tex + vec2(-dx, -dy)), c2 = T(tex + vec2(0.0, -dy)), c3 = T(tex + vec2(dx, -dy));
    vec4 c4 = T(tex + vec2(-dx, 0.0)), c5 = T(tex + vec2(dx, 0.0));
    vec4 c6 = T(tex + vec2(-dx, dy)), c7 = T(tex + vec2(0.0, dy)), c8 = T(tex + vec2(dx, dy));
    vec4 flou = (c1 + c3 + c6 + c8 + 2.0 * (c2 + c4 + c5 + c7) + 4.0 * orig) * 0.0625;
    vec4 corrected = CoefOrig * orig - CoefBlur * flou;
    dx = px; dy = py;
    c1 = T(tex + vec2(-dx, -dy)), c2 = T(tex + vec2(0.0, -dy)), c3 = T(tex + vec2(dx, -dy));
    c4 = T(tex + vec2(-dx, 0.0)), c5 = T(tex + vec2(dx, 0.0));
    c6 = T(tex + vec2(-dx, dy)), c7 = T(tex + vec2(0.0, dy)), c8 = T(tex + vec2(dx, dy));
    // HLSL assigned these vectors to floats, which keeps the first component.
    float d1 = ((c3 + 2.0 * c5 + c8) - (c1 + 2.0 * c4 + c6)).x;
    c1 += c3;
    c6 += c8;
    float d2 = ((c6 + 2.0 * c7) - (c1 + 2.0 * c2)).x;
    if (sqrt(d1 * d1 + d2 * d2) > SharpenEdge)
        return vec4((orig * Sharpen_val0 - (c1 + c2 + c4 + c5 + c6 + c7) * Sharpen_val1).rgb, 1.0);
    return vec4(corrected.rgb, 1.0);
}
