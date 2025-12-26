// config.rs

pub const WINDOW_TITLE: &str = "SkyRoam OSM";

// --- World Generation ---
pub const MAP_FILE_PATH: &str = "nyc.json";
// NYC Coordinates
pub const ORIGIN_LAT: f64 = 40.771220;
pub const ORIGIN_LON: f64 = -73.979577;

// World Grid Settings
pub const CHUNKS_AXIS: usize = 16; 
pub const WORLD_SIZE: f32 = 10000.0;
pub const CHUNK_SIZE: f32 = WORLD_SIZE / CHUNKS_AXIS as f32;

// --- Physics & Collision ---
pub const PHYSICS_GRID_CELL_SIZE: f32 = 50.0;
pub const PLAYER_RADIUS: f64 = 0.3;
pub const WALL_THICKNESS: f64 = 0.5; // Visual thickness of walls

// Movement
pub const MOVE_SPEED: f64 = 15.0;
pub const GRAVITY: f64 = 35.0;
pub const JUMP_FORCE: f64 = 12.0;
pub const TERMINAL_VELOCITY: f64 = -50.0;
pub const PHYSICS_STEP_SIZE: f64 = 0.01; // 10ms substeps
pub const MAX_PHYSICS_STEPS: i32 = 5;

// --- Rendering ---
pub const FOV_Y: f32 = 45.0;
pub const Z_NEAR: f32 = 0.1;
pub const Z_FAR: f32 = 10000.0;
pub const DRAW_DISTANCE: f32 = 3500.0;
pub const FOG_START: f32 = 1000.0;
pub const FOG_END: f32 = 2500.0;       // Reduced so world fades out BEFORE it cuts off

// Chunk culling vertical bounds
pub const CHUNK_MIN_Y: f32 = -20.0;
pub const CHUNK_MAX_Y: f32 = 450.0; // Slightly higher than Empire State Building