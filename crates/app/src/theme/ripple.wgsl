// Ripple: every kick starts a wave at the middle of the bottom edge. It splits,
// runs along the bottom to both corners, climbs the sides and fades where the
// two halves meet at the middle of the top. The snare flashes the whole frame,
// the hi-hat throws sparks along the outermost line.
//
// params[0] = (snare, hat, energy, seconds a wave takes to reach the top)
// params[1..5] = up to eight waves as (age, strength) pairs, two per vec4
// params[5] = (tail length, spark amount, unused, unused), both 1 by default

// Path length from the middle of the bottom edge, going either way round.
fn along_frame(e: Edge) -> f32 {
    let half = u.screen.x * 0.5;
    if e.dx < e.dy {
        return half + (u.screen.y - e.p.y);
    }
    if e.p.y > u.screen.y * 0.5 {
        return abs(e.p.x - half);
    }
    return half + u.screen.y + (half - abs(e.p.x - half));
}

fn wave(s: f32, age: f32, strength: f32, travel: f32, unit: f32) -> f32 {
    let t = age / travel;
    let front = t * (u.screen.x + u.screen.y);
    let gap = s - front;
    // A sharp leading edge and a longer tail behind it.
    let width = unit * 0.05;
    let shape = select(exp(gap / (unit * 0.18 * u.params[5].x)), exp(-(gap * gap) / (width * width)), gap > 0.0);
    // Fades out as the two halves meet at the top.
    let fade = 1.0 - smoothstep(0.8, 1.0, t);
    return strength * shape * fade;
}

fn light(e: Edge) -> Light {
    let snare = u.params[0].x;
    let hat = u.params[0].y;
    let energy = u.params[0].z;
    let travel = u.params[0].w;

    let s = along_frame(e);
    var ripple = 0.0;
    for (var i = 0u; i < 8u; i++) {
        let pair = u.params[1u + i / 2u];
        let w = select(pair.xy, pair.zw, (i & 1u) == 1u);
        ripple += wave(s, w.x, w.y, travel, e.unit);
    }
    ripple = min(ripple, 1.5);

    let reach = e.unit * (0.035 + 0.06 * min(ripple, 1.0));
    let core = exp(-e.d / (0.007 * e.unit));
    let rest = (falloff(e.d, e.unit * 0.05) * (0.12 + 0.1 * energy) + core * (0.2 + 0.1 * energy)) * u.resting;
    let waves = falloff(e.d, reach) * 0.6 * ripple + core * 0.5 * ripple;
    let flash = snare * (0.35 * falloff(e.d, e.unit * 0.07) + 0.4 * core);

    // Sparks: short stretches of the edge that blink on their own random beat.
    let segment = floor(s / 18.0);
    let tick = floor(u.time * 24.0);
    let spark = step(1.0 - 0.45 * hat * u.params[5].y, hash(vec2f(segment, tick)));
    let sparks = spark * hat * exp(-e.d / (0.0035 * e.unit)) * 0.9;

    let tint = u.base.rgb * (rest + waves) + u.accent.rgb * flash + mix(u.accent.rgb, vec3f(1.0), 0.3) * sparks;
    return Light(tint, rest + waves + flash + sparks, PEAK);
}
