use winit::event::*;
use winit::keyboard::{KeyCode, PhysicalKey};

pub struct Camera {
    pub eye: glam::Vec3,
    pub velocity: glam::Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub aspect: f32,
}

impl Camera {
    pub fn build_view_projection_matrix(&self) -> glam::Mat4 {
        let (sin_pitch, cos_pitch) = self.pitch.sin_cos();
        let (sin_yaw, cos_yaw) = self.yaw.sin_cos();
        let target = glam::Vec3::new(cos_pitch * cos_yaw, sin_pitch, cos_pitch * sin_yaw).normalize();
        let view = glam::Mat4::look_at_rh(self.eye, self.eye + target, glam::Vec3::Y);
        let proj = glam::Mat4::perspective_rh(45.0f32.to_radians(), self.aspect, 0.1, 8000.0);
        proj * view
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