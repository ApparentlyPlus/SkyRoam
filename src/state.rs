use winit::{window::Window, event::*};
use wgpu::util::DeviceExt;
use std::time::Instant;
use crate::{camera::{Camera, CameraUniform, CameraController}, world::*, shader};

pub struct GpuContext {
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    pub size: winit::dpi::PhysicalSize<u32>,
    pub msaa_texture: wgpu::TextureView,
    pub depth_texture: wgpu::TextureView,
}

impl GpuContext {
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
            &wgpu::DeviceDescriptor { 
                label: None, 
                required_features: wgpu::Features::empty(), 
                required_limits: wgpu::Limits::default() 
            },
            None,
        ).await.unwrap();

        let config = surface.get_default_config(&adapter, size.width, size.height).unwrap();
        
        let mut final_config = config.clone();
        let caps = surface.get_capabilities(&adapter);
        if caps.present_modes.contains(&wgpu::PresentMode::Mailbox) {
            final_config.present_mode = wgpu::PresentMode::Mailbox;
        } else {
            final_config.present_mode = wgpu::PresentMode::Fifo;
        }

        surface.configure(&device, &final_config);

        let msaa_texture = Self::create_msaa_texture(&device, &final_config);
        let depth_texture = Self::create_depth_texture(&device, &final_config);

        Self { surface, device, queue, config: final_config, size, msaa_texture, depth_texture }
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
            self.msaa_texture = Self::create_msaa_texture(&self.device, &self.config);
            self.depth_texture = Self::create_depth_texture(&self.device, &self.config);
        }
    }

    fn create_depth_texture(device: &wgpu::Device, config: &wgpu::SurfaceConfiguration) -> wgpu::TextureView {
        let size = wgpu::Extent3d { width: config.width, height: config.height, depth_or_array_layers: 1 };
        let desc = wgpu::TextureDescriptor {
            label: Some("Depth Texture"), size, mip_level_count: 1, sample_count: 4,
            dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT, view_formats: &[],
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
}

pub struct GameState {
    pub ctx: GpuContext, 
    render_pipeline: wgpu::RenderPipeline,
    ui_pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    world: World,
    pub camera: Camera,
    camera_controller: CameraController,
    camera_uniform: CameraUniform,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    pub mouse_captured: bool,
    last_frame_time: Instant,
    velocity: glam::DVec3, 
    on_ground: bool,
}

impl GameState {
    pub fn new(mut ctx: GpuContext, world: World) -> Self {
        
        let vertex_buffer = ctx.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"), contents: bytemuck::cast_slice(&world.vertices), usage: wgpu::BufferUsages::VERTEX,
        });
        
        let index_buffer = ctx.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Index Buffer"), contents: bytemuck::cast_slice(&world.indices), usage: wgpu::BufferUsages::INDEX,
        });

        let aspect = ctx.config.width as f32 / ctx.config.height as f32;
        
        let camera = Camera {
            eye: glam::DVec3::new(0.0, 50.0, 0.0), 
            velocity: glam::DVec3::ZERO,
            yaw: -90.0f32.to_radians(), 
            pitch: 0.0, 
            aspect,
        };
        
        let mut camera_uniform = CameraUniform { 
            view_proj: [[0.0; 4]; 4], screen_size: [ctx.config.width as f32, ctx.config.height as f32], 
            fog_dist: [100.0, 3000.0], 
            camera_pos: [camera.eye.x as f32, camera.eye.y as f32, camera.eye.z as f32, 0.0],
        };
        camera_uniform.view_proj = camera.build_view_projection_matrix().to_cols_array_2d();

        let camera_buffer = ctx.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Camera Buffer"), contents: bytemuck::cast_slice(&[camera_uniform]), usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let camera_bind_group_layout = ctx.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0, visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None,
            }], label: None,
        });
        let camera_bind_group = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &camera_bind_group_layout, entries: &[wgpu::BindGroupEntry { binding: 0, resource: camera_buffer.as_entire_binding() }], label: None,
        });

        let shader_module = ctx.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Shader"), source: wgpu::ShaderSource::Wgsl(shader::SCENE_SHADER.into()),
        });

        let render_pipeline_layout = ctx.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None, bind_group_layouts: &[&camera_bind_group_layout], push_constant_ranges: &[],
        });

        let render_pipeline = ctx.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"), layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader_module, entry_point: "vs_main",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<WorldVertex>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute { offset: 0,  shader_location: 0, format: wgpu::VertexFormat::Float32x3 }, 
                        wgpu::VertexAttribute { offset: 12, shader_location: 1, format: wgpu::VertexFormat::Float32x3 }, 
                        wgpu::VertexAttribute { offset: 24, shader_location: 2, format: wgpu::VertexFormat::Float32x3 }, 
                    ],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader_module, entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState { format: ctx.config.format, blend: Some(wgpu::BlendState::REPLACE), write_mask: wgpu::ColorWrites::ALL })],
            }),
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: None, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState { format: wgpu::TextureFormat::Depth32Float, depth_write_enabled: true, depth_compare: wgpu::CompareFunction::Less, stencil: wgpu::StencilState::default(), bias: wgpu::DepthBiasState::default() }),
            multisample: wgpu::MultisampleState { count: 4, mask: !0, alpha_to_coverage_enabled: false },
            multiview: None,
        });

        let ui_shader_module = ctx.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("UI Shader"), source: wgpu::ShaderSource::Wgsl(shader::UI_SHADER.into()),
        });

        let ui_pipeline = ctx.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("UI Pipeline"), layout: None,
            vertex: wgpu::VertexState { module: &ui_shader_module, entry_point: "vs_main", buffers: &[] },
            fragment: Some(wgpu::FragmentState {
                module: &ui_shader_module, entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState { format: ctx.config.format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })],
            }),
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleStrip, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float, depth_write_enabled: false, depth_compare: wgpu::CompareFunction::Always, stencil: wgpu::StencilState::default(), bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState { count: 4, mask: !0, alpha_to_coverage_enabled: false },
            multiview: None,
        });

        Self {
            ctx, render_pipeline, ui_pipeline, vertex_buffer, index_buffer,
            world, camera, camera_controller: CameraController::new(),
            camera_uniform, camera_buffer, camera_bind_group,
            mouse_captured: false, last_frame_time: Instant::now(),
            velocity: glam::DVec3::ZERO, on_ground: false,
        }
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        self.ctx.resize(new_size);
        self.camera.aspect = self.ctx.config.width as f32 / self.ctx.config.height as f32;
        self.camera_uniform.screen_size = [self.ctx.config.width as f32, self.ctx.config.height as f32];
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

    // FIX: Returns Normal AND Penetration Depth
    fn check_collision(&self, new_pos: glam::DVec3) -> Option<(glam::DVec3, f64)> {
        let player_radius = 0.3; 
        
        // FIX: Match world.rs generation thickness (0.5).
        // Total "physics thickness" from center = 0.5 (wall) + 0.3 (player) = 0.8
        let wall_thickness = 0.5; 
        let collision_dist = player_radius + wall_thickness;
        let collision_dist_sq = collision_dist * collision_dist;

        let gx = (new_pos.x / 50.0).floor() as i32;
        let gz = (new_pos.z / 50.0).floor() as i32;
        
        let mut min_dist_sq = collision_dist_sq as f64;
        let mut best_hit = None;

        let px = new_pos.x as f32;
        let pz = new_pos.z as f32;
        let cd = collision_dist as f32;

        for ox in -1..=1 {
            for oz in -1..=1 {
                if let Some(walls) = self.world.collision_map.get(&(gx + ox, gz + oz)) {
                    for wall in walls {
                        if (new_pos.y as f32) > wall.height { continue; }
                        
                        if px < wall.min_x - cd || px > wall.max_x + cd ||
                           pz < wall.min_z - cd || pz > wall.max_z + cd {
                            continue;
                        }

                        let p = glam::DVec2::new(new_pos.x, new_pos.z);
                        let a = glam::DVec2::new(wall.start.x as f64, wall.start.y as f64);
                        let b = glam::DVec2::new(wall.end.x as f64, wall.end.y as f64);
                        let ab = b - a;
                        let ap = p - a;
                        let t = (ap.dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
                        let closest = a + ab * t;
                        
                        let dist_sq = p.distance_squared(closest);

                        if dist_sq < min_dist_sq {
                            min_dist_sq = dist_sq;
                            let push_vec = p - closest;
                            if push_vec.length_squared() > 0.0000001 {
                                let dist = dist_sq.sqrt();
                                let penetration = (collision_dist as f64) - dist;
                                let normal = glam::DVec3::new(push_vec.x / dist, 0.0, push_vec.y / dist);
                                best_hit = Some((normal, penetration));
                            } else {
                                // Direct hit on center? Push X
                                best_hit = Some((glam::DVec3::new(1.0, 0.0, 0.0), collision_dist as f64));
                            }
                        }
                    }
                }
            }
        }
        best_hit
    }

    pub fn update(&mut self) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame_time).as_secs_f64().clamp(0.0001, 0.1);
        self.last_frame_time = now;

        let move_speed = 10.0;
        let gravity = 35.0; 
        let jump_force = 12.0;

        let (sin_yaw, cos_yaw) = self.camera.yaw.sin_cos();
        let forward = glam::DVec3::new(cos_yaw as f64, 0.0, sin_yaw as f64).normalize();
        let right = glam::DVec3::new(-(sin_yaw as f64), 0.0, cos_yaw as f64).normalize();

        let mut input_dir = glam::DVec3::ZERO;
        if self.camera_controller.move_fwd { input_dir += forward; }
        if self.camera_controller.move_back { input_dir -= forward; }
        if self.camera_controller.move_right { input_dir += right; }
        if self.camera_controller.move_left { input_dir -= right; }

        if input_dir.length_squared() > 0.0 { input_dir = input_dir.normalize(); }
        
        self.velocity.x = input_dir.x * move_speed;
        self.velocity.z = input_dir.z * move_speed;
        self.velocity.y -= gravity * dt;

        if self.on_ground && self.camera_controller.jump {
            self.velocity.y = jump_force;
            self.on_ground = false;
        }

        let mut remaining_dt = dt;
        let step_size = 0.02; 

        while remaining_dt > 0.0 {
            let step = remaining_dt.min(step_size);
            
            // Move
            let mut next_pos = self.camera.eye + self.velocity * step;
            
            // Resolve Collisions (4 Passes)
            for _ in 0..4 {
                if let Some((normal, depth)) = self.check_collision(next_pos) {
                    // Slide Velocity
                    let dot = self.velocity.dot(normal);
                    if dot < 0.0 {
                        self.velocity -= normal * dot;
                    }

                    next_pos += normal * (depth + 0.0001); 
                } else {
                    break;
                }
            }

            // Floor
            if next_pos.y < 1.8 {
                next_pos.y = 1.8;
                self.velocity.y = 0.0;
                self.on_ground = true;
            } else {
                self.on_ground = false;
            }

            self.camera.eye = next_pos;
            remaining_dt -= step;
        }

        self.camera_uniform.view_proj = self.camera.build_view_projection_matrix().to_cols_array_2d();
        self.camera_uniform.camera_pos = [self.camera.eye.x as f32, self.camera.eye.y as f32, self.camera.eye.z as f32, 0.0];
        self.ctx.queue.write_buffer(&self.camera_buffer, 0, bytemuck::cast_slice(&[self.camera_uniform]));
    }

    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let output = self.ctx.surface.get_current_texture()?;
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self.ctx.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.ctx.msaa_texture, resolve_target: Some(&view),
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.ctx.depth_texture,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None, occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_bind_group(0, &self.camera_bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);

            let cam_x = self.camera.eye.x as f32;
            let cam_z = self.camera.eye.z as f32;
            let draw_dist = 3000.0f32; 

            for chunk in &self.world.chunks {
                let cx = (chunk.min.x + chunk.max.x) * 0.5;
                let cz = (chunk.min.y + chunk.max.y) * 0.5; 
                
                let dist_sq = (cx - cam_x).powi(2) + (cz - cam_z).powi(2);
                if dist_sq < draw_dist * draw_dist {
                    render_pass.draw_indexed(
                        chunk.index_start..(chunk.index_start + chunk.index_count),
                        0, 0..1
                    );
                }
            }

            render_pass.set_pipeline(&self.ui_pipeline);
            render_pass.draw(0..4, 0..1); 
        }

        self.ctx.queue.submit(std::iter::once(encoder.finish()));
        output.present();
        Ok(())
    }
}