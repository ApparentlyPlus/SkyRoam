// This is the WGSL shader source code used for rendering.
// I embedded it here as a raw string cuz its cooler and doesn't need external files, yippie

pub const SOURCE: &str = r#"

struct CameraUniform {
    view_proj: mat4x4<f32>,
    screen_size: vec2<f32>,
    fog_dist: vec2<f32>, // x = fog_start, y = fog_end
    camera_pos: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
};

struct InstanceInput {
    @location(5) pos: vec3<f32>,
    @location(6) scale: vec3<f32>,
    @location(7) color_val: f32, 
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
    @location(1) world_pos: vec3<f32>,
    @location(2) normal: vec3<f32>,
};

@vertex
fn vs_main(model: VertexInput, instance: InstanceInput) -> VertexOutput {
    let scaled_pos = model.position * instance.scale;
    let world_pos_vec3 = scaled_pos + instance.pos;
    
    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(world_pos_vec3, 1.0);
    out.world_pos = world_pos_vec3;
    out.color = vec3<f32>(instance.color_val, instance.color_val, instance.color_val);
    out.normal = model.normal;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // crosshair
    let center = camera.screen_size * 0.5;
    let screen_dist = distance(in.clip_position.xy, center);
    if (screen_dist < 2.5) {
        return vec4<f32>(1.0, 1.0, 1.0, 1.0);
    }

    // lighting for cubes
    var brightness = 0.5;
    let abs_norm = abs(in.normal);
    if (abs_norm.y > 0.9) { brightness = 1.0; } 
    else if (abs_norm.x > 0.9) { brightness = 0.6; } 
    else if (abs_norm.z > 0.9) { brightness = 0.3; }

    // gradient for visibility
    let height_gradient = clamp(in.world_pos.y / 150.0, 0.5, 1.0);
    let lit_color = in.color * brightness * height_gradient;

    // fog stuff
    // Fog color is black to fade into the void
    let dist = distance(in.world_pos, camera.camera_pos.xyz);
    let fog_start = camera.fog_dist.x; 
    let fog_end = camera.fog_dist.y; 
    
    // Smoothstep creates a smoother transition than linear clamp
    let fog_factor = smoothstep(fog_start, fog_end, dist);
    
    return vec4<f32>(mix(lit_color, vec3<f32>(0.0, 0.0, 0.0), fog_factor), 1.0);
}
"#;