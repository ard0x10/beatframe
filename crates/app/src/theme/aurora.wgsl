// Aurora: a curtain of light that flows along the frame and drifts between the
// two colors. Loudness keeps it moving; the drums only nudge it. The kick
// pushes it a little deeper, the snare warms it toward the accent color and
// the hi-hat glints along the outermost line.
//
// params[0] = (kick, snare, hat, energy)
// params[1] = (flow phase, folds, unused, unused); folds is 1 by default

// Pixels clockwise from the top left corner, along the nearest edge.
fn round_frame(e: Edge) -> f32 {
    let w = u.screen.x;
    let h = u.screen.y;
    let top = e.p.y;
    let bottom = h - e.p.y;
    let left = e.p.x;
    let right = w - e.p.x;
    let nearest = min(min(top, bottom), min(left, right));
    if nearest == top {
        return e.p.x;
    }
    if nearest == right {
        return w + e.p.y;
    }
    if nearest == bottom {
        return w + h + (w - e.p.x);
    }
    return 2.0 * w + h + (h - e.p.y);
}

fn light(e: Edge) -> Light {
    let kick = u.params[0].x;
    let snare = u.params[0].y;
    let hat = u.params[0].z;
    let energy = u.params[0].w;
    let phase = u.params[1].x;
    let folds = u.params[1].y;

    // Three slow waves of different lengths make folds that never repeat exactly.
    let s = round_frame(e);
    let k = 6.2832 * folds / (min(u.screen.x, u.screen.y) * 0.9);
    let curtain = 0.5 + 0.25 * sin(s * k + phase) + 0.15 * sin(s * k * 2.3 - phase * 1.7 + 1.3)
        + 0.1 * sin(s * k * 0.6 + phase * 0.6 + 4.0);

    let depth = e.unit * (0.03 + 0.05 * curtain + 0.02 * kick);
    let glow = falloff(e.d, depth) * (0.2 + 0.35 * curtain);
    let core = exp(-e.d / (0.008 * e.unit)) * (0.15 + 0.2 * curtain);
    let amount = (glow + core) * ((0.6 + 0.4 * energy) * u.resting + 0.35 * kick + 0.25 * snare);

    let line = exp(-e.d / (0.0035 * e.unit));
    let glint = hat * line * 0.35 * (0.5 + 0.5 * sin(s * 0.09 + u.time * 55.0));

    // The color drifts along the frame and with the flow; the snare leans it to the accent.
    let drift = clamp(0.5 + 0.5 * sin(s * k * 0.5 - phase * 0.8) + 0.4 * snare, 0.0, 1.0);
    let color = mix(u.base.rgb, u.accent.rgb, drift);
    let tint = color * amount + mix(u.accent.rgb, vec3f(1.0), 0.3) * glint;
    return Light(tint, amount + glint, PEAK);
}
