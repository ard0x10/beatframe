// Layered: a soft rim in the base color that the kick pushes inward, the snare
// flashing in the accent color and the hi-hat shimmering along the outermost
// line.
//
// params[0] = (kick, snare, hat, energy)

fn light(e: Edge) -> Light {
    let kick = u.params[0].x;
    let snare = u.params[0].y;
    let hat = u.params[0].z;
    let energy = u.params[0].w;

    let reach = e.unit * (0.05 + 0.015 * energy + 0.035 * kick);
    let body = falloff(e.d, reach) * (0.2 + 0.18 * energy + 0.3 * kick);
    let core = exp(-e.d / (0.007 * e.unit)) * (0.25 + 0.15 * energy + 0.2 * kick);
    let flash = snare * (0.45 * falloff(e.d, reach * 1.1) + 0.5 * core);

    // Position along the nearest edge drives the hi-hat shimmer.
    let along = select(e.p.x, e.p.y, e.dx < e.dy);
    let wave = 0.5 + 0.5 * sin(along * 0.09 + u.time * 55.0);
    let slow = 0.5 + 0.5 * sin(along * 0.023 - u.time * 31.0);
    let line = exp(-e.d / (0.0035 * e.unit));
    let shimmer = hat * line * (0.35 + 0.65 * wave * slow) * 0.8;

    let tint = u.base.rgb * (body + core) + u.accent.rgb * flash + mix(u.accent.rgb, vec3f(1.0), 0.3) * shimmer;
    return Light(tint, body + core + flash + shimmer);
}
