use winit::{
    event::*,
    event_loop::{ControlFlow, EventLoop},
    window::{WindowBuilder, CursorGrabMode, Fullscreen, Window},
    keyboard::{KeyCode, PhysicalKey},
};
use wgpu::util::DeviceExt;
use std::time::Instant;
use std::sync::mpsc;
use std::thread;

mod shader;
mod vertex;
mod camera;
mod world;
mod state;
use state::{GameState, GpuContext};
use world::{World, LoaderMessage};

// --- LOADING SCREEN RENDERER ---
#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct LoadingUniforms {
    screen_size: [f32; 2],
    progress: f32,
    _pad: f32,
}

struct LoadingScreen {
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    pub current_progress: f32,
}

impl LoadingScreen {
    fn new(ctx: &GpuContext) -> Self {
        let shader = ctx.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Loading Shader"),
            source: wgpu::ShaderSource::Wgsl(shader::LOADING_SHADER.into()),
        });
        
        // Create Uniform Buffer
        let uniforms = LoadingUniforms {
            screen_size: [ctx.config.width as f32, ctx.config.height as f32],
            progress: 0.0,
            _pad: 0.0,
        };
        let uniform_buffer = ctx.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Loading Uniforms"),
            contents: bytemuck::cast_slice(&[uniforms]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group_layout = ctx.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer { 
                    ty: wgpu::BufferBindingType::Uniform, 
                    has_dynamic_offset: false, 
                    min_binding_size: None 
                },
                count: None,
            }],
            label: Some("Loading Bind Group Layout"),
        });

        let bind_group = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
            label: Some("Loading Bind Group"),
        });

        let pipeline_layout = ctx.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Loading Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = ctx.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Loading Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState { module: &shader, entry_point: "vs_main", buffers: &[] },
            fragment: Some(wgpu::FragmentState {
                module: &shader, entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: ctx.config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, ..Default::default() },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        Self { pipeline, uniform_buffer, bind_group, current_progress: 0.0 }
    }

    fn render(&self, ctx: &mut GpuContext) {
        let Ok(output) = ctx.surface.get_current_texture() else { return };
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = ctx.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        // Update Uniforms (Progress + Screen Size in case of resize)
        let uniforms = LoadingUniforms {
            screen_size: [ctx.config.width as f32, ctx.config.height as f32],
            progress: self.current_progress,
            _pad: 0.0,
        };
        ctx.queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Loading Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view, resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None, occlusion_query_set: None,
            });
            
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.draw(0..4, 0..1); // Draw Quad (Vertex index handles geometry)
        }
        
        ctx.queue.submit(std::iter::once(encoder.finish()));
        output.present();
    }
}

fn set_cursor_grab(window: &Window, grabbed: bool) {
    if grabbed {
        if window.set_cursor_grab(CursorGrabMode::Confined).is_err() {
            let _ = window.set_cursor_grab(CursorGrabMode::Locked);
        }
        window.set_cursor_visible(false);
    } else {
        let _ = window.set_cursor_grab(CursorGrabMode::None);
        window.set_cursor_visible(true);
    }
}

fn main() {
    env_logger::init();
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);
    
    let builder = WindowBuilder::new().with_title("Blazing Mapbox");
    let monitor = event_loop.primary_monitor();
    let window = std::sync::Arc::new(builder.with_fullscreen(Some(Fullscreen::Borderless(monitor))).build(&event_loop).unwrap());
    
    let mut gpu_ctx_opt = Some(pollster::block_on(GpuContext::new(window.clone())));
    let mut loading_screen = LoadingScreen::new(gpu_ctx_opt.as_ref().unwrap());

    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let world = World::generate(tx.clone());
        tx.send(LoaderMessage::Done(world)).unwrap();
    });

    let mut state: Option<GameState> = None;
    let mut last_fps_print = Instant::now();
    let mut frames = 0;

    set_cursor_grab(&window, false);

    event_loop.run(move |event, elwt| {
        match event {
            Event::WindowEvent { ref event, window_id } if window_id == window.id() => {
                if let Some(s) = &mut state {
                    if !s.input(event) {
                        match event {
                            WindowEvent::CloseRequested => elwt.exit(),
                            WindowEvent::Resized(physical_size) => s.resize(*physical_size),
                            WindowEvent::RedrawRequested => {
                                s.update();
                                match s.render() {
                                    Ok(_) => {}
                                    Err(wgpu::SurfaceError::Lost) => s.resize(s.ctx.size),
                                    Err(wgpu::SurfaceError::OutOfMemory) => elwt.exit(),
                                    Err(e) => eprintln!("Render error: {:?}", e),
                                }
                            }
                            WindowEvent::MouseInput { state: element_state, button: MouseButton::Left, .. } => {
                                if *element_state == ElementState::Pressed {
                                    s.mouse_captured = true;
                                    set_cursor_grab(&window, true);
                                }
                            },
                            WindowEvent::KeyboardInput { event: KeyEvent { physical_key: PhysicalKey::Code(KeyCode::Escape), state: element_state, .. }, .. } => {
                                if *element_state == ElementState::Pressed {
                                    s.mouse_captured = false;
                                    set_cursor_grab(&window, false);
                                }
                            },
                            _ => {}
                        }
                    }
                } else {
                    match event {
                        WindowEvent::CloseRequested => elwt.exit(),
                        WindowEvent::Resized(size) => {
                            if let Some(ctx) = &mut gpu_ctx_opt {
                                ctx.resize(*size);
                            }
                        }
                        WindowEvent::RedrawRequested => {
                            if let Some(ctx) = &mut gpu_ctx_opt {
                                loading_screen.render(ctx);
                            }
                        },
                        _ => {}
                    }
                }
            },
            Event::DeviceEvent { event: DeviceEvent::MouseMotion { delta }, .. } => {
                if let Some(s) = &mut state {
                    s.update_camera_rotation(delta);
                }
            },
            Event::AboutToWait => {
                if state.is_none() {
                    let mut updated = false;
                    while let Ok(msg) = rx.try_recv() {
                        match msg {
                            LoaderMessage::Progress(p) => {
                                loading_screen.current_progress = p;
                                updated = true;
                            },
                            LoaderMessage::Done(world) => {
                                loading_screen.current_progress = 1.0;
                                if let Some(mut ctx) = gpu_ctx_opt.take() {
                                    loading_screen.render(&mut ctx);
                                    state = Some(GameState::new(ctx, world));
                                }
                                return; 
                            }
                        }
                    }
                    if updated { window.request_redraw(); }
                } else {
                    frames += 1;
                    if last_fps_print.elapsed().as_secs_f32() >= 1.0 {
                        window.set_title(&format!("Blazing Mapbox | FPS: {}", frames));
                        frames = 0;
                        last_fps_print = Instant::now();
                    }
                    window.request_redraw();
                }
            },
            _ => {}
        }
    }).unwrap();
}