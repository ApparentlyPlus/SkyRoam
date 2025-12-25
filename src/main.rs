use winit::{
    event::*,
    event_loop::{ControlFlow, EventLoop},
    window::{WindowBuilder, CursorGrabMode, Fullscreen, Window},
    keyboard::{KeyCode, PhysicalKey},
};
use std::time::Instant;

mod shader;
mod vertex;
mod camera;
mod world;
mod state;
use state::State;

// Helper to handle the "messy" grab logic explicitly
fn set_cursor_grab(window: &Window, grabbed: bool) {
    if grabbed {
        if window.set_cursor_grab(CursorGrabMode::Confined).is_err() {
            // Fallback if Confined fails
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
    
    // Initial State
    set_cursor_grab(&window, false);

    let mut state = pollster::block_on(State::new(window.clone()));
    let mut last_fps_print = Instant::now();
    let mut frames = 0;

    event_loop.run(move |event, elwt| {
        match event {
            Event::WindowEvent { ref event, window_id } if window_id == window.id() => {
                // Pass input to state first
                if !state.input(event) {
                    match event {
                        WindowEvent::CloseRequested => elwt.exit(),
                        WindowEvent::Resized(physical_size) => state.resize(*physical_size),
                        WindowEvent::RedrawRequested => {
                            state.update();
                            match state.render() {
                                Ok(_) => {}
                                Err(wgpu::SurfaceError::Lost) => state.resize(state.size),
                                Err(wgpu::SurfaceError::OutOfMemory) => elwt.exit(),
                                Err(e) => eprintln!("Render error: {:?}", e),
                            }
                        }
                        // Explicit Mouse Capture Logic
                        WindowEvent::MouseInput { state: element_state, button: MouseButton::Left, .. } => {
                            if *element_state == ElementState::Pressed {
                                state.mouse_captured = true;
                                set_cursor_grab(&window, true);
                            }
                        },
                        WindowEvent::KeyboardInput { event: KeyEvent { physical_key: PhysicalKey::Code(KeyCode::Escape), state: element_state, .. }, .. } => {
                            if *element_state == ElementState::Pressed {
                                state.mouse_captured = false;
                                set_cursor_grab(&window, false);
                            }
                        },
                        _ => {}
                    }
                }
            },
            Event::DeviceEvent { event: DeviceEvent::MouseMotion { delta }, .. } => {
                state.update_camera_rotation(delta);
            },
            Event::AboutToWait => {
                // FPS Counter
                frames += 1;
                if last_fps_print.elapsed().as_secs_f32() >= 1.0 {
                    window.set_title(&format!("Blazing Mapbox | FPS: {}", frames));
                    frames = 0;
                    last_fps_print = Instant::now();
                }
                window.request_redraw();
            },
            _ => {}
        }
    }).unwrap();
}