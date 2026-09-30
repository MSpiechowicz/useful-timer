use std::{
    f32::consts::{PI, TAU},
    sync::{Arc, LazyLock},
    time::Duration,
};

use eframe::egui::{
    Align2, Color32, ColorImage, FontId, Painter, Pos2, Rect, Shape, Stroke, TextureHandle,
    TextureOptions, Vec2,
    epaint::{CubicBezierShape, EllipseShape, Mesh, Vertex},
};
use useful_timer::timer::{TimerPhase, TimerSettings, TimerSnapshot, TimerStyle};

mod cinematic;
mod wand;

/// Completion effects are finite and settle into a static finished pose.
pub fn finish_effect_duration(style: TimerStyle) -> Duration {
    match style {
        TimerStyle::Bomb => Duration::from_millis(3400),
        TimerStyle::Hourglass => Duration::from_millis(1800),
        TimerStyle::Rocket => Duration::from_millis(3600),
        TimerStyle::CodeRain => Duration::from_millis(2600),
        TimerStyle::MachineCore => Duration::from_millis(400),
        TimerStyle::DragonOrb => Duration::from_millis(2800),
        TimerStyle::CrescentWand => Duration::from_millis(3400),
    }
}

pub fn needs_animation(settings: &TimerSettings, snapshot: &TimerSnapshot) -> bool {
    !settings.reduced_motion
        && (snapshot.phase == TimerPhase::Running
            || (snapshot.phase == TimerPhase::Finished
                && snapshot
                    .effect_elapsed
                    .is_some_and(|elapsed| elapsed < finish_effect_duration(settings.style))))
}

/// Positive fractions of a second remain visible as one second, never zero.
pub fn format_remaining(remaining: Duration) -> String {
    let seconds = display_seconds(remaining);
    if seconds >= 3600 {
        format!(
            "{}:{:02}:{:02}",
            seconds / 3600,
            seconds / 60 % 60,
            seconds % 60
        )
    } else {
        format!("{:02}:{:02}", seconds / 60, seconds % 60)
    }
}

fn display_seconds(remaining: Duration) -> u64 {
    remaining
        .as_secs()
        .saturating_add(u64::from(remaining.subsec_nanos() != 0))
}

/// Draws inside a centered 320-point design square without painting a background.
/// The caller owns interaction, labels, accessibility, and timer advancement.
pub fn draw_timer(
    painter: &Painter,
    rect: Rect,
    settings: &TimerSettings,
    snapshot: &TimerSnapshot,
) {
    if !rect.is_finite() || rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }

    let painter = painter.with_clip_rect(painter.clip_rect().intersect(rect));
    let smoke = smoke_textures(painter.ctx());
    let canvas = Canvas {
        painter: &painter,
        smoke: &smoke,
        origin: rect.center(),
        scale: rect.width().min(rect.height()) / 320.0,
        angle: 0.0,
        rotation: Vec2::new(1.0, 0.0),
        offset: Vec2::ZERO,
    };
    // Freeze ambient motion during pause; reduced motion keeps only progress cues.
    let time = if settings.reduced_motion {
        0.0
    } else {
        snapshot.animation_elapsed.as_secs_f64().rem_euclid(120.0) as f32
    };
    let mut settled;
    let snapshot = if settings.reduced_motion && snapshot.phase == TimerPhase::Finished {
        settled = snapshot.clone();
        settled.effect_elapsed = Some(finish_effect_duration(settings.style));
        &settled
    } else {
        snapshot
    };

    match settings.style {
        TimerStyle::Bomb => draw_bomb(&canvas, snapshot, time),
        TimerStyle::Hourglass => draw_hourglass(&canvas, snapshot, time),
        TimerStyle::Rocket => draw_rocket(&canvas, snapshot, time),
        TimerStyle::CodeRain => {
            cinematic::code_rain(&canvas, snapshot, time, settings.reduced_motion)
        }
        TimerStyle::MachineCore => {
            cinematic::machine_core(&canvas, snapshot, time, settings.reduced_motion)
        }
        TimerStyle::DragonOrb => {
            cinematic::dragon_orb(&canvas, snapshot, time, settings.reduced_motion)
        }
        TimerStyle::CrescentWand => wand::draw(&canvas, snapshot, time, settings.reduced_motion),
    }
}

fn smoke_textures(ctx: &eframe::egui::Context) -> Arc<[TextureHandle; 4]> {
    let id = eframe::egui::Id::new("timer-smoke-textures");
    if let Some(textures) = ctx.data(|data| data.get_temp::<Arc<[TextureHandle; 4]>>(id)) {
        return textures;
    }
    let textures = Arc::new(std::array::from_fn(|variant| {
        const SIZE: usize = 256;
        let mut pixels = Vec::with_capacity(SIZE * SIZE);
        for y in 0..SIZE {
            for x in 0..SIZE {
                let px = (x as f32 + 0.5) / SIZE as f32 * 2.0 - 1.0;
                let py = (y as f32 + 0.5) / SIZE as f32 * 2.0 - 1.0;
                let seed = variant as f32 * 17.7;
                let warp = noise(px * 3.0 + seed, py * 3.0) - 0.5;
                let mut density = 0.0;
                let mut amplitude = 0.55;
                let mut frequency = 3.5;
                for _ in 0..5 {
                    density += amplitude
                        * noise(
                            (px + warp * 0.3) * frequency + seed,
                            (py - warp * 0.2) * frequency,
                        );
                    amplitude *= 0.5;
                    frequency *= 2.1;
                }
                let edge = (1.0 - px * px - py * py - (1.0 - density) * 0.38).max(0.0);
                let alpha = (edge * 2.6).min(1.0) * smoothstep((density - 0.19) * 1.9);
                let light = (0.51 + density * 0.40 - py * 0.15).clamp(0.0, 1.0);
                let shade = (255.0 * light) as u8;
                pixels.push(Color32::from_rgba_unmultiplied(
                    shade,
                    shade,
                    shade,
                    (alpha * 230.0) as u8,
                ));
            }
        }
        ctx.load_texture(
            format!("smoke-{variant}"),
            ColorImage::new([SIZE, SIZE], pixels),
            TextureOptions::LINEAR,
        )
    }));
    ctx.data_mut(|data| data.insert_temp(id, Arc::clone(&textures)));
    textures
}

fn noise(x: f32, y: f32) -> f32 {
    fn lattice(x: i32, y: i32) -> f32 {
        let mut n = (x as u32).wrapping_mul(374_761_393) ^ (y as u32).wrapping_mul(668_265_263);
        n = (n ^ (n >> 13)).wrapping_mul(1_274_126_177);
        ((n ^ (n >> 16)) & 0xffff) as f32 / 65535.0
    }
    let ix = x.floor() as i32;
    let iy = y.floor() as i32;
    let fx = smoothstep(x - x.floor());
    let fy = smoothstep(y - y.floor());
    let a = lattice(ix, iy) * (1.0 - fx) + lattice(ix + 1, iy) * fx;
    let b = lattice(ix, iy + 1) * (1.0 - fx) + lattice(ix + 1, iy + 1) * fx;
    a * (1.0 - fy) + b * fy
}

struct Canvas<'a> {
    painter: &'a Painter,
    smoke: &'a [TextureHandle; 4],
    origin: Pos2,
    scale: f32,
    angle: f32,
    rotation: Vec2,
    offset: Vec2,
}

impl Canvas<'_> {
    fn point(&self, x: f32, y: f32) -> Pos2 {
        let cos = self.rotation.x;
        let sin = self.rotation.y;
        let x = x - 160.0;
        let y = y - 160.0;
        self.origin + (Vec2::new(cos * x - sin * y, sin * x + cos * y) + self.offset) * self.scale
    }

    fn stroke(&self, width: f32, color: Color32) -> Stroke {
        Stroke::new(width * self.scale, color)
    }

    fn line(&self, a: [f32; 2], b: [f32; 2], width: f32, color: Color32) {
        self.painter.line_segment(
            [self.point(a[0], a[1]), self.point(b[0], b[1])],
            self.stroke(width, color),
        );
    }

    fn circle(&self, x: f32, y: f32, radius: f32, color: Color32) {
        self.painter
            .circle_filled(self.point(x, y), radius * self.scale, color);
    }

    fn ring(&self, x: f32, y: f32, radius: f32, width: f32, color: Color32) {
        self.painter.circle_stroke(
            self.point(x, y),
            radius * self.scale,
            self.stroke(width, color),
        );
    }

    fn ellipse(&self, x: f32, y: f32, rx: f32, ry: f32, color: Color32) {
        self.painter.add(
            EllipseShape::filled(self.point(x, y), Vec2::new(rx, ry) * self.scale, color)
                .with_angle(self.angle),
        );
    }

    fn smoke(&self, bounds: [f32; 4], rotation: f32, variant: usize, color: Color32) {
        let [x, y, rx, ry] = bounds;
        let (sin, cos) = rotation.sin_cos();
        let mut mesh = Mesh::with_texture(self.smoke[variant % self.smoke.len()].id());
        mesh.reserve_vertices(4);
        mesh.reserve_triangles(2);
        for ([px, py], uv) in [
            ([-1.0, -1.0], Pos2::new(0.0, 0.0)),
            ([1.0, -1.0], Pos2::new(1.0, 0.0)),
            ([1.0, 1.0], Pos2::new(1.0, 1.0)),
            ([-1.0, 1.0], Pos2::new(0.0, 1.0)),
        ] {
            mesh.vertices.push(Vertex {
                pos: self.point(
                    x + px * rx * cos - py * ry * sin,
                    y + px * rx * sin + py * ry * cos,
                ),
                uv,
                color,
            });
        }
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(0, 2, 3);
        self.painter.add(mesh);
    }
    fn polygon(&self, points: &[[f32; 2]], fill: Color32, outline: Color32, width: f32) {
        self.painter.add(Shape::convex_polygon(
            points.iter().map(|p| self.point(p[0], p[1])).collect(),
            fill,
            self.stroke(width, outline),
        ));
    }

    fn rect(&self, x: f32, y: f32, width: f32, height: f32, radius: u8, color: Color32) {
        if self.angle == 0.0 {
            self.painter.rect_filled(
                Rect::from_min_max(self.point(x, y), self.point(x + width, y + height)),
                ((f32::from(radius) * self.scale).round().clamp(0.0, 255.0)) as u8,
                color,
            );
        } else {
            self.polygon(
                &[
                    [x, y],
                    [x + width, y],
                    [x + width, y + height],
                    [x, y + height],
                ],
                color,
                Color32::TRANSPARENT,
                0.0,
            );
        }
    }

    fn curve(&self, points: [[f32; 2]; 4], width: f32, color: Color32) {
        self.painter.add(CubicBezierShape::from_points_stroke(
            points.map(|p| self.point(p[0], p[1])),
            false,
            Color32::TRANSPARENT,
            self.stroke(width, color),
        ));
    }

    fn metallic_rect(&self, x: f32, y: f32, width: f32, height: f32, colors: [Color32; 6]) {
        const STOPS: [f32; 6] = [0.0, 0.12, 0.33, 0.51, 0.80, 1.0];
        let mut mesh = Mesh::default();
        mesh.reserve_vertices(12);
        mesh.reserve_triangles(10);
        for (stop, color) in STOPS.into_iter().zip(colors) {
            mesh.colored_vertex(self.point(x + width * stop, y), color);
            mesh.colored_vertex(self.point(x + width * stop, y + height), color);
        }
        for i in 0..5 {
            let a = i * 2;
            mesh.add_triangle(a, a + 1, a + 2);
            mesh.add_triangle(a + 1, a + 3, a + 2);
        }
        self.painter.add(mesh);
    }

    fn glow(&self, x: f32, y: f32, rx: f32, ry: f32, color: Color32) {
        // Shared radial topology; only position, size and tint vary with particle age.
        static DISC: LazyLock<Mesh> = LazyLock::new(|| {
            let mut mesh = Mesh::default();
            const SIDES: u32 = 32;
            const RINGS: u32 = 5;
            mesh.colored_vertex(Pos2::ZERO, Color32::WHITE);
            for ring in 1..=RINGS {
                let r = ring as f32 / RINGS as f32;
                for side in 0..SIDES {
                    let angle = side as f32 * TAU / SIDES as f32;
                    mesh.colored_vertex(
                        Pos2::new(angle.cos() * r, angle.sin() * r),
                        Color32::WHITE.gamma_multiply((1.0 - r * r).powi(2)),
                    );
                }
            }
            for side in 0..SIDES {
                mesh.add_triangle(0, side + 1, (side + 1) % SIDES + 1);
            }
            for ring in 0..RINGS - 1 {
                for side in 0..SIDES {
                    let a = 1 + ring * SIDES + side;
                    let b = 1 + ring * SIDES + (side + 1) % SIDES;
                    mesh.add_triangle(a, a + SIDES, b);
                    mesh.add_triangle(b, a + SIDES, b + SIDES);
                }
            }
            mesh
        });
        let mesh = Mesh {
            vertices: DISC
                .vertices
                .iter()
                .map(|v| {
                    Vertex::untextured(
                        self.point(x + v.pos.x * rx, y + v.pos.y * ry),
                        color.gamma_multiply(f32::from(v.color.a()) / 255.0),
                    )
                })
                .collect(),
            indices: DISC.indices.clone(),
            texture_id: DISC.texture_id,
        };
        self.painter.add(mesh);
    }

    fn cached_mesh(&self, source: &Mesh) {
        // egui owns submitted geometry; only the screen transform is per-frame.
        // The expensive sphere lighting and triangulation are shared by widgets.
        let mesh = Mesh {
            vertices: source
                .vertices
                .iter()
                .map(|v| Vertex::untextured(self.point(v.pos.x, v.pos.y), v.color))
                .collect(),
            indices: source.indices.clone(),
            texture_id: source.texture_id,
        };
        self.painter.add(mesh);
    }
}

fn readout(c: &Canvas<'_>, snapshot: &TimerSnapshot, y: f32, color: Color32, progress: bool) {
    c.rect(67.0, y - 24.0, 186.0, 48.0, 10, rgba(5, 12, 19, 0.97));
    c.painter.text(
        c.point(160.0, y - 1.0),
        Align2::CENTER_CENTER,
        format_remaining(snapshot.remaining),
        FontId::monospace(31.0 * c.scale),
        color,
    );
    if progress && snapshot.phase != TimerPhase::Finished {
        c.line(
            [88.0, y + 30.0],
            [232.0, y + 30.0],
            2.0,
            color.gamma_multiply(0.13),
        );
        c.line(
            [88.0, y + 30.0],
            [88.0 + 144.0 * snapshot.remaining_fraction, y + 30.0],
            2.0,
            color.gamma_multiply(0.8),
        );
    }
}

const GOLD: [Color32; 6] = [
    Color32::from_rgb(83, 49, 15),
    Color32::from_rgb(164, 109, 36),
    Color32::from_rgb(248, 221, 138),
    Color32::from_rgb(187, 137, 54),
    Color32::from_rgb(117, 73, 23),
    Color32::from_rgb(213, 170, 77),
];
const STEEL: [Color32; 6] = [
    Color32::from_rgb(31, 35, 40),
    Color32::from_rgb(99, 108, 117),
    Color32::from_rgb(179, 188, 195),
    Color32::from_rgb(69, 76, 85),
    Color32::from_rgb(35, 39, 45),
    Color32::from_rgb(124, 132, 141),
];

fn rgba(r: u8, g: u8, b: u8, alpha: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(r, g, b, (alpha.clamp(0.0, 1.0) * 255.0).round() as u8)
}

fn effect_fraction(snapshot: &TimerSnapshot, style: TimerStyle) -> Option<f32> {
    if snapshot.phase != TimerPhase::Finished {
        return None;
    }
    snapshot
        .effect_elapsed
        .filter(|elapsed| *elapsed < finish_effect_duration(style))
        .map(|elapsed| elapsed.as_secs_f32() / finish_effect_duration(style).as_secs_f32())
}

fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn draw_bomb(c: &Canvas<'_>, snapshot: &TimerSnapshot, time: f32) {
    if snapshot.phase == TimerPhase::Finished {
        let seconds = snapshot.effect_elapsed.unwrap_or_default().as_secs_f32();
        explosion(c, seconds.min(3.4) / 3.4);
        return;
    }
    c.glow(160.0, 299.0, 88.0, 12.0, rgba(6, 9, 17, 0.42));

    static SPHERE: LazyLock<Mesh> = LazyLock::new(sphere_mesh);
    c.cached_mesh(&SPHERE);
    static SEAMS: LazyLock<Mesh> = LazyLock::new(bomb_seam_mesh);
    c.cached_mesh(&SEAMS);
    c.ring(160.0, 209.0, 91.0, 1.3, Color32::from_rgb(64, 70, 79));

    // Curved side walls carry the same shading down to their lower edges.
    static FOOT: LazyLock<Mesh> = LazyLock::new(|| bomb_cylinder_mesh(25.0, 116.0, 121.0, 5.0));
    static NECK: LazyLock<Mesh> = LazyLock::new(|| bomb_cylinder_mesh(17.0, 96.0, 116.0, 4.5));
    c.cached_mesh(&FOOT);
    c.ellipse(160.0, 116.0, 25.0, 5.0, Color32::from_rgb(213, 170, 77));
    c.ellipse(160.0, 116.5, 17.8, 4.8, rgba(65, 42, 13, 0.24));
    c.cached_mesh(&NECK);
    let lip_color = Color32::from_rgb(230, 193, 102);
    c.ellipse(160.0, 96.0, 17.0, 4.5, lip_color);
    c.ellipse(160.0, 96.0, 9.0, 3.0, Color32::from_rgb(78, 53, 22));

    let fraction = snapshot.remaining_fraction.clamp(0.0, 1.0);
    let fuse_t = 0.075 + 0.925 * fraction;
    // Continue into the socket; its front lip hides the blunt stroke ends.
    let fuse_points = [[160.0, 99.3], [150.0, 18.0], [233.0, 24.0], [256.0, 79.0]];
    for (width, offset, color) in [
        (6.0, 0.0, Color32::from_rgb(97, 60, 22)),
        (4.0, 0.0, Color32::from_rgb(183, 139, 64)),
        (1.5, -0.8, Color32::from_rgb(239, 209, 139)),
    ] {
        c.painter.add(
            CubicBezierShape::from_points_stroke(
                fuse_points.map(|p| c.point(p[0] + offset, p[1])),
                false,
                Color32::TRANSPARENT,
                c.stroke(width, color),
            )
            .split_range(0.0..fuse_t),
        );
    }
    static FRONT_LIP: LazyLock<Mesh> = LazyLock::new(|| {
        let mut mesh = Mesh::default();
        mesh.reserve_vertices(33 * 4);
        mesh.reserve_triangles(32 * 6);
        for i in 0..=32 {
            let (sin, cos) = (PI * i as f32 / 32.0).sin_cos();
            for (rx, ry, color) in [
                (8.7, 2.7, Color32::TRANSPARENT),
                (9.0, 3.0, Color32::from_rgb(230, 193, 102)),
                (16.7, 4.2, Color32::from_rgb(230, 193, 102)),
                (17.0, 4.5, Color32::TRANSPARENT),
            ] {
                mesh.colored_vertex(Pos2::new(160.0 + rx * cos, 96.0 + ry * sin), color);
            }
        }
        for i in 0..32 {
            for band in 0..3 {
                let a = i * 4 + band;
                mesh.add_triangle(a, a + 1, a + 4);
                mesh.add_triangle(a + 1, a + 5, a + 4);
            }
        }
        mesh
    });
    c.cached_mesh(&FRONT_LIP);

    digital_display(
        c,
        [89.0, 185.0, 142.0, 57.0],
        snapshot.remaining,
        Color32::from_rgb(255, 88, 84),
        false,
    );
    for (x, y) in [(95.0, 191.0), (225.0, 191.0), (95.0, 236.0), (225.0, 236.0)] {
        c.circle(x, y, 2.4, Color32::from_rgb(114, 119, 127));
        c.line(
            [x - 1.2, y],
            [x + 1.2, y],
            0.8,
            Color32::from_rgb(27, 29, 33),
        );
    }

    if snapshot.phase == TimerPhase::Running {
        let tip = cubic_sample(fuse_points, fuse_t);
        fuse_sparks(c, tip, time);
    }
}

fn bomb_cylinder_mesh(radius: f32, top: f32, bottom: f32, depth: f32) -> Mesh {
    const STEPS: u32 = 64;
    const STOPS: [f32; 6] = [0.0, 0.12, 0.33, 0.51, 0.80, 1.0];
    let mut mesh = Mesh::default();
    mesh.reserve_vertices(((STEPS + 1) * 3) as usize);
    mesh.reserve_triangles((STEPS * 4) as usize);
    for i in 0..=STEPS {
        let t = i as f32 / STEPS as f32;
        let x = t * 2.0 - 1.0;
        let segment = STOPS
            .partition_point(|stop| *stop < t)
            .saturating_sub(1)
            .min(4);
        let blend = (t - STOPS[segment]) / (STOPS[segment + 1] - STOPS[segment]);
        let channels: [u8; 3] = std::array::from_fn(|channel| {
            let a = f32::from(GOLD[segment][channel]);
            let b = f32::from(GOLD[segment + 1][channel]);
            (a + (b - a) * blend).round() as u8
        });
        let color = Color32::from_rgb(channels[0], channels[1], channels[2]);
        let lower = bottom + depth * (1.0 - x * x).max(0.0).sqrt();
        mesh.colored_vertex(Pos2::new(160.0 + x * radius, top), color);
        mesh.colored_vertex(Pos2::new(160.0 + x * radius, lower), color);
        mesh.colored_vertex(
            Pos2::new(160.0 + x * radius, lower + 0.5),
            Color32::TRANSPARENT,
        );
    }
    for i in 0..STEPS {
        let a = i * 3;
        mesh.add_triangle(a, a + 1, a + 3);
        mesh.add_triangle(a + 1, a + 4, a + 3);
        mesh.add_triangle(a + 1, a + 2, a + 4);
        mesh.add_triangle(a + 2, a + 5, a + 4);
    }
    mesh
}

fn bomb_seam_mesh() -> Mesh {
    const STEPS: u32 = 96;
    const RADIUS: f32 = 90.5;
    let mut mesh = Mesh::default();
    mesh.reserve_vertices((4 * (STEPS + 1) * 4) as usize);
    mesh.reserve_triangles((4 * STEPS * 6) as usize);
    for seam_y in [173.0, 247.0] {
        for (y, width, color) in [
            (seam_y, 3.0, Color32::from_rgb(9, 11, 15)),
            (seam_y - 0.7, 1.1, Color32::from_rgb(73, 80, 90)),
        ] {
            // Follow a slightly inset sphere, with a small leftward alignment
            // correction. Taper both ends before they meet the shell border.
            let height = y - 209.0;
            let tilt = (4.5 / (RADIUS * RADIUS - height * height).sqrt()).atan();
            let (sin_tilt, cos_tilt) = tilt.sin_cos();
            let latitude = height * cos_tilt;
            let radius = (RADIUS * RADIUS - latitude * latitude).sqrt();
            let start = (latitude * sin_tilt / (radius * cos_tilt)).asin();
            let base = mesh.vertices.len() as u32;
            for i in 0..=STEPS {
                let angle = start + (PI - 2.0 * start) * i as f32 / STEPS as f32;
                let (sin, cos) = angle.sin_cos();
                let center = Pos2::new(
                    159.5 + radius * cos,
                    209.0 + latitude * cos_tilt + radius * sin_tilt * sin,
                );
                let depth = (radius * cos_tilt * sin - latitude * sin_tilt).max(0.0);
                let coverage = smoothstep(depth / (RADIUS * 0.4));
                let normal = Vec2::new(sin_tilt * cos, sin).normalized();
                let half_width = width * 0.5;
                for (offset, tint) in [
                    (-half_width - 0.5, Color32::TRANSPARENT),
                    (-half_width, color.gamma_multiply(coverage)),
                    (half_width, color.gamma_multiply(coverage)),
                    (half_width + 0.5, Color32::TRANSPARENT),
                ] {
                    mesh.colored_vertex(center + normal * (offset * coverage), tint);
                }
            }
            for i in 0..STEPS {
                for band in 0..3 {
                    let a = base + i * 4 + band;
                    mesh.add_triangle(a, a + 1, a + 4);
                    mesh.add_triangle(a + 1, a + 5, a + 4);
                }
            }
        }
    }
    mesh
}

fn sphere_mesh() -> Mesh {
    const RINGS: u32 = 32;
    const SEGMENTS: u32 = 112;
    let mut mesh = Mesh::default();
    mesh.reserve_vertices((1 + (RINGS + 1) * SEGMENTS) as usize);
    mesh.reserve_triangles(((RINGS * 2 + 1) * SEGMENTS) as usize);
    mesh.colored_vertex(Pos2::new(160.0, 209.0), sphere_color(0.0, 0.0));
    for ring in 1..=RINGS {
        let radius = ring as f32 / RINGS as f32;
        for segment in 0..SEGMENTS {
            let angle = segment as f32 * TAU / SEGMENTS as f32;
            let x = radius * angle.cos();
            let y = radius * angle.sin();
            mesh.colored_vertex(
                Pos2::new(160.0 + 91.0 * x, 209.0 + 91.0 * y),
                sphere_color(x, y),
            );
        }
    }
    for segment in 0..SEGMENTS {
        mesh.add_triangle(0, 1 + segment, 1 + (segment + 1) % SEGMENTS);
    }
    for ring in 1..RINGS {
        let inner = 1 + (ring - 1) * SEGMENTS;
        let outer = 1 + ring * SEGMENTS;
        for segment in 0..SEGMENTS {
            let next = (segment + 1) % SEGMENTS;
            mesh.add_triangle(inner + segment, outer + segment, outer + next);
            mesh.add_triangle(inner + segment, outer + next, inner + next);
        }
    }
    // Explicit edge coverage also works on transparent GL surfaces without MSAA.
    let outer = mesh.vertices.len() as u32;
    let inner = 1 + (RINGS - 1) * SEGMENTS;
    for segment in 0..SEGMENTS {
        let angle = segment as f32 * TAU / SEGMENTS as f32;
        mesh.colored_vertex(
            Pos2::new(160.0 + 91.7 * angle.cos(), 209.0 + 91.7 * angle.sin()),
            Color32::TRANSPARENT,
        );
    }
    for segment in 0..SEGMENTS {
        let next = (segment + 1) % SEGMENTS;
        mesh.add_triangle(inner + segment, outer + segment, outer + next);
        mesh.add_triangle(inner + segment, outer + next, inner + next);
    }
    mesh
}

fn sphere_color(x: f32, y: f32) -> Color32 {
    let z = (1.0 - x * x - y * y).max(0.0).sqrt();
    let diffuse = (-0.34 * x - 0.53 * y + 0.72 * z).max(0.0);
    let highlight = (-0.32 * x - 0.47 * y + 0.822 * z).max(0.0).powf(68.0);
    let rim = (1.0 - z).powi(5) * (0.5 + 0.5 * x).max(0.0);
    let spot = (-((x - 0.47).powi(2) / 0.012 + (y + 0.62).powi(2) / 0.018)).exp();
    let softbox = (-((x + 0.40).powi(2) / 0.022 + (y + 0.37).powi(2) / 0.20)).exp();
    let blue_rim = (1.0 - z).powi(3) * (-x).max(0.0);
    let light = 8.0 + diffuse * 23.0 + highlight * 155.0 + rim * 76.0 + softbox * 45.0;
    Color32::from_rgb(
        (light + spot * 86.0).min(255.0) as u8,
        (light * 1.06 + spot * 47.0 + blue_rim * 22.0).min(255.0) as u8,
        (light * 1.20 + spot * 17.0 + blue_rim * 48.0).min(255.0) as u8,
    )
}

fn cubic_sample(points: [[f32; 2]; 4], t: f32) -> [f32; 2] {
    let u = 1.0 - t;
    let weights = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
    let mut result = [0.0, 0.0];
    for (point, weight) in points.into_iter().zip(weights) {
        result[0] += point[0] * weight;
        result[1] += point[1] * weight;
    }
    result
}

fn fuse_sparks(c: &Canvas<'_>, tip: [f32; 2], time: f32) {
    c.glow(tip[0], tip[1], 24.0, 24.0, rgba(255, 111, 20, 0.55));
    c.glow(tip[0], tip[1], 10.0, 10.0, rgba(255, 211, 90, 0.90));
    c.circle(tip[0], tip[1], 2.7, Color32::from_rgb(255, 251, 216));
    for i in 0..16 {
        let seed = i as f32;
        let age = (time * 1.7 + seed * 0.618_034).fract();
        let angle = seed * 2.399_963;
        let distance = 5.0 + age * (14.0 + (seed % 4.0) * 3.0);
        let x = tip[0] + angle.cos() * distance;
        let y = tip[1] + angle.sin() * distance + age * age * 10.0;
        c.line(
            [x, y],
            [x - angle.cos() * 3.2, y - angle.sin() * 3.2],
            1.2,
            rgba(255, 205, 106, (1.0 - age).powi(2)),
        );
    }
}

struct BombFragment {
    landing: Vec2,
    shape: [[f32; 2]; 5],
    size: f32,
    flight_seconds: f32,
    arc_height: f32,
    angle: f32,
    spin: f32,
    fill: Color32,
}

static BOMB_FRAGMENTS: LazyLock<[BombFragment; 26]> = LazyLock::new(|| {
    // Generate the assortment once: independent samples avoid rows and frame-to-frame jitter.
    let mut state = 0x8f3a_72d1_u32;
    let mut random = || {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        (state >> 8) as f32 / 16_777_216.0
    };
    std::array::from_fn(|_| {
        let size = 3.0 + 19.0 * random().powi(2);
        let aspect = 0.40 + random() * 0.75;
        let landing = Vec2::new(25.0 + random() * 270.0, 214.0 + random() * 77.0);
        let shape = std::array::from_fn(|vertex| {
            let angle = vertex as f32 * TAU / 5.0;
            let radius = size * (0.80 + random() * 0.20);
            [angle.cos() * radius, angle.sin() * radius * aspect]
        });
        let shade = 10 + (random() * 12.0) as u8;
        BombFragment {
            landing,
            shape,
            size,
            flight_seconds: 0.75 + random() * 1.1,
            arc_height: 65.0 + random() * 115.0,
            angle: random() * TAU,
            spin: (random() - 0.5) * 15.0,
            fill: Color32::from_rgb(shade, shade + 2, shade + 6),
        }
    })
});
fn explosion(c: &Canvas<'_>, progress: f32) {
    let seconds = progress * 3.4;
    let burst = 1.0 - (-seconds * 9.0).exp();
    let heat = (1.0 - seconds / 1.25).max(0.0);
    let smoke = (1.0 - smoothstep((seconds - 1.0) / 2.1)) * smoothstep(seconds / 0.12);

    // Smoke rolls out behind the incandescent core; every puff has a soft edge.
    for i in 0..20 {
        let seed = i as f32;
        let angle = seed * 2.399_963;
        let distance = burst * (35.0 + seed % 5.0 * 13.0);
        let x = 160.0 + angle.cos() * distance;
        let y = 193.0 + angle.sin() * distance * 0.78 - seconds * (13.0 + seed % 4.0 * 5.0);
        let radius = 18.0 + seconds * 17.0 + seed % 3.0 * 6.0;
        c.smoke(
            [x, y, radius * 1.2, radius],
            angle + seconds * 0.15,
            i,
            rgba(151, 144, 143, smoke * 0.70),
        );
        if heat > 0.0 {
            c.smoke(
                [x, y + 12.0, radius, radius],
                angle - seconds * 0.20,
                i + 1,
                rgba(255, 98, 20, heat),
            );
            c.glow(
                x,
                y + 8.0,
                radius * 0.42,
                radius * 0.45,
                rgba(255, 211, 87, heat),
            );
        }
    }
    if seconds < 0.65 {
        let ring = seconds / 0.65;
        c.ring(
            160.0,
            197.0,
            12.0 + ring * 147.0,
            5.0 * (1.0 - ring) + 0.5,
            rgba(255, 210, 123, (1.0 - ring) * 0.9),
        );
        c.glow(
            160.0,
            196.0,
            115.0 * burst + 12.0,
            105.0 * burst + 12.0,
            rgba(255, 153, 33, heat),
        );
        c.glow(
            160.0,
            196.0,
            64.0 * burst + 10.0,
            64.0 * burst + 10.0,
            rgba(255, 250, 209, (1.0 - seconds / 0.65).powi(2)),
        );
    }

    // Broken steel shell pieces follow distinct ballistic arcs, then settle.
    // The original sphere and clock are never drawn in the finished state.
    for fragment in BOMB_FRAGMENTS.iter() {
        let flight = (seconds / fragment.flight_seconds).min(1.0);
        let spread = 1.0 - (1.0 - flight).powi(3);
        let x = 160.0 + (fragment.landing.x - 160.0) * spread;
        let y = 205.0 + (fragment.landing.y - 205.0) * flight
            - (PI * flight).sin() * fragment.arc_height;
        let rotation = fragment.angle + flight * fragment.spin;
        let (sin, cos) = rotation.sin_cos();
        let points = fragment
            .shape
            .map(|[px, py]| [x + px * cos - py * sin, y + px * sin + py * cos]);
        c.glow(
            x,
            fragment.landing.y + 4.0,
            fragment.size * 1.2,
            3.0,
            rgba(0, 0, 0, flight * 0.32),
        );
        c.polygon(&points, fragment.fill, Color32::from_rgb(43, 48, 56), 0.65);
        c.line(points[0], points[1], 0.8, rgba(89, 96, 107, 0.45));
        c.line(points[0], points[1], 1.2, rgba(255, 176, 73, heat));
    }
    if seconds < 1.8 {
        for i in 0..44 {
            let seed = i as f32;
            let angle = seed * 2.399_963;
            let speed = 52.0 + seed % 7.0 * 19.0;
            let x = 160.0 + angle.cos() * speed * seconds;
            let y = 198.0 + angle.sin() * speed * seconds + 36.0 * seconds * seconds;
            let alpha = (1.0 - seconds / 1.8).powi(2);
            c.line(
                [x, y],
                [
                    x - angle.cos() * 11.0,
                    y - angle.sin() * 11.0 - seconds * 4.0,
                ],
                1.4,
                rgba(255, 189, 74, alpha),
            );
            c.glow(x, y, 4.0, 4.0, rgba(255, 110, 25, alpha));
        }
    }
}

fn draw_hourglass(c: &Canvas<'_>, snapshot: &TimerSnapshot, time: f32) {
    c.glow(160.0, 292.0, 87.0, 11.0, rgba(23, 18, 13, 0.38));
    let finished = snapshot.phase == TimerPhase::Finished;
    let angle = if finished {
        effect_fraction(snapshot, TimerStyle::Hourglass).map_or(PI, |t| PI * smoothstep(t))
    } else {
        0.0
    };
    let (sin, cos) = angle.sin_cos();
    let glass = Canvas {
        painter: c.painter,
        smoke: c.smoke,
        origin: c.origin,
        scale: c.scale,
        angle,
        rotation: Vec2::new(cos, sin),
        offset: c.offset,
    };
    let g = &glass;
    let fraction = if finished {
        0.0
    } else {
        snapshot.remaining_fraction.clamp(0.0, 1.0)
    };

    for x in [85.0, 224.0] {
        g.metallic_rect(x, 56.0, 11.0, 210.0, GOLD);
        g.line(
            [x + 3.0, 62.0],
            [x + 3.0, 260.0],
            1.0,
            rgba(255, 238, 169, 0.6),
        );
        for y in [68.0, 246.0] {
            g.metallic_rect(x - 2.0, y, 15.0, 5.0, GOLD);
        }
    }

    let upper: [[f32; 2]; 50] = std::array::from_fn(|i| {
        let step = if i < 25 { i } else { 49 - i };
        let y = 73.0 + 90.0 * step as f32 / 24.0;
        let side = if i < 25 { 1.0 } else { -1.0 };
        [160.0 + side * upper_glass_width(y), y]
    });
    let lower: [[f32; 2]; 50] = std::array::from_fn(|i| {
        let step = if i < 25 { i } else { 49 - i };
        let y = 163.0 + 88.0 * step as f32 / 24.0;
        let side = if i < 25 { 1.0 } else { -1.0 };
        [160.0 + side * lower_glass_width(y), y]
    });
    g.polygon(
        &upper,
        rgba(168, 209, 220, 0.10),
        rgba(171, 211, 225, 0.54),
        1.6,
    );
    g.polygon(
        &lower,
        rgba(168, 209, 220, 0.10),
        rgba(171, 211, 225, 0.54),
        1.6,
    );

    let [upper_level, lower_level] = hourglass_sand_levels(fraction);
    let drained = 1.0 - fraction;
    let lower_peak = lower_sand_peak(lower_level);
    if fraction > 0.0 {
        // A single shaded surface: no separate oval cap or rectangular backdrop.
        let outline: [Pos2; 51] = std::array::from_fn(|i| {
            if i == 0 {
                return Pos2::new(160.0, upper_level);
            }
            let right = i <= 25;
            let row = if right { i - 1 } else { 50 - i };
            let y = upper_level + (UPPER_SAND_BOTTOM - upper_level) * row as f32 / 24.0;
            let side = if right { 1.0 } else { -1.0 };
            Pos2::new(160.0 + side * glass_inner_width(y), y)
        });
        draw_sand(g, &outline);
    }

    let settled = finished.then(|| (Vec2::new(sin, cos), settled_sand_cut(angle)));
    if let Some((normal, cut)) = settled {
        let mut outline = [Pos2::ZERO; 68];
        let mut count = 0;
        visit_settled_sand(normal, cut, |point| {
            outline[count] = point;
            count += 1;
        });
        draw_sand(g, &outline[..count]);
    } else if drained > 0.0 {
        let outline: [Pos2; 65] = std::array::from_fn(|i| {
            let right = i <= 32;
            let row = if right { i } else { 65 - i };
            let y = lower_peak + (LOWER_SAND_BOTTOM - lower_peak) * row as f32 / 32.0;
            let side = if right { 1.0 } else { -1.0 };
            Pos2::new(
                160.0 + side * lower_sand_width(y, lower_level, lower_peak),
                y,
            )
        });
        draw_sand(g, &outline);
    }

    // Tiny static grain highlights give the sand texture without shimmering at rest.
    for i in 0..110 {
        let seed = i as f32;
        let y = 84.0 + (seed * 31.7) % 165.0;
        let width = if (finished && (164.0..247.0).contains(&y))
            || (fraction > 0.0 && y < 161.0 && y > upper_level + 2.0)
        {
            glass_inner_width(y)
        } else if !finished && drained > 0.0 && y > lower_peak + 2.0 && y < 247.0 {
            lower_sand_width(y, lower_level, lower_peak)
        } else {
            0.0
        };
        if width > 1.0 {
            let x = 160.0 + ((seed * 0.618_034).fract() * 2.0 - 1.0) * (width - 1.0);
            if settled.is_some_and(|(normal, cut)| {
                normal.dot(Vec2::new(x - 160.0, y - 160.0)) < cut + 1.0
            }) {
                continue;
            }
            g.circle(x, y, 0.45, rgba(255, 236, 183, 0.48));
        }
    }
    if snapshot.phase == TimerPhase::Running && fraction > 0.0 {
        let bottom = lower_peak;
        g.line(
            [160.0, 164.0],
            [160.0, bottom.max(167.0)],
            1.1,
            rgba(238, 190, 101, 0.54),
        );
        for i in 0..13 {
            let phase = (time * 1.9 + i as f32 / 13.0).fract();
            let y = 164.0 + phase * (bottom - 164.0).max(3.0);
            let x = 160.0 + (i as f32 * 4.1).sin() * 1.25;
            g.circle(
                x,
                y,
                0.75 + (i % 3) as f32 * 0.15,
                Color32::from_rgb(253, 214, 145),
            );
        }
    }

    static REFLECTIONS: LazyLock<Mesh> = LazyLock::new(glass_reflections);
    g.cached_mesh(&REFLECTIONS);
    g.ellipse(160.0, 74.0, 53.0, 2.0, rgba(222, 247, 255, 0.18));
    g.ellipse(160.0, 250.0, 52.0, 2.0, rgba(222, 247, 255, 0.16));

    for y in [45.0, 260.0] {
        g.rect(76.0, y, 168.0, 15.0, 3, Color32::from_rgb(73, 43, 28));
        g.metallic_rect(78.0, y, 164.0, 4.0, GOLD);
        g.rect(81.0, y + 5.0, 158.0, 7.0, 2, Color32::from_rgb(106, 60, 36));
        g.line(
            [86.0, y + 8.0],
            [234.0, y + 8.0],
            0.8,
            rgba(207, 141, 78, 0.56),
        );
        g.metallic_rect(74.0, y + 13.0, 172.0, 5.0, GOLD);
    }
    digital_display(
        c,
        [84.0, 280.0, 152.0, 38.0],
        snapshot.remaining,
        Color32::from_rgb(245, 208, 134),
        finished,
    );
}

fn upper_glass_width(y: f32) -> f32 {
    let t = ((y - 73.0) / 90.0).clamp(0.0, 1.0);
    53.0 - 51.0 * t * t
}

fn lower_glass_width(y: f32) -> f32 {
    upper_glass_width(326.0 - y).clamp(2.0, 52.0)
}

fn glass_inner_width(y: f32) -> f32 {
    let outer = if y <= 163.0 {
        upper_glass_width(y)
    } else {
        lower_glass_width(y)
    };
    (outer - 2.5).max(0.0)
}

const UPPER_SAND_FULL: f32 = 84.0;
const UPPER_SAND_BOTTOM: f32 = 162.0;
const LOWER_SAND_BOTTOM: f32 = 248.0;

fn lower_sand_peak(level: f32) -> f32 {
    level - ((LOWER_SAND_BOTTOM - level) * 0.20).min(5.0)
}

// One shared volume for both chambers. Each horizontal slice is a circular
// cross-section, including the receiving pile's conical surface.
fn sand_volume(level: f32, lower: bool) -> f32 {
    const SLICES: usize = 64;
    let top = if lower { lower_sand_peak(level) } else { level };
    let bottom = if lower {
        LOWER_SAND_BOTTOM
    } else {
        UPPER_SAND_BOTTOM
    };
    let step = (bottom - top) / SLICES as f32;
    let mut volume = 0.0;
    for i in 0..=SLICES {
        let y = top + step * i as f32;
        let width = if lower {
            lower_sand_width(y, level, top)
        } else {
            glass_inner_width(y)
        };
        let weight = if i == 0 || i == SLICES { 0.5 } else { 1.0 };
        volume += PI * width * width * weight;
    }
    volume * step
}

fn sand_level_for_volume(volume: f32, lower: bool) -> f32 {
    let mut full = if lower { 172.0 } else { UPPER_SAND_FULL };
    let mut empty = if lower {
        LOWER_SAND_BOTTOM
    } else {
        UPPER_SAND_BOTTOM
    };
    if volume <= 0.0 {
        return empty;
    }
    for _ in 0..16 {
        let level = (full + empty) * 0.5;
        if sand_volume(level, lower) > volume {
            full = level;
        } else {
            empty = level;
        }
    }
    (full + empty) * 0.5
}

fn hourglass_sand_levels(remaining: f32) -> [f32; 2] {
    const STEPS: usize = 256;
    // Invert the volume profiles once; animation only interpolates two heights.
    static LEVELS: LazyLock<[[f32; 2]; STEPS + 1]> = LazyLock::new(|| {
        let capacity = sand_volume(UPPER_SAND_FULL, false);
        std::array::from_fn(|i| {
            let fraction = i as f32 / STEPS as f32;
            [
                if i == STEPS {
                    UPPER_SAND_FULL
                } else {
                    sand_level_for_volume(capacity * fraction, false)
                },
                sand_level_for_volume(capacity * (1.0 - fraction), true),
            ]
        })
    });
    let position = remaining.clamp(0.0, 1.0) * STEPS as f32;
    let index = position as usize;
    let next = (index + 1).min(STEPS);
    let blend = position - index as f32;
    std::array::from_fn(|i| LEVELS[index][i] + blend * (LEVELS[next][i] - LEVELS[index][i]))
}

fn lower_sand_width(y: f32, level: f32, peak: f32) -> f32 {
    if !(peak..=LOWER_SAND_BOTTOM).contains(&y) {
        return 0.0;
    }
    let wall = glass_inner_width(y);
    if y < level {
        wall.min(glass_inner_width(level) * (y - peak) / (level - peak))
    } else {
        wall
    }
}

fn draw_sand(c: &Canvas<'_>, points: &[Pos2]) {
    if points.len() < 3 {
        return;
    }
    let center = points
        .iter()
        .fold(Vec2::ZERO, |sum, point| sum + point.to_vec2())
        / points.len() as f32;
    let bounds = Rect::from_points(points);
    let feather = (1.0 / (c.scale * c.painter.ctx().pixels_per_point()))
        .min(bounds.width().min(bounds.height()) * 0.5);
    let mut mesh = Mesh::default();
    mesh.reserve_vertices(points.len() * 2 + 1);
    mesh.reserve_triangles(points.len() * 3);
    mesh.colored_vertex(c.point(center.x, center.y), sand_color(center.to_pos2()));
    for (i, point) in points.iter().enumerate() {
        let previous = (*point - points[(i + points.len() - 1) % points.len()]).normalized();
        let next = (points[(i + 1) % points.len()] - *point).normalized();
        let a = Vec2::new(previous.y, -previous.x);
        let b = Vec2::new(next.y, -next.x);
        let bisector = (a + b).normalized();
        let offset = bisector * (feather * 0.5 / bisector.dot(a).max(0.25));
        let inner = *point - offset;
        let outer = *point + offset;
        mesh.colored_vertex(c.point(inner.x, inner.y), sand_color(*point));
        mesh.colored_vertex(c.point(outer.x, outer.y), Color32::TRANSPARENT);
    }
    for i in 0..points.len() as u32 {
        let a = 1 + i * 2;
        let b = 1 + ((i + 1) % points.len() as u32) * 2;
        mesh.add_triangle(0, a, b);
        mesh.add_triangle(a, a + 1, b + 1);
        mesh.add_triangle(a, b + 1, b);
    }
    c.painter.add(mesh);
}

fn sand_color(point: Pos2) -> Color32 {
    let side = ((point.x - 160.0) / glass_inner_width(point.y).max(1.0)).clamp(-1.0, 1.0);
    let center = [244.0, 203.0, 122.0];
    let edge = if side < 0.0 {
        [168.0, 108.0, 43.0]
    } else {
        [192.0, 131.0, 51.0]
    };
    let color: [u8; 3] =
        std::array::from_fn(|i| (center[i] + (edge[i] - center[i]) * side.abs()) as u8);
    Color32::from_rgb(color[0], color[1], color[2])
}

static LOWER_SAND_CHAMBER: LazyLock<[Pos2; 66]> = LazyLock::new(|| {
    std::array::from_fn(|i| {
        let right = i < 33;
        let row = if right { i } else { 65 - i };
        let y = 164.0 + (LOWER_SAND_BOTTOM - 164.0) * row as f32 / 32.0;
        let side = if right { 1.0 } else { -1.0 };
        Pos2::new(160.0 + side * glass_inner_width(y), y)
    })
});

// Clip the receiving chamber against a world-horizontal sand surface. During
// the flip gravity moves the sand toward the neck, not into a floating wedge.
fn visit_settled_sand(normal: Vec2, cut: f32, mut visit: impl FnMut(Pos2)) {
    let distance = |point: Pos2| normal.dot(point.to_vec2() - Vec2::splat(160.0)) - cut;
    let mut previous = LOWER_SAND_CHAMBER[65];
    let mut previous_distance = distance(previous);
    for &point in LOWER_SAND_CHAMBER.iter() {
        let current_distance = distance(point);
        if (previous_distance >= 0.0) != (current_distance >= 0.0) {
            visit(
                previous
                    + (point - previous)
                        * (previous_distance / (previous_distance - current_distance)),
            );
        }
        if current_distance >= 0.0 {
            visit(point);
        }
        previous = point;
        previous_distance = current_distance;
    }
}

fn settled_sand_volume(normal: Vec2, cut: f32) -> f32 {
    let mut top = LOWER_SAND_BOTTOM;
    let mut bottom = 164.0_f32;
    visit_settled_sand(normal, cut, |point| {
        top = top.min(point.y);
        bottom = bottom.max(point.y);
    });
    if bottom <= top {
        return 0.0;
    }

    // Integrate circular segments below the world-horizontal settling plane.
    // Bounds come from the rendered clip, including the horizontal end poses.
    const SLICES: usize = 128;
    let step = (bottom - top) / SLICES as f32;
    let horizontal = normal.x.abs();
    let mut volume = 0.0;
    for i in 0..SLICES {
        let y = top + (i as f32 + 0.5) * step;
        let radius = glass_inner_width(y);
        let area = if horizontal < 0.00001 {
            PI * radius * radius
        } else {
            let x = (cut - normal.y * (y - 160.0)) / horizontal;
            if x >= radius {
                0.0
            } else if x <= -radius {
                PI * radius * radius
            } else {
                radius * radius * (x / radius).acos()
                    - x * (radius * radius - x * x).max(0.0).sqrt()
            }
        };
        volume += area;
    }
    volume * step
}

fn settled_sand_cut(angle: f32) -> f32 {
    const STEPS: usize = 128;
    static CUTS: LazyLock<[f32; STEPS + 1]> = LazyLock::new(|| {
        let capacity = sand_volume(UPPER_SAND_FULL, false);
        std::array::from_fn(|i| {
            let (sin, cos) = (PI * i as f32 / STEPS as f32).sin_cos();
            let normal = Vec2::new(sin, cos);
            let (mut low, mut high) = (-120.0, 120.0);
            for _ in 0..18 {
                let cut = (low + high) * 0.5;
                if settled_sand_volume(normal, cut) > capacity {
                    low = cut;
                } else {
                    high = cut;
                }
            }
            (low + high) * 0.5
        })
    });
    let position = (angle / PI).clamp(0.0, 1.0) * STEPS as f32;
    let index = position as usize;
    let blend = position - index as f32;
    CUTS[index] + blend * (CUTS[(index + 1).min(STEPS)] - CUTS[index])
}

fn glass_reflections() -> Mesh {
    let mut mesh = Mesh::default();
    mesh.reserve_vertices(4 * 99);
    mesh.reserve_triangles(4 * 128);
    for lower in [false, true] {
        for side in [-1.0, 1.0] {
            let start = mesh.vertices.len() as u32;
            for row in 0..=32 {
                let t = row as f32 / 32.0;
                let y = if lower {
                    175.0 + t * 68.0
                } else {
                    82.0 + t * 69.0
                };
                let wall = if lower {
                    lower_glass_width(y)
                } else {
                    upper_glass_width(y)
                };
                let x = 160.0 + side * (wall - 1.2);
                let opacity = (PI * t).sin() * if side < 0.0 { 0.55 } else { 0.24 };
                // Tapered ribbons follow the walls inside the clear glass margin.
                for (offset, color) in [
                    (-0.8, Color32::TRANSPARENT),
                    (0.0, rgba(232, 247, 255, opacity)),
                    (0.8, Color32::TRANSPARENT),
                ] {
                    mesh.colored_vertex(Pos2::new(x + offset, y), color);
                }
            }
            for row in 0..32 {
                for column in 0..2 {
                    let a = start + row * 3 + column;
                    mesh.add_triangle(a, a + 3, a + 1);
                    mesh.add_triangle(a + 1, a + 3, a + 4);
                }
            }
        }
    }
    mesh
}

fn draw_rocket(c: &Canvas<'_>, snapshot: &TimerSnapshot, time: f32) {
    if snapshot.phase == TimerPhase::Finished {
        let seconds = snapshot.effect_elapsed.unwrap_or_default().as_secs_f32();
        if seconds >= 3.6 {
            return;
        }
        // Accelerate through the viewport's top edge, rather than shrinking into a fade.
        let ascent = (seconds - 0.18).max(0.0);
        let altitude = 190.0 * ascent * ascent;
        let shake = (time * 91.0).sin() * (1.0 - seconds / 0.6).max(0.0) * 2.2;
        let rocket = Canvas {
            painter: c.painter,
            smoke: c.smoke,
            origin: c.origin,
            scale: c.scale,
            angle: 0.0,
            rotation: Vec2::new(1.0, 0.0),
            offset: Vec2::new(shake, -altitude),
        };
        let cloud_alpha = 1.0 - smoothstep((seconds - 1.1) / 2.5);
        for i in 0..24 {
            let seed = i as f32;
            let side = if i % 2 == 0 { -1.0 } else { 1.0 };
            let spread = (seconds * 1.4).min(1.0);
            let x = 160.0 + side * (12.0 + seed % 6.0 * 16.0) * spread;
            let y = 284.0 - seed % 5.0 * 6.0 - seconds * (4.0 + seed % 4.0 * 5.0);
            let radius = 13.0 + seconds * 13.0 + seed % 5.0 * 4.0;
            c.smoke(
                [x, y, radius * 1.15, radius * 0.82],
                seed * 1.7 + seconds * side * 0.14,
                i,
                rgba(222, 230, 239, cloud_alpha * 0.65),
            );
        }
        if altitude < 420.0 {
            exhaust(&rocket, time, true);
            rocket_body(&rocket, snapshot, time, true);
        }
        return;
    }
    c.glow(160.0, 299.0, 72.0, 10.0, rgba(10, 24, 38, 0.35));
    let running = snapshot.phase == TimerPhase::Running;
    let hover = if running {
        (time * 2.4).sin() * 2.0
    } else {
        0.0
    };
    let rocket = Canvas {
        painter: c.painter,
        smoke: c.smoke,
        origin: c.origin,
        scale: c.scale,
        angle: 0.0,
        rotation: Vec2::new(1.0, 0.0),
        offset: Vec2::new(0.0, hover),
    };
    if running {
        exhaust(&rocket, time, false);
    }
    rocket_body(&rocket, snapshot, time, running);
}

fn exhaust(c: &Canvas<'_>, time: f32, launching: bool) {
    let strength = if launching { 1.5 } else { 1.0 };
    for i in 0..22 {
        let seed = i as f32;
        let age = (time * 0.9 + seed * 0.618_034).fract();
        let side = if i % 2 == 0 { -1.0 } else { 1.0 };
        let x = 160.0 + side * age * (15.0 + seed % 5.0 * 7.0) * strength;
        let y = 266.0 + age * 39.0;
        let radius = (6.0 + age * 20.0) * strength;
        let opacity = (PI * age).sin() * 0.50;
        c.smoke(
            [x, y, radius * 1.1, radius * 0.85],
            seed * 1.7 + age * side * 0.5,
            i,
            rgba(206, 219, 230, opacity),
        );
    }
    c.glow(160.0, 263.0, 29.0, 45.0, rgba(255, 122, 28, 0.35));
}

const ROCKET_PROFILE: [[f32; 2]; 4] =
    [[160.0, 36.0], [115.0, 64.0], [114.0, 152.0], [129.0, 228.0]];
const ROCKET_HULL_ROWS: usize = 80;

fn rocket_body(c: &Canvas<'_>, snapshot: &TimerSnapshot, time: f32, engine: bool) {
    let finished = snapshot.phase == TimerPhase::Finished;
    let coral = Color32::from_rgb(211, 89, 68);
    c.polygon(
        &[
            [123.0, 179.0],
            [113.0, 187.0],
            [92.0, 243.0],
            [121.0, 232.0],
            [140.0, 205.0],
        ],
        coral,
        Color32::from_rgb(118, 51, 44),
        1.3,
    );
    c.polygon(
        &[
            [197.0, 179.0],
            [207.0, 187.0],
            [228.0, 243.0],
            [199.0, 232.0],
            [180.0, 205.0],
        ],
        Color32::from_rgb(162, 62, 52),
        Color32::from_rgb(107, 45, 40),
        1.3,
    );
    for (side, color) in [
        (-1.0, Color32::from_rgb(248, 145, 112)),
        (1.0, Color32::from_rgb(217, 103, 83)),
    ] {
        c.line(
            [160.0 + side * 45.0, 190.0],
            [160.0 + side * 61.0, 235.0],
            2.0,
            color,
        );
    }
    c.metallic_rect(139.0, 227.0, 42.0, 17.0, STEEL);
    c.ellipse(160.0, 244.0, 21.0, 4.0, Color32::from_rgb(43, 49, 56));
    c.ellipse(160.0, 244.0, 14.0, 2.2, Color32::from_rgb(17, 22, 29));
    // The plume starts inside the visible opening, in front of the nozzle's underside.
    if engine {
        let pulse = 0.5 + 0.5 * (time * 17.0).sin();
        let length = if finished { 65.0 } else { 31.0 + pulse * 9.0 };
        c.glow(
            160.0,
            250.0 + length * 0.35,
            27.0,
            length * 0.8,
            rgba(255, 119, 30, 0.55),
        );
        let mut flame = Mesh::default();
        flame.reserve_vertices(125);
        flame.reserve_triangles(192);
        for row in 0..=24 {
            let t = row as f32 / 24.0;
            let half_width = (1.0 - t).powf(0.75) * (11.0 + 2.0 * (time * 24.0 - t * 11.0).sin());
            let bend = (time * 19.0 - t * 8.0).sin() * t * 3.0;
            let opacity = 1.0 - smoothstep((t - 0.75) / 0.25);
            for (column, color) in [
                (-1.0, Color32::TRANSPARENT),
                (-0.60, rgba(255, 116, 23, opacity)),
                (0.0, rgba(255, 248, 204, opacity)),
                (0.60, rgba(255, 157, 42, opacity)),
                (1.0, Color32::TRANSPARENT),
            ] {
                flame.colored_vertex(
                    c.point(160.0 + bend + column * half_width, 244.0 + length * t),
                    color,
                );
            }
        }
        for row in 0..24 {
            for column in 0..4 {
                let a = row * 5 + column;
                flame.add_triangle(a, a + 5, a + 1);
                flame.add_triangle(a + 1, a + 5, a + 6);
            }
        }
        c.painter.add(flame);
        for i in 0..8 {
            let age = (time * 2.5 + i as f32 * 0.618_034).fract();
            let x = 160.0 + (i as f32 * 2.399_963).sin() * age * 16.0;
            c.circle(x, 251.0 + age * 43.0, 0.9, rgba(255, 216, 135, 1.0 - age));
        }
    }

    // Symmetric convex hull with horizontal metallic shading.
    static BODY: LazyLock<Mesh> = LazyLock::new(rocket_mesh);
    c.cached_mesh(&BODY);
    // One closed contour joins both sides to the bottom rim without loose caps.
    static OUTLINE: LazyLock<[[f32; 2]; ROCKET_HULL_ROWS * 2 - 1]> = LazyLock::new(|| {
        std::array::from_fn(|i| {
            let left = i < ROCKET_HULL_ROWS;
            let row = if left {
                i
            } else {
                ROCKET_HULL_ROWS * 2 - 1 - i
            };
            let [x, y] = cubic_sample(ROCKET_PROFILE, row as f32 / (ROCKET_HULL_ROWS - 1) as f32);
            [if left { x } else { 320.0 - x }, y]
        })
    });
    c.polygon(
        &*OUTLINE,
        Color32::TRANSPARENT,
        Color32::from_rgb(100, 108, 110),
        1.3,
    );
    c.curve(
        [[138.0, 62.0], [151.0, 68.0], [168.0, 70.0], [180.0, 64.0]],
        1.5,
        Color32::from_rgb(158, 157, 141),
    );
    let [band_edge, band_y] = cubic_sample(ROCKET_PROFILE, 0.95);
    let band_inset = 1.8;
    c.curve(
        [
            [band_edge + band_inset, band_y],
            [145.0, band_y + 7.0],
            [175.0, band_y + 7.0],
            [320.0 - band_edge - band_inset, band_y],
        ],
        3.0,
        Color32::from_rgb(130, 137, 136),
    );

    c.circle(160.0, 114.0, 27.0, Color32::from_rgb(99, 110, 114));
    c.ring(160.0, 114.0, 26.0, 2.5, Color32::from_rgb(237, 233, 205));
    c.circle(160.0, 114.0, 21.0, Color32::from_rgb(10, 31, 49));
    c.glow(160.0, 119.0, 20.0, 17.0, Color32::from_rgb(46, 144, 174));
    c.glow(150.0, 104.0, 9.0, 6.0, rgba(212, 250, 255, 0.88));
    c.curve(
        [
            [147.0, 106.0],
            [148.0, 102.0],
            [151.0, 100.0],
            [157.0, 99.0],
        ],
        2.2,
        rgba(224, 250, 255, 0.8),
    );
    c.glow(169.0, 126.0, 6.0, 4.0, rgba(114, 210, 241, 0.65));
    for i in 0..8 {
        let angle = i as f32 * TAU / 8.0;
        c.circle(
            160.0 + angle.cos() * 24.0,
            114.0 + angle.sin() * 24.0,
            1.1,
            Color32::from_rgb(85, 95, 98),
        );
    }

    digital_display(
        c,
        [124.0, 155.0, 72.0, 28.0],
        snapshot.remaining,
        Color32::from_rgb(211, 239, 231),
        finished,
    );
    c.rect(136.0, 193.0, 48.0, 5.0, 2, Color32::from_rgb(96, 112, 113));
    c.rect(
        137.0,
        194.0,
        46.0 * snapshot.remaining_fraction.clamp(0.0, 1.0),
        3.0,
        1,
        Color32::from_rgb(225, 120, 86),
    );
    c.polygon(
        &[
            [158.0, 207.0],
            [162.0, 207.0],
            [165.0, 241.0],
            [160.0, 250.0],
            [155.0, 241.0],
        ],
        coral,
        Color32::from_rgb(131, 54, 46),
        0.8,
    );
    c.line(
        [159.0, 211.0],
        [159.0, 240.0],
        1.2,
        Color32::from_rgb(245, 153, 114),
    );
}

fn rocket_mesh() -> Mesh {
    // Fill and outline share the same Bezier silhouette, with subpixel-sized segments.
    const ROWS: usize = ROCKET_HULL_ROWS;
    const COLORS: [Color32; 6] = [
        Color32::from_rgb(92, 116, 128),
        Color32::from_rgb(214, 230, 228),
        Color32::from_rgb(253, 251, 239),
        Color32::from_rgb(225, 229, 220),
        Color32::from_rgb(159, 181, 187),
        Color32::from_rgb(68, 100, 124),
    ];
    let mut mesh = Mesh::default();
    mesh.reserve_vertices(ROWS * 8);
    mesh.reserve_triangles((ROWS - 1) * 14);
    for row in 0..ROWS {
        let t = row as f32 / (ROWS - 1) as f32;
        let [x, y] = cubic_sample(ROCKET_PROFILE, t);
        let width = 160.0 - x;
        mesh.colored_vertex(Pos2::new(160.0 - width - 0.7, y), Color32::TRANSPARENT);
        for (column, color) in COLORS.into_iter().enumerate() {
            mesh.colored_vertex(
                Pos2::new(160.0 - width + 2.0 * width * column as f32 / 5.0, y),
                color,
            );
        }
        mesh.colored_vertex(Pos2::new(160.0 + width + 0.7, y), Color32::TRANSPARENT);
    }
    for row in 0..ROWS - 1 {
        for column in 0..7 {
            let a = (row * 8 + column) as u32;
            mesh.add_triangle(a, a + 8, a + 1);
            mesh.add_triangle(a + 1, a + 8, a + 9);
        }
    }
    mesh
}

fn digital_display(
    c: &Canvas<'_>,
    bounds: [f32; 4],
    remaining: Duration,
    color: Color32,
    finished: bool,
) {
    let [x, y, width, height] = bounds;
    c.rect(
        x - 2.0,
        y - 2.0,
        width + 4.0,
        height + 4.0,
        5,
        Color32::from_rgb(16, 18, 22),
    );
    c.rect(x, y, width, height, 3, Color32::from_rgb(87, 91, 97));
    c.rect(
        x + 2.0,
        y + 2.0,
        width - 4.0,
        height - 4.0,
        2,
        Color32::from_rgb(26, 22, 25),
    );
    c.rect(
        x + 4.0,
        y + 4.0,
        width - 8.0,
        height - 8.0,
        2,
        if finished {
            Color32::from_rgb(43, 27, 25)
        } else {
            Color32::from_rgb(54, 31, 33)
        },
    );

    let seconds = display_seconds(remaining);
    let mut characters = [0_u8; 8];
    let count = if seconds >= 3600 {
        let hours = (seconds / 3600).min(99) as u8;
        let start = usize::from(hours >= 10);
        if start > 0 {
            characters[0] = hours / 10;
        }
        characters[start] = hours % 10;
        characters[start + 1] = 10;
        characters[start + 2] = (seconds / 60 % 60 / 10) as u8;
        characters[start + 3] = (seconds / 60 % 10) as u8;
        characters[start + 4] = 10;
        characters[start + 5] = (seconds % 60 / 10) as u8;
        characters[start + 6] = (seconds % 10) as u8;
        start + 7
    } else {
        characters[0] = (seconds / 60 / 10) as u8;
        characters[1] = (seconds / 60 % 10) as u8;
        characters[2] = 10;
        characters[3] = (seconds % 60 / 10) as u8;
        characters[4] = (seconds % 10) as u8;
        5
    };
    let colon_count = if count > 5 { 2.0 } else { 1.0 };
    let digit_count = count as f32 - colon_count;
    let unit =
        ((width - 17.0) / (digit_count * 1.0 + colon_count * 0.40)).min((height - 12.0) / 1.7);
    let total_width = unit * (digit_count + colon_count * 0.40);
    let mut cursor = x + (width - total_width) / 2.0;
    let top = y + (height - unit * 1.7) / 2.0;
    let mut segments = Mesh::default();
    segments.reserve_vertices((digit_count as usize * 7 * 12) + colon_count as usize * 16);
    segments.reserve_triangles(digit_count as usize * 7 * 16 + colon_count as usize * 20);
    for character in &characters[..count] {
        if *character == 10 {
            for offset in [0.53, 1.22] {
                let size = unit * 0.11;
                add_quad(
                    &mut segments,
                    c,
                    cursor + unit * 0.13,
                    top + unit * offset,
                    size,
                    size,
                    color,
                );
            }
            cursor += unit * 0.40;
        } else {
            seven_segment(&mut segments, c, cursor, top, unit, *character, color);
            cursor += unit;
        }
    }
    c.painter.add(segments);
}

fn add_quad(
    mesh: &mut Mesh,
    c: &Canvas<'_>,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    color: Color32,
) {
    feathered_polygon(
        mesh,
        [
            c.point(x, y),
            c.point(x + width, y),
            c.point(x + width, y + height),
            c.point(x, y + height),
        ],
        color,
        (0.65 / c.painter.ctx().pixels_per_point()).min(width.min(height) * c.scale * 0.6),
    );
}

fn feathered_polygon<const N: usize>(
    mesh: &mut Mesh,
    points: [Pos2; N],
    color: Color32,
    feather: f32,
) {
    let start = mesh.vertices.len() as u32;
    for i in 0..N {
        let previous = (points[i] - points[(i + N - 1) % N]).normalized();
        let next = (points[(i + 1) % N] - points[i]).normalized();
        let a = Vec2::new(previous.y, -previous.x);
        let b = Vec2::new(next.y, -next.x);
        let bisector = (a + b).normalized();
        let offset = bisector * (feather * 0.5 / bisector.dot(a).max(0.25));
        mesh.colored_vertex(points[i] - offset, color);
        mesh.colored_vertex(points[i] + offset, Color32::TRANSPARENT);
    }
    for i in 1..N as u32 - 1 {
        mesh.add_triangle(start, start + i * 2, start + (i + 1) * 2);
    }
    for i in 0..N as u32 {
        let a = start + i * 2;
        let b = start + ((i + 1) % N as u32) * 2;
        mesh.add_triangle(a, a + 1, b + 1);
        mesh.add_triangle(a, b + 1, b);
    }
}

fn seven_segment(
    mesh: &mut Mesh,
    c: &Canvas<'_>,
    x: f32,
    y: f32,
    unit: f32,
    digit: u8,
    color: Color32,
) {
    const MASKS: [u8; 10] = [
        0b0111111, 0b0000110, 0b1011011, 0b1001111, 0b1100110, 0b1101101, 0b1111101, 0b0000111,
        0b1111111, 0b1101111,
    ];
    // a,b,c,d,e,f,g. Beveled ends keep small digits distinct without font assets.
    const SEGMENTS: [[[f32; 2]; 6]; 7] = [
        [
            [0.19, 0.04],
            [0.70, 0.04],
            [0.79, 0.12],
            [0.70, 0.20],
            [0.19, 0.20],
            [0.10, 0.12],
        ],
        [
            [0.74, 0.23],
            [0.82, 0.15],
            [0.88, 0.22],
            [0.88, 0.69],
            [0.81, 0.77],
            [0.74, 0.69],
        ],
        [
            [0.74, 0.96],
            [0.82, 0.88],
            [0.88, 0.95],
            [0.88, 1.42],
            [0.81, 1.50],
            [0.74, 1.42],
        ],
        [
            [0.19, 1.48],
            [0.70, 1.48],
            [0.79, 1.56],
            [0.70, 1.64],
            [0.19, 1.64],
            [0.10, 1.56],
        ],
        [
            [0.03, 0.95],
            [0.10, 0.88],
            [0.17, 0.96],
            [0.17, 1.42],
            [0.10, 1.50],
            [0.03, 1.42],
        ],
        [
            [0.03, 0.22],
            [0.10, 0.15],
            [0.17, 0.23],
            [0.17, 0.69],
            [0.10, 0.77],
            [0.03, 0.69],
        ],
        [
            [0.19, 0.76],
            [0.70, 0.76],
            [0.79, 0.84],
            [0.70, 0.92],
            [0.19, 0.92],
            [0.10, 0.84],
        ],
    ];
    let mask = MASKS[usize::from(digit.min(9))];
    for (index, polygon) in SEGMENTS.into_iter().enumerate() {
        let active = mask & (1 << index) != 0;
        let fill = if active {
            color
        } else {
            rgba(color.r(), color.g(), color.b(), 0.075)
        };
        feathered_polygon(
            mesh,
            polygon.map(|point| c.point(x + point[0] * unit, y + point[1] * unit)),
            fill,
            (0.65 / c.painter.ctx().pixels_per_point()).min(unit * c.scale * 0.10),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Integrate the rendered piecewise-linear silhouette as conical frusta,
    // independently of the lookup-table calibration.
    fn rendered_volume(level: f32, lower: bool) -> f32 {
        let peak = lower_sand_peak(level);
        let top = if lower { peak } else { level };
        let bottom = if lower {
            LOWER_SAND_BOTTOM
        } else {
            UPPER_SAND_BOTTOM
        };
        let rows = if lower { 32 } else { 24 };
        let step = (bottom - top) / rows as f32;
        let radius = |y| {
            if lower {
                lower_sand_width(y, level, peak)
            } else {
                glass_inner_width(y)
            }
        };
        (0..rows)
            .map(|i| {
                let a = radius(top + step * i as f32);
                let b = radius(top + step * (i + 1) as f32);
                PI * step * (a * a + a * b + b * b) / 3.0
            })
            .sum()
    }

    #[test]
    fn countdown_transfers_the_initial_sand_volume() {
        let capacity = rendered_volume(UPPER_SAND_FULL, false);
        for i in 0..=1000 {
            let remaining = i as f32 / 1000.0;
            let [upper, lower] = hourglass_sand_levels(remaining);
            let above = rendered_volume(upper, false) / capacity;
            let below = rendered_volume(lower, true) / capacity;
            assert!(
                (above - remaining).abs() < 0.002,
                "remaining={remaining}: upper volume={above}"
            );
            assert!(
                (below - (1.0 - remaining)).abs() < 0.002,
                "remaining={remaining}: lower volume={below}"
            );
            assert!(
                (above + below - 1.0).abs() < 0.002,
                "remaining={remaining}: total volume={}",
                above + below
            );
        }
    }

    // Measure depth through the clipped, rendered polygon on an independent
    // midpoint grid. This catches gaining or losing sand during the flip.
    fn rendered_flip_volume(normal: Vec2, cut: f32) -> f32 {
        let mut points = Vec::new();
        visit_settled_sand(normal, cut, |p| points.push(p));
        let bounds = Rect::from_points(&points);
        let dy = bounds.height() / 256.0;
        let mut volume = 0.0;
        for row in 0..256 {
            let y = bounds.top() + (row as f32 + 0.5) * dy;
            let mut left = f32::INFINITY;
            let mut right = f32::NEG_INFINITY;
            for i in 0..points.len() {
                let a = points[i];
                let b = points[(i + 1) % points.len()];
                if (a.y <= y && y < b.y) || (b.y <= y && y < a.y) {
                    let x = a.x + (b.x - a.x) * (y - a.y) / (b.y - a.y) - 160.0;
                    left = left.min(x);
                    right = right.max(x);
                }
            }
            let radius = glass_inner_width(y);
            let dx = (right - left) / 256.0;
            for column in 0..256 {
                let x = left + (column as f32 + 0.5) * dx;
                volume += 2.0 * (radius * radius - x * x).max(0.0).sqrt() * dx * dy;
            }
        }
        volume
    }

    #[test]
    fn completion_flip_preserves_the_initial_sand_volume() {
        let capacity = rendered_volume(UPPER_SAND_FULL, false);
        // Include interpolated poses, not just lookup-table knots.
        for i in 0..=65 {
            let angle = PI * i as f32 / 65.0;
            let (sin, cos) = angle.sin_cos();
            let volume = rendered_flip_volume(Vec2::new(sin, cos), settled_sand_cut(angle));
            assert!(
                (volume / capacity - 1.0).abs() < 0.003,
                "angle={angle}: volume fraction={}",
                volume / capacity
            );
        }
    }
}
