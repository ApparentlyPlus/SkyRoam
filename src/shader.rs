// SCENE SHADER (Directional Light + Black Sky)
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
    // 1. DIRECTIONAL LIGHTING (Fixes "blob" look)
    // "Sun" direction coming from top-right-front
    let sun_dir = normalize(vec3<f32>(0.5, 1.0, 0.5));
    let normal = normalize(in.normal);
    
    // Diffuse shading (Dot product)
    // range 0.2 to 1.0 so shadows aren't pitch black
    let diff = max(dot(normal, sun_dir), 0.0);
    let light = 0.2 + (diff * 0.8);

    // 2. HEIGHT GRADIENT (Subtle)
    // Darkens the base of tall buildings slightly to ground them
    let height_gradient = clamp((in.world_pos.y + 20.0) / 150.0, 0.4, 1.0);
    
    // Combine base color + lighting + gradient
    let lit_color = in.color * light * height_gradient;

    // 3. FOG (PURE BLACK)
    let dist = distance(in.world_pos, camera.camera_pos.xyz);
    let fog_factor = smoothstep(camera.fog_dist.x, camera.fog_dist.y, dist);
    let fog_color = vec3<f32>(0.0, 0.0, 0.0);

    return vec4<f32>(mix(lit_color, fog_color, fog_factor), 1.0);
}
"#;

// UI SHADER (Unchanged - White Dot)
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
    let center = vec2<f32>(0.5, 0.5);
    let dist = distance(in.uv, center);
    if (dist > 0.5) { discard; }
    return vec4<f32>(1.0, 1.0, 1.0, 1.0);
}
"#;