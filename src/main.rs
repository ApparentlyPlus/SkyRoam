// main.rs
use winit::{
    event::*, event_loop::{ControlFlow, EventLoop}, window::{WindowBuilder, CursorGrabMode, Fullscreen, Window},
    keyboard::{KeyCode, PhysicalKey},
};
use wgpu::util::DeviceExt;
use std::time::Instant;
use std::sync::mpsc;
use std::thread;
use std::sync::Arc;

mod config;
mod shader;
mod vertex;
mod camera;
mod world;
mod map_loader;
mod state;

use state::{GameState, GpuContext};
use world::LoaderMessage;

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
    pub status_text: String,
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
        Self { pipeline, uniform_buffer, bind_group, current_progress: 0.0, status_text: "Initializing".into() }
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
    let window = Arc::new(builder.with_fullscreen(Some(Fullscreen::Borderless(monitor))).build(&event_loop).unwrap());
    
    let mut gpu_ctx_opt = Some(pollster::block_on(GpuContext::new(window.clone())));
    let mut loading_screen = LoadingScreen::new(gpu_ctx_opt.as_ref().unwrap());

    // Threading setup
    let (tx, rx) = mpsc::channel();
    // Clone for the thread
    let tx_thread = tx.clone();
    
    thread::spawn(move || {
        // Clone for the callback closure inside the thread
        let tx_callback = tx_thread.clone();
        
        map_loader::load_chunks_from_osm_stream(config::MAP_FILE_PATH, move |chunk_batch_opt, progress, status| {
             if let Some(batch) = chunk_batch_opt {
                 tx_callback.send(LoaderMessage::BatchLoaded(batch)).ok();
             }
             if progress > 0.0 {
                tx_callback.send(LoaderMessage::Progress(progress)).ok();
             }
             tx_callback.send(LoaderMessage::Status(status.to_string())).ok();
        });
        
        // Use the thread's copy of tx for the final signal
        tx_thread.send(LoaderMessage::Done).ok();
    });

    let mut state: Option<GameState> = None;
    let mut is_loading_phase = true;
    let mut last_fps_print = Instant::now();
    let mut frames = 0;
    
    set_cursor_grab(&window, false);

    event_loop.run(move |event, elwt| {
        match event {
            Event::WindowEvent { ref event, window_id } if window_id == window.id() => {
                match event {
                    WindowEvent::CloseRequested => elwt.exit(),
                    WindowEvent::Resized(size) => {
                        if let Some(s) = &mut state { s.resize(*size); }
                        else if let Some(ctx) = &mut gpu_ctx_opt { ctx.resize(*size); }
                    },
                    WindowEvent::RedrawRequested => {
                        if is_loading_phase {
                            if let Some(s) = &mut state {
                                loading_screen.render(&mut s.ctx);
                            } else if let Some(ctx) = &mut gpu_ctx_opt {
                                loading_screen.render(ctx);
                            }
                        } else if let Some(s) = &mut state {
                            s.update();
                            match s.render() {
                                Ok(_) => {}
                                Err(wgpu::SurfaceError::Lost) => s.resize(s.ctx.size),
                                Err(wgpu::SurfaceError::OutOfMemory) => elwt.exit(),
                                Err(e) => eprintln!("Render Error: {:?}", e),
                            }
                        }
                    },
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
                let mut chunk_loaded = false;
                while let Ok(msg) = rx.try_recv() {
                    match msg {
                        LoaderMessage::Status(s) => {
                            loading_screen.status_text = s;
                            window.request_redraw(); 
                        },
                        LoaderMessage::Progress(p) => {
                            loading_screen.current_progress = p;
                            window.request_redraw();
                        },
                        LoaderMessage::BatchLoaded(batch) => {
                            // Init State on first chunk batch
                            if state.is_none() {
                                if let Some(ctx) = gpu_ctx_opt.take() {
                                    state = Some(GameState::new(ctx));
                                }
                                is_loading_phase = false;
                                set_cursor_grab(&window, true);
                            }
                            if let Some(s) = &mut state {
                                for chunk in batch {
                                    s.world.insert_chunk(&s.ctx.device, chunk);
                                }
                            }
                            chunk_loaded = true;
                        },
                        LoaderMessage::Done => {
                            loading_screen.current_progress = 1.0;
                            loading_screen.status_text = "Done".into();
                            if state.is_none() {
                                if let Some(ctx) = gpu_ctx_opt.take() { state = Some(GameState::new(ctx)); }
                            }
                            is_loading_phase = false;
                        }
                    }
                }
                
                if chunk_loaded || !is_loading_phase {
                     window.request_redraw();
                }

                if !is_loading_phase {
                    frames += 1;
                    if last_fps_print.elapsed().as_secs_f32() >= 1.0 {
                        let chunk_count = state.as_ref().map(|s| s.world.chunks.len()).unwrap_or(0);
                        let cam_y = state.as_ref().map(|s| s.camera.eye.y).unwrap_or(0.0);
                        window.set_title(&format!("{} | FPS: {} | Chunks: {} | Y: {:.1}", config::WINDOW_TITLE, frames, chunk_count, cam_y));
                        frames = 0;
                        last_fps_print = Instant::now();
                    }
                } else {
                    thread::sleep(std::time::Duration::from_millis(5));
                }
            },
            _ => {}
        }
    }).unwrap();
}