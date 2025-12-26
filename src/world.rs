// world.rs
use std::collections::HashMap;
use crate::{config, vertex::Vertex};

pub enum LoaderMessage {
    Status(String),
    Progress(f32),
    BatchLoaded(Vec<ChunkData>),
    Done,
}

#[derive(Debug, Clone, Copy)]
pub struct WallCollider {
    pub start: glam::Vec2,
    pub end: glam::Vec2,
    pub height: f32,
    pub min_x: f32, pub max_x: f32,
    pub min_z: f32, pub max_z: f32,
}

#[derive(Clone)]
pub struct ChunkData {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub walls: Vec<WallCollider>,
    pub coord: (i32, i32),
}

pub struct LocalCollisionGrid {
    pub cells: Vec<Vec<WallCollider>>,
    pub cell_size: f32,
    pub grid_dim: usize,
    pub chunk_offset: glam::Vec2,
}

impl LocalCollisionGrid {
    pub fn new(walls: &[WallCollider], chunk_offset: glam::Vec2) -> Self {
        let cell_size = config::PHYSICS_GRID_CELL_SIZE;
        let grid_dim = (config::CHUNK_SIZE / cell_size).ceil() as usize;
        let mut cells = vec![Vec::new(); grid_dim * grid_dim];

        for wall in walls {
            let local_min_x = wall.min_x - chunk_offset.x;
            let local_max_x = wall.max_x - chunk_offset.x;
            let local_min_z = wall.min_z - chunk_offset.y;
            let local_max_z = wall.max_z - chunk_offset.y;

            let min_gx = (local_min_x / cell_size).floor() as i32;
            let max_gx = (local_max_x / cell_size).floor() as i32;
            let min_gz = (local_min_z / cell_size).floor() as i32;
            let max_gz = (local_max_z / cell_size).floor() as i32;

            for gx in min_gx..=max_gx {
                for gz in min_gz..=max_gz {
                    if gx >= 0 && gx < grid_dim as i32 && gz >= 0 && gz < grid_dim as i32 {
                        let idx = (gz as usize) * grid_dim + (gx as usize);
                        cells[idx].push(*wall);
                    }
                }
            }
        }
        Self { cells, cell_size, grid_dim, chunk_offset }
    }

    pub fn get_walls(&self, x: f32, z: f32) -> Option<&Vec<WallCollider>> {
        let lx = x - self.chunk_offset.x;
        let lz = z - self.chunk_offset.y;
        if lx < 0.0 || lz < 0.0 { return None; }
        
        let gx = (lx / self.cell_size).floor() as i32;
        let gz = (lz / self.cell_size).floor() as i32;

        if gx >= 0 && gx < self.grid_dim as i32 && gz >= 0 && gz < self.grid_dim as i32 {
            Some(&self.cells[(gz as usize) * self.grid_dim + (gx as usize)])
        } else {
            None
        }
    }
}

pub struct Chunk {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub index_count: u32,
    pub collision: LocalCollisionGrid,
    pub min: glam::Vec2,
    pub max: glam::Vec2,
}

pub struct World {
    pub chunks: HashMap<(i32, i32), Chunk>,
}

impl World {
    pub fn new() -> Self {
        Self { chunks: HashMap::new() }
    }

    pub fn insert_chunk(&mut self, device: &wgpu::Device, data: ChunkData) {
        use wgpu::util::DeviceExt;
        
        // Don't upload empty chunks
        if data.indices.is_empty() { return; }

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("Chunk {:?} V", data.coord)),
            contents: bytemuck::cast_slice(&data.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("Chunk {:?} I", data.coord)),
            contents: bytemuck::cast_slice(&data.indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        
        let cx = data.coord.0 as f32 * config::CHUNK_SIZE - (config::WORLD_SIZE / 2.0);
        let cz = data.coord.1 as f32 * config::CHUNK_SIZE - (config::WORLD_SIZE / 2.0);
        let offset = glam::Vec2::new(cx, cz);

        let chunk = Chunk {
            vertex_buffer, index_buffer,
            index_count: data.indices.len() as u32,
            collision: LocalCollisionGrid::new(&data.walls, offset),
            min: offset,
            max: offset + glam::Vec2::splat(config::CHUNK_SIZE),
        };
        self.chunks.insert(data.coord, chunk);
    }
}