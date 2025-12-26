// main.rs
use winit::{
    event::*, event_loop::{ControlFlow, EventLoop}, window::{WindowBuilder, CursorGrabMode, Fullscreen, Window},
    keyboard::{KeyCode, PhysicalKey},
};
use wgpu::util::DeviceExt;
use std::time::Instant;
use std::sync::mpsc;
use std::thread;

mod config;
mod shader;
mod vertex;
mod camera;
mod world;
mod map_loader;
mod state;

use state::{GameState, GpuContext};
use world::LoaderMessage;

// --- Loading Screen Renderer ---
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct LoadingUniforms {
    screen_size: [f32; 2], progress: f32, _pad: f32,
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
            label: Some("Loading"), source: wgpu::ShaderSource::Wgsl(shader::LOADING_SHADER.into()),
        });
        let uniforms = LoadingUniforms { screen_size: [ctx.config.width as f32, ctx.config.height as f32], progress: 0.0, _pad: 0.0 };
        let uniform_buffer = ctx.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None, contents: bytemuck::cast_slice(&[uniforms]), usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let bind_group_layout = ctx.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            entries: &[wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None }], label: None,
        });
        let bind_group = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor { layout: &bind_group_layout, entries: &[wgpu::BindGroupEntry { binding: 0, resource: uniform_buffer.as_entire_binding() }], label: None });
        let pipeline_layout = ctx.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[&bind_group_layout], push_constant_ranges: &[] });
        let pipeline = ctx.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None, layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState { module: &shader, entry_point: "vs_main", buffers: &[] },
            fragment: Some(wgpu::FragmentState { module: &shader, entry_point: "fs_main", targets: &[Some(wgpu::ColorTargetState { format: ctx.config.format, blend: Some(wgpu::BlendState::REPLACE), write_mask: wgpu::ColorWrites::ALL })] }),
            primitive: wgpu::PrimitiveState::default(), depth_stencil: None, multisample: wgpu::MultisampleState::default(), multiview: None,
        });
        Self { pipeline, uniform_buffer, bind_group, current_progress: 0.0 }
    }
    
    fn render(&self, ctx: &mut GpuContext) {
        let Ok(output) = ctx.surface.get_current_texture() else { return };
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = ctx.device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        
        let uniforms = LoadingUniforms { screen_size: [ctx.config.width as f32, ctx.config.height as f32], progress: self.current_progress, _pad: 0.0 };
        ctx.queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));
        
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None, color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &view, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store } })],
                depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.draw(0..4, 0..1);
        }
        ctx.queue.submit(std::iter::once(encoder.finish()));
        output.present();
    }
}

fn set_cursor_grab(window: &Window, grabbed: bool) {
    if grabbed { let _ = window.set_cursor_grab(CursorGrabMode::Confined).or_else(|_| window.set_cursor_grab(CursorGrabMode::Locked)); window.set_cursor_visible(false); } 
    else { let _ = window.set_cursor_grab(CursorGrabMode::None); window.set_cursor_visible(true); }
}

fn main() {
    env_logger::init();
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);
    
    let builder = WindowBuilder::new().with_title(config::WINDOW_TITLE);
    let monitor = event_loop.primary_monitor();
    let window = std::sync::Arc::new(builder.with_fullscreen(Some(Fullscreen::Borderless(monitor))).build(&event_loop).unwrap());
    
    // "Floating" Context: Starts here, then moves into GameState
    let mut gpu_ctx_opt = Some(pollster::block_on(GpuContext::new(window.clone())));
    let mut loading_screen = LoadingScreen::new(gpu_ctx_opt.as_ref().unwrap());

    // Threading
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let chunks = map_loader::load_chunks_from_osm(config::MAP_FILE_PATH);
        let total = chunks.len();
        // Emulate streaming/processing delay
        for (i, chunk) in chunks.into_iter().enumerate() {
            tx.send(LoaderMessage::ChunkLoaded(chunk)).ok();
            if i % 5 == 0 {
                 let _ = tx.send(LoaderMessage::Progress((i + 1) as f32 / total as f32));
                 // Tiny sleep to make the loading screen visible for the demo
                 thread::sleep(std::time::Duration::from_millis(5)); 
            }
        }
        let _ = tx.send(LoaderMessage::Done);
    });

    let mut state: Option<GameState> = None;
    let mut is_loading_phase = true; // PRODUCTION FIX: Separate Flag
    let mut last_fps_print = Instant::now();
    let mut frames = 0;
    
    set_cursor_grab(&window, false);

    event_loop.run(move |event, elwt| {
        match event {
            Event::WindowEvent { ref event, window_id } if window_id == window.id() => {
                match event {
                    WindowEvent::CloseRequested => elwt.exit(),
                    WindowEvent::Resized(size) => {
                        // Resize whoever currently holds the context
                        if let Some(s) = &mut state { s.resize(*size); }
                        else if let Some(ctx) = &mut gpu_ctx_opt { ctx.resize(*size); }
                    },
                    WindowEvent::RedrawRequested => {
                        if is_loading_phase {
                            // Render Loading Screen
                            if let Some(s) = &mut state {
                                loading_screen.render(&mut s.ctx);
                            } else if let Some(ctx) = &mut gpu_ctx_opt {
                                loading_screen.render(ctx);
                            }
                        } else if let Some(s) = &mut state {
                            // Render Game
                            s.update();
                            match s.render() {
                                Ok(_) => {}
                                Err(wgpu::SurfaceError::Lost) => s.resize(s.ctx.size),
                                Err(wgpu::SurfaceError::OutOfMemory) => elwt.exit(),
                                Err(e) => eprintln!("Render Error: {:?}", e),
                            }
                        }
                    },
                    // Input Handling (Only if game is active)
                    WindowEvent::MouseInput { state: element_state, button: MouseButton::Left, .. } if !is_loading_phase => {
                        if *element_state == ElementState::Pressed { 
                            if let Some(s) = &mut state { s.mouse_captured = true; }
                            set_cursor_grab(&window, true); 
                        }
                    },
                    WindowEvent::KeyboardInput { event: KeyEvent { physical_key: PhysicalKey::Code(KeyCode::Escape), state: element_state, .. }, .. } => {
                        if *element_state == ElementState::Pressed { 
                             if let Some(s) = &mut state { s.mouse_captured = false; }
                             set_cursor_grab(&window, false); 
                        }
                    },
                    _ => {
                        // Pass other input to GameState
                        if !is_loading_phase {
                            if let Some(s) = &mut state { s.input(event); }
                        }
                    }
                }
            },
            Event::DeviceEvent { event: DeviceEvent::MouseMotion { delta }, .. } => {
                if !is_loading_phase {
                     if let Some(s) = &mut state { s.update_camera_rotation(delta); }
                }
            },
            Event::AboutToWait => {
                // 1. Process Message Queue (Non-blocking)
                while let Ok(msg) = rx.try_recv() {
                    match msg {
                        LoaderMessage::Progress(p) => {
                            loading_screen.current_progress = p;
                            window.request_redraw();
                        },
                        LoaderMessage::ChunkLoaded(data) => {
                            // Initialize GameState on first chunk if needed
                            if state.is_none() {
                                if let Some(ctx) = gpu_ctx_opt.take() {
                                    state = Some(GameState::new(ctx));
                                }
                            }
                            // Insert data
                            if let Some(s) = &mut state {
                                s.world.insert_chunk(&s.ctx.device, data);
                            }
                        },
                        LoaderMessage::Done => {
                            loading_screen.current_progress = 1.0;
                            // Ensure state is created even if no chunks loaded (edge case)
                            if state.is_none() {
                                if let Some(ctx) = gpu_ctx_opt.take() { state = Some(GameState::new(ctx)); }
                            }
                            is_loading_phase = false; // SWITCH TO GAME
                            window.request_redraw();
                        }
                    }
                }

                // 2. Loop Management
                if !is_loading_phase {
                    frames += 1;
                    if last_fps_print.elapsed().as_secs_f32() >= 1.0 {
                        window.set_title(&format!("{} | FPS: {}", config::WINDOW_TITLE, frames));
                        frames = 0;
                        last_fps_print = Instant::now();
                    }
                    window.request_redraw();
                } else {
                    // Reduce CPU usage slightly during loading
                    thread::sleep(std::time::Duration::from_millis(5));
                }
            },
            _ => {}
        }
    }).unwrap();
}