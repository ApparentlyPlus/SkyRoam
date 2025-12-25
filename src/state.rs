use winit::{window::Window, event::*};
use wgpu::util::DeviceExt;
use std::time::Instant;
use crate::{camera::{Camera, CameraUniform, CameraController}, vertex::*, world::*, shader};

pub struct State {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    pub size: winit::dpi::PhysicalSize<u32>,
    render_pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    instance_buffer: wgpu::Buffer,
    instance_capacity: usize,
    num_indices: u32,           
    num_draw_instances: u32,
    
    // Components
    world: World,
    pub camera: Camera,
    camera_controller: CameraController,
    camera_uniform: CameraUniform,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    
    // Textures
    depth_texture: wgpu::TextureView,
    msaa_texture: wgpu::TextureView,
    
    // Logic
    pub mouse_captured: bool,
    last_frame_time: Instant,
    on_ground: bool,
    scratch_instances: Vec<InstanceRaw>,
    last_camera_pos: glam::Vec3,
    last_camera_yaw: f32,
}

impl State {
    pub async fn new(window: std::sync::Arc<Window>) -> Self {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
        let surface = instance.create_surface(window.clone()).unwrap();
        let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }).await.unwrap();

        let (device, queue) = adapter.request_device(
            &wgpu::DeviceDescriptor { label: None, required_features: wgpu::Features::empty(), required_limits: wgpu::Limits::default() },
            None,
        ).await.unwrap();

        let config = surface.get_default_config(&adapter, size.width, size.height).unwrap();
        let mut vsync_config = config.clone();
        vsync_config.present_mode = wgpu::PresentMode::Immediate; 
        surface.configure(&device, &vsync_config);

        // --- WORLD & BUFFERS ---
        let world = World::generate();
        let instance_capacity = 20_000;
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Instance Buffer"),
            size: (instance_capacity * std::mem::size_of::<InstanceRaw>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"), contents: bytemuck::cast_slice(VERTICES), usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Index Buffer"), contents: bytemuck::cast_slice(INDICES), usage: wgpu::BufferUsages::INDEX,
        });

        // --- PIPELINE ---
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Shader"), source: wgpu::ShaderSource::Wgsl(shader::SOURCE.into()),
        });

        let aspect = if vsync_config.height > 0 { vsync_config.width as f32 / vsync_config.height as f32 } else { 1.0 };
        let camera = Camera {
            eye: (0.0, 1.8, 0.0).into(), velocity: glam::Vec3::ZERO,
            yaw: -90.0f32.to_radians(), pitch: 0.0, aspect,
        };
        
        let mut camera_uniform = CameraUniform { 
            view_proj: [[0.0; 4]; 4], 
            screen_size: [vsync_config.width as f32, vsync_config.height as f32], 
            fog_dist: [RENDER_DISTANCE_WORLD * 0.5, RENDER_DISTANCE_WORLD * 0.95],
            camera_pos: [camera.eye.x, camera.eye.y, camera.eye.z, 0.0],
        };
        camera_uniform.view_proj = camera.build_view_projection_matrix().to_cols_array_2d();

        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Camera Buffer"), contents: bytemuck::cast_slice(&[camera_uniform]), usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let camera_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0, visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None,
            }], label: None,
        });
        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &camera_bind_group_layout, entries: &[wgpu::BindGroupEntry { binding: 0, resource: camera_buffer.as_entire_binding() }], label: None,
        });

        let render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None, bind_group_layouts: &[&camera_bind_group_layout], push_constant_ranges: &[],
        });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None, layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader, entry_point: "vs_main",
                buffers: &[
                    wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress, step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &[wgpu::VertexAttribute { offset: 0, shader_location: 0, format: wgpu::VertexFormat::Float32x3 }, wgpu::VertexAttribute { offset: 12, shader_location: 1, format: wgpu::VertexFormat::Float32x3 }],
                    },
                    wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<InstanceRaw>() as wgpu::BufferAddress, step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &[
                            wgpu::VertexAttribute { offset: 0, shader_location: 5, format: wgpu::VertexFormat::Float32x3 }, 
                            wgpu::VertexAttribute { offset: 12, shader_location: 6, format: wgpu::VertexFormat::Float32x3 }, 
                            wgpu::VertexAttribute { offset: 24, shader_location: 7, format: wgpu::VertexFormat::Float32 }
                        ],
                    }
                ],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader, entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState { format: vsync_config.format, blend: Some(wgpu::BlendState::REPLACE), write_mask: wgpu::ColorWrites::ALL })],
            }),
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: Some(wgpu::Face::Back), ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState { format: wgpu::TextureFormat::Depth32Float, depth_write_enabled: true, depth_compare: wgpu::CompareFunction::Less, stencil: wgpu::StencilState::default(), bias: wgpu::DepthBiasState::default() }),
            multisample: wgpu::MultisampleState { count: 4, mask: !0, alpha_to_coverage_enabled: false },
            multiview: None,
        });

        let depth_texture = Self::create_depth_texture(&device, &vsync_config);
        let msaa_texture = Self::create_msaa_texture(&device, &vsync_config);

        Self {
            surface, device, queue, config: vsync_config, size,
            render_pipeline, vertex_buffer, index_buffer, instance_buffer, instance_capacity,
            num_indices: INDICES.len() as u32, num_draw_instances: 0,
            world, camera, camera_controller: CameraController::new(),
            camera_uniform, camera_buffer, camera_bind_group,
            depth_texture, msaa_texture,
            mouse_captured: false, last_frame_time: Instant::now(), on_ground: false,
            scratch_instances: Vec::with_capacity(50000),
            last_camera_pos: glam::Vec3::ZERO, last_camera_yaw: 0.0,
        }
    }

    fn create_depth_texture(device: &wgpu::Device, config: &wgpu::SurfaceConfiguration) -> wgpu::TextureView {
        let size = wgpu::Extent3d { width: config.width, height: config.height, depth_or_array_layers: 1 };
        let desc = wgpu::TextureDescriptor {
            label: Some("Depth Texture"), size, mip_level_count: 1, sample_count: 4,
            dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING, view_formats: &[],
        };
        device.create_texture(&desc).create_view(&wgpu::TextureViewDescriptor::default())
    }

    fn create_msaa_texture(device: &wgpu::Device, config: &wgpu::SurfaceConfiguration) -> wgpu::TextureView {
        let size = wgpu::Extent3d { width: config.width, height: config.height, depth_or_array_layers: 1 };
        let desc = wgpu::TextureDescriptor {
            label: Some("MSAA Texture"), size, mip_level_count: 1, sample_count: 4,
            dimension: wgpu::TextureDimension::D2, format: config.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT, view_formats: &[],
        };
        device.create_texture(&desc).create_view(&wgpu::TextureViewDescriptor::default())
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
            self.camera.aspect = self.config.width as f32 / self.config.height as f32;
            self.depth_texture = Self::create_depth_texture(&self.device, &self.config);
            self.msaa_texture = Self::create_msaa_texture(&self.device, &self.config);
            self.camera_uniform.screen_size = [self.config.width as f32, self.config.height as f32];
        }
    }

    pub fn input(&mut self, event: &WindowEvent) -> bool {
        self.camera_controller.process_events(event)
    }

    pub fn update_camera_rotation(&mut self, delta: (f64, f64)) {
        if self.mouse_captured {
            let sensitivity = 0.003;
            self.camera.yaw += delta.0 as f32 * sensitivity;
            self.camera.pitch -= delta.1 as f32 * sensitivity;
            self.camera.pitch = self.camera.pitch.clamp(-1.5, 1.5);
        }
    }

    fn check_collision(&self, new_pos: glam::Vec3) -> bool {
        let player_radius = 0.5;
        let gx = (new_pos.x / 25.0).round() as i32; // Hardcoded spread match
        let gz = (new_pos.z / 25.0).round() as i32;
        
        for ox in -1..=1 {
            for oz in -1..=1 {
                if let Some(bbox) = self.world.collision_map.get(&(gx + ox, gz + oz)) {
                    if new_pos.x + player_radius > bbox.min.x && new_pos.x - player_radius < bbox.max.x &&
                       new_pos.z + player_radius > bbox.min.z && new_pos.z - player_radius < bbox.max.z {
                        return true;
                    }
                }
            }
        }
        false
    }

    pub fn update(&mut self) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame_time).as_secs_f32().clamp(0.0001, 0.1);
        self.last_frame_time = now;

        let speed = 8.5; 
        let gravity = 20.0; 
        let jump_force = 8.0; 
        let mut camera_moved = false; 

        if self.mouse_captured {
            if self.last_camera_yaw != self.camera.yaw {
                camera_moved = true;
                self.last_camera_yaw = self.camera.yaw;
            }

            let (sin_yaw, cos_yaw) = self.camera.yaw.sin_cos();
            let forward = glam::Vec3::new(cos_yaw, 0.0, sin_yaw).normalize();
            let right = glam::Vec3::new(-sin_yaw, 0.0, cos_yaw).normalize();

            let mut move_dir = glam::Vec3::ZERO;
            if self.camera_controller.move_fwd { move_dir += forward; }
            if self.camera_controller.move_back { move_dir -= forward; }
            if self.camera_controller.move_right { move_dir += right; }
            if self.camera_controller.move_left { move_dir -= right; }

            if move_dir.length_squared() > 0.0 {
                camera_moved = true; 
                let velocity = move_dir.normalize() * speed * dt;
                let mut target_x = self.camera.eye; target_x.x += velocity.x;
                let mut target_z = self.camera.eye; target_z.z += velocity.z;
                if !self.check_collision(target_x) { self.camera.eye.x += velocity.x; }
                if !self.check_collision(target_z) { self.camera.eye.z += velocity.z; }
            }
            if self.on_ground && self.camera_controller.jump {
                self.camera.velocity.y = jump_force;
                self.on_ground = false;
                camera_moved = true;
            }
        }

        self.camera.velocity.y -= gravity * dt;
        self.camera.eye.y += self.camera.velocity.y * dt;
        
        if self.camera.eye.y < 1.8 { 
            self.camera.eye.y = 1.8; 
            self.camera.velocity.y = 0.0; 
            self.on_ground = true; 
        } else if self.camera.velocity.y.abs() > 0.01 {
             camera_moved = true; 
        }

        if camera_moved {
            self.camera_uniform.view_proj = self.camera.build_view_projection_matrix().to_cols_array_2d();
            self.camera_uniform.camera_pos = [self.camera.eye.x, self.camera.eye.y, self.camera.eye.z, 0.0];
            self.queue.write_buffer(&self.camera_buffer, 0, bytemuck::cast_slice(&[self.camera_uniform]));
        }

        // Culling Logic
        let dist_moved = self.camera.eye.distance_squared(self.last_camera_pos);
        if camera_moved && (dist_moved > 0.01 || self.last_camera_yaw != self.camera.yaw) {
            self.last_camera_pos = self.camera.eye;
            self.scratch_instances.clear();
            self.scratch_instances.extend_from_slice(&self.world.global_instances);

            let cam_chunk_x = (self.camera.eye.x / CHUNK_SIZE).floor() as i32;
            let cam_chunk_z = (self.camera.eye.z / CHUNK_SIZE).floor() as i32;
            
            let (sin_yaw, cos_yaw) = self.camera.yaw.sin_cos();
            let cam_forward = glam::Vec2::new(cos_yaw, sin_yaw).normalize();
            
            for x in (cam_chunk_x - RENDER_DISTANCE_CHUNKS)..=(cam_chunk_x + RENDER_DISTANCE_CHUNKS) {
                for z in (cam_chunk_z - RENDER_DISTANCE_CHUNKS)..=(cam_chunk_z + RENDER_DISTANCE_CHUNKS) {
                    let dx = x - cam_chunk_x;
                    let dz = z - cam_chunk_z;
                    if (dx*dx + dz*dz) as f32 > (RENDER_DISTANCE_CHUNKS * RENDER_DISTANCE_CHUNKS) as f32 { continue; }

                    let chunk_center_x = (x as f32 * CHUNK_SIZE) + (CHUNK_SIZE * 0.5);
                    let chunk_center_z = (z as f32 * CHUNK_SIZE) + (CHUNK_SIZE * 0.5);
                    let chunk_dir = glam::Vec2::new(chunk_center_x - self.camera.eye.x, chunk_center_z - self.camera.eye.z).normalize();
                    
                    let dist_sq = (dx * dx + dz * dz) as f32;
                    let is_near = dist_sq < 100.0;

                    let dot_prod = cam_forward.dot(chunk_dir);
                    let is_in_view = dot_prod > 0.3;

                    if is_near || is_in_view {
                        if let Some(chunk_instances) = self.world.chunks.get(&(x, z)) {
                            self.scratch_instances.extend_from_slice(chunk_instances);
                        }
                    }
                }
            }

            self.num_draw_instances = self.scratch_instances.len() as u32;
            if self.scratch_instances.len() > self.instance_capacity {
                self.instance_capacity = (self.scratch_instances.len() as f32 * 1.5) as usize;
                self.instance_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Instance Buffer"),
                    size: (self.instance_capacity * std::mem::size_of::<InstanceRaw>()) as u64,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
            }
            if self.num_draw_instances > 0 {
                self.queue.write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(&self.scratch_instances));
            }
        }
    }

    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.msaa_texture, resolve_target: Some(&view), 
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_texture,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None, occlusion_query_set: None,
            });
            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_bind_group(0, &self.camera_bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            render_pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
            render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            render_pass.draw_indexed(0..self.num_indices, 0, 0..self.num_draw_instances);
        }
        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
        Ok(())
    }
}