use std::fs::File;
use std::io::BufReader;
use serde::Deserialize;
use std::collections::HashMap;
use crate::config;
use crate::vertex::Vertex;
use crate::world::{ChunkData, WallCollider};

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

pub fn load_chunks_from_osm(path: &str) -> Vec<ChunkData> {
    let lat_rad = config::ORIGIN_LAT.to_radians();
    let meters_per_deg_lat = 111132.0;
    let meters_per_deg_lon = 111319.5 * lat_rad.cos();

    // Safe File Loading
    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Error opening map file '{}': {}", path, e);
            eprintln!("Returning empty world with ground plane.");
            return generate_fallback_ground();
        }
    };
    
    let reader = BufReader::new(file);
    let osm_data: OsmResponse = match serde_json::from_reader(reader) {
        Ok(data) => data,
        Err(e) => {
            eprintln!("Error parsing JSON: {}", e);
            return generate_fallback_ground();
        }
    };
    
    // Node Mapping
    let mut node_map: HashMap<u64, glam::Vec2> = HashMap::new();
    for el in &osm_data.elements {
        if el.e_type == "node" {
            let x = (el.lon - config::ORIGIN_LON) * meters_per_deg_lon;
            let z = -(el.lat - config::ORIGIN_LAT) * meters_per_deg_lat;
            node_map.insert(el.id, glam::Vec2::new(x as f32, z as f32));
        }
    }

    let mut builders: HashMap<(i32, i32), (Vec<Vertex>, Vec<u32>, Vec<WallCollider>)> = HashMap::new();
    
    // Bounds tracking for dynamic ground generation
    let mut min_cx = i32::MAX;
    let mut max_cx = i32::MIN;
    let mut min_cz = i32::MAX;
    let mut max_cz = i32::MIN;

    // Building Generation
    for el in &osm_data.elements {
        if el.e_type == "way" && el.tags.as_ref().map_or(false, |t| t.contains_key("building")) {
            let tags = el.tags.as_ref().unwrap();
            
            let height: f32 = tags.get("height")
                .and_then(|h| h.trim_matches(|c: char| !c.is_numeric() && c != '.').parse().ok())
                .unwrap_or(20.0);
            
            let seed = (el.id % 100) as f32 / 100.0;
            let grey = 0.15 + (seed * 0.20);
            let color = [grey, grey, grey];

            let mut points = Vec::new();
            for node_id in &el.nodes {
                if let Some(pos) = node_map.get(node_id) { points.push(*pos); }
            }
            if points.len() < 3 { continue; }
            
            // Winding check
            let mut sum = 0.0;
            for i in 0..points.len() {
                let p1 = points[i];
                let p2 = points[(i + 1) % points.len()];
                sum += (p2.x - p1.x) * (p2.y + p1.y);
            }
            if sum > 0.0 { points.reverse(); }

            // Assign to chunk
            let mut center = glam::Vec2::ZERO;
            for p in &points { center += *p; }
            center /= points.len() as f32;

            let offset_x = center.x + (config::WORLD_SIZE / 2.0);
            let offset_z = center.y + (config::WORLD_SIZE / 2.0);
            let cx = (offset_x / config::CHUNK_SIZE).floor() as i32;
            let cz = (offset_z / config::CHUNK_SIZE).floor() as i32;
            
            // Update map bounds
            if cx < min_cx { min_cx = cx; }
            if cx > max_cx { max_cx = cx; }
            if cz < min_cz { min_cz = cz; }
            if cz > max_cz { max_cz = cz; }
            
            let entry = builders.entry((cx, cz)).or_insert((Vec::new(), Vec::new(), Vec::new()));
            let (c_verts, c_inds, c_walls) = entry;

            // Roof
            let flat_poly: Vec<f64> = points.iter().flat_map(|v| vec![v.x as f64, v.y as f64]).collect();
            if let Ok(tris) = earcutr::earcut(&flat_poly, &[], 2) {
                let base_idx = c_verts.len() as u32;
                for p in &points {
                    c_verts.push(Vertex { position: [p.x, height, p.y], normal: [0.0, 1.0, 0.0], color });
                }
                for idx in tris { c_inds.push(base_idx + idx as u32); }
            }

            // Walls
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

                c_walls.push(WallCollider {
                    start: p1, end: p2, height,
                    min_x: p1.x.min(p2.x) - config::WALL_THICKNESS as f32,
                    max_x: p1.x.max(p2.x) + config::WALL_THICKNESS as f32,
                    min_z: p1.y.min(p2.y) - config::WALL_THICKNESS as f32,
                    max_z: p1.y.max(p2.y) + config::WALL_THICKNESS as f32,
                });
            }
        }
    }

    // Dynamic Ground Generation
    // We pad the bounds by 1 chunk to prevent seeing the void at the edges
    if min_cx <= max_cx {
        add_ground_plane(&mut builders, min_cx - 1, max_cx + 1, min_cz - 1, max_cz + 1);
    } else {
        add_ground_plane(&mut builders, 0, 1, 0, 1);
    }

    builders.into_iter().map(|(coord, (vertices, indices, walls))| {
        ChunkData { vertices, indices, walls, coord }
    }).collect()
}

fn generate_fallback_ground() -> Vec<ChunkData> {
    let mut builders = HashMap::new();
    add_ground_plane(&mut builders, 0, 1, 0, 1);
    builders.into_iter().map(|(coord, (vertices, indices, walls))| {
        ChunkData { vertices, indices, walls, coord }
    }).collect()
}

fn add_ground_plane(
    builders: &mut HashMap<(i32, i32), (Vec<Vertex>, Vec<u32>, Vec<WallCollider>)>, 
    min_x: i32, max_x: i32, min_z: i32, max_z: i32
) {
    for cx in min_x..=max_x {
        for cz in min_z..=max_z {
            let entry = builders.entry((cx, cz)).or_insert((Vec::new(), Vec::new(), Vec::new()));
            let (v, i, _) = entry;
            let base = v.len() as u32;
            let chunk_x = cx as f32 * config::CHUNK_SIZE - (config::WORLD_SIZE/2.0);
            let chunk_z = cz as f32 * config::CHUNK_SIZE - (config::WORLD_SIZE/2.0);
            let s = config::CHUNK_SIZE;
            
            v.push(Vertex{ position: [chunk_x, -0.1, chunk_z], normal:[0.0,1.0,0.0], color:[0.05,0.05,0.05] });
            v.push(Vertex{ position: [chunk_x+s, -0.1, chunk_z], normal:[0.0,1.0,0.0], color:[0.05,0.05,0.05] });
            v.push(Vertex{ position: [chunk_x+s, -0.1, chunk_z+s], normal:[0.0,1.0,0.0], color:[0.05,0.05,0.05] });
            v.push(Vertex{ position: [chunk_x, -0.1, chunk_z+s], normal:[0.0,1.0,0.0], color:[0.05,0.05,0.05] });
            i.extend_from_slice(&[base, base+1, base+2, base, base+2, base+3]);
        }
    }
}