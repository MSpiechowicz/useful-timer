use super::*;

const GREEN: Color32 = Color32::from_rgb(103, 255, 174);
const RED: Color32 = Color32::from_rgb(255, 94, 83);
const AMBER: Color32 = Color32::from_rgb(255, 190, 76);

fn finished_seconds(snapshot: &TimerSnapshot) -> Option<f32> {
    (snapshot.phase == TimerPhase::Finished)
        .then(|| snapshot.effect_elapsed.unwrap_or_default().as_secs_f32())
}

fn ignition(snapshot: &TimerSnapshot, reduced: bool) -> f32 {
    if reduced || snapshot.phase == TimerPhase::Idle {
        1.0
    } else {
        smoothstep((snapshot.animation_elapsed.as_secs_f32() / 0.6).min(1.0))
    }
}

fn urgency(snapshot: &TimerSnapshot) -> f32 {
    if snapshot.phase == TimerPhase::Idle || snapshot.phase == TimerPhase::Finished {
        0.0
    } else {
        (1.0 - snapshot.remaining.as_secs_f32() / 10.0).clamp(0.0, 1.0)
    }
}

// Original bitmap alphabet: hexadecimal characters and angular circuit marks.
// No external font dependency or borrowed franchise assets; rasterized once per context.
const GLYPHS: [[u8; 7]; 24] = [
    [14, 17, 19, 21, 25, 17, 14],
    [4, 12, 4, 4, 4, 4, 14],
    [14, 17, 1, 2, 4, 8, 31],
    [30, 1, 1, 14, 1, 1, 30],
    [2, 6, 10, 18, 31, 2, 2],
    [31, 16, 16, 30, 1, 1, 30],
    [14, 16, 16, 30, 17, 17, 14],
    [31, 1, 2, 4, 8, 8, 8],
    [14, 17, 17, 14, 17, 17, 14],
    [14, 17, 17, 15, 1, 1, 14],
    [14, 17, 17, 31, 17, 17, 17],
    [30, 17, 17, 30, 17, 17, 30],
    [15, 16, 16, 16, 16, 16, 15],
    [30, 17, 17, 17, 17, 17, 30],
    [31, 16, 16, 30, 16, 16, 31],
    [31, 16, 16, 30, 16, 16, 16],
    [4, 31, 4, 14, 21, 4, 4],
    [16, 30, 2, 7, 2, 2, 3],
    [17, 10, 31, 4, 31, 4, 4],
    [31, 1, 5, 21, 20, 16, 16],
    [4, 4, 31, 1, 15, 8, 24],
    [8, 15, 9, 31, 1, 2, 4],
    [21, 21, 31, 4, 14, 17, 17],
    [31, 17, 23, 20, 23, 1, 31],
];

fn glyph_atlas(ctx: &eframe::egui::Context) -> TextureHandle {
    let id = eframe::egui::Id::new("code-rain-original-glyphs");
    if let Some(texture) = ctx.data(|data| data.get_temp::<TextureHandle>(id)) {
        return texture;
    }
    let mut image = ColorImage::filled([24 * 14, 18], Color32::TRANSPARENT);
    for (glyph, rows) in GLYPHS.iter().enumerate() {
        for (y, bits) in rows.iter().enumerate() {
            for x in 0..5 {
                if bits & (1 << (4 - x)) != 0 {
                    for dy in 0..2 {
                        for dx in 0..2 {
                            image[(glyph * 14 + 2 + x * 2 + dx, 2 + y * 2 + dy)] = Color32::WHITE;
                        }
                    }
                }
            }
        }
    }
    // Loading a texture also locks the context; never do it inside data_mut.
    let texture = ctx.load_texture("code-rain-glyphs", image, TextureOptions::LINEAR);
    ctx.data_mut(|data| data.insert_temp(id, texture.clone()));
    texture
}

fn seeded(mut value: u32) -> f32 {
    value ^= value >> 16;
    value = value.wrapping_mul(0x7feb_352d);
    value ^= value >> 15;
    value = value.wrapping_mul(0x846c_a68b);
    value ^= value >> 16;
    (value >> 8) as f32 / 16_777_216.0
}

pub(super) fn code_rain(c: &Canvas<'_>, snapshot: &TimerSnapshot, time: f32, reduced: bool) {
    let finish = finished_seconds(snapshot);
    let fade = finish.map_or(1.0, |t| 1.0 - smoothstep(((t - 0.3) / 2.3).clamp(0.0, 1.0)));
    if fade > 0.0 {
        let atlas = glyph_atlas(c.painter.ctx());
        let mut mesh = Mesh::with_texture(atlas.id());
        mesh.reserve_vertices(3 * 22 * 17 * 4);
        mesh.reserve_triangles(3 * 22 * 17 * 2);
        let start = ignition(snapshot, reduced);
        for layer in 0..3_u32 {
            let size = 7.0 + layer as f32 * 1.4;
            let step = 11.0 + layer as f32 * 1.5;
            for column in 0..22_u32 {
                let seed = layer * 113 + column * 31 + 7;
                let x = 32.0 + column as f32 * 11.8 + seeded(seed) * 6.0;
                let cycles = 9.0 + (seeded(seed + 1) * 12.0).floor();
                let phase = (time * cycles / 120.0 + seeded(seed + 2)).fract();
                let head = if let Some(t) = finish {
                    let aligned = 70.0 + seeded(seed + 3) * 55.0;
                    let original = phase * 420.0 - 40.0;
                    original
                        + (aligned - original) * smoothstep((t / 0.3).min(1.0))
                        + (t - 0.3).max(0.0).powi(2) * 190.0
                } else {
                    phase * 420.0 - 40.0
                };
                let length = 8 + (seeded(seed + 4) * 9.0) as u32;
                for row in 0..length {
                    let y = head - row as f32 * step;
                    if !(37.0..=279.0).contains(&y) {
                        continue;
                    }
                    let edge = ((y - 37.0) / 18.0).min((280.0 - y) / 28.0).clamp(0.0, 1.0);
                    let trail = (1.0 - row as f32 / length as f32).powf(1.5);
                    let depth = 0.22 + layer as f32 * 0.28;
                    let clearing = if (90.0..230.0).contains(&x) && (127.0..207.0).contains(&y) {
                        0.18
                    } else {
                        1.0
                    };
                    let pulse = if reduced {
                        0.0
                    } else {
                        let wave = (y / 260.0 - snapshot.remaining.as_secs_f32().fract()).abs();
                        (1.0 - wave * 8.0).max(0.0) * urgency(snapshot) * 0.3
                    };
                    let alpha = (trail * depth + pulse) * edge * clearing * fade * start;
                    let color = if row == 0 {
                        rgba(211, 255, 229, alpha)
                    } else {
                        rgba(43, 229, 119, alpha)
                    };
                    let cadence = 2.0 + (seeded(seed + row + 50) * 3.0).floor();
                    let tick = (time * cadence).floor() as u32;
                    let glyph =
                        (seeded(seed + row * 17 + tick * 37) * GLYPHS.len() as f32) as usize;
                    let uv = Rect::from_min_max(
                        Pos2::new(glyph as f32 / 24.0, 0.0),
                        Pos2::new((glyph + 1) as f32 / 24.0, 1.0),
                    );
                    let bounds = Rect::from_min_max(
                        c.point(x, y),
                        c.point(x + size, y + size * 18.0 / 14.0),
                    );
                    if row < 2 && layer == 2 {
                        mesh.add_rect_with_uv(
                            bounds.expand(1.2 * c.scale),
                            uv,
                            color.gamma_multiply(0.18),
                        );
                    }
                    mesh.add_rect_with_uv(bounds, uv, color);
                }
            }
        }
        c.painter.add(mesh);
    }
    readout(c, snapshot, 164.0, GREEN, true);
}

fn radial(cx: f32, cy: f32, radius: f32, angle: f32) -> [f32; 2] {
    [cx + radius * angle.cos(), cy + radius * angle.sin()]
}

fn arc(c: &Canvas<'_>, radius: f32, start: f32, sweep: f32, width: f32, color: Color32) {
    let steps = ((sweep.abs() * radius / 7.0).ceil() as usize).max(1);
    let points = (0..=steps)
        .map(|i| {
            let p = radial(
                160.0,
                142.0,
                radius,
                start + sweep * i as f32 / steps as f32,
            );
            c.point(p[0], p[1])
        })
        .collect();
    c.painter.add(Shape::line(points, c.stroke(width, color)));
}

fn housing_mesh() -> Mesh {
    let mut mesh = Mesh::default();
    const SIDES: u32 = 128;
    for (inner, outer, low, high) in [
        (99.0, 108.0, 28.0, 109.0),
        (88.0, 99.0, 11.0, 40.0),
        (73.0, 87.0, 27.0, 69.0),
        (67.0, 73.0, 6.0, 29.0),
    ] {
        let base = mesh.vertices.len() as u32;
        for side in 0..=SIDES {
            let angle = side as f32 * TAU / SIDES as f32;
            let light = (angle + 2.2).cos() * 0.5 + 0.5;
            for (radius, edge) in [(inner, 0.6), (outer, 1.0)] {
                let shade = ((low + (high - low) * light) * edge) as u8;
                let p = radial(160.0, 142.0, radius, angle);
                mesh.colored_vertex(
                    Pos2::new(p[0], p[1]),
                    Color32::from_rgb(shade, shade.saturating_add(5), shade.saturating_add(10)),
                );
            }
        }
        for side in 0..SIDES {
            let a = base + side * 2;
            mesh.add_triangle(a, a + 1, a + 2);
            mesh.add_triangle(a + 1, a + 3, a + 2);
        }
    }
    mesh
}

pub(super) fn machine_core(full: &Canvas<'_>, snapshot: &TimerSnapshot, time: f32, reduced: bool) {
    static HOUSING: LazyLock<Mesh> = LazyLock::new(housing_mesh);
    // Scale only the hardware, around the optical center. Keep the readout full-sized.
    // Slightly enlarge the dark housing to balance the orb's brighter visual weight.
    let scale = full.scale * 93.0 / 108.4;
    let core = Canvas {
        scale,
        origin: full.point(160.0, 127.0) + Vec2::new(0.0, 18.0 * scale),
        ..*full
    };
    let c = &core;
    let finish = finished_seconds(snapshot);
    let closing = finish.map_or(0.0, |t| smoothstep((t / 0.38).min(1.0)));
    let motion_time = if reduced {
        0.0
    } else {
        time + finish.unwrap_or_default().min(0.38)
    };
    let urgency = urgency(snapshot);
    let ignition = ignition(snapshot, reduced);
    c.circle(160.0, 142.0, 108.0, Color32::from_rgb(12, 17, 23));
    c.cached_mesh(&HOUSING);
    c.ring(160.0, 142.0, 108.0, 0.8, rgba(154, 178, 194, 0.55));
    c.ring(160.0, 142.0, 87.0, 0.7, rgba(151, 175, 184, 0.3));
    for i in 0..8 {
        let angle = i as f32 * TAU / 8.0 + PI / 8.0;
        let [x, y] = radial(160.0, 142.0, 103.0, angle);
        c.circle(x, y, 2.5, Color32::from_rgb(9, 12, 17));
        c.line(
            [x - 1.2, y - 0.6],
            [x + 1.2, y + 0.6],
            0.7,
            rgba(147, 162, 178, 0.7),
        );
    }
    for i in 0..48 {
        let angle = -PI / 2.0 + i as f32 * TAU / 48.0;
        let active = finish.is_none() && (i as f32 / 48.0) < snapshot.remaining_fraction;
        arc(
            c,
            93.0,
            angle,
            TAU / 48.0 * 0.56,
            2.6,
            if active {
                RED.gamma_multiply(0.8)
            } else {
                rgba(104, 64, 66, 0.45)
            },
        );
    }
    let turn = motion_time * TAU / 12.0;
    for i in 0..3 {
        arc(
            c,
            80.0,
            turn + i as f32 * TAU / 3.0,
            0.5,
            1.5,
            rgba(192, 210, 218, 0.65),
        );
    }
    c.circle(160.0, 142.0, 66.0, Color32::from_rgb(5, 8, 13));
    let aperture = (13.0 + 24.0 * ignition) * (1.0 - closing);
    for blade in 0..8 {
        let a = blade as f32 * TAU / 8.0 + 0.22 * ignition;
        let points = [
            radial(160.0, 142.0, aperture, a),
            radial(160.0, 142.0, 65.0, a + 0.18),
            radial(160.0, 142.0, 65.0, a + TAU / 8.0 + 0.18),
            radial(160.0, 142.0, aperture, a + TAU / 8.0),
        ];
        let shade = 26 + blade * 3;
        c.polygon(
            &points[..if aperture > 0.0 { 4 } else { 3 }],
            Color32::from_rgb(shade, shade + 4, shade + 9),
            rgba(120, 139, 156, 0.48),
            0.7,
        );
    }
    let opening = 1.0 - closing;
    let breath = if reduced {
        0.5
    } else {
        0.5 + 0.5 * (motion_time * TAU / 3.0).sin()
    };
    let power = (0.68 + breath * 0.24 + urgency * 0.08) * opening;
    let halo = (48.0 + breath * 18.0) * opening;
    if closing < 1.0 {
        c.glow(
            160.0,
            142.0,
            halo,
            halo,
            rgba(255, 27, 30, power * (0.65 + breath * 0.2)),
        );
        c.circle(160.0, 142.0, aperture * 0.86, rgba(91, 5, 13, 1.0));
        c.ring(160.0, 142.0, aperture * 0.82, 1.3, rgba(255, 68, 62, power));
        c.glow(160.0, 142.0, aperture, aperture, rgba(255, 47, 34, power));
        let bloom = (14.0 + breath * 11.0) * opening;
        c.glow(160.0, 142.0, bloom, bloom, rgba(255, 155, 109, power * 0.7));
        c.circle(
            160.0,
            142.0,
            (5.0 + urgency * 2.0) * opening,
            rgba(255, 224, 202, power),
        );
        let scan = (time * TAU / 6.0).sin() * aperture * 0.65;
        let half = (aperture.powi(2) * 0.75 - scan.powi(2)).max(0.0).sqrt() * 0.48;
        c.line(
            [160.0 - half, 142.0 + scan],
            [160.0 + half, 142.0 + scan],
            0.5,
            rgba(255, 179, 166, 0.24 * opening),
        );
    }
    readout(full, snapshot, 266.0, RED, false);
}

fn orb_textures(ctx: &eframe::egui::Context) -> Arc<[TextureHandle; 2]> {
    let id = eframe::egui::Id::new("dragon-orb-glass");
    if let Some(textures) = ctx.data(|data| data.get_temp::<Arc<[TextureHandle; 2]>>(id)) {
        return textures;
    }
    const SIZE: usize = 384;
    let mut body = ColorImage::filled([SIZE, SIZE], Color32::TRANSPARENT);
    let mut glass = ColorImage::filled([SIZE, SIZE], Color32::TRANSPARENT);
    for py in 0..SIZE {
        for px in 0..SIZE {
            let x = (px as f32 + 0.5) * 2.0 / SIZE as f32 - 1.0;
            let y = (py as f32 + 0.5) * 2.0 / SIZE as f32 - 1.0;
            let r = x.hypot(y);
            if r >= 1.0 {
                continue;
            }
            let z = (1.0 - r * r).sqrt();
            let edge = ((1.0 - r) * SIZE as f32 * 0.5).min(1.0);
            let light = (-0.43 * x - 0.55 * y + 0.71 * z).max(0.0);
            let caustic = (-((r - 0.87) / 0.07).powi(2)).exp() * (y * 0.7 + x * 0.3).max(0.0);
            body[(px, py)] = rgba(
                (150.0 + 78.0 * z + 29.0 * light + 85.0 * caustic).min(255.0) as u8,
                (40.0 + 87.0 * z + 47.0 * light + 95.0 * caustic).min(225.0) as u8,
                (3.0 + 8.0 * light + 15.0 * z + 35.0 * caustic).min(85.0) as u8,
                edge,
            );
            // Layered environment reflections: broad soft light, separated glints,
            // and a broken rim highlight following the sphere's surface.
            let qx = (x + 0.40) * 0.88 - (y + 0.50) * 0.48;
            let qy = (x + 0.40) * 0.48 + (y + 0.50) * 0.88;
            let broad = (-(qx / 0.31).powi(2) - (qy / 0.19).powi(2)).exp() * 0.28;
            let glint =
                (-((qx + 0.06) / 0.115).powi(2) - ((qy + 0.025) / 0.065).powi(2)).exp() * 0.78;
            let satellite =
                (-((qx - 0.135) / 0.045).powi(2) - ((qy - 0.065) / 0.025).powi(2)).exp() * 0.68;
            let angle = y.atan2(x);
            let upper_rim =
                (-((r - 0.91) / 0.023).powi(2) - ((angle + 2.42) / 0.27).powi(4)).exp() * 0.53;
            let lower_rim =
                (-((r - 0.86) / 0.035).powi(2) - ((angle - 0.74) / 0.23).powi(2)).exp() * 0.24;
            let fresnel = (1.0 - z).powi(5) * 0.27;
            glass[(px, py)] = rgba(
                255,
                244,
                207,
                ((broad + glint + satellite + upper_rim + lower_rim + fresnel) * edge).min(0.94),
            );
        }
    }
    let textures = Arc::new([
        ctx.load_texture("dragon-orb-amber", body, TextureOptions::LINEAR),
        ctx.load_texture("dragon-orb-reflections", glass, TextureOptions::LINEAR),
    ]);
    ctx.data_mut(|data| data.insert_temp(id, Arc::clone(&textures)));
    textures
}

fn embedded_stars(c: &Canvas<'_>, yaw: f32, pitch: f32) {
    // Bake the four stars onto a spherical inner surface once, then rotate in 3D.
    static POINTS: LazyLock<[[f32; 3]; 44]> = LazyLock::new(|| {
        std::array::from_fn(|i| {
            let star = i / 11;
            let vertex = i % 11;
            let cx = if star % 2 == 0 { -24.0 } else { 24.0 };
            let cy = if star < 2 { -22.0 } else { 22.0 };
            let (dx, dy) = if vertex == 0 {
                (0.0, 0.0)
            } else {
                let a = -PI / 2.0 + (vertex - 1) as f32 * PI / 5.0;
                let r = if vertex % 2 == 1 { 14.5 } else { 6.3 };
                (a.cos() * r, a.sin() * r)
            };
            let x = (cx + dx) / 88.0;
            let y = (cy + dy) / 88.0;
            [x, y, (1.0 - x * x - y * y).sqrt() * 0.9]
        })
    });
    let (sy, cy) = yaw.sin_cos();
    let (sp, cp) = pitch.sin_cos();
    let mut mesh = Mesh::default();
    mesh.reserve_vertices(44);
    mesh.reserve_triangles(40);
    for &[x, y, z] in POINTS.iter() {
        let rx = x * cy + z * sy;
        let rz = z * cy - x * sy;
        let ry = y * cp - rz * sp;
        mesh.colored_vertex(
            c.point(160.0 + rx * 88.0, 145.0 + ry * 88.0),
            Color32::from_rgb(184, 37, 10),
        );
    }
    for star in 0..4 {
        let base = star * 11;
        for point in 0..10 {
            mesh.add_triangle(base, base + 1 + point, base + 1 + (point + 1) % 10);
        }
    }
    c.painter.add(mesh);
}

fn orb_light_sweep(c: &Canvas<'_>, elapsed: f32) {
    let p = ((elapsed - 0.2) / 1.6).clamp(0.0, 1.0);
    if p <= 0.0 || p >= 1.0 {
        return;
    }
    let center = -1.35 + p * 2.7;
    let strength = (PI * p).sin().powi(2) * 0.26;
    let mut mesh = Mesh::default();
    mesh.reserve_vertices(33 * 7);
    mesh.reserve_triangles(32 * 6 * 2);
    for row in 0..=32 {
        let y = -1.0 + row as f32 / 16.0;
        let half = (1.0 - y * y).max(0.0).sqrt();
        for column in 0..7 {
            let offset = (column as f32 - 3.0) / 3.0;
            let x = (center + offset * 0.23).clamp(-half, half);
            let alpha = (1.0 - offset * offset) * (1.0 - x * x - y * y).max(0.0).sqrt() * strength;
            mesh.colored_vertex(
                c.point(160.0 + x * 88.0, 145.0 + y * 88.0),
                rgba(255, 253, 211, alpha),
            );
        }
    }
    for row in 0..32 {
        for column in 0..6 {
            let a = row * 7 + column;
            mesh.add_triangle(a, a + 7, a + 1);
            mesh.add_triangle(a + 1, a + 7, a + 8);
        }
    }
    c.painter.add(mesh);
}

pub(super) fn dragon_orb(c: &Canvas<'_>, snapshot: &TimerSnapshot, time: f32, reduced: bool) {
    let textures = orb_textures(c.painter.ctx());
    let finish = finished_seconds(snapshot);
    let settling = finish.map_or(1.0, |t| 1.0 - smoothstep((t / 2.1).min(1.0)));
    let motion = if reduced || snapshot.phase == TimerPhase::Idle {
        0.0
    } else {
        settling
    };
    let yaw = (time * TAU / 12.0).sin() * 0.24 * motion;
    let pitch = (time * TAU / 15.0).sin() * 0.075 * motion;
    let bob = (time * TAU / 6.0).sin() * 2.3 * motion;
    let orb = Canvas {
        offset: c.offset + Vec2::new(0.0, bob),
        ..*c
    };
    c.glow(160.0, 237.0, 64.0 - bob, 8.0, rgba(21, 12, 4, 0.28));
    let bounds = Rect::from_min_max(orb.point(72.0, 57.0), orb.point(248.0, 233.0));
    let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
    orb.painter
        .image(textures[0].id(), bounds, uv, Color32::WHITE);
    embedded_stars(&orb, yaw, pitch);
    orb.painter
        .image(textures[1].id(), bounds, uv, Color32::WHITE);
    if let Some(t) = finish
        && !reduced
    {
        orb_light_sweep(&orb, t);
    }
    readout(c, snapshot, 269.0, AMBER, true);
}
