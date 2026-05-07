#import noisy_bevy::fbm_simplex_3d_seeded

struct Params {
    chunk_idx: vec3<i32>,
    map_w: u32,
    seed_u: vec3<f32>,
    map_h: u32,
    seed_v: vec3<f32>,
    _pad: u32,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var gen_map: texture_2d<u32>;
@group(0) @binding(2) var<storage, read_write> output: array<u32>;

const N: u32 = 16u;
const H: f32 = 1.0 / 16.0;

@compute @workgroup_size(4, 4, 4)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let local = vec3<f32>(f32(gid.x), f32(gid.y), f32(gid.z));
    let p = vec3<f32>(params.chunk_idx) + local * H;
    let w = f32(params.map_w);
    let h = f32(params.map_h);
    let u = fbm_simplex_3d_seeded(p / w, 3, 2.0, 0.5, params.seed_u) * 0.5 + 0.5;
    let v = fbm_simplex_3d_seeded(p / h, 3, 2.0, 0.5, params.seed_v) * 0.25;
    let sx = clamp(i32(u * w), 0, i32(params.map_w) - 1);
    let sy = i32(v * h - p.y);
    var tag: u32 = 0u;
    if (sy >= 0 && sy < i32(params.map_h)) {
        tag = textureLoad(gen_map, vec2<i32>(sx, sy), 0).r;
    }
    let lin = gid.x + N * (gid.y + N * gid.z);
    output[lin] = tag;
}
