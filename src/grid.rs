use bevy::ecs::change_detection::*;
use bevy::math::*;
use bevy::prelude::*;

use std::collections::*;
use std::ops::*;

use crate::prelude::*;

pub const N: usize = 16;
pub const DIMS: IVec3 = IVec3::splat(N as i32);

/* --------------------------- chunk --------------------------- */

#[derive(Default)]
pub struct Chunk<T> {
    data: [[[T; N]; N]; N],
}

impl<T: Copy> Chunk<T> {
    pub fn new(fill: T) -> Self {
        Self {
            data: [[[fill; N]; N]; N],
        }
    }
}

impl<T> Chunk<T> {
    pub fn get(&self, idx: IVec3) -> Option<&T> {
        self.data
            .get(idx.y as usize)?
            .get(idx.z as usize)?
            .get(idx.x as usize)
    }

    pub fn get_mut(&mut self, idx: IVec3) -> Option<&mut T> {
        self.data
            .get_mut(idx.y as usize)?
            .get_mut(idx.z as usize)?
            .get_mut(idx.x as usize)
    }
}

impl<T> Index<IVec3> for Chunk<T> {
    type Output = T;

    fn index(&self, idx: IVec3) -> &Self::Output {
        &self.data[idx.y as usize][idx.z as usize][idx.x as usize]
    }
}

impl<T> IndexMut<IVec3> for Chunk<T> {
    fn index_mut(&mut self, idx: IVec3) -> &mut Self::Output {
        &mut self.data[idx.y as usize][idx.z as usize][idx.x as usize]
    }
}

/* --------------------------- grid ---------------------------- */

struct Entry<T> {
    chunk: Box<Chunk<T>>,
    changed: Tick,
    child: Option<Entity>,
}

#[derive(Component)]
pub struct Grid<T> {
    chunks: HashMap<IVec3, Entry<T>>,
}

impl<T: Default + Copy + PartialEq> Grid<T> {
    /// counts the number of cells that are `!= T::default()`
    pub fn count(&self) -> usize {
        let mut count = 0;
        for chunk in self.chunks.values() {
            for idx in prism(IVec3::ZERO, DIMS) {
                if chunk.chunk[idx] != T::default() {
                    count += 1;
                }
            }
        }
        count
    }
}

impl<T> Grid<T> {
    pub fn new() -> Self {
        Self {
            chunks: HashMap::new(),
        }
    }

    pub fn get(&self, idx: IVec3) -> Option<&T> {
        let major = idx.div_euclid(DIMS);
        let minor = idx.rem_euclid(DIMS);

        self.chunks.get(&major).map(|chunk| &chunk.chunk[minor])
    }

    pub fn get_chunk(&self, idx: IVec3) -> Option<&Chunk<T>> {
        self.chunks.get(&idx).map(|chunk| &*chunk.chunk)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&IVec3, &Chunk<T>)> {
        self.chunks.iter().map(|(k, v)| (k, &*v.chunk))
    }

    fn check_change_ticks(&mut self, check: CheckChangeTicks) {
        for (_, chunk) in &mut self.chunks {
            chunk.changed.check_tick(check);
        }
    }
}

/* ------------------------- mutation -------------------------- */

/// Extension trait for Mut<Grid>.
pub trait GridMut {
    type T;
    fn set(&mut self, idx: IVec3, val: Self::T);
    fn get_mut(&mut self, idx: IVec3) -> Option<&mut Self::T>;
}

impl<'w, T: Default> GridMut for Mut<'w, Grid<T>> {
    type T = T;

    /// This will allocate a chunk if `idx` is out of bounds.
    fn set(&mut self, idx: IVec3, val: Self::T) {
        let major = idx.div_euclid(DIMS);
        let minor = idx.rem_euclid(DIMS);

        // in order to get current tick, we have to update our changed
        // tick and then get our last change
        self.deref_mut();
        let changed = self.last_changed();

        self.chunks
            .entry(major)
            .or_insert_with(|| Entry {
                chunk: Box::new(Chunk::default()),
                changed,
                child: None,
            })
            .chunk[minor] = val;
    }

    fn get_mut(&mut self, idx: IVec3) -> Option<&mut Self::T> {
        let major = idx.div_euclid(DIMS);
        let minor = idx.rem_euclid(DIMS);

        // in order to get current tick, we have to update our changed
        // tick and then get our last change
        self.deref_mut();
        let changed = self.last_changed();

        self.chunks.get_mut(&major).map(|chunk| {
            chunk.changed = changed;
            &mut chunk.chunk[minor]
        })
    }
}

/* -------------------------- plugin --------------------------- */

#[derive(Default)]
pub struct VoxelPlugin<T> {
    marker: std::marker::PhantomData<T>,
}

impl<T: Send + Sync + 'static> Plugin for VoxelPlugin<T> {
    fn build(&self, app: &mut App) {
        app.add_observer(
            |check: On<bevy::ecs::change_detection::CheckChangeTicks>,
             grids: Query<&mut Grid<T>>| {
                for mut grid in grids {
                    // idk if it matters whether grid is marked as changed
                    // when we wrap the ticks...
                    grid.bypass_change_detection().check_change_ticks(*check)
                }
            },
        );
    }
}
