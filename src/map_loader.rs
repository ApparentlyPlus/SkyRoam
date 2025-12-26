// map_loader.rs
use std::fs::File;
use std::io::{BufReader, Read};
use std::sync::{Arc, atomic::{AtomicU64, Ordering}};
use std::thread;
use std::time::Duration;
use osmpbf::{ElementReader, Element};
use glam::Vec2;
use rayon::prelude::*;
use crate::{config, vertex::Vertex, world::{ChunkData, WallCollider}};

// 12 bytes per node.
#[derive(Clone, Copy)]
struct CompactNode {
    id: i64,
    x: f32,
    y: f32
}

// Wraps a file reader and increments an atomic counter on every read.
struct ProgressReader {
    inner: BufReader<File>,
    counter: Arc<AtomicU64>,
}

impl Read for ProgressReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.counter.fetch_add(n as u64, Ordering::Relaxed);
        Ok(n)
    }
}

#[inline(always)]
fn coords_to_local(lat: f64, lon: f64) -> (f32, f32) {
    let lat_rad = config::ORIGIN_LAT.to_radians();
    const METERS_LAT: f64 = 111132.0;
    let meters_lon = 111319.5 * lat_rad.cos();

    let x = (lon - config::ORIGIN_LON) * meters_lon;
    let z = -(lat - config::ORIGIN_LAT) * METERS_LAT;
    (x as f32, z as f32)
}

struct RawBuilding {
    points: Vec<Vec2>,
    height: f32,
    color: [f32; 3],
}

pub fn load_chunks_from_osm_stream<F>(path: &str, on_update: F) 
where F: Fn(Option<Vec<ChunkData>>, f32, &str) + Send + Sync + 'static 
{
    let path_str = path.to_string();
    
    // Get File Size for progress calc
    let file_meta = match File::open(&path_str) {
        Ok(f) => f.metadata().ok(),
        Err(_) => None,
    };
    
    let total_bytes = if let Some(meta) = file_meta { meta.len() } else { 1 };
    
    // Shared Atomic Counter
    let bytes_read = Arc::new(AtomicU64::new(0));
    
    let bytes_monitor = bytes_read.clone();
    let monitor_callback = Arc::new(on_update);
    let callback_ref = monitor_callback.clone();
    
    let phase = Arc::new(std::sync::atomic::AtomicU8::new(0));
    let phase_monitor = phase.clone();

    let monitor_handle = thread::spawn(move || {
        loop {
            let p_val = phase_monitor.load(Ordering::Relaxed);
            if p_val == 99 { break; } // Exit signal

            let b = bytes_monitor.load(Ordering::Relaxed);
            let file_progress = (b as f64 / total_bytes as f64) as f32;

            match p_val {
                0 => { // Nodes: 0% -> 50%
                    let p = file_progress * 0.5;
                    monitor_callback(None, p, "Reading Nodes...");
                },
                1 => { // Sorting: 50% -> 55% (Fake interpolation or hold)
                     monitor_callback(None, 0.52, "Sorting...");
                },
                2 => { // Ways: 55% -> 95%
                    let p = 0.55 + (file_progress * 0.40);
                    monitor_callback(None, p, "Parsing Ways...");
                },
                _ => {}
            }
            thread::sleep(Duration::from_millis(30));
        }
    });

    let file = match File::open(&path_str) {
        Ok(f) => f,
        Err(_) => {
            callback_ref(None, 1.0, "Error: File Not Found");
            return;
        }
    };
    
    let reader = ProgressReader {
        inner: BufReader::with_capacity(1024 * 1024, file), // 1MB Buffer
        counter: bytes_read.clone(),
    };
    
    let mut node_store: Vec<CompactNode> = Vec::with_capacity(8_000_000);
    let pbf_reader = ElementReader::new(reader);
    
    let _ = pbf_reader.for_each(|element| {
        match element {
            Element::DenseNode(n) => {
                let (x, y) = coords_to_local(n.lat(), n.lon());
                node_store.push(CompactNode { id: n.id, x, y });
            }
            Element::Node(n) => {
                let (x, y) = coords_to_local(n.lat(), n.lon());
                node_store.push(CompactNode { id: n.id(), x, y });
            }
            _ => {}
        }
    });

    phase.store(1, Ordering::Relaxed);
    node_store.par_sort_unstable_by_key(|n| n.id);

    phase.store(2, Ordering::Relaxed);
    // Reset byte counter for the second pass so progress math works
    bytes_read.store(0, Ordering::Relaxed);
    
    let file2 = File::open(&path_str).unwrap();
    let reader2 = ProgressReader {
        inner: BufReader::with_capacity(1024 * 1024, file2),
        counter: bytes_read.clone(),
    };
    let pbf_reader2 = ElementReader::new(reader2);
    
    let grid_size = config::CHUNK_GRID_AXIS * config::CHUNK_GRID_AXIS;
    let mut chunk_buckets: Vec<Vec<RawBuilding>> = (0..grid_size).map(|_| Vec::new()).collect();
    
    let _ = pbf_reader2.for_each(|element| {
        if let Element::Way(way) = element {
            if way.tags().any(|(k, _)| k == "building") {
                let mut height = 20.0;
                if let Some(h_str) = way.tags().find(|(k, _)| *k == "height").map(|(_, v)| v) {
                    if let Ok(h) = h_str.trim_matches(|c: char| !c.is_numeric() && c != '.').parse::<f32>() {
                        height = h;
                    }
                }
                
                let seed = (way.id() % 100) as f32 / 100.0;
                let grey = 0.15 + (seed * 0.20);
                let color = [grey, grey, grey];

                let mut points = Vec::new();
                let mut valid = true;
                let mut cx = 0.0; let mut cy = 0.0;

                for id in way.refs() {
                    if let Ok(idx) = node_store.binary_search_by_key(&id, |n| n.id) {
                        let n = node_store[idx];
                        points.push(Vec2::new(n.x, n.y));
                        cx += n.x; cy += n.y;
                    } else {
                        valid = false;
                        break;
                    }
                }

                if valid && points.len() >= 3 {
                    // Winding
                    let mut sum = 0.0;
                    for i in 0..points.len() {
                        let p1 = points[i];
                        let p2 = points[(i+1)%points.len()];
                        sum += (p2.x - p1.x)*(p2.y + p1.y);
                    }
                    if sum > 0.0 { points.reverse(); }

                    cx /= points.len() as f32;
                    cy /= points.len() as f32;

                    let off_x = cx + (config::WORLD_SIZE / 2.0);
                    let off_z = cy + (config::WORLD_SIZE / 2.0);
                    let gx = (off_x / config::CHUNK_SIZE).floor() as i32;
                    let gz = (off_z / config::CHUNK_SIZE).floor() as i32;

                    if gx >= 0 && gx < config::CHUNK_GRID_AXIS as i32 && gz >= 0 && gz < config::CHUNK_GRID_AXIS as i32 {
                        let idx = (gz as usize) * config::CHUNK_GRID_AXIS + (gx as usize);
                        chunk_buckets[idx].push(RawBuilding { points, height, color });
                    }
                }
            }
        }
    });

    drop(node_store); // Free RAM
    phase.store(99, Ordering::Relaxed); // Stop monitor thread

    callback_ref(None, 0.95, "Meshing...");

    let numbered_chunks: Vec<(usize, Vec<RawBuilding>)> = chunk_buckets.into_iter().enumerate().collect();
    let total_chunks = numbered_chunks.len();
    let mut batch = Vec::new();

    for (i, (idx, buildings)) in numbered_chunks.into_iter().enumerate() {
        if buildings.is_empty() { continue; }
        
        let gz = idx / config::CHUNK_GRID_AXIS;
        let gx = idx % config::CHUNK_GRID_AXIS;
        let coord = (gx as i32, gz as i32);

        let chunk = build_chunk_geometry(buildings, coord);
        batch.push(chunk);

        if batch.len() >= 4 {
            let p = 0.95 + (i as f32 / total_chunks as f32) * 0.05;
            callback_ref(Some(batch.clone()), p, "Streaming...");
            batch.clear();
        }
    }
    
    if !batch.is_empty() {
        callback_ref(Some(batch), 1.0, "Done");
    } else {
        callback_ref(None, 1.0, "Done");
    }
}

fn build_chunk_geometry(buildings: Vec<RawBuilding>, coord: (i32, i32)) -> ChunkData {
    let mut vertices = Vec::with_capacity(buildings.len() * 24);
    let mut indices = Vec::with_capacity(buildings.len() * 36);
    let mut walls = Vec::with_capacity(buildings.len() * 4);

    let cx = coord.0 as f32 * config::CHUNK_SIZE - (config::WORLD_SIZE/2.0);
    let cz = coord.1 as f32 * config::CHUNK_SIZE - (config::WORLD_SIZE/2.0);
    let s = config::CHUNK_SIZE;
    
    let base = 0;
    vertices.push(Vertex{ position: [cx, -0.1, cz], normal:[0.0,1.0,0.0], color:[0.05,0.05,0.05] });
    vertices.push(Vertex{ position: [cx+s, -0.1, cz], normal:[0.0,1.0,0.0], color:[0.05,0.05,0.05] });
    vertices.push(Vertex{ position: [cx+s, -0.1, cz+s], normal:[0.0,1.0,0.0], color:[0.05,0.05,0.05] });
    vertices.push(Vertex{ position: [cx, -0.1, cz+s], normal:[0.0,1.0,0.0], color:[0.05,0.05,0.05] });
    indices.extend_from_slice(&[base, base+1, base+2, base, base+2, base+3]);

    for b in buildings {
        let flat_poly: Vec<f64> = b.points.iter().flat_map(|v| vec![v.x as f64, v.y as f64]).collect();
        if let Ok(tris) = earcutr::earcut(&flat_poly, &[], 2) {
            let base_idx = vertices.len() as u32;
            for p in &b.points {
                vertices.push(Vertex { position: [p.x, b.height, p.y], normal: [0.0, 1.0, 0.0], color: b.color });
            }
            for idx in tris { indices.push(base_idx + idx as u32); }
        }

        for j in 0..b.points.len() {
            let p1 = b.points[j];
            let p2 = b.points[(j + 1) % b.points.len()];
            if (p1.x-p2.x).abs() < 0.01 && (p1.y-p2.y).abs() < 0.01 { continue; }
            let edge = p2 - p1;
            let normal = glam::Vec3::new(edge.y, 0.0, -edge.x).normalize().to_array();
            
            let base = vertices.len() as u32;
            vertices.push(Vertex { position: [p1.x, 0.0, p1.y], normal, color: b.color });
            vertices.push(Vertex { position: [p2.x, 0.0, p2.y], normal, color: b.color });
            vertices.push(Vertex { position: [p2.x, b.height, p2.y], normal, color: b.color });
            vertices.push(Vertex { position: [p1.x, b.height, p1.y], normal, color: b.color });
            indices.extend_from_slice(&[base, base+1, base+2, base, base+2, base+3]);

            walls.push(WallCollider {
                start: p1, end: p2, height: b.height,
                min_x: p1.x.min(p2.x) - config::WALL_THICKNESS as f32,
                max_x: p1.x.max(p2.x) + config::WALL_THICKNESS as f32,
                min_z: p1.y.min(p2.y) - config::WALL_THICKNESS as f32,
                max_z: p1.y.max(p2.y) + config::WALL_THICKNESS as f32,
            });
        }
    }
    ChunkData { vertices, indices, walls, coord }
}