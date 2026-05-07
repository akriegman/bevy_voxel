mod boundary;
mod element;

use std::collections::{HashMap, HashSet};

use avian3d::prelude::*;
use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};

pub use boundary::BoundaryCollider;
pub use element::Element;

pub const N: usize = 16;
pub const H: f32 = 1.0 / N as f32;
const N_I: i32 = N as i32;

#[derive(Default)]
pub struct VoxelPlugin;

impl Plugin for VoxelPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(FixedUpdate, clean_body);
        app.add_systems(FixedUpdate, boundary::sync_boundary_collider);
    }
}

/// A voxel object. Owns the voxel data for one or more chunks; the plugin
/// manages plugin-private child entities for each chunk's mesh and collider.
/// Game code only ever touches `Grid` directly — chunk entities are not
/// part of the public API.
#[derive(Component, Default)]
#[require(Transform, Visibility)]
pub struct Grid {
    pub(crate) voxels: HashMap<IVec3, Box<[u8; N * N * N]>>,
    /// Chunks whose mesh + collider need a rebuild.
    dirty_chunks: HashSet<IVec3>,
    /// Chunks added since the last `BoundaryCollider` sync.
    pub(crate) new_chunks: HashSet<IVec3>,
    chunk_entities: HashMap<IVec3, Entity>,
}

#[derive(Resource)]
pub struct TerrainMaterial(pub Handle<StandardMaterial>);

impl Grid {
    pub fn len(&self) -> usize {
        self.voxels.len() * N * N * N
    }

    /// Add a chunk at `idx` if one isn't already loaded there, returning
    /// whether it was newly added. `tags` is invoked only on miss, so callers
    /// can defer expensive generation. The new chunk and any present
    /// neighbors are marked dirty (boundary face visibility depends on
    /// neighbors).
    pub fn add_chunk(&mut self, idx: IVec3, tags: impl FnOnce() -> Box<[u8; N * N * N]>) -> bool {
        if self.voxels.contains_key(&idx) {
            return false;
        }
        self.voxels.insert(idx, tags());
        self.dirty_chunks.insert(idx);
        self.new_chunks.insert(idx);
        for d in Element::FACES {
            let n = idx + d;
            if self.voxels.contains_key(&n) {
                self.dirty_chunks.insert(n);
            }
        }
        true
    }

    /// Given a raycast hit on this grid's collider, return the global voxel
    /// index of the voxel whose face was hit, plus which face. Assumes the
    /// grid is at the world origin; non-origin grids will need a transform.
    pub fn rayhit_face(&self, origin: Vec3, dir: Vec3, hit: &RayHitData) -> (IVec3, Element) {
        // Nudge half a voxel against the normal so the floor lands inside
        // the hit voxel rather than on the boundary.
        let p = origin + dir * hit.distance - hit.normal * (H * 0.5);
        let voxel = (p / H).floor().as_ivec3();
        (voxel, Element::from_normal(hit.normal))
    }

    pub fn get(&self, v: IVec3) -> u8 {
        let (chunk_idx, local) = split(v);
        self.voxels.get(&chunk_idx).map_or(0, |c| c[lin(local)])
    }

    /// Set voxel `v` to `tag`, returning the previous value. Marks the owning
    /// chunk dirty (and any boundary-touching neighbors so their meshes
    /// rebuild). No-op (returns 0) if no chunk covers `v`.
    pub fn set(&mut self, v: IVec3, tag: u8) -> u8 {
        let (chunk_idx, local) = split(v);
        let Some(chunk) = self.voxels.get_mut(&chunk_idx) else {
            return 0;
        };
        let i = lin(local);
        let prev = chunk[i];
        if prev == tag {
            return prev;
        }
        chunk[i] = tag;
        self.dirty_chunks.insert(chunk_idx);
        for axis in 0..3 {
            let step = if local[axis] == 0 {
                -1
            } else if local[axis] == N_I - 1 {
                1
            } else {
                continue;
            };
            let mut d = IVec3::ZERO;
            d[axis] = step;
            let n = chunk_idx + d;
            if self.voxels.contains_key(&n) {
                self.dirty_chunks.insert(n);
            }
        }
        prev
    }
}

fn split(v: IVec3) -> (IVec3, IVec3) {
    (
        IVec3::new(
            v.x.div_euclid(N_I),
            v.y.div_euclid(N_I),
            v.z.div_euclid(N_I),
        ),
        IVec3::new(
            v.x.rem_euclid(N_I),
            v.y.rem_euclid(N_I),
            v.z.rem_euclid(N_I),
        ),
    )
}

fn lin(local: IVec3) -> usize {
    local.x as usize + N * (local.y as usize + N * local.z as usize)
}

fn clean_body(
    mut commands: Commands,
    mut grids: Query<(Entity, &mut Grid)>,
    mut meshes: ResMut<Assets<Mesh>>,
    terrain_mat: Res<TerrainMaterial>,
) {
    for (body_e, mut grid) in &mut grids {
        if grid.dirty_chunks.is_empty() {
            continue;
        }
        let dirty: Vec<IVec3> = grid.dirty_chunks.drain().collect();
        for chunk_idx in dirty {
            if !grid.voxels.contains_key(&chunk_idx) {
                if let Some(e) = grid.chunk_entities.remove(&chunk_idx) {
                    commands.entity(e).despawn();
                }
                continue;
            }
            let mesh = chunk_to_mesh(chunk_idx, &grid.voxels);
            let chunk = grid.voxels.get(&chunk_idx).unwrap();
            let mut local_coords: Vec<IVec3> = Vec::new();
            for lz in 0..N {
                for ly in 0..N {
                    for lx in 0..N {
                        if chunk[lx + N * (ly + N * lz)] != 0 {
                            local_coords.push(IVec3::new(lx as i32, ly as i32, lz as i32));
                        }
                    }
                }
            }

            let entity = match grid.chunk_entities.get(&chunk_idx) {
                Some(&e) => e,
                None => {
                    let e = commands
                        .spawn((
                            Transform::from_translation(chunk_idx.as_vec3()),
                            Visibility::default(),
                        ))
                        .id();
                    commands.entity(body_e).add_child(e);
                    grid.chunk_entities.insert(chunk_idx, e);
                    e
                }
            };
            let mut ent = commands.entity(entity);
            ent.insert((
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(terrain_mat.0.clone()),
            ));
            // Avian's voxel collider panics on an empty input, so strip the
            // collider for fully-empty chunks instead of inserting one.
            if local_coords.is_empty() {
                ent.remove::<Collider>();
            } else {
                ent.insert(Collider::voxels(Vec3::splat(H), &local_coords));
            }
        }
    }
}

fn chunk_to_mesh(chunk_idx: IVec3, voxels: &HashMap<IVec3, Box<[u8; N * N * N]>>) -> Mesh {
    let chunk = voxels.get(&chunk_idx).unwrap();
    let n = N as i32;
    // Sample at local coord; if outside this chunk, walk into the neighbor
    // chunk so we don't draw a face between two solid voxels at a chunk seam.
    let get = |x: i32, y: i32, z: i32| -> u8 {
        if 0 <= x && x < n && 0 <= y && y < n && 0 <= z && z < n {
            return chunk[x as usize + N * (y as usize + N * z as usize)];
        }
        let mut nidx = chunk_idx;
        let mut lx = x;
        let mut ly = y;
        let mut lz = z;
        if x < 0 {
            nidx.x -= 1;
            lx = n - 1;
        } else if x >= n {
            nidx.x += 1;
            lx = 0;
        }
        if y < 0 {
            nidx.y -= 1;
            ly = n - 1;
        } else if y >= n {
            nidx.y += 1;
            ly = 0;
        }
        if z < 0 {
            nidx.z -= 1;
            lz = n - 1;
        } else if z >= n {
            nidx.z += 1;
            lz = 0;
        }
        voxels
            .get(&nidx)
            .map_or(0, |c| c[lx as usize + N * (ly as usize + N * lz as usize)])
    };

    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut colors: Vec<[f32; 4]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    // (axis, dir, u_axis, v_axis): u×v = dir*axis so winding is CCW from outside.
    let dirs = [
        (IVec3::X, 1, IVec3::Y, IVec3::Z),
        (IVec3::X, -1, IVec3::Z, IVec3::Y),
        (IVec3::Y, 1, IVec3::Z, IVec3::X),
        (IVec3::Y, -1, IVec3::X, IVec3::Z),
        (IVec3::Z, 1, IVec3::X, IVec3::Y),
        (IVec3::Z, -1, IVec3::Y, IVec3::X),
    ];

    for z in 0..n {
        for y in 0..n {
            for x in 0..n {
                let tag = get(x, y, z);
                if tag == 0 {
                    continue;
                }
                let color = tag_color(tag);
                let v = IVec3::new(x, y, z);
                for &(axis, dir, u_axis, v_axis) in &dirs {
                    let neighbor = v + axis * dir;
                    if get(neighbor.x, neighbor.y, neighbor.z) != 0 {
                        continue;
                    }
                    let origin = if dir > 0 { v + axis } else { v }.as_vec3();
                    let normal = (axis * dir).as_vec3().to_array();
                    let world = |p: Vec3| (p * H).to_array();
                    let base = positions.len() as u32;
                    let u = u_axis.as_vec3();
                    let w = v_axis.as_vec3();
                    for p in [origin, origin + u, origin + u + w, origin + w] {
                        positions.push(world(p));
                        normals.push(normal);
                        colors.push(color);
                    }
                    indices.extend_from_slice(&[
                        base,
                        base + 1,
                        base + 2,
                        base,
                        base + 2,
                        base + 3,
                    ]);
                }
            }
        }
    }

    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    .with_inserted_indices(Indices::U32(indices))
}

fn tag_color(tag: u8) -> [f32; 4] {
    let [r, g, b] = match tag {
        0x40 => [0.15, 0.35, 0.7],  // water
        0x80 => [0.5, 0.5, 0.52],   // stone
        0x81 => [0.45, 0.3, 0.18],  // dirt
        0x82 => [0.6, 0.42, 0.25],  // wood
        0x83 => [0.3, 0.2, 0.12],   // bark
        0x84 => [0.25, 0.55, 0.2],  // leaf
        0xc0 => [0.85, 0.78, 0.55], // sand
        _ => [1.0, 0.0, 1.0],
    };
    [r, g, b, 1.0]
}
