struct Data {
    // from camera to screen
    proj: mat4x4<f32>,
    // from screen to camera
    proj_inv: mat4x4<f32>,
    // from world to camera
    view: mat4x4<f32>,
    // camera position
    cam_pos: vec4<f32>,
};
@group(0)
@binding(0)
var<uniform> r_data: Data;

struct EntityOutput {
    @builtin(position) position: vec4<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) texture: vec2<f32>,
    @location(3) view: vec3<f32>,
};

@vertex
fn vs_mirror(
    @builtin(instance_index) instance_index: u32,
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) texture: vec2<f32>,
) -> EntityOutput {
    var result: EntityOutput;
    result.normal = normal;
    result.view = pos - r_data.cam_pos.xyz;
    result.position = r_data.proj * r_data.view * instance_matrix[instance_index] *local_matrix * vec4<f32>(pos, 1.0);

    result.texture = texture;
    return result;
}

//@group(0)
//@binding(1)
//var mirror_texture: texture_2d<f32>;

@fragment
fn fs_mirror(vertex: EntityOutput) -> @location(0) vec4<f32> {
    return (1.0f,1.0f,1.0f,1.0f);
}
