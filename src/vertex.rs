#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
}

// Just a cube
pub const VERTICES: &[Vertex] = &[
    // Front (Z+)
    Vertex { position: [-0.5, -0.5, 0.5], normal: [0.0, 0.0, 1.0] }, Vertex { position: [0.5, -0.5, 0.5], normal: [0.0, 0.0, 1.0] }, 
    Vertex { position: [0.5, 0.5, 0.5], normal: [0.0, 0.0, 1.0] },   Vertex { position: [-0.5, 0.5, 0.5], normal: [0.0, 0.0, 1.0] },
    // Back (Z-)
    Vertex { position: [-0.5, -0.5, -0.5], normal: [0.0, 0.0, -1.0] }, Vertex { position: [-0.5, 0.5, -0.5], normal: [0.0, 0.0, -1.0] }, 
    Vertex { position: [0.5, 0.5, -0.5], normal: [0.0, 0.0, -1.0] },   Vertex { position: [0.5, -0.5, -0.5], normal: [0.0, 0.0, -1.0] },
    // Top (Y+)
    Vertex { position: [-0.5, 0.5, -0.5], normal: [0.0, 1.0, 0.0] }, Vertex { position: [-0.5, 0.5, 0.5], normal: [0.0, 1.0, 0.0] }, 
    Vertex { position: [0.5, 0.5, 0.5], normal: [0.0, 1.0, 0.0] },   Vertex { position: [0.5, 0.5, -0.5], normal: [0.0, 1.0, 0.0] },
    // Bottom (Y-)
    Vertex { position: [-0.5, -0.5, -0.5], normal: [0.0, -1.0, 0.0] }, Vertex { position: [0.5, -0.5, -0.5], normal: [0.0, -1.0, 0.0] }, 
    Vertex { position: [0.5, -0.5, 0.5], normal: [0.0, -1.0, 0.0] },   Vertex { position: [-0.5, -0.5, 0.5], normal: [0.0, -1.0, 0.0] },
    // Right (X+)
    Vertex { position: [0.5, -0.5, -0.5], normal: [1.0, 0.0, 0.0] }, Vertex { position: [0.5, 0.5, -0.5], normal: [1.0, 0.0, 0.0] }, 
    Vertex { position: [0.5, 0.5, 0.5], normal: [1.0, 0.0, 0.0] },   Vertex { position: [0.5, -0.5, 0.5], normal: [1.0, 0.0, 0.0] },
    // Left (X-)
    Vertex { position: [-0.5, -0.5, -0.5], normal: [-1.0, 0.0, 0.0] }, Vertex { position: [-0.5, -0.5, 0.5], normal: [-1.0, 0.0, 0.0] }, 
    Vertex { position: [-0.5, 0.5, 0.5], normal: [-1.0, 0.0, 0.0] },   Vertex { position: [-0.5, 0.5, -0.5], normal: [-1.0, 0.0, 0.0] },
];

pub const INDICES: &[u16] = &[
    0, 1, 2, 2, 3, 0, 4, 5, 6, 6, 7, 4, 8, 9, 10, 10, 11, 8, 
    12, 13, 14, 14, 15, 12, 16, 17, 18, 18, 19, 16, 20, 21, 22, 22, 23, 20,
];

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct InstanceRaw {
    pub pos: [f32; 3],
    pub scale: [f32; 3],
    pub color_val: f32, 
}

pub struct Instance {
    pub position: glam::Vec3,
    pub scale: glam::Vec3,
    pub color_val: f32,
}

impl Instance {
    pub fn to_raw(&self) -> InstanceRaw {
        InstanceRaw {
            pos: self.position.into(),
            scale: self.scale.into(),
            color_val: self.color_val,
        }
    }
}