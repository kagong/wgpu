struct SkyOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec3<f32>,
};

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

@vertex
fn vs_sky(@builtin(vertex_index) vertex_index: u32) -> SkyOutput {
    // hacky way to draw a large triangle
    let tmp1 = i32(vertex_index) / 2;
    let tmp2 = i32(vertex_index) & 1;
    let pos = vec4<f32>(
        f32(tmp1) * 4.0 - 1.0,
        f32(tmp2) * 4.0 - 1.0,
        1.0,
        1.0
    );

    // transposition = inversion for this orthonormal matrix
    let inv_model_view = transpose(mat3x3<f32>(r_data.view.x.xyz, r_data.view.y.xyz, r_data.view.z.xyz));
    let unprojected = r_data.proj_inv * pos;

    var result: SkyOutput;
    result.uv = inv_model_view * unprojected.xyz;
    result.position = pos;
    return result;
}

@group(1)
@binding(0)
var r_texture: texture_cube<f32>;

@group(1)
@binding(1)
var r_sampler: sampler;

@fragment
fn fs_sky(vertex: SkyOutput) -> @location(0) vec4<f32> {
    return textureSample(r_texture, r_sampler, vertex.uv);
}