// Band: a solid strip hugs the edge of the screen, its inner side soft and a
// little uneven, and a wide glow fades inward from it. One color blends into
// the other along the frame. The music turns the colors round the frame:
// loudness sets the pace and each kick pushes them on. The kick also swells
// the band, the snare brightens the glow and the hi-hat stirs its inner edge.
//
// params[0] = (kick, snare, hat, energy)
// params[1] = (turn, stir, waves, corners): how far the colors have turned, in
//   radians; where the waves of the inner edges are; how much those edges
//   wave, 1 by default; how round the corners are, from 0 for square to 1 for
//   the roundness the other themes have

// The strip is meant to hide what is under it, unlike the other themes' light.
const SOLID: f32 = 1.0;

fn light(e: Edge) -> Light {
    let kick = min(u.params[0].x, 1.5);
    let snare = u.params[0].y;
    let hat = min(u.params[0].z, 1.0);
    let energy = u.params[0].w;
    let turn = u.params[1].x;
    let stir = u.params[1].y;
    let waves = u.params[1].z;
    // The frame's own distance, with the corners as round as asked; the strip
    // shows a corner's curve far more than a soft glow does.
    let d = smin(e.dx, e.dy, max(0.06 * e.unit * u.params[1].w, 0.001));

    // The angle round the middle of the screen carries the blend smoothly
    // through the corners; one color faces the other across the screen.
    let middle = u.screen * 0.5;
    let angle = atan2(e.p.y - middle.y, e.p.x - middle.x);
    let blend = smoothstep(0.0, 1.0, 0.5 + 0.5 * cos(angle - turn));
    let color = mix(u.base.rgb, u.accent.rgb, blend);

    // Whole numbers of waves round the frame, so they meet without a seam.
    let wave = 0.55 * sin(angle * 6.0 + stir) + 0.3 * sin(angle * 11.0 - stir * 1.3 + 1.7)
        + 0.15 * sin(angle * 17.0 + stir * 0.7 + 4.1);
    let slow = 0.6 * sin(angle * 3.0 + stir * 0.5) + 0.4 * sin(angle * 7.0 - stir * 0.8 + 2.0);

    // The glow, fading inward from the strip.
    let depth = e.unit * (0.075 + 0.03 * kick) * (1.0 + 0.15 * waves * (1.0 + 0.5 * hat) * wave);
    let body = falloff(d, depth);
    let rim = exp(-d / (0.012 * e.unit));
    let level = (0.55 + 0.25 * energy) * u.resting + 0.35 * kick + 0.3 * snare;
    let glow = (0.9 * body + 0.6 * rim) * level;

    // The strip: solid at the edge, its inner side a soft slope that swells
    // and narrows slowly along the frame.
    let width = e.unit * (0.011 + 0.004 * kick) * (1.0 + 0.3 * waves * slow);
    let strip = 1.0 - smoothstep(width * 0.55, width * 1.25, d);
    let solid = strip * 5.0 * clamp((0.85 + 0.15 * energy) * u.resting + 0.3 * kick, 0.0, 1.2);

    let amount = glow + solid;
    return Light(color * amount, amount, SOLID);
}
