use super::{PETALS, Sculpture};
use eframe::{
    egui::PaintCallbackInfo,
    glow::{self, HasContext as _},
};

pub(super) struct Renderer {
    scene: glow::Program,
    composite: glow::Program,
    scene_vao: glow::VertexArray,
    quad_vao: glow::VertexArray,
    vertices: glow::Buffer,
    indices: glow::Buffer,
    index_count: i32,
    angles: glow::UniformLocation,
    output_size: glow::UniformLocation,
    multisample_fbo: glow::Framebuffer,
    resolve_fbo: glow::Framebuffer,
    color: glow::Renderbuffer,
    depth: glow::Renderbuffer,
    texture: glow::Texture,
    samples: i32,
    capacity: [i32; 2],
    last_size: [i32; 2],
    last_angles: Option<[f32; PETALS]>,
}

fn program(gl: &glow::Context, vertex: &str, fragment: &str, attributes: &[&str]) -> glow::Program {
    // Creation, painting, and destruction all run with eframe's GL context current.
    unsafe {
        let prefix = if gl.version().is_embedded {
            "#version 300 es\nprecision highp float;\n"
        } else {
            "#version 140\n"
        };
        let program = gl.create_program().expect("creating bloom shader program");
        for (kind, source) in [
            (glow::VERTEX_SHADER, vertex),
            (glow::FRAGMENT_SHADER, fragment),
        ] {
            let shader = gl.create_shader(kind).expect("creating bloom shader");
            gl.shader_source(shader, &format!("{prefix}{source}"));
            gl.compile_shader(shader);
            assert!(
                gl.get_shader_compile_status(shader),
                "Bloom shader: {}",
                gl.get_shader_info_log(shader)
            );
            gl.attach_shader(program, shader);
            gl.delete_shader(shader);
        }
        for (index, name) in attributes.iter().enumerate() {
            gl.bind_attrib_location(program, index as u32, name);
        }
        gl.link_program(program);
        assert!(
            gl.get_program_link_status(program),
            "Bloom program: {}",
            gl.get_program_info_log(program)
        );
        program
    }
}

impl Renderer {
    pub(super) fn new(gl: &glow::Context, source: &Sculpture) -> Self {
        let scene = program(
            gl,
            include_str!("surface.vert"),
            include_str!("surface.frag"),
            &["a_position", "a_normal", "a_material", "a_shade", "a_petal"],
        );
        let composite = program(
            gl,
            include_str!("composite.vert"),
            include_str!("composite.frag"),
            &[],
        );
        let vertex_count = source.hardware.len()
            + source
                .petals
                .iter()
                .map(|petal| petal.surface.len())
                .sum::<usize>();
        let mut packed = Vec::with_capacity(vertex_count * 9);
        let mut indices =
            Vec::with_capacity(source.hardware_indices.len() + PETALS * source.petal_indices.len());
        let append = |packed: &mut Vec<f32>, vertex: &super::SurfaceVertex, petal: f32| {
            let material = match vertex.material {
                super::Material::Porcelain => 0.0,
                super::Material::Gold => 1.0,
                super::Material::Enamel => 2.0,
            };
            packed.extend([
                vertex.point.x,
                vertex.point.y,
                vertex.point.z,
                vertex.normal.x,
                vertex.normal.y,
                vertex.normal.z,
                material,
                vertex.shade,
                petal,
            ]);
        };
        for vertex in &source.hardware {
            append(&mut packed, vertex, -1.0);
        }
        indices.extend_from_slice(&source.hardware_indices);
        let mut roots = [0.0; PETALS * 3];
        let mut radials = [0.0; PETALS * 2];
        for (index, petal) in source.petals.iter().enumerate() {
            let offset = (packed.len() / 9) as u32;
            for vertex in &petal.surface {
                append(&mut packed, vertex, index as f32);
            }
            indices.extend(source.petal_indices.iter().map(|&i| i + offset));
            roots[index * 3..index * 3 + 3].copy_from_slice(&[
                petal.root.x,
                petal.root.y,
                petal.root.z,
            ]);
            radials[index * 2..index * 2 + 2].copy_from_slice(&[petal.radial.x, petal.radial.y]);
        }
        unsafe {
            let scene_vao = gl
                .create_vertex_array()
                .expect("creating bloom vertex array");
            let quad_vao = gl
                .create_vertex_array()
                .expect("creating bloom composite array");
            let vertices = gl.create_buffer().expect("creating bloom vertex buffer");
            let element_buffer = gl.create_buffer().expect("creating bloom index buffer");
            gl.bind_vertex_array(Some(scene_vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vertices));
            // f32/u32 slices have initialized, padding-free elements; GL copies
            // their bytes synchronously before these temporary vectors are dropped.
            let vertex_bytes =
                std::slice::from_raw_parts(packed.as_ptr().cast::<u8>(), packed.len() * 4);
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, vertex_bytes, glow::STATIC_DRAW);
            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(element_buffer));
            let index_bytes =
                std::slice::from_raw_parts(indices.as_ptr().cast::<u8>(), indices.len() * 4);
            gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, index_bytes, glow::STATIC_DRAW);
            for (index, size, offset) in [(0, 3, 0), (1, 3, 12), (2, 1, 24), (3, 1, 28), (4, 1, 32)]
            {
                gl.enable_vertex_attrib_array(index);
                gl.vertex_attrib_pointer_f32(index, size, glow::FLOAT, false, 36, offset);
            }
            gl.use_program(Some(scene));
            gl.uniform_3_f32_slice(gl.get_uniform_location(scene, "u_root[0]").as_ref(), &roots);
            gl.uniform_2_f32_slice(
                gl.get_uniform_location(scene, "u_radial[0]").as_ref(),
                &radials,
            );
            gl.uniform_2_f32(
                gl.get_uniform_location(scene, "u_elevation").as_ref(),
                super::ELEVATION_COS,
                super::ELEVATION_SIN,
            );
            let angles = gl
                .get_uniform_location(scene, "u_angle[0]")
                .expect("bloom angle uniform");
            gl.use_program(Some(composite));
            gl.uniform_1_i32(gl.get_uniform_location(composite, "u_texture").as_ref(), 0);
            let output_size = gl
                .get_uniform_location(composite, "u_output_size")
                .expect("bloom output size uniform");
            Self {
                scene,
                composite,
                scene_vao,
                quad_vao,
                vertices,
                indices: element_buffer,
                index_count: indices.len() as i32,
                angles,
                output_size,
                multisample_fbo: gl
                    .create_framebuffer()
                    .expect("creating bloom multisample framebuffer"),
                resolve_fbo: gl
                    .create_framebuffer()
                    .expect("creating bloom resolve framebuffer"),
                color: gl
                    .create_renderbuffer()
                    .expect("creating bloom color target"),
                depth: gl
                    .create_renderbuffer()
                    .expect("creating bloom depth target"),
                texture: gl
                    .create_texture()
                    .expect("creating bloom resolved texture"),
                samples: gl.get_parameter_i32(glow::MAX_SAMPLES).clamp(1, 8),
                capacity: [0, 0],
                last_size: [0, 0],
                last_angles: None,
            }
        }
    }

    fn allocate(&mut self, gl: &glow::Context, size: [i32; 2]) {
        if size[0] <= self.capacity[0] && size[1] <= self.capacity[1] {
            return;
        }
        self.capacity = std::array::from_fn(|i| {
            (size[i].max(self.capacity[i]) as u32).next_power_of_two() as i32
        });
        self.last_angles = None;
        let [width, height] = self.capacity;
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.multisample_fbo));
            gl.bind_renderbuffer(glow::RENDERBUFFER, Some(self.color));
            gl.renderbuffer_storage_multisample(
                glow::RENDERBUFFER,
                self.samples,
                glow::RGBA8,
                width,
                height,
            );
            gl.framebuffer_renderbuffer(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::RENDERBUFFER,
                Some(self.color),
            );
            gl.bind_renderbuffer(glow::RENDERBUFFER, Some(self.depth));
            gl.renderbuffer_storage_multisample(
                glow::RENDERBUFFER,
                self.samples,
                glow::DEPTH_COMPONENT24,
                width,
                height,
            );
            gl.framebuffer_renderbuffer(
                glow::FRAMEBUFFER,
                glow::DEPTH_ATTACHMENT,
                glow::RENDERBUFFER,
                Some(self.depth),
            );
            assert_eq!(
                gl.check_framebuffer_status(glow::FRAMEBUFFER),
                glow::FRAMEBUFFER_COMPLETE,
                "Bloom multisample target incomplete"
            );
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.texture));
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA8 as i32,
                width,
                height,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(None),
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MIN_FILTER,
                glow::NEAREST as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_MAG_FILTER,
                glow::NEAREST as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_S,
                glow::CLAMP_TO_EDGE as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_T,
                glow::CLAMP_TO_EDGE as i32,
            );
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.resolve_fbo));
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                Some(self.texture),
                0,
            );
            assert_eq!(
                gl.check_framebuffer_status(glow::FRAMEBUFFER),
                glow::FRAMEBUFFER_COMPLETE,
                "Bloom resolve target incomplete"
            );
            gl.bind_renderbuffer(glow::RENDERBUFFER, None);
        }
    }

    pub(super) fn paint(
        &mut self,
        gl: &glow::Context,
        info: PaintCallbackInfo,
        destination: Option<glow::Framebuffer>,
        angles: [f32; PETALS],
    ) {
        let viewport = info.viewport_in_pixels();
        let size = [viewport.width_px, viewport.height_px];
        if size[0] <= 0 || size[1] <= 0 {
            return;
        }
        let render_size = [size[0] * 2, size[1] * 2];
        unsafe {
            let mut scissor = [0; 4];
            gl.get_parameter_i32_slice(glow::SCISSOR_BOX, &mut scissor);
            self.allocate(gl, render_size);
            if self.last_size != size || self.last_angles != Some(angles) {
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.multisample_fbo));
                gl.viewport(0, 0, render_size[0], render_size[1]);
                gl.enable(glow::SCISSOR_TEST);
                gl.scissor(0, 0, render_size[0], render_size[1]);
                gl.disable(glow::BLEND);
                gl.disable(glow::CULL_FACE);
                gl.enable(glow::DEPTH_TEST);
                gl.depth_func(glow::LEQUAL);
                gl.depth_mask(true);
                if !gl.version().is_embedded {
                    gl.enable(glow::MULTISAMPLE);
                }
                gl.clear_color(0.0, 0.0, 0.0, 0.0);
                gl.clear_depth_f32(1.0);
                gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
                gl.use_program(Some(self.scene));
                gl.uniform_1_f32_slice(Some(&self.angles), &angles);
                gl.bind_vertex_array(Some(self.scene_vao));
                gl.draw_elements(glow::TRIANGLES, self.index_count, glow::UNSIGNED_INT, 0);
                gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(self.multisample_fbo));
                gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(self.resolve_fbo));
                gl.blit_framebuffer(
                    0,
                    0,
                    render_size[0],
                    render_size[1],
                    0,
                    0,
                    render_size[0],
                    render_size[1],
                    glow::COLOR_BUFFER_BIT,
                    glow::NEAREST,
                );
                self.last_size = size;
                self.last_angles = Some(angles);
            }
            gl.bind_framebuffer(glow::FRAMEBUFFER, destination);
            gl.viewport(viewport.left_px, viewport.from_bottom_px, size[0], size[1]);
            gl.scissor(scissor[0], scissor[1], scissor[2], scissor[3]);
            gl.disable(glow::DEPTH_TEST);
            gl.depth_mask(false);
            gl.enable(glow::BLEND);
            // Resolving opaque samples against transparent black produces
            // premultiplied coverage, matching egui and transparent native windows.
            gl.blend_equation_separate(glow::FUNC_ADD, glow::FUNC_ADD);
            gl.blend_func_separate(
                glow::ONE,
                glow::ONE_MINUS_SRC_ALPHA,
                glow::ONE_MINUS_DST_ALPHA,
                glow::ONE,
            );
            gl.use_program(Some(self.composite));
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.texture));
            gl.uniform_2_f32(Some(&self.output_size), size[0] as f32, size[1] as f32);
            gl.bind_vertex_array(Some(self.quad_vao));
            gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
            gl.depth_mask(true);
        }
    }

    pub(super) fn destroy(self, gl: &glow::Context) {
        unsafe {
            gl.delete_program(self.scene);
            gl.delete_program(self.composite);
            gl.delete_vertex_array(self.scene_vao);
            gl.delete_vertex_array(self.quad_vao);
            gl.delete_buffer(self.vertices);
            gl.delete_buffer(self.indices);
            gl.delete_framebuffer(self.multisample_fbo);
            gl.delete_framebuffer(self.resolve_fbo);
            gl.delete_renderbuffer(self.color);
            gl.delete_renderbuffer(self.depth);
            gl.delete_texture(self.texture);
        }
    }
}
