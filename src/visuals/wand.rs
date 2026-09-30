use super::*;

const ACCENT: Color32 = Color32::from_rgb(227, 209, 248);
const CRYSTAL: [f32; 2] = [160.0, 95.0];
const CRYSTAL_SCALE: f32 = 0.75;
const SPARK_PERIOD: f64 = 6.0;

fn artwork(ctx: &eframe::egui::Context) -> TextureHandle {
    let id = eframe::egui::Id::new("illustrated-crescent-wand");
    if let Some(texture) = ctx.data(|data| data.get_temp::<TextureHandle>(id)) {
        return texture;
    }
    let image = eframe::icon_data::from_png_bytes(include_bytes!("../../assets/crescent-wand.png"))
        .expect("bundled crescent wand artwork must be a valid PNG");
    let texture = ctx.load_texture(
        "crescent-wand-layers",
        ColorImage::from_rgba_unmultiplied(
            [image.width as usize, image.height as usize],
            &image.rgba,
        ),
        TextureOptions::LINEAR.with_mipmap_mode(Some(eframe::egui::TextureFilter::Linear)),
    );
    ctx.data_mut(|data| data.insert_temp(id, texture.clone()));
    texture
}

fn layer(c: &Canvas<'_>, texture: &TextureHandle, index: u32, tint: Color32) {
    let u = (index % 2) as f32 * 0.5;
    let v = (index / 2) as f32 * 0.5;
    let mut mesh = Mesh::with_texture(texture.id());
    mesh.reserve_vertices(4);
    mesh.reserve_triangles(2);
    for (x, y) in [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)] {
        mesh.vertices.push(Vertex {
            pos: c.point(x * 320.0, y * 320.0),
            uv: Pos2::new(u + x * 0.5, v + y * 0.5),
            color: tint,
        });
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    c.painter.add(mesh);
}

fn crystal_bloom(c: &Canvas<'_>, radii: [f32; 2], tint: Color32) {
    let ctx = c.painter.ctx();
    let id = eframe::egui::Id::new("wand-crystal-bloom");
    let texture = ctx
        .data(|data| data.get_temp::<TextureHandle>(id))
        .unwrap_or_else(|| {
            const SIZE: usize = 128;
            let mut pixels = Vec::with_capacity(SIZE * SIZE);
            for y in 0..SIZE {
                for x in 0..SIZE {
                    let dx = (x as f32 + 0.5) / SIZE as f32 * 2.0 - 1.0;
                    let dy = (y as f32 + 0.5) / SIZE as f32 * 2.0 - 1.0;
                    // A Gaussian fades to transparent well before the quad's edge.
                    let alpha = (-8.0 * (dx * dx + dy * dy)).exp();
                    pixels.push(rgba(255, 255, 255, alpha));
                }
            }
            let texture = ctx.load_texture(
                "wand-crystal-bloom",
                ColorImage::new([SIZE, SIZE], pixels),
                TextureOptions::LINEAR,
            );
            ctx.data_mut(|data| data.insert_temp(id, texture.clone()));
            texture
        });
    let mut mesh = Mesh::with_texture(texture.id());
    mesh.reserve_vertices(4);
    mesh.reserve_triangles(2);
    for (x, y) in [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)] {
        mesh.vertices.push(Vertex {
            pos: c.point(
                CRYSTAL[0] + (x * 2.0 - 1.0) * radii[0] * CRYSTAL_SCALE,
                CRYSTAL[1] + (y * 2.0 - 1.0) * radii[1] * CRYSTAL_SCALE,
            ),
            uv: Pos2::new(x, y),
            color: tint,
        });
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    c.painter.add(mesh);
}

fn light_trails(c: &Canvas<'_>, elapsed: f32, strength: f32) {
    const RAYS: u32 = 24;
    const STEPS: u32 = 8;
    let mut mesh = Mesh::default();
    mesh.reserve_vertices((RAYS * (STEPS + 1) * 4) as usize);
    mesh.reserve_triangles((RAYS * STEPS * 6) as usize);
    let origin = c.point(CRYSTAL[0], CRYSTAL[1]);
    let feather = c.painter.ctx().pixels_per_point().recip();
    for ray in 0..RAYS {
        let phase = seed(ray + 401) * TAU;
        let angle = ray as f32 * TAU / RAYS as f32
            + (seed(ray + 419) - 0.5) * 0.12
            + elapsed * 0.12
            + (elapsed * 1.7 + phase).sin() * 0.035
            + c.angle;
        let direction = Vec2::angled(angle);
        let normal = Vec2::new(-direction.y, direction.x);
        let length = (58.0 + seed(ray + 431) * 68.0)
            * (0.88 + 0.12 * (elapsed * 2.0 + phase).sin())
            * c.scale
            * CRYSTAL_SCALE;
        let width = (0.35 + seed(ray + 449) * 0.3) * c.scale * CRYSTAL_SCALE;
        let travel = (elapsed * (0.7 + seed(ray + 461) * 0.2) + seed(ray + 479)).fract();
        let brightness = strength * (0.65 + seed(ray + 487) * 0.35);
        let base = mesh.vertices.len() as u32;
        for row in 0..=STEPS {
            let t = row as f32 / STEPS as f32;
            let pulse = smoothstep(1.0 - ((t - travel) / 0.28).abs());
            let alpha = brightness * (1.0 - smoothstep(t)) * (0.6 + pulse * 0.4);
            let center = origin + direction * (length * t);
            let half_width = width * (1.0 - t * 0.6);
            let color = rgba(255, 248, 216, alpha);
            for (offset, tint) in [
                (-half_width - feather, Color32::TRANSPARENT),
                (-half_width, color),
                (half_width, color),
                (half_width + feather, Color32::TRANSPARENT),
            ] {
                mesh.colored_vertex(center + normal * offset, tint);
            }
        }
        for row in 0..STEPS {
            for band in 0..3 {
                let a = base + row * 4 + band;
                mesh.add_triangle(a, a + 1, a + 4);
                mesh.add_triangle(a + 1, a + 5, a + 4);
            }
        }
    }
    c.painter.add(mesh);
}

fn pose<'a>(c: &Canvas<'a>, phase: f32, motion: f32) -> Canvas<'a> {
    let angle = -0.08 + 0.035 * (phase * TAU / 10.0).sin() * motion;
    Canvas {
        angle,
        rotation: Vec2::angled(angle),
        offset: c.offset
            + Vec2::new(
                2.0 * (phase * TAU / 8.0).sin() * motion,
                2.8 * (phase * TAU / 6.0).sin() * motion,
            ),
        ..*c
    }
}

fn settling(elapsed: f32) -> f32 {
    1.0 - smoothstep(((elapsed - 1.1) / 2.1).clamp(0.0, 1.0))
}

fn seed(index: u32) -> f32 {
    let mut value = index.wrapping_mul(747_796_405).wrapping_add(2_891_336_453);
    value = ((value >> ((value >> 28) + 4)) ^ value).wrapping_mul(277_803_737);
    ((value >> 22) ^ value) as f32 / u32::MAX as f32
}

// Soft four-point stars and small pinpricks share one bounded particle mesh.
fn spark(mesh: &mut Mesh, center: Pos2, radius: f32, alpha: f32, star: bool) {
    let base = mesh.vertices.len() as u32;
    mesh.colored_vertex(center, rgba(255, 253, 237, alpha));
    for corner in 0..8 {
        let angle = corner as f32 * TAU / 8.0;
        let length = if star && corner % 2 == 1 {
            radius * 0.23
        } else {
            radius
        };
        let direction = Vec2::angled(angle) * length;
        mesh.colored_vertex(center + direction * 0.6, rgba(249, 219, 168, alpha));
        mesh.colored_vertex(center + direction, Color32::TRANSPARENT);
    }
    for corner in 0..8 {
        let a = base + 1 + corner * 2;
        let b = base + 1 + (corner + 1) % 8 * 2;
        mesh.add_triangle(base, a, b);
        mesh.add_triangle(a, a + 1, b);
        mesh.add_triangle(b, a + 1, b + 1);
    }
}

fn fall(
    c: &Canvas<'_>,
    mesh: &mut Mesh,
    origin: Pos2,
    index: u32,
    age: f32,
    lifetime: f32,
    shower: bool,
) {
    let sway = (age * 2.1 + seed(index + 53) * TAU).sin() - (seed(index + 53) * TAU).sin();
    let position = origin
        + Vec2::new(
            (seed(index + 11) - 0.5) * age * if shower { 37.0 } else { 15.0 } + sway * 2.8,
            (17.0 + seed(index + 23) * 17.0) * age + 8.0 * age * age,
        ) * c.scale;
    let edge = ((c.point(0.0, 238.0).y - position.y) / (18.0 * c.scale)).clamp(0.0, 1.0);
    let envelope = smoothstep((age / 0.2).min(1.0))
        * (1.0 - smoothstep(((age / lifetime - 0.45) / 0.55).clamp(0.0, 1.0)));
    let twinkle = 0.65 + 0.35 * (age * 5.0 + seed(index + 37) * TAU).sin().powi(2);
    let brightness = if shower { 1.0 } else { 0.85 };
    let alpha = brightness * envelope * twinkle * edge;
    if alpha > 0.005 {
        let star = index.is_multiple_of(3);
        let radius = if star {
            4.0 + seed(index + 7) * 2.0
        } else {
            1.2 + seed(index + 7) * 0.8
        };
        spark(
            mesh,
            position,
            radius * c.scale * if shower { 1.25 } else { 1.0 },
            alpha,
            star,
        );
    }
}

fn particles(c: &Canvas<'_>, snapshot: &TimerSnapshot, phase: f32, finish: Option<f32>) {
    let elapsed = snapshot.animation_elapsed.as_secs_f64();
    let ending = f64::from(finish.unwrap_or(0.0));
    let mut mesh = Mesh::default();
    let capacity = if finish.is_some() { 52 } else { 24 };
    mesh.reserve_vertices(capacity * 17);
    mesh.reserve_triangles(capacity * 24);
    for index in 0..24 {
        let offset = f64::from(seed(index + 101)) * SPARK_PERIOD;
        let clock = elapsed + ending - offset;
        if clock < 0.0 {
            continue;
        }
        let age = clock.rem_euclid(SPARK_PERIOD) as f32;
        let lifetime = 1.8 + seed(index + 127) * 0.8;
        // Existing sparks finish falling at zero, but the ambient emitter stops.
        if age >= lifetime || (finish.is_some() && f64::from(age) < ending) {
            continue;
        }
        let emitter = pose(c, phase + ending as f32 - age, 1.0);
        let origin = emitter.point(
            112.0 + seed(index + 149) * 99.0,
            60.0 + seed(index + 173) * 69.0,
        );
        fall(c, &mut mesh, origin, index, age, lifetime, false);
    }
    if let Some(ending) = finish {
        for index in 0..28 {
            let birth = 0.35 + seed(index + 211) * 0.85;
            let age = ending - birth;
            let lifetime = 1.25 + seed(index + 239) * 0.85;
            if age < 0.0 || age >= lifetime {
                continue;
            }
            let emitter = pose(c, phase + birth, settling(birth));
            let origin = emitter.point(
                CRYSTAL[0] + (seed(index + 263) - 0.5) * 44.0 * CRYSTAL_SCALE,
                CRYSTAL[1] + (seed(index + 281) - 0.5) * 24.0 * CRYSTAL_SCALE,
            );
            fall(c, &mut mesh, origin, index + 307, age, lifetime, true);
        }
    }
    if !mesh.vertices.is_empty() {
        c.painter.add(mesh);
    }
}

fn crescent_sheen(c: &Canvas<'_>, texture: &TextureHandle, elapsed: f32) {
    if !(0.0..0.8).contains(&elapsed) {
        return;
    }
    let progress = elapsed / 0.8;
    let center = 28.0 + progress * 115.0;
    let strength = (PI * progress).sin() * 0.8;
    let mut mesh = Mesh::with_texture(texture.id());
    mesh.reserve_vertices(82);
    mesh.reserve_triangles(80);
    for row in 0..=40 {
        let y = row as f32 * 8.0;
        let alpha = (-((y - center) / 12.0).powi(2)).exp() * strength;
        for x in [0.0, 320.0] {
            mesh.vertices.push(Vertex {
                pos: c.point(x, y),
                uv: Pos2::new(0.5 + x / 640.0, 0.5 + y / 640.0),
                color: Color32::WHITE.gamma_multiply(alpha),
            });
        }
    }
    for row in 0..40 {
        let a = row * 2;
        mesh.add_triangle(a, a + 1, a + 2);
        mesh.add_triangle(a + 1, a + 3, a + 2);
    }
    c.painter.add(mesh);
}

pub(super) fn draw(c: &Canvas<'_>, snapshot: &TimerSnapshot, time: f32, reduced: bool) {
    let finish = (snapshot.phase == TimerPhase::Finished)
        .then(|| snapshot.effect_elapsed.unwrap_or_default().as_secs_f32());
    let motion = if reduced {
        0.0
    } else {
        finish.map_or(1.0, settling)
    };
    let wand = pose(c, time + finish.unwrap_or(0.0).min(3.4), motion);
    let texture = artwork(c.painter.ctx());
    let breath = 0.5 + 0.5 * (time * TAU / 4.0).sin() * motion;
    let illumination = finish.filter(|_| !reduced).map_or(0.0, |elapsed| {
        smoothstep((elapsed / 0.4).min(1.0))
            * (1.0 - smoothstep(((elapsed - 1.1) / 1.2).clamp(0.0, 1.0)))
    });
    wand.glow(
        CRYSTAL[0],
        CRYSTAL[1],
        23.0 * CRYSTAL_SCALE,
        23.0 * CRYSTAL_SCALE,
        rgba(220, 194, 255, 0.12),
    );
    if illumination > 0.0 {
        crystal_bloom(
            &wand,
            [136.0, 152.0],
            rgba(255, 245, 215, illumination * 0.55),
        );
    }
    layer(&wand, &texture, 0, Color32::WHITE);
    layer(&wand, &texture, 1, Color32::WHITE);
    wand.glow(
        CRYSTAL[0] - 2.0 * CRYSTAL_SCALE,
        CRYSTAL[1] - 3.0 * CRYSTAL_SCALE,
        13.0 * CRYSTAL_SCALE,
        15.0 * CRYSTAL_SCALE,
        rgba(255, 250, 255, 0.06 + breath * 0.05),
    );
    // The crescent's foreground rim hides the base of the crystal's pedestal.
    layer(&wand, &texture, 2, Color32::WHITE);
    if let Some(elapsed) = finish.filter(|_| !reduced) {
        crescent_sheen(&wand, &texture, elapsed);
        if illumination > 0.0 {
            light_trails(&wand, elapsed, illumination);
            crystal_bloom(
                &wand,
                [64.0, 70.0],
                rgba(255, 250, 235, illumination * 0.98),
            );
        }
    }
    if !reduced && snapshot.phase != TimerPhase::Idle && finish.is_none_or(|elapsed| elapsed < 3.4)
    {
        particles(c, snapshot, time, finish);
    }
    readout(c, snapshot, 269.0, ACCENT, true);
}
