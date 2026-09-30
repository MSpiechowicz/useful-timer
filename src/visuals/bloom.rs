use super::*;
use std::sync::Mutex;

#[path = "bloom/gpu.rs"]
mod gpu;

const ROWS: usize = 48;
const COLUMNS: usize = 14;
const PETALS_PER_RING: usize = 6;
const PETALS: usize = PETALS_PER_RING * 3;
const ELEVATION_SIN: f32 = 0.57;
const ELEVATION_COS: f32 = 0.821_644;
const ACCENT: Color32 = Color32::from_rgb(226, 195, 135);
const FINAL_ANGLES: [f32; 3] = [1.20, 1.07, 0.87];
const ROOT_RADII: [f32; 3] = [22.0, 16.0, 10.0];
const ROOT_HEIGHTS: [f32; 3] = [6.0, 12.0, 19.0];
const BOUNDS_STEPS: usize = 256;
static SCULPTURE: LazyLock<Sculpture> = LazyLock::new(make_sculpture);

#[derive(Clone, Copy, Default)]
struct V3 {
    x: f32,
    y: f32,
    z: f32,
}

impl V3 {
    const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    fn dot(self, b: Self) -> f32 {
        self.x * b.x + self.y * b.y + self.z * b.z
    }

    fn cross(self, b: Self) -> Self {
        Self::new(
            self.y * b.z - self.z * b.y,
            self.z * b.x - self.x * b.z,
            self.x * b.y - self.y * b.x,
        )
    }

    fn normalized(self) -> Self {
        self * self.dot(self).max(1e-12).sqrt().recip()
    }
}

impl std::ops::Add for V3 {
    type Output = Self;

    fn add(self, b: Self) -> Self {
        Self::new(self.x + b.x, self.y + b.y, self.z + b.z)
    }
}

impl std::ops::Sub for V3 {
    type Output = Self;

    fn sub(self, b: Self) -> Self {
        Self::new(self.x - b.x, self.y - b.y, self.z - b.z)
    }
}

impl std::ops::Mul<f32> for V3 {
    type Output = Self;

    fn mul(self, factor: f32) -> Self {
        Self::new(self.x * factor, self.y * factor, self.z * factor)
    }
}

#[derive(Clone, Copy)]
enum Material {
    Porcelain,
    Gold,
    Enamel,
}

#[derive(Clone, Copy)]
struct SurfaceVertex {
    point: V3,
    normal: V3,
    material: Material,
    shade: f32,
}

struct Petal {
    root: V3,
    radial: Vec2,
    surface: Vec<SurfaceVertex>,
}

struct Sculpture {
    petals: [Petal; PETALS],
    hardware: Vec<SurfaceVertex>,
    hardware_indices: Vec<u32>,
    petal_indices: Vec<u32>,
    artwork_top: f32,
}

fn smootherstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    // Evaluate near one by symmetry, avoiding cancellation and tiny overshoots.
    let h = t.min(1.0 - t);
    let eased = h * h * h * (h * (h * 6.0 - 15.0) + 10.0);
    if t <= 0.5 { eased } else { 1.0 - eased }
}

fn countdown_angle(ring: usize, remaining: f32) -> f32 {
    let elapsed = 1.0 - remaining.clamp(0.0, 1.0);
    let (start, travel, delay) = [(0.035, 0.70, 0.0), (0.015, 0.38, 0.22), (0.0, 0.13, 0.55)][ring];
    start + travel * smootherstep((elapsed - delay) / (1.0 - delay))
}

fn petal_angle(index: usize, snapshot: &TimerSnapshot) -> f32 {
    let ring = index / PETALS_PER_RING;
    if snapshot.phase != TimerPhase::Finished {
        return countdown_angle(ring, snapshot.remaining_fraction);
    }
    let elapsed = snapshot.effect_elapsed.unwrap_or_default().as_secs_f32();
    let initial = countdown_angle(ring, 0.0);
    if elapsed >= 3.0 {
        return FINAL_ANGLES[ring];
    }
    // Alternating pairs release together, not as a rotating radial wipe.
    let pair_delay = [0.0, 0.035, 0.015, 0.0, 0.035, 0.015][index % PETALS_PER_RING];
    let onset = ring as f32 * 0.16 + pair_delay;
    let duration = 1.60 + ring as f32 * 0.10;
    let opening = smootherstep((elapsed - onset) / duration);
    initial + (FINAL_ANGLES[ring] - initial) * opening
}

fn pose_angles(snapshot: &TimerSnapshot) -> [f32; PETALS] {
    if snapshot.phase == TimerPhase::Finished {
        std::array::from_fn(|index| petal_angle(index, snapshot))
    } else {
        let rings: [f32; 3] =
            std::array::from_fn(|ring| countdown_angle(ring, snapshot.remaining_fraction));
        std::array::from_fn(|index| rings[index / PETALS_PER_RING])
    }
}

fn projected_top(p: V3) -> f32 {
    let depth = p.y * ELEVATION_COS + p.z * ELEVATION_SIN;
    199.0 + (p.y * ELEVATION_SIN - p.z * ELEVATION_COS) * 900.0 / (900.0 - depth)
}

fn petal_top(petal: &Petal, angle: f32) -> f32 {
    let (sin, cos) = angle.sin_cos();
    petal
        .surface
        .iter()
        .map(|vertex| {
            let p = vertex.point;
            let outward = p.x * cos + p.z * sin;
            projected_top(V3::new(
                0.0,
                petal.root.y + outward * petal.radial.y + p.y * petal.radial.x,
                petal.root.z + p.z * cos - p.x * sin,
            ))
        })
        .fold(f32::INFINITY, f32::min)
}

pub(super) fn artwork_top() -> f32 {
    SCULPTURE.artwork_top
}

fn petal_surface(ring: usize, u: f32, v: f32) -> V3 {
    let root = ROOT_RADII[ring];
    let length = [133.0, 126.0, 118.0][ring];
    let bulge = [29.0, 23.0, 17.0][ring];
    let arch = (PI * u).sin().max(0.0);
    let radius = root * (1.0 - u) + 0.8 * u + bulge * arch;
    // Closed petals occupy less than their sixty-degree sector. Opening
    // increases their radius, so neighboring petals separate rather than intersect.
    let half_angle = 0.12 + 0.37 * arch;
    let (sin, cos) = (v * half_angle).sin_cos();
    V3::new(
        radius * cos - root,
        radius * sin,
        length * u + 1.8 * arch * v * v,
    )
}

fn make_petal(index: usize) -> Petal {
    let ring = index / PETALS_PER_RING;
    let azimuth = index as f32 * TAU / PETALS_PER_RING as f32 + ring as f32 * PI / 6.0 + 0.12;
    let (sin, cos) = azimuth.sin_cos();
    let radius = ROOT_RADII[ring];
    let mut surface = Vec::with_capacity((ROWS + 1) * (COLUMNS + 1));
    const WIDTHS: [f32; COLUMNS + 1] = [
        -1.0, -0.975, -0.90, -0.75, -0.55, -0.35, -0.15, 0.0, 0.15, 0.35, 0.55, 0.75, 0.90, 0.975,
        1.0,
    ];
    for row in 0..=ROWS {
        let u = row as f32 / ROWS as f32;
        for v in WIDTHS {
            let a = petal_surface(ring, (u - 0.001).max(0.0), v);
            let b = petal_surface(ring, (u + 0.001).min(1.0), v);
            let left = petal_surface(ring, u, v - 0.001);
            let right = petal_surface(ring, u, v + 0.001);
            surface.push(SurfaceVertex {
                point: petal_surface(ring, u, v),
                normal: (right - left).cross(b - a).normalized(),
                material: if v.abs() >= 0.975 || row <= 1 {
                    Material::Gold
                } else {
                    Material::Porcelain
                },
                shade: 1.0 - 0.25 * (1.0 - u).powi(3) - 0.035 * v * v,
            });
        }
    }
    Petal {
        root: V3::new(radius * cos, radius * sin, ROOT_HEIGHTS[ring]),
        radial: Vec2::new(cos, sin),
        surface,
    }
}

fn add_lathe(
    vertices: &mut Vec<SurfaceVertex>,
    indices: &mut Vec<u32>,
    center: V3,
    profile: &[(f32, f32)],
    material: Material,
    sides: usize,
) {
    let start = vertices.len() as u32;
    for (row, &(radius, height)) in profile.iter().enumerate() {
        let previous = profile[row.saturating_sub(1)];
        let next = profile[(row + 1).min(profile.len() - 1)];
        for side in 0..sides {
            let (sin, cos) = (side as f32 * TAU / sides as f32).sin_cos();
            vertices.push(SurfaceVertex {
                point: center + V3::new(radius * cos, radius * sin, height),
                normal: V3::new(
                    (next.1 - previous.1) * cos,
                    (next.1 - previous.1) * sin,
                    previous.0 - next.0,
                )
                .normalized(),
                material,
                shade: 1.0,
            });
        }
    }
    for row in 0..profile.len() - 1 {
        for side in 0..sides {
            let a = start + (row * sides + side) as u32;
            let b = start + (row * sides + (side + 1) % sides) as u32;
            let c = a + sides as u32;
            let d = b + sides as u32;
            indices.extend([a, b, c, b, d, c]);
        }
    }
}

fn add_hinge(vertices: &mut Vec<SurfaceVertex>, indices: &mut Vec<u32>, petal: &Petal) {
    let start = vertices.len();
    add_lathe(
        vertices,
        indices,
        V3::default(),
        &[
            (0.0, -4.0),
            (1.3, -4.0),
            (1.8, -3.3),
            (1.8, 3.3),
            (1.3, 4.0),
            (0.0, 4.0),
        ],
        Material::Gold,
        24,
    );
    let transform = |v: V3| {
        V3::new(
            v.x * petal.radial.x - v.z * petal.radial.y,
            v.x * petal.radial.y + v.z * petal.radial.x,
            v.y,
        )
    };
    for vertex in &mut vertices[start..] {
        vertex.point = petal.root + transform(vertex.point);
        vertex.normal = transform(vertex.normal);
    }
}

fn make_sculpture() -> Sculpture {
    let petals = std::array::from_fn(make_petal);
    let mut hardware = Vec::new();
    let mut hardware_indices = Vec::new();
    // The stepped receptacle reaches every pivot: the inner petals no longer
    // hang above a shallow cup. Each hinge axis is the articulation origin.
    for (profile, material) in [
        (
            &[
                (0.0, -35.0),
                (39.0, -35.0),
                (43.0, -32.0),
                (43.0, -27.0),
                (39.0, -24.0),
                (0.0, -24.0),
            ][..],
            Material::Enamel,
        ),
        (
            &[
                (36.0, -23.8),
                (37.0, -23.2),
                (37.0, -22.8),
                (33.0, -22.8),
                (32.5, -23.3),
            ][..],
            Material::Gold,
        ),
        (
            &[
                (9.0, -24.0),
                (10.0, -22.5),
                (7.0, -21.0),
                (5.0, -10.0),
                (9.0, -6.0),
            ][..],
            Material::Gold,
        ),
        (
            &[
                (9.0, -8.0),
                (17.0, -5.0),
                (24.0, 0.0),
                (24.0, 4.0),
                (22.0, 6.0),
                (20.0, 9.0),
                (16.0, 12.0),
                (13.0, 16.0),
                (10.0, 19.0),
                (7.0, 22.0),
                (0.0, 24.0),
            ][..],
            Material::Gold,
        ),
        (
            &[(19.7, 8.5), (20.1, 9.0), (19.6, 9.5)][..],
            Material::Enamel,
        ),
        (
            &[(12.7, 15.5), (13.1, 16.0), (12.6, 16.5)][..],
            Material::Enamel,
        ),
        (
            &[
                (0.0, 24.0),
                (6.0, 24.0),
                (7.0, 24.5),
                (6.3, 25.0),
                (0.0, 25.0),
            ][..],
            Material::Gold,
        ),
    ] {
        add_lathe(
            &mut hardware,
            &mut hardware_indices,
            V3::default(),
            profile,
            material,
            256,
        );
    }
    for petal in &petals {
        add_hinge(&mut hardware, &mut hardware_indices, petal);
    }
    // Small recessed screw heads encircle the milled cap beneath the filaments.
    for index in 0..6 {
        let angle = index as f32 * TAU / 6.0 + 0.2;
        let center = V3::new(7.5 * angle.cos(), 7.5 * angle.sin(), 0.0);
        add_lathe(
            &mut hardware,
            &mut hardware_indices,
            center,
            &[
                (0.0, 23.5),
                (1.1, 23.5),
                (1.2, 24.0),
                (0.8, 24.5),
                (0.0, 24.5),
            ],
            Material::Enamel,
            16,
        );
    }
    // All filaments start inside the central cap, not in midair or beneath the hub.
    for index in 0..13 {
        let angle = index as f32 * TAU / 12.0;
        let radius = if index == 12 {
            0.0
        } else {
            3.0 + (index % 2) as f32 * 2.0
        };
        let top = 43.0 + (index % 3) as f32 * 3.0;
        let center = V3::new(radius * angle.cos(), radius * angle.sin(), 0.0);
        add_lathe(
            &mut hardware,
            &mut hardware_indices,
            center,
            &[(0.48, 24.5), (0.48, top)],
            Material::Gold,
            16,
        );
        add_lathe(
            &mut hardware,
            &mut hardware_indices,
            center,
            &[
                (0.0, top - 1.6),
                (1.4, top - 1.0),
                (1.65, top),
                (1.3, top + 1.1),
                (0.0, top + 1.7),
            ],
            Material::Gold,
            24,
        );
    }
    let mut petal_indices = Vec::with_capacity(ROWS * COLUMNS * 6);
    for row in 0..ROWS {
        for column in 0..COLUMNS {
            let a = (row * (COLUMNS + 1) + column) as u32;
            let b = a + 1;
            let c = a + (COLUMNS + 1) as u32;
            petal_indices.extend([a, b, c, b, c + 1, c]);
        }
    }
    // One fixed upper envelope for all reachable petal angles. Captions stay
    // still throughout countdown, completion, pause, and reduced motion.
    let mut artwork_top = hardware
        .iter()
        .map(|vertex| projected_top(vertex.point))
        .fold(f32::INFINITY, f32::min);
    for (index, petal) in petals.iter().enumerate() {
        let ring = index / PETALS_PER_RING;
        let start = countdown_angle(ring, 1.0);
        for step in 0..=BOUNDS_STEPS {
            let angle = start + (FINAL_ANGLES[ring] - start) * step as f32 / BOUNDS_STEPS as f32;
            artwork_top = artwork_top.min(petal_top(petal, angle));
        }
    }
    artwork_top -= 0.2; // Cover the subpixel envelope between sampled angles.
    Sculpture {
        petals,
        hardware,
        hardware_indices,
        petal_indices,
        artwork_top,
    }
}

type RenderState = Arc<Mutex<Option<gpu::Renderer>>>;

fn renderer_id() -> eframe::egui::Id {
    eframe::egui::Id::new("clockwork-bloom-gpu")
}

pub(super) fn draw(c: &Canvas<'_>, snapshot: &TimerSnapshot) {
    let state = c.painter.ctx().data_mut(|data| {
        data.get_temp_mut_or_insert_with::<RenderState>(renderer_id(), || {
            Arc::new(Mutex::new(None))
        })
        .clone()
    });
    let angles = pose_angles(snapshot);
    let callback = eframe::egui_glow::CallbackFn::new(move |info, painter| {
        let mut state = state.lock().expect("bloom renderer lock poisoned");
        let renderer = state.get_or_insert_with(|| gpu::Renderer::new(painter.gl(), &SCULPTURE));
        renderer.paint(painter.gl(), info, painter.intermediate_fbo(), angles);
    });
    c.glow(160.0, 242.0, 49.0, 7.0, rgba(5, 8, 12, 0.32));
    c.painter.add(eframe::egui::PaintCallback {
        rect: Rect::from_center_size(c.origin, Vec2::splat(320.0 * c.scale)),
        callback: Arc::new(callback),
    });
    readout(c, snapshot, 280.0, ACCENT, true);
}

pub(super) fn destroy(ctx: &eframe::egui::Context, gl: &eframe::glow::Context) {
    if let Some(state) = ctx.data(|data| data.get_temp::<RenderState>(renderer_id())) {
        if let Some(renderer) = state.lock().expect("bloom renderer lock poisoned").take() {
            renderer.destroy(gl);
        }
        ctx.data_mut(|data| data.remove::<RenderState>(renderer_id()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(remaining: f32, finish: Option<f32>) -> TimerSnapshot {
        TimerSnapshot {
            phase: if finish.is_some() {
                TimerPhase::Finished
            } else {
                TimerPhase::Running
            },
            remaining: Duration::from_secs_f32(remaining * 60.0),
            remaining_fraction: remaining,
            effect_elapsed: finish.map(Duration::from_secs_f32),
            animation_elapsed: Duration::from_secs(60),
        }
    }

    #[test]
    fn completion_is_continuous_and_settles_without_a_pose_jump() {
        for index in 0..PETALS {
            let before = petal_angle(index, &snapshot(0.0, None));
            let at_zero = petal_angle(index, &snapshot(0.0, Some(0.0)));
            assert!((before - at_zero).abs() < 1e-6);
            let at_end = petal_angle(index, &snapshot(0.0, Some(3.0)));
            let after_end = petal_angle(index, &snapshot(0.0, Some(30.0)));
            assert_eq!(at_end, after_end);
            assert!((petal_angle(index, &snapshot(0.0, Some(2.999))) - at_end).abs() < 1e-5);
            assert!(at_end > before + 0.4);
        }
    }

    #[test]
    fn completion_opens_without_reversals_or_overshoot() {
        for index in 0..PETALS {
            let initial = petal_angle(index, &snapshot(0.0, Some(0.0)));
            let final_angle = petal_angle(index, &snapshot(0.0, Some(3.0)));
            let mut previous = initial;
            for millisecond in 1..=3000 {
                let angle = petal_angle(index, &snapshot(0.0, Some(millisecond as f32 / 1000.0)));
                assert!(
                    angle + 1e-6 >= previous,
                    "petal {index} reversed at {millisecond}ms"
                );
                assert!(angle + 1e-6 >= initial);
                assert!(
                    angle <= final_angle + 1e-6,
                    "petal {index} overshot at {millisecond}ms"
                );
                previous = angle;
            }
        }
    }

    #[test]
    fn progress_unfolds_each_ring_without_reversals() {
        for index in 0..PETALS {
            let mut previous = petal_angle(index, &snapshot(1.0, None));
            for step in 1..=100 {
                let angle = petal_angle(index, &snapshot(1.0 - step as f32 / 100.0, None));
                assert!(angle >= previous);
                previous = angle;
            }
        }
    }

    #[test]
    fn stationary_caption_clears_every_pose_without_excess_margin() {
        let caption_bound = artwork_top();
        let mut visible_top = SCULPTURE
            .hardware
            .iter()
            .map(|vertex| projected_top(vertex.point))
            .fold(f32::INFINITY, f32::min);
        // A finer, independent angular grid checks the cached envelope rather
        // than comparing the caption with the same sample points used to build it.
        for (index, petal) in SCULPTURE.petals.iter().enumerate() {
            let ring = index / PETALS_PER_RING;
            let start = countdown_angle(ring, 1.0);
            for step in 0..=1024 {
                let angle = start + (FINAL_ANGLES[ring] - start) * step as f32 / 1024.0;
                let top = petal_top(petal, angle);
                assert!(
                    caption_bound <= top + 0.001,
                    "caption overlaps the animation"
                );
                visible_top = visible_top.min(top);
            }
        }
        assert!(
            visible_top - caption_bound < 0.4,
            "caption is unnecessarily far above the animation"
        );
    }
}
