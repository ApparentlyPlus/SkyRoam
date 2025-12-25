use std::collections::HashMap;
use rand::{Rng, SeedableRng, rngs::StdRng};
use crate::vertex::{Instance, InstanceRaw};

pub const CHUNK_SIZE: f32 = 150.0;
pub const RENDER_DISTANCE_CHUNKS: i32 = 20; 
pub const RENDER_DISTANCE_WORLD: f32 = RENDER_DISTANCE_CHUNKS as f32 * CHUNK_SIZE;

pub struct CollisionBox { 
    pub min: glam::Vec3, 
    pub max: glam::Vec3 
}

pub struct World {
    pub chunks: HashMap<(i32, i32), Vec<InstanceRaw>>,
    pub collision_map: HashMap<(i32, i32), CollisionBox>,
    pub global_instances: Vec<InstanceRaw>,
}

impl World {
    pub fn generate() -> Self {
        let mut chunks = HashMap::new();
        let mut collision_map = HashMap::new();
        let mut rng = StdRng::seed_from_u64(42);
        
        let spread = 25.0; 
        let render_radius = 250;
        
        for x in -render_radius..render_radius {
            for z in -render_radius..render_radius {
                if x % 6 == 0 || z % 6 == 0 { continue; }

                let w = rng.gen_range(10.0..22.0);
                let d = rng.gen_range(10.0..22.0);
                let h = rng.gen_range(20.0..150.0);
                
                let px = x as f32 * spread;
                let pz = z as f32 * spread;
                let position = glam::Vec3::new(px, h / 2.0, pz);
                
                let base_color = 0.08;
                let variation = rng.gen_range(0.8..1.2); 
                let color_val = base_color * variation;

                let instance = Instance { position, scale: glam::Vec3::new(w, h, d), color_val };

                let half_w = w / 2.0;
                let half_d = d / 2.0;
                let min = glam::Vec3::new(position.x - half_w, 0.0, position.z - half_d);
                let max = glam::Vec3::new(position.x + half_w, h, position.z + half_d);
                
                collision_map.insert((x, z), CollisionBox { min, max });

                let chunk_x = (px / CHUNK_SIZE).floor() as i32;
                let chunk_z = (pz / CHUNK_SIZE).floor() as i32;
                chunks.entry((chunk_x, chunk_z)).or_insert_with(Vec::new).push(instance.to_raw());
            }
        }

        let mut global_instances = Vec::new();
        global_instances.push(Instance { position: glam::Vec3::new(0.0, -1.0, 0.0), scale: glam::Vec3::new(10000.0, 1.0, 10000.0), color_val: 0.029 }.to_raw());
        global_instances.push(Instance { position: glam::Vec3::ZERO, scale: glam::Vec3::new(-6000.0, -6000.0, -6000.0), color_val: 0.0 }.to_raw());

        Self { chunks, collision_map, global_instances }
    }
}