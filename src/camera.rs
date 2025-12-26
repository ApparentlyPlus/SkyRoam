use winit::event::*;
use winit::keyboard::{KeyCode, PhysicalKey};

// Use DVec3 (f64) for position to prevent jitter at large world coordinates
pub struct Camera {
    pub eye: glam::DVec3,
    pub velocity: glam::DVec3,
    pub yaw: f32,
    pub pitch: f32,
    pub aspect: f32,
}

impl Camera {
    pub fn build_view_projection_matrix(&self) -> glam::Mat4 {
        let (sin_pitch, cos_pitch) = self.pitch.sin_cos();
        let (sin_yaw, cos_yaw) = self.yaw.sin_cos();

        // Calculate target in f64 then downcast for the matrix creation if needed, 
        // or keep high precision for the LookAt calculation.
        let target = glam::DVec3::new(
            (cos_pitch * cos_yaw) as f64, 
            sin_pitch as f64, 
            (cos_pitch * sin_yaw) as f64
        ).normalize();

        // We calculate the View Matrix in f64 first
        let view = glam::DMat4::look_at_rh(self.eye, self.eye + target, glam::DVec3::Y);
        
        // Perspective is usually fine in f32
        let proj = glam::Mat4::perspective_rh(45.0f32.to_radians(), self.aspect, 0.1, 10000.0);

        // Convert View to f32 and multiply
        proj * view.as_mat4()
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraUniform {
    pub view_proj: [[f32; 4]; 4], 
    pub screen_size: [f32; 2],    
    pub fog_dist: [f32; 2], 
    pub camera_pos: [f32; 4],      
}

pub struct CameraController {
    pub move_fwd: bool,
    pub move_back: bool,
    pub move_left: bool,
    pub move_right: bool,
    pub jump: bool,
}

impl CameraController {
    pub fn new() -> Self {
        Self { move_fwd: false, move_back: false, move_left: false, move_right: false, jump: false }
    }

    pub fn process_events(&mut self, event: &WindowEvent) -> bool {
        match event {
            WindowEvent::KeyboardInput {
                event: KeyEvent { physical_key: PhysicalKey::Code(key), state, .. }, ..
            } => {
                let pressed = *state == ElementState::Pressed;
                match key {
                    KeyCode::KeyW => { self.move_fwd = pressed; true }
                    KeyCode::KeyS => { self.move_back = pressed; true }
                    KeyCode::KeyA => { self.move_left = pressed; true }
                    KeyCode::KeyD => { self.move_right = pressed; true }
                    KeyCode::Space => { self.jump = pressed; true }
                    _ => false,
                }
            }
            _ => false,
        }
    }
}