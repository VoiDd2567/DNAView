pub mod camera;
pub mod mesh;
pub mod primitives;
pub mod selection;

use crate::{
    config::AppConfig,
    dna::{DnaModel, GenePair},
};
use camera::Camera;
use cgmath::{vec3, InnerSpace, Matrix4, SquareMatrix, Vector3, Vector4};
use mesh::{MeshBuilder, Vertex};
use std::sync::Arc;
use wgpu::util::DeviceExt;
use winit::{dpi::PhysicalSize, window::Window};

const SHADER: &str = r#"
struct Uniforms {
    view_proj: mat4x4<f32>,
    model: mat4x4<f32>,
    camera_position: vec4<f32>,
    light_position: vec4<f32>,
    camera_right: vec4<f32>,
    camera_up: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) world_position: vec3<f32>,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let world = uniforms.model * vec4<f32>(input.position, 1.0);
    out.clip_position = uniforms.view_proj * world;
    out.normal = normalize((uniforms.model * vec4<f32>(input.normal, 0.0)).xyz);
    out.color = input.color;
    out.world_position = world.xyz;
    return out;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let normal = normalize(input.normal);
    let light_dir = normalize(uniforms.light_position.xyz - input.world_position);
    let view_dir = normalize(uniforms.camera_position.xyz - input.world_position);
    let halfway = normalize(light_dir + view_dir);
    let diffuse = max(dot(normal, light_dir), 0.0);

    let color_max = max(input.color.r, max(input.color.g, input.color.b));
    let color_min = min(input.color.r, min(input.color.g, input.color.b));
    let chroma = color_max - color_min;
    let is_base_sphere = smoothstep(0.08, 0.18, chroma);

    let ndotv = max(dot(normal, view_dir), 0.0);
    let fresnel = pow(1.0 - ndotv, 4.0);
    let thickness = 0.34 + pow(1.0 - ndotv, 1.7) * 1.45;
    let absorption = input.color.rgb * (0.22 + thickness * 0.28);
    let transmitted = mix(input.color.rgb * 0.38, absorption, 0.82);

    let band = sin((input.world_position.y * 10.5) + (input.world_position.x * 4.0) + (input.world_position.z * 2.8));
    let inner_streak = smoothstep(0.45, 1.0, band) * 0.08;
    let lower_glow = smoothstep(-0.75, 0.25, -normal.y) * input.color.rgb * 0.10;
    let core = transmitted * (0.28 + diffuse * 0.30) + input.color.rgb * inner_streak + lower_glow;

    let clearcoat = pow(max(dot(normal, halfway), 0.0), 220.0);
    let broad_reflection = pow(max(dot(normal, halfway), 0.0), 16.0);
    let side_sparkle = pow(max(dot(reflect(-light_dir, normal), view_dir), 0.0), 90.0);
    let warm_highlight = vec3<f32>(1.0, 0.92, 0.72) * (clearcoat * 1.65 + side_sparkle * 0.82);
    let sky_reflection = vec3<f32>(0.72, 0.88, 1.0) * (fresnel * 0.24 + broad_reflection * 0.07);
    let glass_lit = core + warm_highlight + sky_reflection;

    let rod_lit = input.color.rgb * (0.30 + diffuse * 0.82) + vec3<f32>(1.0, 0.96, 0.84) * clearcoat * 0.55;
    let lit = mix(rod_lit, glass_lit, is_base_sphere);
    return vec4<f32>(min(lit, vec3<f32>(1.0, 1.0, 1.0)), input.color.a);
}
"#;

const BACKGROUND_SHADER: &str = r#"
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VertexOutput {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0)
    );
    let position = positions[index];
    var out: VertexOutput;
    out.clip_position = vec4<f32>(position, 0.0, 1.0);
    out.uv = position * 0.5 + vec2<f32>(0.5, 0.5);
    return out;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let uv = input.uv;
    let t = clamp(uv.y, 0.0, 1.0);

    let horizon = vec3<f32>(0.93, 0.975, 1.0);
    let upper = vec3<f32>(0.38, 0.68, 0.98);
    var color = mix(horizon, upper, smoothstep(0.05, 1.0, t));

    let sun_center = vec2<f32>(0.78, 0.82);
    let sun_distance = distance(uv, sun_center);
    let sun_core = exp(-sun_distance * 58.0);
    let sun_halo = exp(-sun_distance * 8.0);
    color += vec3<f32>(1.0, 0.78, 0.38) * sun_halo * 0.36;
    color += vec3<f32>(1.0, 0.96, 0.82) * sun_core * 1.15;

    let cloud_wave_a = sin(uv.x * 18.0 + uv.y * 5.0) * 0.5 + 0.5;
    let cloud_wave_b = sin(uv.x * 31.0 - uv.y * 9.0 + 1.7) * 0.5 + 0.5;
    let cloud_wave_c = sin(uv.x * 9.0 + uv.y * 19.0 + 3.2) * 0.5 + 0.5;
    let cloud_noise = cloud_wave_a * 0.48 + cloud_wave_b * 0.34 + cloud_wave_c * 0.18;
    let cloud_band = smoothstep(0.47, 0.72, cloud_noise) * smoothstep(0.18, 0.36, t) * (1.0 - smoothstep(0.82, 0.98, t));
    let cloud_color = mix(vec3<f32>(0.88, 0.93, 0.98), vec3<f32>(1.0, 0.985, 0.94), sun_halo);
    color = mix(color, cloud_color, cloud_band * 0.32);

    let haze = exp(-t * 4.5) * 0.18;
    color = mix(color, vec3<f32>(0.96, 0.99, 1.0), haze);
    return vec4<f32>(min(color, vec3<f32>(1.0, 1.0, 1.0)), 1.0);
}
"#;

const SPHERE_SHADER: &str = r#"
struct Uniforms {
    view_proj: mat4x4<f32>,
    model: mat4x4<f32>,
    camera_position: vec4<f32>,
    light_position: vec4<f32>,
    camera_right: vec4<f32>,
    camera_up: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

struct VertexInput {
    @location(0) local: vec2<f32>,
    @location(1) center_radius: vec4<f32>,
    @location(2) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) center: vec3<f32>,
    @location(2) color: vec4<f32>,
    @location(3) radius: f32,
    @location(4) world_center: vec3<f32>,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    let center = input.center_radius.xyz;
    let radius = input.center_radius.w;
    let world_center = (uniforms.model * vec4<f32>(center, 1.0)).xyz;
    let world_position =
        world_center + uniforms.camera_right.xyz * input.local.x * radius + uniforms.camera_up.xyz * input.local.y * radius;

    var out: VertexOutput;
    out.clip_position = uniforms.view_proj * vec4<f32>(world_position, 1.0);
    out.local = input.local;
    out.center = center;
    out.color = input.color;
    out.radius = radius;
    out.world_center = world_center;
    return out;
}

fn glass_environment(direction: vec3<f32>) -> vec3<f32> {
    let sky = vec3<f32>(0.62, 0.82, 1.0);
    let horizon = vec3<f32>(0.96, 0.99, 1.0);
    let warm = vec3<f32>(1.0, 0.72, 0.36);
    let t = clamp(direction.y * 0.5 + 0.5, 0.0, 1.0);
    let base = mix(horizon, sky, t);
    let sun = pow(max(dot(normalize(direction), normalize(vec3<f32>(-0.35, 0.55, 0.76))), 0.0), 180.0);
    return base + warm * sun * 2.4;
}

struct FragmentOutput {
    @location(0) color: vec4<f32>,
    @builtin(frag_depth) depth: f32,
};

@fragment
fn fs_main(input: VertexOutput) -> FragmentOutput {
    let r2 = dot(input.local, input.local);
    if (r2 > 1.0) {
        discard;
    }

    let view_axis = normalize(uniforms.camera_position.xyz - input.world_center);
    let z = sqrt(max(1.0 - r2, 0.0));
    let normal = normalize(uniforms.camera_right.xyz * input.local.x + uniforms.camera_up.xyz * input.local.y + view_axis * z);
    let surface_position = input.world_center + normal * input.radius;
    let view_dir = normalize(uniforms.camera_position.xyz - surface_position);
    let light_dir = normalize(uniforms.light_position.xyz - surface_position);
    let halfway = normalize(light_dir + view_dir);

    let ndotv = max(dot(normal, view_dir), 0.0);
    let fresnel = pow(1.0 - ndotv, 4.6);
    let diffuse = max(dot(normal, light_dir), 0.0);

    let reflect_dir = reflect(-view_dir, normal);
    let refract_dir = refract(-view_dir, normal, 1.0 / 1.46);
    let reflection = glass_environment(reflect_dir);
    let refraction = glass_environment(refract_dir) * input.color.rgb;

    let path_length = 0.34 + (1.0 - ndotv) * 1.55;
    let absorption = exp(-((vec3<f32>(1.0, 1.0, 1.0) - input.color.rgb) * 1.65 + vec3<f32>(0.04, 0.04, 0.04)) * path_length);
    let body = input.color.rgb * absorption * (0.035 + diffuse * 0.06);
    let caustic_band = pow(max(sin(surface_position.y * 11.0 + surface_position.x * 5.0), 0.0), 6.0) * 0.08;

    let clearcoat = pow(max(dot(normal, halfway), 0.0), 340.0);
    let broad = pow(max(dot(normal, halfway), 0.0), 18.0);
    let side_flash = pow(max(dot(reflect(-light_dir, normal), view_dir), 0.0), 150.0);
    let warm_light = vec3<f32>(1.0, 0.90, 0.64) * (clearcoat * 3.8 + side_flash * 1.55);

    let glass = body
        + refraction * (0.48 + (1.0 - fresnel) * 0.18)
        + reflection * (0.10 + fresnel * 0.74)
        + warm_light
        + input.color.rgb * caustic_band
        + broad * 0.08;
    let surface_clip = uniforms.view_proj * vec4<f32>(surface_position, 1.0);
    var out: FragmentOutput;
    out.color = vec4<f32>(min(glass, vec3<f32>(1.0, 1.0, 1.0)), input.color.a);
    out.depth = clamp(surface_clip.z / surface_clip.w, 0.0, 1.0);
    return out;
}
"#;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    view_proj: [[f32; 4]; 4],
    model: [[f32; 4]; 4],
    camera_position: [f32; 4],
    light_position: [f32; 4],
    camera_right: [f32; 4],
    camera_up: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct SphereVertex {
    local: [f32; 2],
}

impl SphereVertex {
    fn layout<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[wgpu::VertexAttribute {
                offset: 0,
                shader_location: 0,
                format: wgpu::VertexFormat::Float32x2,
            }],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct SphereInstance {
    center_radius: [f32; 4],
    color: [f32; 4],
}

impl SphereInstance {
    fn layout<'a>() -> wgpu::VertexBufferLayout<'a> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 4]>() as wgpu::BufferAddress,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

pub struct Renderer {
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    pub size: PhysicalSize<u32>,
    background_pipeline: wgpu::RenderPipeline,
    render_pipeline: wgpu::RenderPipeline,
    sphere_pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    depth_texture: Texture,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
    sphere_vertex_buffer: wgpu::Buffer,
    sphere_instance_buffer: wgpu::Buffer,
    sphere_instance_count: u32,
    pub model_rotation: f32,
    pub model_tilt: f32,
    model_axis_start: Vector3<f32>,
    model_axis_end: Vector3<f32>,
    model_axis_length: f32,
    pub camera: Camera,
}

impl Renderer {
    pub async fn new(window: Arc<Window>) -> Self {
        let size = window.inner_size();
        let instance = wgpu::Instance::default();
        let surface = instance
            .create_surface(window)
            .expect("failed to create surface");
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("no suitable GPU adapter found");

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                },
                None,
            )
            .await
            .expect("failed to create device");

        let surface_caps = surface.get_capabilities(&adapter);
        let format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|format| format.is_srgb())
            .unwrap_or(surface_caps.formats[0]);
        let present_mode = surface_caps
            .present_modes
            .iter()
            .copied()
            .find(|mode| *mode == wgpu::PresentMode::Fifo)
            .unwrap_or(surface_caps.present_modes[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let depth_texture = Texture::create_depth(&device, &config);
        let camera = Camera::new(config.width as f32 / config.height as f32);
        let uniforms = Uniforms {
            view_proj: Matrix4::<f32>::identity().into(),
            model: Matrix4::<f32>::identity().into(),
            camera_position: [0.0, 0.0, 0.0, 1.0],
            light_position: [-3.5, 6.5, 4.5, 1.0],
            camera_right: [1.0, 0.0, 0.0, 0.0],
            camera_up: [0.0, 1.0, 0.0, 0.0],
        };
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("uniform buffer"),
            contents: bytemuck::bytes_of(&uniforms),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("uniform layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("uniform bind group"),
            layout: &uniform_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("dna shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pipeline layout"),
            bind_group_layouts: &[&uniform_layout],
            push_constant_ranges: &[],
        });
        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("render pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[Vertex::layout()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: Texture::DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });
        let sphere_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ray traced sphere shader"),
            source: wgpu::ShaderSource::Wgsl(SPHERE_SHADER.into()),
        });
        let sphere_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ray traced sphere pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &sphere_shader,
                entry_point: "vs_main",
                buffers: &[SphereVertex::layout(), SphereInstance::layout()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &sphere_shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: Texture::DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });
        let background_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("background shader"),
            source: wgpu::ShaderSource::Wgsl(BACKGROUND_SHADER.into()),
        });
        let background_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("background pipeline layout"),
                bind_group_layouts: &[],
                push_constant_ranges: &[],
            });
        let background_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("background pipeline"),
            layout: Some(&background_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &background_shader,
                entry_point: "vs_main",
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &background_shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: Texture::DEPTH_FORMAT,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("empty vertex buffer"),
            size: 4,
            usage: wgpu::BufferUsages::VERTEX,
            mapped_at_creation: false,
        });
        let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("empty index buffer"),
            size: 4,
            usage: wgpu::BufferUsages::INDEX,
            mapped_at_creation: false,
        });
        let sphere_vertices = [
            SphereVertex {
                local: [-1.0, -1.0],
            },
            SphereVertex { local: [1.0, -1.0] },
            SphereVertex { local: [1.0, 1.0] },
            SphereVertex {
                local: [-1.0, -1.0],
            },
            SphereVertex { local: [1.0, 1.0] },
            SphereVertex { local: [-1.0, 1.0] },
        ];
        let sphere_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("sphere quad vertex buffer"),
            contents: bytemuck::cast_slice(&sphere_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let sphere_instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("empty sphere instance buffer"),
            size: 4,
            usage: wgpu::BufferUsages::VERTEX,
            mapped_at_creation: false,
        });

        Self {
            surface,
            device,
            queue,
            config,
            size,
            background_pipeline,
            render_pipeline,
            sphere_pipeline,
            uniform_buffer,
            uniform_bind_group,
            depth_texture,
            vertex_buffer,
            index_buffer,
            index_count: 0,
            sphere_vertex_buffer,
            sphere_instance_buffer,
            sphere_instance_count: 0,
            model_rotation: 0.0,
            model_tilt: 0.0,
            model_axis_start: vec3(0.0, 0.0, 0.0),
            model_axis_end: vec3(0.0, 1.0, 0.0),
            model_axis_length: 1.0,
            camera,
        }
    }

    pub fn resize(&mut self, size: PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 {
            return;
        }
        self.size = size;
        self.config.width = size.width;
        self.config.height = size.height;
        self.surface.configure(&self.device, &self.config);
        self.depth_texture = Texture::create_depth(&self.device, &self.config);
        self.camera.aspect = self.config.width as f32 / self.config.height as f32;
    }

    pub fn rebuild_mesh(
        &mut self,
        dna: &DnaModel,
        app_config: &AppConfig,
        visible_start: usize,
        visible_pairs: usize,
    ) {
        let mut builder = MeshBuilder::default();
        let mut sphere_instances = Vec::new();
        let visible = dna.visible_pairs(visible_start, visible_pairs);
        let y_offset = visible
            .first()
            .map(|pair| pair.left_position.y)
            .unwrap_or_default();
        let axis_offset = vec3(0.0, -y_offset, 0.0);
        self.model_axis_start = dna.object.central_axis.start + axis_offset;
        self.model_axis_end = dna.object.central_axis.end + axis_offset;
        self.model_axis_length = dna.object.central_axis.length();
        for pair in visible {
            add_pair(
                &mut builder,
                &mut sphere_instances,
                pair,
                dna,
                app_config,
                axis_offset,
            );
        }
        self.index_count = builder.indices.len() as u32;
        self.sphere_instance_count = sphere_instances.len() as u32;

        if !builder.vertices.is_empty() {
            self.vertex_buffer =
                self.device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("dna vertex buffer"),
                        contents: bytemuck::cast_slice(&builder.vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
            self.index_buffer = self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("dna index buffer"),
                    contents: bytemuck::cast_slice(&builder.indices),
                    usage: wgpu::BufferUsages::INDEX,
                });
        }

        if !sphere_instances.is_empty() {
            self.sphere_instance_buffer =
                self.device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("sphere instance buffer"),
                        contents: bytemuck::cast_slice(&sphere_instances),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
        }
    }

    pub fn update_camera(&self) {
        let eye = self.camera.eye_position();
        let forward = (self.camera.pan - eye).normalize();
        let right = forward.cross(vec3(0.0, 1.0, 0.0)).normalize();
        let up = right.cross(forward).normalize();
        let model = self.model_matrix();
        let uniforms = Uniforms {
            view_proj: self.camera.view_projection().into(),
            model: model.into(),
            camera_position: eye.extend(1.0).into(),
            light_position: [
                self.camera.pan.x - 1.4,
                self.camera.pan.y + 2.2,
                self.camera.pan.z + 2.0,
                1.0,
            ],
            camera_right: right.extend(0.0).into(),
            camera_up: up.extend(0.0).into(),
        };
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
    }

    pub fn dna_axis_world_point(&self, dna_y: f32) -> Vector3<f32> {
        let axis = self.model_axis_end - self.model_axis_start;
        let axis_t = if self.model_axis_length <= f32::EPSILON {
            0.0
        } else {
            (dna_y / self.model_axis_length).clamp(0.0, 1.0)
        };
        let local_point = self.model_axis_start + axis * axis_t;
        let world =
            self.model_matrix() * Vector4::new(local_point.x, local_point.y, local_point.z, 1.0);
        vec3(world.x, world.y, world.z)
    }

    fn model_matrix(&self) -> Matrix4<f32> {
        let axis = self.model_axis_end - self.model_axis_start;
        let axis_direction =
            if self.model_axis_length <= f32::EPSILON || axis.magnitude2() <= f32::EPSILON {
                vec3(0.0, 1.0, 0.0)
            } else {
                axis.normalize()
            };
        let axis_center = (self.model_axis_start + self.model_axis_end) * 0.5;
        let spin = Matrix4::from_translation(self.model_axis_start)
            * Matrix4::from_axis_angle(axis_direction, cgmath::Rad(self.model_rotation))
            * Matrix4::from_translation(-self.model_axis_start);
        let tilt = Matrix4::from_translation(axis_center)
            * Matrix4::from_angle_x(cgmath::Rad(self.model_tilt))
            * Matrix4::from_translation(-axis_center);
        tilt * spin
    }

    pub fn render_dna<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) {
        if self.index_count == 0 {
            return;
        }
        pass.set_pipeline(&self.render_pipeline);
        pass.set_bind_group(0, &self.uniform_bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..self.index_count, 0, 0..1);

        if self.sphere_instance_count > 0 {
            pass.set_pipeline(&self.sphere_pipeline);
            pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            pass.set_vertex_buffer(0, self.sphere_vertex_buffer.slice(..));
            pass.set_vertex_buffer(1, self.sphere_instance_buffer.slice(..));
            pass.draw(0..6, 0..self.sphere_instance_count);
        }
    }

    pub fn render_background<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) {
        pass.set_pipeline(&self.background_pipeline);
        pass.draw(0..3, 0..1);
    }

    pub fn depth_view(&self) -> &wgpu::TextureView {
        &self.depth_texture.view
    }
}

fn add_pair(
    builder: &mut MeshBuilder,
    sphere_instances: &mut Vec<SphereInstance>,
    pair: &GenePair,
    dna: &DnaModel,
    config: &AppConfig,
    offset: Vector3<f32>,
) {
    let selected = dna.selected_indices.contains(&pair.index);
    let left_color = color_with_selection(config.color_for(pair.left), selected);
    let right_color = color_with_selection(config.color_for(pair.right), selected);
    let stick_color = if selected {
        [1.0, 1.0, 1.0, 1.0]
    } else {
        [0.72, 0.75, 0.78, 1.0]
    };

    let left_position = pair.left_position + offset;
    let right_position = pair.right_position + offset;
    let pair_axis = (right_position - left_position).normalize();
    let sphere_radius = config.effective_sphere_radius();
    let stick_start = left_position + pair_axis * sphere_radius * 1.08;
    let stick_end = right_position - pair_axis * sphere_radius * 1.08;

    sphere_instances.push(SphereInstance {
        center_radius: [
            left_position.x,
            left_position.y,
            left_position.z,
            sphere_radius,
        ],
        color: left_color,
    });
    sphere_instances.push(SphereInstance {
        center_radius: [
            right_position.x,
            right_position.y,
            right_position.z,
            sphere_radius,
        ],
        color: right_color,
    });
    primitives::add_cylinder_between(
        builder,
        stick_start,
        stick_end,
        config.stick_radius,
        60,
        stick_color,
    );
}

fn color_with_selection(mut color: [f32; 4], selected: bool) -> [f32; 4] {
    if selected {
        color[0] = (color[0] + 0.55).min(1.0);
        color[1] = (color[1] + 0.55).min(1.0);
        color[2] = (color[2] + 0.55).min(1.0);
    }
    color
}

pub struct RenderOutput {
    pub frame: wgpu::SurfaceTexture,
    pub view: wgpu::TextureView,
}

impl Renderer {
    pub fn begin_frame(&self) -> Result<RenderOutput, wgpu::SurfaceError> {
        let frame = self.surface.get_current_texture()?;
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        Ok(RenderOutput { frame, view })
    }
}

struct Texture {
    view: wgpu::TextureView,
}

impl Texture {
    const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

    fn create_depth(device: &wgpu::Device, config: &wgpu::SurfaceConfiguration) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth texture"),
            size: wgpu::Extent3d {
                width: config.width,
                height: config.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: Self::DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Self { view }
    }
}
