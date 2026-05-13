#import noisy_bevy::fbm_simplex_3d_seeded

struct Params {
    palette_size: vec3<u32>,
    chunk_count: u32,
    seed_u: vec3<f32>,
    _pad0: u32,
    seed_v: vec3<f32>,
    _pad1: u32,
    seed_w: vec3<f32>,
    _pad2: u32,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var palette: texture_3d<u32>;
@group(0) @binding(2) var<storage, read> chunk_indices: array<vec4<i32>>;
@group(0) @binding(3) var<storage, read_write> output: array<u32>;

const N: u32 = 16u;
const H: f32 = 1.0 / 16.0;
const WORDS_PER_CHUNK: u32 = (N * N * N) / 4u;

// Each thread writes one u32 holding 4 tags packed along x.
// Dispatch: (1, N/4, chunk_count * N/4) workgroups of size (4, 4, 4)
//   → (4, N, chunk_count * N) total threads. gid.x picks the x-word
//     (4 voxels along x); gid.y is the voxel y; gid.z encodes
//     (chunk_id, local_z).
@compute @workgroup_size(4, 4, 4)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let chunk_id = gid.z / N;
    let local_z = gid.z % N;
    let chunk_idx = chunk_indices[chunk_id].xyz;
    let x_base = gid.x * 4u;

    let size = vec3<f32>(params.palette_size);
    var packed: u32 = 0u;
    for (var i = 0u; i < 4u; i = i + 1u) {
        let local = vec3<f32>(f32(x_base + i), f32(gid.y), f32(local_z));
        let p = vec3<f32>(chunk_idx) + local * H;
        let u = fbm_simplex_3d_seeded(p / size.x, 3, 12.0, 1./32., params.seed_u) * 0.5 + 0.5;
        let v = fbm_simplex_3d_seeded(p / size.y, 3, 12.0, 1./32., params.seed_v) * 0.5 + 0.5;
        let w = fbm_simplex_3d_seeded(p / size.z, 3, 12.0, 1./32., params.seed_w) * 0.125;
        let sx = clamp(i32(u * size.x), 0, i32(params.palette_size.x) - 1);
        let sy = i32(w * size.y + p.y);
        let sz = clamp(i32(v * size.z), 0, i32(params.palette_size.z) - 1);
        var tag: u32 = 0u;
        if (sy >= 0 && sy < i32(params.palette_size.y)) {
            tag = textureLoad(palette, vec3<i32>(sx, sz, sy), 0).r;
        }
        packed = packed | ((tag & 0xffu) << (i * 8u));
    }

    let word = chunk_id * WORDS_PER_CHUNK
        + (x_base / 4u)
        + (N / 4u) * (gid.y + N * local_z);
    output[word] = packed;
}
