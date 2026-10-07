// Shared by every theme. A theme adds `fn light(e: Edge) -> Light`, which says
// how much light falls on a pixel and in which color; this file turns that into
// premultiplied output with the same peak cap and dithering for all themes, and
// applies the user's thickness and brightness the same way for every theme.
// Focus mode fades the finished light, so even Band's solid strip turns see-through.

struct Uniforms {
    origin: vec2f,
    screen: vec2f,
    base: vec4f,
    accent: vec4f,
    time: f32,
    // Factor on every size a theme draws with.
    thickness: f32,
    // Factor on how much light there is, before the peak cap.
    brightness: f32,
    // Factor each theme puts on its glow between hits.
    resting: f32,
    // Theme-specific values, laid out by each theme.
    params: array<vec4f, 8>,
    // Share of the finished light that shows, color and coverage alike; focus
    // mode lowers it while the user works.
    shown: f32,
}

@group(0) @binding(0) var<uniform> u: Uniforms;

// Coverage approaches this but never reaches it, so peaks stop short of a solid
// band. A theme that draws a solid strip on purpose asks for more.
const PEAK: f32 = 0.72;

struct Edge {
    // Pixel position on the whole screen.
    p: vec2f,
    // Shorter screen side times the thickness; themes size everything against it.
    unit: f32,
    // Distance to the nearest vertical and horizontal edge.
    dx: f32,
    dy: f32,
    // Distance to the frame, with softly rounded corners.
    d: f32,
}

struct Light {
    // Sum of color times amount over every layer.
    tint: vec3f,
    amount: f32,
    // The most coverage this light may reach, PEAK for most themes.
    peak: f32,
}

@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4f {
    let xy = vec2f(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4f(xy * 2.0 - 1.0, 0.0, 1.0);
}

fn smin(a: f32, b: f32, k: f32) -> f32 {
    let h = max(k - abs(a - b), 0.0) / k;
    return min(a, b) - h * h * k * 0.25;
}

fn falloff(d: f32, reach: f32) -> f32 {
    let t = 1.0 - smoothstep(0.0, reach, d);
    return t * t;
}

fn hash(p: vec2f) -> f32 {
    return fract(sin(dot(p, vec2f(12.9898, 78.233))) * 43758.5453);
}

@fragment
fn fs(@builtin(position) frag: vec4f) -> @location(0) vec4f {
    let p = frag.xy + u.origin;
    let unit = min(u.screen.x, u.screen.y) * u.thickness;
    let dx = min(p.x, u.screen.x - p.x);
    let dy = min(p.y, u.screen.y - p.y);
    let l = light(Edge(p, unit, dx, dy, smin(dx, dy, 0.06 * unit)));

    let a = l.peak * (1.0 - exp(-l.amount * u.brightness / l.peak));
    var rgb = l.tint / max(l.amount, 1e-4) * a;

    // Dither keeps the long, faint gradient from banding in 8 bits.
    let n = (hash(frag.xy) - 0.5) / 255.0;
    rgb = clamp(rgb + n, vec3f(0.0), vec3f(a));
    return vec4f(rgb, a) * u.shown;
}
