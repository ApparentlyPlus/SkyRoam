// shader.rs

// SCENE SHADER (Unchanged)
pub const SCENE_SHADER: &str = r#"
struct CameraUniform {
    view_proj: mat4x4<f32>,
    screen_size: vec2<f32>,
    fog_dist: vec2<f32>,
    camera_pos: vec4<f32>,
};
@group(0) @binding(0) var<uniform> camera: CameraUniform;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
    @location(1) world_pos: vec3<f32>,
    @location(2) normal: vec3<f32>,
};

@vertex
fn vs_main(model: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.world_pos = model.position;
    out.clip_position = camera.view_proj * vec4<f32>(model.position, 1.0);
    out.normal = model.normal;
    out.color = model.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let sun_dir = normalize(vec3<f32>(0.5, 1.0, 0.5));
    let normal = normalize(in.normal);
    let diff = max(dot(normal, sun_dir), 0.0);
    let light = 0.2 + (diff * 0.8);
    let height_gradient = clamp((in.world_pos.y + 20.0) / 150.0, 0.4, 1.0);
    let lit_color = in.color * light * height_gradient;
    let dist = distance(in.world_pos, camera.camera_pos.xyz);
    let fog_factor = smoothstep(camera.fog_dist.x, camera.fog_dist.y, dist);
    return vec4<f32>(mix(lit_color, vec3<f32>(0.0, 0.0, 0.0), fog_factor), 1.0);
}
"#;

// UI SHADER (Unchanged)
pub const UI_SHADER: &str = r#"
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};
@vertex
fn vs_main(@builtin(vertex_index) in_vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;
    let size = 0.003; 
    var pos = vec2<f32>(0.0, 0.0);
    var uv = vec2<f32>(0.5, 0.5);
    if (in_vertex_index == 0u) { pos = vec2<f32>(-size, -size); uv = vec2<f32>(0.0, 0.0); }
    if (in_vertex_index == 1u) { pos = vec2<f32>( size, -size); uv = vec2<f32>(1.0, 0.0); }
    if (in_vertex_index == 2u) { pos = vec2<f32>(-size,  size); uv = vec2<f32>(0.0, 1.0); }
    if (in_vertex_index == 3u) { pos = vec2<f32>( size,  size); uv = vec2<f32>(1.0, 1.0); }
    out.position = vec4<f32>(pos.x, pos.y * 1.77, 0.0, 1.0);
    out.uv = uv;
    return out;
}
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let dist = distance(in.uv, vec2<f32>(0.5));
    if (dist > 0.5) { discard; }
    return vec4<f32>(1.0, 1.0, 1.0, 1.0);
}
"#;

// --- LOADING SHADER (Updated: Pixel Font & 3px Bar) ---
pub const LOADING_SHADER: &str = r#"
struct Uniforms {
    screen_size: vec2<f32>,
    progress: f32,
    _pad: f32,
};
@group(0) @binding(0) var<uniform> u: Uniforms;

@vertex
fn vs_main(@builtin(vertex_index) in_vertex_index: u32) -> @builtin(position) vec4<f32> {
    var pos = vec2<f32>(-1.0, -1.0);
    if (in_vertex_index == 1u) { pos = vec2<f32>(3.0, -1.0); }
    if (in_vertex_index == 2u) { pos = vec2<f32>(-1.0, 3.0); }
    return vec4<f32>(pos, 0.0, 1.0);
}

// 3x5 Pixel Font Logic
fn has_pixel(char_idx: i32, x: i32, y: i32) -> bool {
    // Space
    if (char_idx == 32) { return false; }
    // L
    if (char_idx == 76) { if (x == 0 || y == 4) { return true; } return false; }
    // o
    if (char_idx == 111) { if (y==0||y==4) { return x==1; } return x!=1; }
    // a
    if (char_idx == 97) { if (y==0||y==2) { return true; } if (y==1) { return x!=1; } return x==2 || (x==0 && y>2); }
    // d
    if (char_idx == 100) { if (x==2) { return true; } if (y==2||y==4) { return x>0; } if (y==3) { return x==0; } return false; }
    // i
    if (char_idx == 105) { return x == 1 && y != 1; }
    // n
    if (char_idx == 110) { if (y==0) { return false; } if (y==1) { return true; } return x!=1; }
    // g
    if (char_idx == 103) { if (y==0) { return x>0; } if (y==2) { return x>0; } if (y==4) { return x<2; } if (x==2) { return true; } if (x==0 && y>0 && y<3) { return true; } return false; }
    // %
    if (char_idx == 37) { if (x==0 && y==0) { return true; } if (x==2 && y==4) { return true; } if (x==1 && y==2) { return true; } if (x==2 && y==1) { return true; } if (x==0 && y==3) { return true; } return false; }
    
    // Digits 0-9
    if (char_idx >= 48 && char_idx <= 57) {
        let d = char_idx - 48;
        if (d == 0) { return x!=1 || (y!=1 && y!=2 && y!=3); }
        if (d == 1) { return x == 1; } 
        if (d == 2) { return y==0 || y==2 || y==4 || (x==2 && y==1) || (x==0 && y==3); }
        if (d == 3) { return y==0 || y==2 || y==4 || x==2; }
        if (d == 4) { return y==2 || x==2 || (x==0 && y<2); }
        if (d == 5) { return y==0 || y==2 || y==4 || (x==0 && y==1) || (x==2 && y==3); }
        if (d == 6) { return y==0 || y==2 || y==4 || x==0 || (x==2 && y>2); }
        if (d == 7) { return y==0 || x==2; }
        if (d == 8) { return y==0 || y==2 || y==4 || x==0 || x==2; }
        if (d == 9) { return y==0 || y==2 || y==4 || x==2 || (x==0 && y<2); }
    }
    return false;
}

@fragment
fn fs_main(@builtin(position) frag_coord: vec4<f32>) -> @location(0) vec4<f32> {
    let screen_pos = frag_coord.xy;
    let center = u.screen_size * 0.5;
    
    // --- Config ---
    let bar_width = 300.0;
    let bar_height = 3.0; // Fixed 3px
    
    // Pure Black Background
    var color = vec3<f32>(0.0, 0.0, 0.0);
    
    // --- Progress Bar ---
    let half_w = bar_width * 0.5;
    let half_h = bar_height * 0.5;
    let dx = abs(screen_pos.x - center.x);
    let dy = abs(screen_pos.y - center.y); // Centered vertically
    
    // Background Line (Dark Grey)
    if (dx < half_w && dy < half_h) { color = vec3<f32>(0.1, 0.1, 0.1); }
    
    // Filled Line (Pure White)
    let fill_w = bar_width * u.progress;
    let start_x = center.x - half_w;
    if (screen_pos.x >= start_x && screen_pos.x < start_x + fill_w) {
        if (dy < half_h) { color = vec3<f32>(1.0, 1.0, 1.0); }
    }
    
    // --- Text: "Loading XX%" ---
    let scale = 3.0;
    let char_w = 3.0 * scale; 
    let char_h = 5.0 * scale;
    let spacing = 2.0 * scale;
    
    let pct = i32(clamp(u.progress * 100.0, 0.0, 100.0));
    
    var num_digits = 1;
    if (pct >= 10) { num_digits = 2; }
    if (pct >= 100) { num_digits = 3; }
    
    let total_chars = 8 + num_digits + 1; // "Loading " + Digits + "%"
    let total_w = f32(total_chars) * (char_w + spacing) - spacing;
    
    let text_start_x = center.x - total_w * 0.5;
    let text_start_y = center.y - 30.0; // 30px ABOVE bar
    
    if (screen_pos.y >= text_start_y && screen_pos.y < text_start_y + char_h) {
        let rel_x = screen_pos.x - text_start_x;
        if (rel_x >= 0.0 && rel_x < total_w) {
            let slot = i32(rel_x / (char_w + spacing));
            let in_x = rel_x % (char_w + spacing);
            
            if (in_x < char_w) {
                let gx = i32(in_x / scale);
                let gy = i32((screen_pos.y - text_start_y) / scale);
                
                var c = 32;
                if (slot == 0) { c = 76; } // L
                else if (slot == 1) { c = 111; } // o
                else if (slot == 2) { c = 97; } // a
                else if (slot == 3) { c = 100; } // d
                else if (slot == 4) { c = 105; } // i
                else if (slot == 5) { c = 110; } // n
                else if (slot == 6) { c = 103; } // g
                else if (slot == 7) { c = 32; } // Space
                else if (slot < 8 + num_digits) {
                    let d_idx = slot - 8;
                    var d = 0;
                    if (num_digits == 3) { if (d_idx==0) {d=pct/100;} if (d_idx==1) {d=(pct/10)%10;} if (d_idx==2) {d=pct%10;} }
                    else if (num_digits == 2) { if (d_idx==0) {d=pct/10;} if (d_idx==1) {d=pct%10;} }
                    else { d=pct; }
                    c = 48 + d;
                } else { c = 37; } // %
                
                if (has_pixel(c, gx, gy)) { color = vec3<f32>(1.0, 1.0, 1.0); }
            }
        }
    }

    return vec4<f32>(color, 1.0);
}
"#;