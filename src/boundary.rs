use std::collections::HashSet;

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::{Element, Grid};

/// The set of unloaded chunk indices adjacent to loaded ones, mirrored as a
/// sensor [`Collider`] on the same entity for spatial queries.
#[derive(Component, Default)]
pub struct BoundaryCollider {
    pub chunks: HashSet<IVec3>,
}

pub(crate) fn sync_boundary_collider(
    mut commands: Commands,
    mut grids: Query<(Entity, &mut Grid, &mut BoundaryCollider)>,
) {
    for (e, mut grid, mut boundary) in &mut grids {
        let new_chunks = std::mem::take(&mut grid.new_chunks);
        if new_chunks.is_empty() {
            continue;
        }
        for c in &new_chunks {
            boundary.chunks.remove(c);
            for d in Element::FACES {
                let n = *c + d;
                if !grid.voxels.contains_key(&n) {
                    boundary.chunks.insert(n);
                }
            }
        }

        let mut ent = commands.entity(e);
        if boundary.chunks.is_empty() {
            ent.remove::<Collider>();
        } else {
            let voxels: Vec<IVec3> = boundary.chunks.iter().copied().collect();
            ent.insert((Collider::voxels(Vec3::ONE, &voxels), Sensor));
        }
    }
}
