use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use serde::Deserialize;
use earcutr::earcut;
use std::sync::mpsc::Sender;

pub enum LoaderMessage {
    Progress(f32), 
    Done(World),
}

const ORIGIN_LAT: f64 = 40.771220;
const ORIGIN_LON: f64 = -73.979577;
const METERS_PER_DEGREE_LAT: f64 = 111132.0;

// World Grid Settings
const CHUNKS_AXIS: i32 = 16; 
const WORLD_SIZE: f32 = 10000.0; 
const CHUNK_SIZE: f32 = WORLD_SIZE / CHUNKS_AXIS as f32;

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct WorldVertex {
    pub position: [f32; 3],
    pub normal:   [f32; 3],
    pub color:    [f32; 3],
}

#[derive(Debug, Clone)]
pub struct WallCollider {
    pub start: glam::Vec2,
    pub end: glam::Vec2,
    pub height: f32,
    pub min_x: f32, pub max_x: f32,
    pub min_z: f32, pub max_z: f32,
}

#[derive(Deserialize, Debug)]
struct OsmResponse { elements: Vec<OsmElement> }
#[derive(Deserialize, Debug)]
struct OsmElement {
    #[serde(default)] id: u64,
    #[serde(rename = "type")] e_type: String,
    #[serde(default)] nodes: Vec<u64>,
    #[serde(default)] lat: f64,
    #[serde(default)] lon: f64,
    #[serde(default)] tags: Option<HashMap<String, String>>,
}

#[derive(Debug)]
pub struct ChunkView {
    pub index_start: u32,
    pub index_count: u32,
    pub min: glam::Vec2, 
    pub max: glam::Vec2, 
}

pub struct World {
    pub vertices: Vec<WorldVertex>,
    pub indices: Vec<u32>,
    pub chunks: Vec<ChunkView>,
    pub collision_map: HashMap<(i32, i32), Vec<WallCollider>>,
}

impl World {
    pub fn generate(tx: Sender<LoaderMessage>) -> Self {
        let mut chunk_builders: Vec<(Vec<WorldVertex>, Vec<u32>)> = 
            (0..(CHUNKS_AXIS * CHUNKS_AXIS)).map(|_| (Vec::new(), Vec::new())).collect();

        let mut collision_map: HashMap<(i32, i32), Vec<WallCollider>> = HashMap::new();

        let lat_rad = ORIGIN_LAT.to_radians();
        let meters_per_degree_lon = 111319.5 * lat_rad.cos();

        let _ = tx.send(LoaderMessage::Progress(0.01));
        
        let file = match File::open("nyc.json") {
            Ok(f) => f,
            Err(_) => return Self { vertices: vec![], indices: vec![], chunks: vec![], collision_map },
        };
        let reader = BufReader::new(file);
        let osm_data: OsmResponse = serde_json::from_reader(reader).unwrap_or(OsmResponse { elements: vec![] });
        let _ = tx.send(LoaderMessage::Progress(0.10));

        let mut node_map: HashMap<u64, (f64, f64)> = HashMap::new();
        for el in &osm_data.elements {
            if el.e_type == "node" {
                node_map.insert(el.id, (el.lat, el.lon));
            }
        }

        let total_elements = osm_data.elements.len();
        let mut last_percent = 0;

        for (i, el) in osm_data.elements.iter().enumerate() {
            if i % 500 == 0 {
                let percent = ((i as f32 / total_elements as f32) * 100.0) as i32;
                if percent > last_percent {
                    last_percent = percent;
                    let p = 0.10 + (percent as f32 / 100.0) * 0.85;
                    let _ = tx.send(LoaderMessage::Progress(p));
                }
            }

            if el.e_type == "way" && el.tags.as_ref().map_or(false, |t| t.contains_key("building")) {
                let tags = el.tags.as_ref().unwrap();
                
                let height: f32 = if let Some(h_str) = tags.get("height") {
                    h_str.trim_matches(|c: char| !c.is_numeric() && c != '.').parse().unwrap_or(20.0)
                } else if let Some(l_str) = tags.get("building:levels") {
                    let levels: f32 = l_str.parse().unwrap_or(3.0);
                    levels * 4.0 
                } else {
                    let pseudo_rand = (el.id % 100) as f32; 
                    8.0 + (pseudo_rand * 0.3)
                };

                // FIX: GREYSCALE CONCRETE COLORS
                // Varies slightly from 0.15 (dark grey) to 0.35 (light grey)
                let seed = (el.id % 100) as f32 / 100.0; 
                let grey = 0.15 + (seed * 0.20);
                let building_color = [grey, grey, grey];

                let mut raw_pts = Vec::new();
                for node_id in &el.nodes {
                    if let Some(&(lat, lon)) = node_map.get(node_id) {
                        let x = (lon - ORIGIN_LON) * meters_per_degree_lon;
                        let z = -(lat - ORIGIN_LAT) * METERS_PER_DEGREE_LAT;
                        raw_pts.push(glam::Vec2::new(x as f32, z as f32));
                    }
                }

                if raw_pts.len() < 3 { continue; }

                // Winding Check (Still good to have, though Culling: None makes it forgiving)
                force_ccw(&mut raw_pts);

                // Centroid & Inset
                let mut center = glam::Vec2::ZERO;
                for p in &raw_pts { center += *p; }
                center /= raw_pts.len() as f32;

                let mut final_pts = Vec::new();
                let mut poly_flat = Vec::new();
                for p in &raw_pts {
                    let inset = center + (*p - center) * 0.90; 
                    final_pts.push(glam::Vec3::new(inset.x, 0.0, inset.y));
                    poly_flat.push(inset.x as f64);
                    poly_flat.push(inset.y as f64);
                }

                // Chunking
                let offset_x = center.x + (WORLD_SIZE / 2.0);
                let offset_z = center.y + (WORLD_SIZE / 2.0);
                let cx = (offset_x / CHUNK_SIZE).floor() as i32;
                let cz = (offset_z / CHUNK_SIZE).floor() as i32;
                
                let chunk_idx = (cx.clamp(0, CHUNKS_AXIS - 1) + cz.clamp(0, CHUNKS_AXIS - 1) * CHUNKS_AXIS) as usize;
                let (c_verts, c_inds) = &mut chunk_builders[chunk_idx];

                // Roofs
                if let Ok(tris) = earcut(&poly_flat, &[], 2) {
                    let start = c_verts.len() as u32;
                    for pt in &final_pts {
                        c_verts.push(WorldVertex { 
                            position: [pt.x, height, pt.z], 
                            normal: [0.0, 1.0, 0.0], 
                            color: building_color 
                        });
                    }
                    for idx in tris {
                        c_inds.push(start + idx as u32);
                    }
                }

                // Walls
                for i in 0..final_pts.len() - 1 {
                    let p1 = final_pts[i];
                    let p2 = final_pts[i+1];
                    let base = c_verts.len() as u32;

                    let edge = p2 - p1;
                    let normal = glam::Vec3::new(edge.z, 0.0, -edge.x).normalize().to_array();

                    c_verts.push(WorldVertex { position: [p1.x, 0.0, p1.z], normal, color: building_color }); // 0
                    c_verts.push(WorldVertex { position: [p2.x, 0.0, p2.z], normal, color: building_color }); // 1
                    c_verts.push(WorldVertex { position: [p2.x, height, p2.z], normal, color: building_color }); // 2
                    c_verts.push(WorldVertex { position: [p1.x, height, p1.z], normal, color: building_color }); // 3

                    c_inds.push(base + 0); c_inds.push(base + 1); c_inds.push(base + 2);
                    c_inds.push(base + 0); c_inds.push(base + 2); c_inds.push(base + 3);

                    // Physics
                    let collider = WallCollider {
                        start: glam::Vec2::new(p1.x, p1.z),
                        end: glam::Vec2::new(p2.x, p2.z),
                        height,
                        min_x: p1.x.min(p2.x) - 0.5, max_x: p1.x.max(p2.x) + 0.5,
                        min_z: p1.z.min(p2.z) - 0.5, max_z: p1.z.max(p2.z) + 0.5,
                    };
                    let sgx = (collider.min_x / 50.0).floor() as i32;
                    let egx = (collider.max_x / 50.0).floor() as i32;
                    let sgz = (collider.min_z / 50.0).floor() as i32;
                    let egz = (collider.max_z / 50.0).floor() as i32;

                    for gx in sgx..=egx {
                        for gz in sgz..=egz {
                            collision_map.entry((gx, gz)).or_default().push(collider.clone());
                        }
                    }
                }
            }
        }

        // Flatten
        let mut master_vertices = Vec::new();
        let mut master_indices = Vec::new();
        let mut chunk_views = Vec::new();

        // Ground
        {
            let sz = 20000.0; 
            let g_col = [0.05, 0.05, 0.05]; // Dark ground
            let base = master_vertices.len() as u32;
            master_vertices.push(WorldVertex { position: [-sz,-0.1,-sz], normal:[0.0,1.0,0.0], color:g_col });
            master_vertices.push(WorldVertex { position: [ sz,-0.1,-sz], normal:[0.0,1.0,0.0], color:g_col });
            master_vertices.push(WorldVertex { position: [ sz,-0.1, sz], normal:[0.0,1.0,0.0], color:g_col });
            master_vertices.push(WorldVertex { position: [-sz,-0.1, sz], normal:[0.0,1.0,0.0], color:g_col });
            master_indices.push(base+0); master_indices.push(base+1); master_indices.push(base+2);
            master_indices.push(base+0); master_indices.push(base+2); master_indices.push(base+3);
            
            chunk_views.push(ChunkView { 
                index_start: 0, index_count: 6, 
                min: glam::Vec2::splat(-sz), max: glam::Vec2::splat(sz) 
            });
        }

        for (idx, (verts, inds)) in chunk_builders.into_iter().enumerate() {
            if verts.is_empty() { continue; }
            let v_offset = master_vertices.len() as u32;
            let i_start = master_indices.len() as u32;
            let i_count = inds.len() as u32;
            master_vertices.extend(verts);
            for i in inds { master_indices.push(i + v_offset); }
            let cx = (idx as i32 % CHUNKS_AXIS) as f32 * CHUNK_SIZE - (WORLD_SIZE/2.0);
            let cz = (idx as i32 / CHUNKS_AXIS) as f32 * CHUNK_SIZE - (WORLD_SIZE/2.0);
            chunk_views.push(ChunkView {
                index_start: i_start,
                index_count: i_count,
                min: glam::Vec2::new(cx, cz),
                max: glam::Vec2::new(cx + CHUNK_SIZE, cz + CHUNK_SIZE),
            });
        }

        let _ = tx.send(LoaderMessage::Progress(1.0));
        Self { vertices: master_vertices, indices: master_indices, chunks: chunk_views, collision_map }
    }
}

fn force_ccw(pts: &mut Vec<glam::Vec2>) {
    let mut sum = 0.0;
    for i in 0..pts.len() {
        let p1 = pts[i];
        let p2 = pts[(i + 1) % pts.len()];
        sum += (p2.x - p1.x) * (p2.y + p1.y);
    }
    if sum > 0.0 { pts.reverse(); }
}