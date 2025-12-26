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

#[derive(Debug, Clone, Copy)]
pub struct Plane {
    pub normal: glam::Vec3,
    pub distance: f32,
}

impl Plane {
    pub fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        let normal = glam::Vec3::new(x, y, z);
        let length = normal.length();
        // Normalize the plane equation so distance checks are in meters
        Self {
            normal: normal / length,
            distance: w / length,
        }
    }

    /// Returns signed distance from plane to point. Positive = inside/front, Negative = outside/back.
    pub fn distance_to_point(&self, point: glam::Vec3) -> f32 {
        self.normal.dot(point) + self.distance
    }
}

pub struct Frustum {
    pub planes: [Plane; 6],
}

impl Frustum {
    /// Extracts frustum planes from a View-Projection matrix.
    /// This works for the standard glam::perspective_rh depth range (-1 to 1).
    pub fn from_mat4(m: glam::Mat4) -> Self {
        // Extract rows for clearer access (Gribb-Hartmann extraction)
        let row0 = m.row(0);
        let row1 = m.row(1);
        let row2 = m.row(2);
        let row3 = m.row(3);

        let planes = [
            // Left
            Plane::new(
                row3.x + row0.x,
                row3.y + row0.y,
                row3.z + row0.z,
                row3.w + row0.w,
            ),
            // Right
            Plane::new(
                row3.x - row0.x,
                row3.y - row0.y,
                row3.z - row0.z,
                row3.w - row0.w,
            ),
            // Bottom
            Plane::new(
                row3.x + row1.x,
                row3.y + row1.y,
                row3.z + row1.z,
                row3.w + row1.w,
            ),
            // Top
            Plane::new(
                row3.x - row1.x,
                row3.y - row1.y,
                row3.z - row1.z,
                row3.w - row1.w,
            ),
            // Near (Z > -1)
            Plane::new(
                row3.x + row2.x,
                row3.y + row2.y,
                row3.z + row2.z,
                row3.w + row2.w,
            ),
            // Far (Z < 1)
            Plane::new(
                row3.x - row2.x,
                row3.y - row2.y,
                row3.z - row2.z,
                row3.w - row2.w,
            ),
        ];

        Self { planes }
    }

    /// Checks if an AABB (Axis Aligned Bounding Box) is inside the frustum.
    /// Uses the "Positive Vertex" optimization for fast rejection.
    pub fn intersects_aabb(&self, min: &glam::Vec3, max: &glam::Vec3) -> bool {
        for plane in &self.planes {
            // Find the "positive vertex" (the corner most aligned with the normal)
            let p_vertex = glam::Vec3::new(
                if plane.normal.x >= 0.0 { max.x } else { min.x },
                if plane.normal.y >= 0.0 { max.y } else { min.y },
                if plane.normal.z >= 0.0 { max.z } else { min.z },
            );

            // If the "positive" corner is behind the plane, the whole box is outside
            if plane.distance_to_point(p_vertex) < 0.0 {
                return false;
            }
        }
        true
    }
}