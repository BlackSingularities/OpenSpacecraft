// OpenSpacecraft – scena kosmiczna: gwiazdy, ciała niebieskie, atmosfery, pierścienie, linie i punkty.
// Wszystkie położenia są względne wobec kamery (oko w początku układu), z odwróconą głębią (bliżej = 1).

struct Globals {
    view_proj: mat4x4<f32>,
    // sama rotacja widoku z rzutowaniem (gwiazdy w nieskończoności)
    sky_proj: mat4x4<f32>,
    // szerokość, wysokość (px), 1/szer., 1/wys.
    viewport: vec4<f32>,
    // położenie Słońca względem kamery, jasność gwiazd
    sun_rel: vec4<f32>,
    // czas [s], ekspozycja, płaszczyzna bliska [m], -
    params: vec4<f32>,
};

@group(0) @binding(0) var<uniform> g: Globals;

/* ── gwiazdy ─────────────────────────────────────────────────────────── */

struct StarIn {
    @location(0) dir: vec3<f32>,
    @location(1) mag: f32,
    @location(2) color: vec3<f32>,
};

struct StarOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

const QUAD = array<vec2<f32>, 6>(vec2(-1.0, -1.0), vec2(1.0, -1.0), vec2(1.0, 1.0), vec2(-1.0, -1.0), vec2(1.0, 1.0), vec2(-1.0, 1.0));

@vertex
fn vs_star(@builtin(vertex_index) vi: u32, s: StarIn) -> StarOut {
    var o: StarOut;
    var c = g.sky_proj * vec4(s.dir, 1.0);
    let corner = QUAD[vi];
    let size = clamp(3.6 - s.mag * 0.42, 1.1, 4.6);
    c = vec4(c.xy + corner * size * 2.0 * g.viewport.zw * c.w, 0.0, c.w);
    o.pos = c;
    o.uv = corner;
    let b = clamp(pow(10.0, -0.4 * (s.mag - 1.2)), 0.05, 1.6) * g.sun_rel.w;
    o.color = vec4(s.color * b, 1.0);
    return o;
}

@fragment
fn fs_star(i: StarOut) -> @location(0) vec4<f32> {
    let d = dot(i.uv, i.uv);
    let a = exp(-d * 3.2);
    return vec4(i.color.rgb * a, a);
}

/* ── ciała niebieskie ─────────────────────────────────────────────────── */

struct Body {
    model: mat4x4<f32>,
    // orientacja (bez skali): układ ciała → świat
    rot: mat4x4<f32>,
    // barwa podstawowa, czy jest tekstura
    color: vec4<f32>,
    // barwa atmosfery, siła
    atmo: vec4<f32>,
    // środek względem kamery, promień
    center: vec4<f32>,
    // x: świeci (Słońce), y: przesunięcie chmur (0–1), z: chmury, w: promień powłoki atmosfery / promień
    extra: vec4<f32>,
};

@group(1) @binding(0) var<uniform> b: Body;
@group(2) @binding(0) var tex: texture_2d<f32>;
@group(2) @binding(1) var clouds: texture_2d<f32>;
@group(2) @binding(2) var samp: sampler;

struct BodyIn {
    @location(0) p: vec3<f32>,
    @location(1) uv: vec2<f32>,
};

struct BodyOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) uv: vec2<f32>,
};

@vertex
fn vs_body(v: BodyIn) -> BodyOut {
    var o: BodyOut;
    let w = b.model * vec4(v.p, 1.0);
    o.world = w.xyz;
    o.pos = g.view_proj * w;
    o.uv = v.uv;
    return o;
}

@vertex
fn vs_atmo(v: BodyIn) -> BodyOut {
    var o: BodyOut;
    let w = b.model * vec4(v.p * b.extra.w, 1.0);
    o.world = w.xyz;
    o.pos = g.view_proj * w;
    o.uv = v.uv;
    return o;
}

@fragment
fn fs_body(i: BodyOut) -> @location(0) vec4<f32> {
    let n = normalize(i.world - b.center.xyz);
    let v = normalize(-i.world);
    if (b.extra.x > 0.5) {
        // Słońce: pociemnienie brzegowe
        let mu = max(dot(n, v), 0.0);
        let limb = 0.45 + 0.55 * pow(mu, 0.6);
        return vec4(b.color.rgb * limb * 1.6, 1.0);
    }
    let l = normalize(g.sun_rel.xyz - i.world);
    var albedo = b.color.rgb;
    if (b.color.w > 0.5) {
        albedo = textureSample(tex, samp, i.uv).rgb;
    }
    if (b.extra.z > 0.5) {
        let c = textureSample(clouds, samp, vec2(fract(i.uv.x + b.extra.y), i.uv.y)).r;
        albedo = mix(albedo, vec3(0.95, 0.96, 0.98), clamp(c * 1.1, 0.0, 0.9));
    }
    let ndl = dot(n, l);
    let lit = smoothstep(-0.06, 0.22, ndl) * max(ndl * 0.8 + 0.2, 0.0);
    var col = albedo * (lit * 1.05 + 0.025);
    // poświata atmosfery na brzegu tarczy (od strony oświetlonej)
    let rim = pow(1.0 - max(dot(n, v), 0.0), 3.0) * b.atmo.w;
    col += b.atmo.rgb * rim * clamp(ndl + 0.35, 0.0, 1.0) * 0.9;
    // mroczna strona Ziemi: delikatny błękit rozproszonego światła
    col += b.atmo.rgb * 0.012 * b.atmo.w * (1.0 - smoothstep(-0.2, 0.1, ndl));
    // połysk oceanów (barwa ciemna i niebieska)
    if (b.extra.z > 0.5) {
        let water = smoothstep(0.08, 0.02, albedo.r + albedo.g * 0.5 - albedo.b * 0.6);
        let h = normalize(l + v);
        col += vec3(1.0, 0.95, 0.85) * pow(max(dot(n, h), 0.0), 60.0) * 0.35 * water * step(0.0, ndl);
    }
    return vec4(col * g.params.y, 1.0);
}

@fragment
fn fs_atmo(i: BodyOut) -> @location(0) vec4<f32> {
    let n = normalize(i.world - b.center.xyz);
    let v = normalize(-i.world);
    let l = normalize(g.sun_rel.xyz - i.world);
    let mu = abs(dot(n, v));
    let ndl = dot(n, l);
    let glow = pow(1.0 - mu, 2.2) * smoothstep(-0.35, 0.3, ndl);
    // zanik ku zewnętrznej krawędzi powłoki
    let edge = smoothstep(0.0, 0.35, mu);
    let a = glow * edge * b.atmo.w * 0.85;
    return vec4(b.atmo.rgb * a, a);
}

/* ── pierścienie Saturna (proceduralne, wg promieni pierścieni) ───────── */

struct RingIn {
    @location(0) p: vec3<f32>,
    @location(1) uv: vec2<f32>,
};

@vertex
fn vs_ring(v: RingIn) -> BodyOut {
    var o: BodyOut;
    let w = b.model * vec4(v.p, 1.0);
    o.world = w.xyz;
    o.pos = g.view_proj * w;
    o.uv = v.uv;
    return o;
}

fn band(r: f32, a: f32, c: f32, soft: f32) -> f32 {
    return smoothstep(a - soft, a + soft, r) * (1.0 - smoothstep(c - soft, c + soft, r));
}

@fragment
fn fs_ring(i: BodyOut) -> @location(0) vec4<f32> {
    // promień w tysiącach km
    let r = length(i.world - b.center.xyz) / 1.0e6;
    let fine = 0.5 + 0.5 * sin(r * 6.3) * sin(r * 1.7 + 1.3);
    var alpha = 0.0;
    alpha += band(r, 66.9, 74.5, 0.4) * 0.05;               // D
    alpha += band(r, 74.5, 92.0, 0.3) * (0.18 + 0.05 * fine); // C
    alpha += band(r, 92.0, 117.58, 0.2) * (0.78 + 0.15 * fine); // B
    alpha += band(r, 117.58, 122.17, 0.2) * 0.07;            // przerwa Cassiniego
    alpha += band(r, 122.17, 136.78, 0.15) * (0.5 + 0.1 * fine); // A
    alpha *= 1.0 - band(r, 133.42, 133.75, 0.05) * 0.85;     // przerwa Enckego
    alpha += band(r, 140.1, 140.3, 0.05) * 0.35;             // F
    let tint = mix(vec3(0.74, 0.68, 0.58), vec3(0.93, 0.88, 0.78), smoothstep(90.0, 120.0, r));
    // cień planety na pierścieniach
    let l = normalize(g.sun_rel.xyz - i.world);
    let oc = i.world - b.center.xyz;
    let bq = dot(oc, l);
    let cq = dot(oc, oc) - b.center.w * b.center.w;
    let shadow = select(1.0, 0.08, bq < 0.0 && bq * bq - cq > 0.0);
    let col = tint * shadow * 0.95;
    return vec4(col * alpha * g.params.y, alpha);
}

/* ── linie (orbity, trajektorie) ──────────────────────────────────────── */

struct LineIn {
    @location(0) p0: vec3<f32>,
    @location(1) p1: vec3<f32>,
    @location(2) color: vec4<f32>,
    @location(3) width: f32,
};

struct LineOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) across: f32,
};

@vertex
fn vs_line(@builtin(vertex_index) vi: u32, l: LineIn) -> LineOut {
    var o: LineOut;
    var c0 = g.view_proj * vec4(l.p0, 1.0);
    var c1 = g.view_proj * vec4(l.p1, 1.0);
    // odcinki za kamerą przycinamy do płaszczyzny bliskiej – inaczej rzut daje ogromne liczby
    let w_min = max(g.params.z, 1e-3);
    if (c0.w < w_min && c1.w < w_min) {
        o.pos = vec4(0.0, 0.0, -2.0, 1.0);
        return o;
    }
    if (c0.w < w_min) { c0 = mix(c0, c1, (w_min - c0.w) / (c1.w - c0.w)); }
    if (c1.w < w_min) { c1 = mix(c1, c0, (w_min - c1.w) / (c0.w - c1.w)); }
    let vp = g.viewport.xy;
    let s0 = c0.xy / c0.w * vp * 0.5;
    let s1 = c1.xy / c1.w * vp * 0.5;
    var dir = s1 - s0;
    let len = length(dir);
    dir = select(vec2(1.0, 0.0), dir / len, len > 1e-4);
    let nrm = vec2(-dir.y, dir.x);
    let corner = array<vec2<f32>, 6>(vec2(0.0, -1.0), vec2(1.0, -1.0), vec2(1.0, 1.0), vec2(0.0, -1.0), vec2(1.0, 1.0), vec2(0.0, 1.0));
    let k = corner[vi];
    var c = select(c0, c1, k.x > 0.5);
    let half_w = l.width * 0.5 + 0.5;
    let ext = select(-dir, dir, k.x > 0.5) * half_w;
    let off = (nrm * k.y * half_w + ext) / (vp * 0.5) * c.w;
    c = vec4(c.xy + off, c.z, c.w);
    o.pos = c;
    o.color = l.color;
    o.across = k.y * half_w / max(l.width * 0.5, 0.5);
    return o;
}

@fragment
fn fs_line(i: LineOut) -> @location(0) vec4<f32> {
    let a = 1.0 - smoothstep(0.7, 1.15, abs(i.across));
    return vec4(i.color.rgb * i.color.a * a, i.color.a * a);
}

/* ── punkty (satelity, znaczniki, poświata Słońca) ─────────────────────── */

struct PointIn {
    @location(0) p: vec3<f32>,
    @location(1) size: f32,
    @location(2) color: vec4<f32>,
};

struct PointOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_point(@builtin(vertex_index) vi: u32, p: PointIn) -> PointOut {
    var o: PointOut;
    var c = g.view_proj * vec4(p.p, 1.0);
    if (c.w <= 0.0) {
        o.pos = vec4(0.0, 0.0, -2.0, 1.0);
        return o;
    }
    let corner = QUAD[vi];
    c = vec4(c.xy + corner * p.size * g.viewport.zw * c.w, c.z, c.w);
    o.pos = c;
    o.uv = corner;
    o.color = p.color;
    return o;
}

@fragment
fn fs_point(i: PointOut) -> @location(0) vec4<f32> {
    let d = length(i.uv);
    let a = (1.0 - smoothstep(0.55, 1.0, d)) * i.color.a;
    return vec4(i.color.rgb * a, a);
}

@fragment
fn fs_glow(i: PointOut) -> @location(0) vec4<f32> {
    let d = length(i.uv);
    let core = exp(-d * d * 18.0);
    let halo = exp(-d * 4.5) * 0.35;
    let a = (core + halo) * (1.0 - smoothstep(0.85, 1.0, d)) * i.color.a;
    return vec4(i.color.rgb * a, a);
}
