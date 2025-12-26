// config.rs

pub const WINDOW_TITLE: &str = "SkyRoam";

// World Generation
pub const MAP_FILE_PATH: &str = "nyc.pbf"; 

// NYC Coordinates
pub const ORIGIN_LAT: f64 = 40.7580;
pub const ORIGIN_LON: f64 = -73.9855;

// Performance
// 12x12 grid = 144 MegaChunks. 
// This creates a perfect balance between culling and draw call reduction.
pub const WORLD_SIZE: f32 = 12000.0;
pub const CHUNK_GRID_AXIS: usize = 12; 
pub const CHUNK_SIZE: f32 = WORLD_SIZE / CHUNK_GRID_AXIS as f32;

// Physics
pub const PHYSICS_GRID_CELL_SIZE: f32 = 50.0;
pub const PLAYER_RADIUS: f64 = 0.5;
pub const WALL_THICKNESS: f64 = 0.2; 

// Movement
pub const MOVE_SPEED: f64 = 60.0; // Fast dev speed
pub const GRAVITY: f64 = 70.0;
pub const JUMP_FORCE: f64 = 25.0;
pub const TERMINAL_VELOCITY: f64 = -120.0;
pub const PHYSICS_STEP_SIZE: f64 = 0.005; 
pub const MAX_PHYSICS_STEPS: i32 = 3;

// Rendering
pub const FOV_Y: f32 = 65.0;
pub const Z_NEAR: f32 = 0.5;
pub const Z_FAR: f32 = 25000.0;
pub const DRAW_DISTANCE: f32 = 15000.0; 
pub const FOG_START: f32 = 10000.0;
pub const FOG_END: f32 = 14000.0;       

pub const CHUNK_MIN_Y: f32 = -50.0;
pub const CHUNK_MAX_Y: f32 = 1200.0;