// camera.rs
use glam::{DMat4, DVec3, Mat4, Vec3};
use winit::event::*;
use winit::keyboard::{KeyCode, PhysicalKey};
use crate::config;

#[derive(Debug)]
pub struct Camera {
    pub eye: DVec3,
    pub yaw: f32,
    pub pitch: f32,
    pub aspect: f32,
}

impl Camera {
    pub fn new(aspect: f32) -> Self {
        Self {
            eye: DVec3::new(0.0, 50.0, 0.0),
            yaw: -90.0f32.to_radians(),
            pitch: 0.0,
            aspect,
        }
    }

    pub fn build_view_projection_matrix(&self) -> Mat4 {
        let (sin_pitch, cos_pitch) = self.pitch.sin_cos();
        let (sin_yaw, cos_yaw) = self.yaw.sin_cos();

        // Calculate target direction
        let target = DVec3::new(
            (cos_pitch * cos_yaw) as f64,
            sin_pitch as f64,
            (cos_pitch * sin_yaw) as f64,
        ).normalize();

        // View Matrix (High Precision)
        let view = DMat4::look_at_rh(self.eye, self.eye + target, DVec3::Y);
        
        // Projection Matrix (Standard f32 is sufficient)
        let proj = Mat4::perspective_rh(
            config::FOV_Y.to_radians(),
            self.aspect,
            config::Z_NEAR,
            config::Z_FAR,
        );

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

// Frustum Culling

#[derive(Debug, Clone, Copy)]
struct Plane {
    normal: Vec3,
    distance: f32,
}

impl Plane {
    fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        let normal = Vec3::new(x, y, z);
        let length_inv = 1.0 / normal.length();
        Self {
            normal: normal * length_inv,
            distance: w * length_inv,
        }
    }

    fn distance_to_point(&self, point: Vec3) -> f32 {
        self.normal.dot(point) + self.distance
    }
}

pub struct Frustum {
    planes: [Plane; 6],
}

impl Frustum {
    pub fn from_mat4(m: Mat4) -> Self {
        let row0 = m.row(0);
        let row1 = m.row(1);
        let row2 = m.row(2);
        let row3 = m.row(3);

        Self {
            planes: [
                Plane::new(row3.x + row0.x, row3.y + row0.y, row3.z + row0.z, row3.w + row0.w), // Left
                Plane::new(row3.x - row0.x, row3.y - row0.y, row3.z - row0.z, row3.w - row0.w), // Right
                Plane::new(row3.x + row1.x, row3.y + row1.y, row3.z + row1.z, row3.w + row1.w), // Bottom
                Plane::new(row3.x - row1.x, row3.y - row1.y, row3.z - row1.z, row3.w - row1.w), // Top
                Plane::new(row3.x + row2.x, row3.y + row2.y, row3.z + row2.z, row3.w + row2.w), // Near
                Plane::new(row3.x - row2.x, row3.y - row2.y, row3.z - row2.z, row3.w - row2.w), // Far
            ]
        }
    }

    pub fn intersects_aabb(&self, min: &Vec3, max: &Vec3) -> bool {
        for plane in &self.planes {
            let p_vertex = Vec3::new(
                if plane.normal.x >= 0.0 { max.x } else { min.x },
                if plane.normal.y >= 0.0 { max.y } else { min.y },
                if plane.normal.z >= 0.0 { max.z } else { min.z },
            );
            if plane.distance_to_point(p_vertex) < 0.0 {
                return false;
            }
        }
        true
    }
}