// world.rs
use std::fs::File;
use std::io::BufReader;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::mpsc::Sender;
use crate::config;
use crate::vertex::Vertex;

pub enum LoaderMessage {
    Progress(f32),
    Done(World),
}

#[derive(Debug, Clone)]
pub struct WallCollider {
    pub start: glam::Vec2,
    pub end: glam::Vec2,
    pub height: f32,
    pub min_x: f32, pub max_x: f32,
    pub min_z: f32, pub max_z: f32,
}

#[derive(Debug)]
pub struct ChunkView {
    pub index_start: u32,
    pub index_count: u32,
    pub min: glam::Vec2,
    pub max: glam::Vec2,
}

/// A spatial grid optimization for collision.
/// Instead of a HashMap, we use a 1D vector mapped to 2D coordinates.
pub struct CollisionGrid {
    pub cells: Vec<Vec<WallCollider>>,
    pub width: usize,
    pub height: usize,
    pub cell_size: f32,
    pub offset_x: f32,
    pub offset_z: f32,
}

impl CollisionGrid {
    pub fn new(world_size: f32, cell_size: f32) -> Self {
        let dim = (world_size / cell_size).ceil() as usize + 2; // +2 for padding
        Self {
            cells: vec![Vec::new(); dim * dim],
            width: dim,
            height: dim,
            cell_size,
            offset_x: world_size * 0.5,
            offset_z: world_size * 0.5,
        }
    }

    /// O(1) Access to collision cell
    pub fn get_cell(&self, x: f32, z: f32) -> Option<&Vec<WallCollider>> {
        let gx = ((x + self.offset_x) / self.cell_size).floor() as i32;
        let gz = ((z + self.offset_z) / self.cell_size).floor() as i32;

        if gx >= 0 && gx < self.width as i32 && gz >= 0 && gz < self.height as i32 {
            Some(&self.cells[(gz as usize) * self.width + (gx as usize)])
        } else {
            None
        }
    }

    pub fn insert(&mut self, wall: WallCollider) {
        let min_gx = ((wall.min_x + self.offset_x) / self.cell_size).floor() as i32;
        let max_gx = ((wall.max_x + self.offset_x) / self.cell_size).floor() as i32;
        let min_gz = ((wall.min_z + self.offset_z) / self.cell_size).floor() as i32;
        let max_gz = ((wall.max_z + self.offset_z) / self.cell_size).floor() as i32;

        for gx in min_gx..=max_gx {
            for gz in min_gz..=max_gz {
                if gx >= 0 && gx < self.width as i32 && gz >= 0 && gz < self.height as i32 {
                    let idx = (gz as usize) * self.width + (gx as usize);
                    self.cells[idx].push(wall.clone());
                }
            }
        }
    }
}

pub struct World {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub chunks: Vec<ChunkView>,
    pub collision: CollisionGrid,
}

// Internal OSM structs
#[derive(Deserialize)]
struct OsmResponse { elements: Vec<OsmElement> }
#[derive(Deserialize)]
struct OsmElement {
    #[serde(default)] id: u64,
    #[serde(rename = "type")] e_type: String,
    #[serde(default)] nodes: Vec<u64>,
    #[serde(default)] lat: f64,
    #[serde(default)] lon: f64,
    #[serde(default)] tags: Option<HashMap<String, String>>,
}

impl World {
    pub fn generate(tx: Sender<LoaderMessage>) -> Self {
        let _ = tx.send(LoaderMessage::Progress(0.01));

        // Coordinate Conversion Data
        let lat_rad = config::ORIGIN_LAT.to_radians();
        let meters_per_deg_lat = 111132.0;
        let meters_per_deg_lon = 111319.5 * lat_rad.cos();

        // Initialize Containers
        let mut collision = CollisionGrid::new(config::WORLD_SIZE, config::PHYSICS_GRID_CELL_SIZE);
        let mut chunk_builders: Vec<(Vec<Vertex>, Vec<u32>)> = 
            (0..(config::CHUNKS_AXIS * config::CHUNKS_AXIS)).map(|_| (Vec::new(), Vec::new())).collect();

        // Load File
        let file = match File::open(config::MAP_FILE_PATH) {
            Ok(f) => f,
            Err(_) => {
                eprintln!("Map file not found: {}", config::MAP_FILE_PATH);
                return Self { vertices: vec![], indices: vec![], chunks: vec![], collision };
            }
        };
        
        let reader = BufReader::new(file);
        let osm_data: OsmResponse = serde_json::from_reader(reader).unwrap_or(OsmResponse { elements: vec![] });
        let _ = tx.send(LoaderMessage::Progress(0.15));

        // Map Node IDs to Positions
        let mut node_map: HashMap<u64, glam::Vec2> = HashMap::with_capacity(osm_data.elements.len());
        for el in &osm_data.elements {
            if el.e_type == "node" {
                let x = (el.lon - config::ORIGIN_LON) * meters_per_deg_lon;
                let z = -(el.lat - config::ORIGIN_LAT) * meters_per_deg_lat;
                node_map.insert(el.id, glam::Vec2::new(x as f32, z as f32));
            }
        }

        let total_elements = osm_data.elements.len();
        let mut last_percent = 0;

        // Process Ways (Buildings)
        for (i, el) in osm_data.elements.iter().enumerate() {
            // Loading Progress
            if i % 1000 == 0 {
                let percent = ((i as f32 / total_elements as f32) * 100.0) as i32;
                if percent > last_percent {
                    last_percent = percent;
                    let p = 0.15 + (percent as f32 / 100.0) * 0.85;
                    let _ = tx.send(LoaderMessage::Progress(p));
                }
            }

            if el.e_type == "way" && el.tags.as_ref().map_or(false, |t| t.contains_key("building")) {
                let tags = el.tags.as_ref().unwrap();

                // Height Heuristics
                let height: f32 = if let Some(h) = tags.get("height").and_then(|s| s.trim_matches(|c: char| !c.is_numeric() && c != '.').parse().ok()) {
                    h
                } else if let Some(l) = tags.get("building:levels").and_then(|s| s.parse::<f32>().ok()) {
                    l * 4.0
                } else {
                    8.0 + ((el.id % 100) as f32 * 0.3)
                };

                // Color Heuristics (Concrete variations)
                let seed = (el.id % 100) as f32 / 100.0;
                let grey = 0.15 + (seed * 0.20);
                let color = [grey, grey, grey];

                // Gather Points
                let mut points = Vec::new();
                for node_id in &el.nodes {
                    if let Some(pos) = node_map.get(node_id) {
                        points.push(*pos);
                    }
                }

                if points.len() < 3 { continue; }
                
                // Ensure Winding Order
                if !is_ccw(&points) { points.reverse(); }

                // Calculate Centroid & Chunk Index
                let mut center = glam::Vec2::ZERO;
                for p in &points { center += *p; }
                center /= points.len() as f32;

                let offset_x = center.x + (config::WORLD_SIZE / 2.0);
                let offset_z = center.y + (config::WORLD_SIZE / 2.0);
                let cx = (offset_x / config::CHUNK_SIZE).floor() as i32;
                let cz = (offset_z / config::CHUNK_SIZE).floor() as i32;
                
                // Skip if out of bounds
                if cx < 0 || cx >= config::CHUNKS_AXIS as i32 || cz < 0 || cz >= config::CHUNKS_AXIS as i32 { continue; }
                
                let chunk_idx = (cx + cz * config::CHUNKS_AXIS as i32) as usize;
                let (c_verts, c_inds) = &mut chunk_builders[chunk_idx];

                // 1. Roof Triangulation
                let flat_poly: Vec<f64> = points.iter().flat_map(|v| vec![v.x as f64, v.y as f64]).collect();
                if let Ok(tris) = earcutr::earcut(&flat_poly, &[], 2) {
                    let base_idx = c_verts.len() as u32;
                    for p in &points {
                        c_verts.push(Vertex { position: [p.x, height, p.y], normal: [0.0, 1.0, 0.0], color });
                    }
                    for idx in tris {
                        c_inds.push(base_idx + idx as u32);
                    }
                }

                // 2. Walls & Collision
                for j in 0..points.len() - 1 {
                    let p1 = points[j];
                    let p2 = points[j+1];
                    let edge = p2 - p1;
                    let normal = glam::Vec3::new(edge.y, 0.0, -edge.x).normalize().to_array();

                    let base = c_verts.len() as u32;
                    c_verts.push(Vertex { position: [p1.x, 0.0, p1.y], normal, color });
                    c_verts.push(Vertex { position: [p2.x, 0.0, p2.y], normal, color });
                    c_verts.push(Vertex { position: [p2.x, height, p2.y], normal, color });
                    c_verts.push(Vertex { position: [p1.x, height, p1.y], normal, color });

                    c_inds.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);

                    // Add to Collision Grid
                    collision.insert(WallCollider {
                        start: p1, end: p2, height,
                        min_x: p1.x.min(p2.x) - config::WALL_THICKNESS as f32,
                        max_x: p1.x.max(p2.x) + config::WALL_THICKNESS as f32,
                        min_z: p1.y.min(p2.y) - config::WALL_THICKNESS as f32,
                        max_z: p1.y.max(p2.y) + config::WALL_THICKNESS as f32,
                    });
                }
            }
        }

        // Flatten Chunk Builders into Main Buffer
        let mut master_vertices = Vec::new();
        let mut master_indices = Vec::new();
        let mut chunk_views = Vec::new();

        // Add Ground Plane (Global Chunk)
        {
             let sz = config::WORLD_SIZE * 2.0; 
             let base = master_vertices.len() as u32;
             master_vertices.push(Vertex { position: [-sz,-0.1,-sz], normal:[0.0,1.0,0.0], color:[0.05,0.05,0.05] });
             master_vertices.push(Vertex { position: [ sz,-0.1,-sz], normal:[0.0,1.0,0.0], color:[0.05,0.05,0.05] });
             master_vertices.push(Vertex { position: [ sz,-0.1, sz], normal:[0.0,1.0,0.0], color:[0.05,0.05,0.05] });
             master_vertices.push(Vertex { position: [-sz,-0.1, sz], normal:[0.0,1.0,0.0], color:[0.05,0.05,0.05] });
             master_indices.extend_from_slice(&[base, base+1, base+2, base, base+2, base+3]);
             
             chunk_views.push(ChunkView { 
                 index_start: 0, index_count: 6, 
                 min: glam::Vec2::splat(-sz), max: glam::Vec2::splat(sz) 
             });
        }

        for (idx, (verts, inds)) in chunk_builders.into_iter().enumerate() {
            if verts.is_empty() { continue; }
            let v_offset = master_vertices.len() as u32;
            let i_start = master_indices.len() as u32;
            
            master_vertices.extend(verts);
            master_indices.extend(inds.iter().map(|i| i + v_offset));
            
            let cx = (idx % config::CHUNKS_AXIS) as f32 * config::CHUNK_SIZE - (config::WORLD_SIZE / 2.0);
            let cz = (idx / config::CHUNKS_AXIS) as f32 * config::CHUNK_SIZE - (config::WORLD_SIZE / 2.0);
            
            chunk_views.push(ChunkView {
                index_start: i_start,
                index_count: inds.len() as u32,
                min: glam::Vec2::new(cx, cz),
                max: glam::Vec2::new(cx + config::CHUNK_SIZE, cz + config::CHUNK_SIZE),
            });
        }

        let _ = tx.send(LoaderMessage::Progress(1.0));
        Self { vertices: master_vertices, indices: master_indices, chunks: chunk_views, collision }
    }
}

fn is_ccw(pts: &[glam::Vec2]) -> bool {
    let mut sum = 0.0;
    for i in 0..pts.len() {
        let p1 = pts[i];
        let p2 = pts[(i + 1) % pts.len()];
        sum += (p2.x - p1.x) * (p2.y + p1.y);
    }
    sum <= 0.0 
}