#[path = "../framework.rs"]
mod framework;

use bytemuck::{Pod, Zeroable};
use std::{borrow::Cow, f32::consts};
use wgpu::{util::DeviceExt, AstcBlock, AstcChannel};

const IMAGE_SIZE: u32 = 128;

#[derive(Clone, Copy, Pod, Zeroable)]
#[repr(C)]
struct Vertex {
    pos: [f32; 3],
    normal: [f32; 3],
}

struct Entity {
    vertex_count: u32,
    vertex_buf: wgpu::Buffer,
}

// Note: we use the Y=up coordinate space in this example.
struct Camera {
    screen_size: (u32, u32),
    pos: glam::Vec3,
    dir: glam::Vec3,

}

impl Camera {
    fn to_uniform_data(&self) -> [f32; 16 * 3 + 4] {
        let aspect = self.screen_size.0 as f32 / self.screen_size.1 as f32;
        let proj = glam::Mat4::perspective_rh(consts::FRAC_PI_4, aspect, 1.0, 5000.0);
        
        let view = glam::Mat4::look_at_rh(
            self.pos,
            self.pos + self.dir,
            glam::Vec3::Y,
        );
        let proj_inv = proj.inverse();

        let mut raw = [0f32; 16 * 3 + 4];
        raw[..16].copy_from_slice(&AsRef::<[f32; 16]>::as_ref(&proj)[..]);
        raw[16..32].copy_from_slice(&AsRef::<[f32; 16]>::as_ref(&proj_inv)[..]);
        raw[32..48].copy_from_slice(&AsRef::<[f32; 16]>::as_ref(&view)[..]);
        raw[48..51].copy_from_slice(AsRef::<[f32; 3]>::as_ref(&self.pos));
        raw[51] = 1.0;
        raw
    }
}

pub struct Skybox {
    camera: Camera,
    sky_pipeline: wgpu::RenderPipeline,
    entity_pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    uniform_buf: wgpu::Buffer,
    uniform_local_matrix_buf: wgpu::Buffer,
    entities: Vec<Entity>,
    depth_view: wgpu::TextureView,
    staging_belt: wgpu::util::StagingBelt,
    left_click : bool,
    dxdy : (f32, f32),
    local_matrix : glam::Mat4,
}

impl Skybox {
    const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth24Plus;

    fn create_depth_texture(
        config: &wgpu::SurfaceConfiguration,
        device: &wgpu::Device,
    ) -> wgpu::TextureView {
        let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
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
            label: None,
        });

        depth_texture.create_view(&wgpu::TextureViewDescriptor::default())
    }

    fn get_entities(
        device: &wgpu::Device,
    ) ->Vec<Entity>{
        let mut entities = Vec::new();
        let mut obj_load = obj::Obj::load("C:/MIDAS/wgpu/wgpu/examples/kagong/asset/Skull/12140_Skull_v3_L2.obj").unwrap();
        
            let _ = obj_load.load_mtls();
            
            let data = obj_load.data;
            
            let mut vertices = Vec::new();
            for object in data.objects {
                for group in object.groups {
                    vertices.clear();
                    for poly in group.polys {
                        for end_index in 2..poly.0.len() {
                            for &index in &[0, end_index - 1, end_index] {
                                let obj::IndexTuple(position_id, _texture_id, normal_id) =
                                    poly.0[index];
                                vertices.push(Vertex {
                                    pos: data.position[position_id],
                                    normal: data.normal[normal_id.unwrap()],
                                })
                            }
                        }
                    }
                    let vertex_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Vertex"),
                        contents: bytemuck::cast_slice(&vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                    entities.push(Entity {
                        vertex_count: vertices.len() as u32,
                        vertex_buf,
                    });
                }
            }
        entities
    }
}

impl framework::Example for Skybox {
    fn optional_features() -> wgpu::Features {
        wgpu::Features::TEXTURE_COMPRESSION_ASTC_LDR
            | wgpu::Features::TEXTURE_COMPRESSION_ETC2
            | wgpu::Features::TEXTURE_COMPRESSION_BC
    }
    
    fn init(
        config: &wgpu::SurfaceConfiguration,
        _adapter: &wgpu::Adapter,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Self {

        let entities = Self::get_entities(device);

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::Cube,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        // Create the render pipeline
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("shader.wgsl"))),
        });

        let camera = Camera {
            screen_size: (config.width, config.height),
            pos : glam::vec3(0.0, 0.0, 0.0),
            dir : glam::vec3(0.0, 0.0, -1.0)
        };

        let cam_uniforms = camera.to_uniform_data();
        let uniform_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Buffer"),
            contents: bytemuck::cast_slice(&cam_uniforms),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let local_matrix = glam::Mat4::default();
        let init_local_matrix = local_matrix.to_cols_array();
        let uniform_local_matrix_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("local matrix Buffer"),
            contents: bytemuck::cast_slice(&init_local_matrix),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        // Create the render pipelines
        let sky_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Sky"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_sky",
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_sky",
                targets: &[Some(config.format.into())],
            }),
            primitive: wgpu::PrimitiveState {
                front_face: wgpu::FrontFace::Cw,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: Self::DEPTH_FORMAT,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });
        let entity_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Entity"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_entity",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_entity",
                targets: &[Some(config.format.into())],
            }),
            primitive: wgpu::PrimitiveState {
                front_face: wgpu::FrontFace::Cw,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: Self::DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: None,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let device_features = device.features();

        let skybox_format =
            if device_features.contains(wgpu::Features::TEXTURE_COMPRESSION_ASTC_LDR) {
                log::info!("Using ASTC_LDR");
                wgpu::TextureFormat::Astc {
                    block: AstcBlock::B4x4,
                    channel: AstcChannel::UnormSrgb,
                }
            } else if device_features.contains(wgpu::Features::TEXTURE_COMPRESSION_ETC2) {
                log::info!("Using ETC2");
                wgpu::TextureFormat::Etc2Rgb8UnormSrgb
            } else if device_features.contains(wgpu::Features::TEXTURE_COMPRESSION_BC) {
                log::info!("Using BC");
                wgpu::TextureFormat::Bc1RgbaUnormSrgb
            } else {
                log::info!("Using plain");
                wgpu::TextureFormat::Bgra8UnormSrgb
            };

        let size = wgpu::Extent3d {
            width: IMAGE_SIZE,
            height: IMAGE_SIZE,
            depth_or_array_layers: 6,
        };

        let layer_size = wgpu::Extent3d {
            depth_or_array_layers: 1,
            ..size
        };
        let max_mips = layer_size.max_mips(wgpu::TextureDimension::D2);

        log::debug!(
            "Copying {:?} skybox images of size {}, {}, 6 with {} mips to gpu",
            skybox_format,
            IMAGE_SIZE,
            IMAGE_SIZE,
            max_mips,
        );

        let bytes = match skybox_format {
            wgpu::TextureFormat::Astc {
                block: AstcBlock::B4x4,
                channel: AstcChannel::UnormSrgb,
            } => &include_bytes!("images/astc.dds")[..],
            wgpu::TextureFormat::Etc2Rgb8UnormSrgb => &include_bytes!("images/etc2.dds")[..],
            wgpu::TextureFormat::Bc1RgbaUnormSrgb => &include_bytes!("images/bc1.dds")[..],
            wgpu::TextureFormat::Bgra8UnormSrgb => &include_bytes!("images/bgra.dds")[..],
            _ => unreachable!(),
        };

        let image = ddsfile::Dds::read(&mut std::io::Cursor::new(&bytes)).unwrap();

        let texture = device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                size,
                mip_level_count: max_mips as u32,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: skybox_format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                label: None,
            },
            &image.data,
        );

        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: None,
            dimension: Some(wgpu::TextureViewDimension::Cube),
            ..wgpu::TextureViewDescriptor::default()
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: uniform_local_matrix_buf.as_entire_binding(),
                },
            ],
            label: None,
        });

        let depth_view = Self::create_depth_texture(config, device);

        let left_click = false;

        let dxdy = (0.0, 0.0);

        Skybox {
            camera,
            sky_pipeline,
            entity_pipeline,
            bind_group,
            uniform_buf,
            uniform_local_matrix_buf,
            entities,
            depth_view,
            staging_belt: wgpu::util::StagingBelt::new(0x100),
            left_click,
            dxdy,
            local_matrix,
        }
    }

    #[allow(clippy::single_match)]
    fn update(&mut self, event: winit::event::WindowEvent) {
        match event {
            winit::event::WindowEvent::MouseInput { state, button, .. } => {
                if winit::event::MouseButton::Left == button{ 
                    if winit::event::ElementState::Pressed == state{
                        self.left_click = true;
                    } else{
                        self.left_click = false;
                    }
                }
            }
            winit::event::WindowEvent::CursorMoved { position, .. } => {
                //-0.5 ~ 0.5
                let norm_x = position.x as f32 / self.camera.screen_size.0 as f32 - 0.5;
                let norm_y = position.y as f32 / self.camera.screen_size.1 as f32 - 0.5;

                let d_x = norm_x - self.dxdy.0;
                let d_y = norm_y - self.dxdy.1;
                    
                self.dxdy.0 = norm_x;
                self.dxdy.1 = norm_y;
                if self.left_click == true{
                        
                    let rot = glam::Mat3::from_rotation_y(d_x) * glam::Mat3::from_rotation_x(d_y);
                    self.camera.dir = rot * self.camera.dir;
                }
            }

            winit::event::WindowEvent::KeyboardInput { input, .. } => {
                if winit::event::ElementState::Pressed == input.state{ 
                    match input.virtual_keycode {
                        Some(winit::event::VirtualKeyCode::W) => { 
                            self.camera.pos += glam::Vec3::Y * 0.1;
                        },
                        Some(winit::event::VirtualKeyCode::S) => { 
                            self.camera.pos -= glam::Vec3::Y * 0.1;
                        },
                        Some(winit::event::VirtualKeyCode::D) => { 
                            self.camera.pos += glam::Vec3::X * 0.1;
                        },
                        Some(winit::event::VirtualKeyCode::A) => { 
                            self.camera.pos -= glam::Vec3::X * 0.1;
                        },
                        Some(winit::event::VirtualKeyCode::Z) => { 
                            self.local_matrix = glam::Mat4::from_rotation_x(1.0) * self.local_matrix;
                        },
                        Some(winit::event::VirtualKeyCode::X) => { 
                            self.local_matrix = glam::Mat4::from_rotation_y(1.0) * self.local_matrix;
                        },
                        Some(winit::event::VirtualKeyCode::C) => { 
                            self.local_matrix = glam::Mat4::from_rotation_z(1.0) * self.local_matrix;
                        },

                        _ => {
                        },
                    }

                }
            }

            winit::event::WindowEvent::MouseWheel { delta, .. } => {
                if let winit::event::MouseScrollDelta::LineDelta(_, y ) = delta { 
                    self.camera.pos += self.camera.dir * (y) * 1.0;
                }
                
            }
            _ => {}
        }
    }

    fn resize(
        &mut self,
        config: &wgpu::SurfaceConfiguration,
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
    ) {
        self.depth_view = Self::create_depth_texture(config, device);
        self.camera.screen_size = (config.width, config.height);
    }

    fn render(
        &mut self,
        view: &wgpu::TextureView,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _spawner: &framework::Spawner,
    ) {
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        // update rotation
        let raw_uniforms = self.camera.to_uniform_data();
        self.staging_belt
            .write_buffer(
                &mut encoder,
                &self.uniform_buf,
                0,
                wgpu::BufferSize::new((raw_uniforms.len() * 4) as wgpu::BufferAddress).unwrap(),
                device,
            )
            .copy_from_slice(bytemuck::cast_slice(&raw_uniforms));

        let raw_local = self.local_matrix.to_cols_array();
        self.staging_belt
            .write_buffer(
                &mut encoder,
                &self.uniform_local_matrix_buf,
                0,
                wgpu::BufferSize::new((raw_local.len() * 4) as wgpu::BufferAddress).unwrap(),
                device,
            )
            .copy_from_slice(bytemuck::cast_slice(&raw_local));

        self.staging_belt.finish();

        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.1,
                            g: 0.2,
                            b: 0.3,
                            a: 1.0,
                        }),
                        store: true,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: false,
                    }),
                    stencil_ops: None,
                }),
            });

            rpass.set_bind_group(0, &self.bind_group, &[]);
            rpass.set_pipeline(&self.entity_pipeline);

            for entity in self.entities.iter() {
                rpass.set_vertex_buffer(0, entity.vertex_buf.slice(..));
                rpass.draw(0..entity.vertex_count, 0..1);
            }

            rpass.set_pipeline(&self.sky_pipeline);
            rpass.draw(0..3, 0..1);
        }

        queue.submit(std::iter::once(encoder.finish()));

        self.staging_belt.recall();
    }
}

fn main() {
    framework::run::<Skybox>("skybox");
}

#[test]
fn skybox() {
    framework::test::<Skybox>(framework::FrameworkRefTest {
        image_path: "/examples/skybox/screenshot.png",
        width: 1024,
        height: 768,
        optional_features: wgpu::Features::default(),
        base_test_parameters: framework::test_common::TestParameters::default()
            .backend_failure(wgpu::Backends::GL),
        tolerance: 3,
        max_outliers: 3,
    });
}

#[test]
fn skybox_bc1() {
    framework::test::<Skybox>(framework::FrameworkRefTest {
        image_path: "/examples/skybox/screenshot-bc1.png",
        width: 1024,
        height: 768,
        optional_features: wgpu::Features::TEXTURE_COMPRESSION_BC,
        base_test_parameters: framework::test_common::TestParameters::default(), // https://bugs.chromium.org/p/angleproject/issues/detail?id=7056
        tolerance: 5,
        max_outliers: 105, // Bounded by llvmpipe
    });
}

#[test]
fn skybox_etc2() {
    framework::test::<Skybox>(framework::FrameworkRefTest {
        image_path: "/examples/skybox/screenshot-etc2.png",
        width: 1024,
        height: 768,
        optional_features: wgpu::Features::TEXTURE_COMPRESSION_ETC2,
        base_test_parameters: framework::test_common::TestParameters::default(), // https://bugs.chromium.org/p/angleproject/issues/detail?id=7056
        tolerance: 5,
        max_outliers: 105, // Bounded by llvmpipe
    });
}

#[test]
fn skybox_astc() {
    framework::test::<Skybox>(framework::FrameworkRefTest {
        image_path: "/examples/skybox/screenshot-astc.png",
        width: 1024,
        height: 768,
        optional_features: wgpu::Features::TEXTURE_COMPRESSION_ASTC_LDR,
        base_test_parameters: framework::test_common::TestParameters::default(), // https://bugs.chromium.org/p/angleproject/issues/detail?id=7056
        tolerance: 5,
        max_outliers: 300, // Bounded by rp4 on vk
    });
}
