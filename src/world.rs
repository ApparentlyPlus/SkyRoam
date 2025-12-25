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
const METERS_PER_DEGREE_LAT: f64 = 111000.0;
const METERS_PER_DEGREE_LON: f64 = 85000.0; 

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

pub struct World {
    pub vertices: Vec<WorldVertex>,
    pub indices:  Vec<u32>,
    pub collision_map: HashMap<(i32, i32), Vec<WallCollider>>,
}

impl World {
    pub fn generate(tx: Sender<LoaderMessage>) -> Self {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        let mut collision_map: HashMap<(i32, i32), Vec<WallCollider>> = HashMap::new();

        let _ = tx.send(LoaderMessage::Progress(0.01));
        
        let file = File::open("nyc.json").expect("Failed to open nyc.json");
        let reader = BufReader::new(file);
        
        let _ = tx.send(LoaderMessage::Progress(0.05));
        let osm_data: OsmResponse = serde_json::from_reader(reader).expect("Failed to parse JSON");
        
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
            
            // --- THROTTLING LOGIC ---
            // Only send message if percentage changed integer value to reduce channel overhead
            let percent = ((i as f32 / total_elements as f32) * 100.0) as i32;
            if percent > last_percent {
                last_percent = percent;
                // Map 10% -> 95% range
                let p = 0.10 + (percent as f32 / 100.0) * 0.85;
                let _ = tx.send(LoaderMessage::Progress(p));
            }
            // ------------------------

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

                let noise = (el.id % 30) as f32 / 100.0; 
                let c = 0.1 + noise;
                let building_color = [c, c, c]; 

                let mut perimeter_pts = Vec::new();
                let mut polygon_flat = Vec::new();
                
                for node_id in &el.nodes {
                    if let Some(&(lat, lon)) = node_map.get(node_id) {
                        let x = (lon - ORIGIN_LON) * METERS_PER_DEGREE_LON;
                        let z = -(lat - ORIGIN_LAT) * METERS_PER_DEGREE_LAT;
                        perimeter_pts.push(glam::Vec3::new(x as f32, 0.0, z as f32));
                        polygon_flat.push(x);
                        polygon_flat.push(z);
                    }
                }

                if perimeter_pts.len() < 3 { continue; }

                if let Ok(tris) = earcut(&polygon_flat, &[], 2) {
                    let start_v_idx = vertices.len() as u32;
                    for pt in &perimeter_pts {
                        vertices.push(WorldVertex { position: [pt.x, height, pt.z], normal: [0.0, 1.0, 0.0], color: building_color });
                    }
                    for &idx in &tris {
                        indices.push(start_v_idx + (idx as u32));
                    }
                }

                for i in 0..perimeter_pts.len() - 1 {
                    let p1 = perimeter_pts[i];
                    let p2 = perimeter_pts[i+1];

                    let edge = p2 - p1;
                    let normal = glam::Vec3::new(-edge.z, 0.0, edge.x).normalize().to_array(); 
                    let base_idx = vertices.len() as u32;
                    
                    vertices.push(WorldVertex { position: [p1.x, 0.0,    p1.z], normal, color: building_color });
                    vertices.push(WorldVertex { position: [p2.x, 0.0,    p2.z], normal, color: building_color });
                    vertices.push(WorldVertex { position: [p2.x, height, p2.z], normal, color: building_color });
                    vertices.push(WorldVertex { position: [p1.x, height, p1.z], normal, color: building_color });

                    indices.push(base_idx + 0); indices.push(base_idx + 2); indices.push(base_idx + 1);
                    indices.push(base_idx + 0); indices.push(base_idx + 3); indices.push(base_idx + 2);

                    let thickness = 0.5;
                    let min_x = p1.x.min(p2.x) - thickness;
                    let max_x = p1.x.max(p2.x) + thickness;
                    let min_z = p1.z.min(p2.z) - thickness;
                    let max_z = p1.z.max(p2.z) + thickness;

                    let collider = WallCollider {
                        start: glam::Vec2::new(p1.x, p1.z),
                        end: glam::Vec2::new(p2.x, p2.z),
                        height,
                        min_x, max_x, min_z, max_z
                    };

                    let start_gx = (min_x / 50.0).floor() as i32;
                    let end_gx = (max_x / 50.0).floor() as i32;
                    let start_gz = (min_z / 50.0).floor() as i32;
                    let end_gz = (max_z / 50.0).floor() as i32;

                    for gx in start_gx..=end_gx {
                        for gz in start_gz..=end_gz {
                             collision_map.entry((gx, gz)).or_insert_with(Vec::new).push(collider.clone());
                        }
                    }
                }
            }
        }
        
        let ground_color = [0.05, 0.05, 0.05];
        let sz = 20000.0; 
        let v_start = vertices.len() as u32;
        vertices.push(WorldVertex { position: [-sz, 0.0, -sz], normal: [0.0,1.0,0.0], color: ground_color });
        vertices.push(WorldVertex { position: [ sz, 0.0, -sz], normal: [0.0,1.0,0.0], color: ground_color });
        vertices.push(WorldVertex { position: [ sz, 0.0,  sz], normal: [0.0,1.0,0.0], color: ground_color });
        vertices.push(WorldVertex { position: [-sz, 0.0,  sz], normal: [0.0,1.0,0.0], color: ground_color });
        indices.push(v_start+0); indices.push(v_start+2); indices.push(v_start+1);
        indices.push(v_start+0); indices.push(v_start+3); indices.push(v_start+2);

        let _ = tx.send(LoaderMessage::Progress(1.0));
        
        Self { vertices, indices, collision_map }
    }
}