// Split: each drum owns an edge. The kick lights the bottom in the base color,
// the snare both sides in the accent color, the hi-hat the top. Where two edges
// meet in a corner their light adds up.
//
// params[0] = (kick, snare, hat, energy)
// params[1].x = 1 keeps a faint line on a quiet edge, 0 lets it go dark

// Light along one edge, `dist` pixels away from it, for a drum at `hit`.
fn band(dist: f32, unit: f32, hit: f32, rest: f32) -> f32 {
    let reach = unit * (0.04 + 0.05 * hit);
    let body = falloff(dist, reach) * (0.6 * rest + 0.55 * hit);
    let core = exp(-dist / (0.007 * unit)) * (rest + 0.4 * hit);
    return body + core;
}

fn light(e: Edge) -> Light {
    let kick = u.params[0].x;
    let snare = u.params[0].y;
    let hat = u.params[0].z;
    let energy = u.params[0].w;
    let rest = u.params[1].x * (0.2 + 0.1 * energy);

    let top = e.p.y;
    let bottom = u.screen.y - e.p.y;
    let side = e.dx;

    let low = band(bottom, e.unit, kick, rest);
    let sides = band(side, e.unit, snare, rest);

    // The hi-hat edge is thinner and shimmers along its length.
    let wave = 0.5 + 0.5 * sin(e.p.x * 0.09 + u.time * 55.0);
    let slow = 0.5 + 0.5 * sin(e.p.x * 0.023 - u.time * 31.0);
    let high = band(top, e.unit * 0.7, hat * (0.45 + 0.55 * wave * slow), rest);

    let hat_color = mix(u.accent.rgb, vec3f(1.0), 0.3);
    let tint = u.base.rgb * low + u.accent.rgb * sides + hat_color * high;
    return Light(tint, low + sides + high);
}
