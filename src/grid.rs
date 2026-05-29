use bevy::ecs::query::QueryEntityError;
use bevy::ecs::system::*;
use bevy::math::*;
use bevy::prelude::*;

use std::collections::*;
use std::ops::*;

use crate::prelude::*;

/* --------------------------- chunk --------------------------- */

pub const N: usize = 16;
pub const DIMS: IVec3 = IVec3::splat(N as i32);

#[derive(Component, Default)]
pub struct Chunk<T>([[[T; N]; N]; N]);

impl<T: Copy> Chunk<T> {
    pub fn new(fill: T) -> Self {
        Self([[[fill; N]; N]; N])
    }
}

impl<T> Chunk<T> {
    pub fn get(&self, idx: IVec3) -> Option<&T> {
        self.0
            .get(idx.y as usize)?
            .get(idx.z as usize)?
            .get(idx.x as usize)
    }

    pub fn get_mut(&mut self, idx: IVec3) -> Option<&mut T> {
        self.0
            .get_mut(idx.y as usize)?
            .get_mut(idx.z as usize)?
            .get_mut(idx.x as usize)
    }
}

impl<T> Index<IVec3> for Chunk<T> {
    type Output = T;

    fn index(&self, idx: IVec3) -> &Self::Output {
        &self.0[idx.y as usize][idx.z as usize][idx.x as usize]
    }
}

impl<T> IndexMut<IVec3> for Chunk<T> {
    fn index_mut(&mut self, idx: IVec3) -> &mut Self::Output {
        &mut self.0[idx.y as usize][idx.z as usize][idx.x as usize]
    }
}

/* --------------------------- grid ---------------------------- */

#[derive(Component)]
pub struct Grid<T> {
    // todo: try btreemap and btreemap with zordered keys, see what's fastest
    chunks: HashMap<IVec3, Entity>,
    marker: std::marker::PhantomData<T>,
}

impl<T> Grid<T> {
    pub fn new() -> Self {
        Self {
            chunks: HashMap::new(),
            marker: std::marker::PhantomData,
        }
    }
}

/* ------------------- gridsmut systemparam -------------------- */

// we could add a type parameter for query filters, but that wouldn't help us
// prove exclusivity of the chunks queries. however, maybe we could manually
// implement SystemParam and allow overlapping mutable queries on the chunks
// as long as the grids are not overlapping, so long as we make sure we only
// ever access chunks that belong to a grid in our query, and the user can't
// take out their own chunks query at the same time.
#[derive(SystemParam)]
pub struct GridsMut<'w, 's, T: Default + Send + Sync + 'static> {
    grids: Query<'w, 's, &'static mut Grid<T>>,
    chunks: Query<'w, 's, &'static mut Chunk<T>>,
    // cmd has to come before new_chunks so that the deferred spawn happens
    // before we try to insert a component on it.
    // todo check if this is even true.
    cmd: Commands<'w, 's>,
    new_chunks: Deferred<'s, NewChunks<T>>,
}

#[derive(Default)]
struct NewChunks<T>(HashMap<Entity, Chunk<T>>);

pub struct GridMut<'w, 's, T: Default + Send + Sync + 'static> {
    param: &'w mut GridsMut<'w, 's, T>,
    entity: Entity,
}

impl<'w, 's, T: Default + Send + Sync + 'static> GridsMut<'w, 's, T> {
    pub fn grid(&'w mut self, entity: Entity) -> Result<GridMut<'w, 's, T>, QueryEntityError> {
        self.grids.get_mut(entity)?;
        Ok(GridMut {
            param: self,
            entity,
        })
    }
}

impl<T: Default + Send + Sync + 'static> SystemBuffer for NewChunks<T> {
    fn apply(&mut self, _: &SystemMeta, world: &mut World) {
        for (entity, chunk) in self.0.drain() {
            world.entity_mut(entity).insert(chunk);
        }
    }
}

impl<T> Deref for NewChunks<T> {
    type Target = HashMap<Entity, Chunk<T>>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl<T> DerefMut for NewChunks<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<'w, 's, T: Default + Copy + PartialEq + Send + Sync + 'static> GridMut<'w, 's, T> {
    /// This will allocate a chunk if `idx` is out of bounds.
    pub fn set(&mut self, idx: IVec3, val: T) {
        let major = idx.div_euclid(DIMS);
        let minor = idx.rem_euclid(DIMS);

        let mut grid = self
            .param
            .grids
            .get_mut(self.entity)
            .expect("grid has disappeared");
        if let Some(entity) = grid.chunks.get(&major) {
            if let Some(chunk) = self.param.new_chunks.get_mut(entity) {
                chunk[minor] = val;
            } else if let Ok(mut chunk) = self.param.chunks.get_mut(*entity) {
                chunk[minor] = val;
            } else {
                panic!("Grid can't find its child");
            }
        } else {
            let mut chunk = Chunk::new(T::default());
            chunk[minor] = val;
            let child = self.param.cmd.spawn(chunk).id();
            grid.chunks.insert(major, child);
            self.param.cmd.entity(self.entity).add_child(child);
        }
    }

    /// counts the number of cells that are `!= T::default()`
    pub fn count(&self) -> usize {
        let mut count = 0;
        let grid = self
            .param
            .grids
            .get(self.entity)
            .expect("grid has disappeared");
        for entity in grid.chunks.values() {
            let chunk = if let Some(chunk) = self.param.new_chunks.get(entity) {
                chunk
            } else if let Ok(chunk) = self.param.chunks.get(*entity) {
                chunk
            } else {
                panic!("Grid can't find its child");
            };
            for idx in prism(IVec3::ZERO, DIMS) {
                if chunk[idx] != T::default() {
                    count += 1;
                }
            }
        }
        count
    }

    pub fn get(&'w self, idx: IVec3) -> Option<&'w T> {
        let major = idx.div_euclid(DIMS);
        let minor = idx.rem_euclid(DIMS);

        let grid = self
            .param
            .grids
            .get(self.entity)
            .expect("grid has disappeared");
        if let Some(entity) = grid.chunks.get(&major) {
            if let Some(chunk) = self.param.new_chunks.get(entity) {
                Some(&chunk[minor])
            } else if let Ok(chunk) = self.param.chunks.get(*entity) {
                Some(&chunk[minor])
            } else {
                panic!("Grid can't find its child");
            }
        } else {
            None
        }
    }

    pub fn get_mut(&'w mut self, idx: IVec3) -> Option<&'w mut T> {
        let major = idx.div_euclid(DIMS);
        let minor = idx.rem_euclid(DIMS);

        let grid = self
            .param
            .grids
            .get(self.entity)
            .expect("grid has disappeared");
        if let Some(entity) = grid.chunks.get(&major) {
            if let Some(chunk) = self.param.new_chunks.get_mut(entity) {
                Some(&mut chunk[minor])
            } else if let Ok(chunk) = self.param.chunks.get_mut(*entity) {
                Some(&mut chunk.into_inner()[minor])
            } else {
                panic!("Grid can't find its child");
            }
        } else {
            None
        }
    }
}

/* -------------------- grids systemparam ---------------------- */

// we could add a type parameter for query filters, but that wouldn't help us
// prove exclusivity of the chunks queries. however, maybe we could manually
// implement SystemParam and allow overlapping mutable queries on the chunks
// as long as the grids are not overlapping, so long as we make sure we only
// ever access chunks that belong to a grid in our query, and the user can't
// take out their own chunks query at the same time.
#[derive(SystemParam)]
pub struct Grids<'w, 's, T: Send + Sync + 'static> {
    grids: Query<'w, 's, &'static Grid<T>>,
    chunks: Query<'w, 's, &'static Chunk<T>>,
}

pub struct GridRef<'w, 's, T: Send + Sync + 'static> {
    param: &'w Grids<'w, 's, T>,
    grid: &'w Grid<T>,
}

impl<'w, 's, T: Send + Sync + 'static> Grids<'w, 's, T> {
    pub fn grid(&'w self, entity: Entity) -> Result<GridRef<'w, 's, T>, QueryEntityError> {
        Ok(GridRef {
            param: self,
            grid: self.grids.get(entity)?,
        })
    }
}

impl<'w, 's, T: Default + PartialEq + Send + Sync + 'static> GridRef<'w, 's, T> {
    /// counts the number of cells that are `!= T::default()`
    pub fn count(&self) -> usize {
        let mut count = 0;
        for entity in self.grid.chunks.values() {
            let chunk = self
                .param
                .chunks
                .get(*entity)
                .expect("Grid's spatial index out of sync with children");
            for idx in prism(IVec3::ZERO, DIMS) {
                if chunk[idx] != T::default() {
                    count += 1;
                }
            }
        }
        count
    }
}

impl<'w, 's, T: Send + Sync + 'static> GridRef<'w, 's, T> {
    pub fn get(&self, idx: IVec3) -> Option<&T> {
        let major = idx.div_euclid(DIMS);
        let minor = idx.rem_euclid(DIMS);

        self.grid.chunks.get(&major).map(|entity| {
            &self
                .param
                .chunks
                .get(*entity)
                .expect("Grid can't find its child")[minor]
        })
    }

    pub fn get_chunk(&self, idx: IVec3) -> Option<&Chunk<T>> {
        self.grid.chunks.get(&idx).map(|entity| {
            self.param
                .chunks
                .get(*entity)
                .expect("Grid can't find its child")
        })
    }

    // todo this is almost definitely not the right api
    pub fn chunks(&self) -> impl Iterator<Item = (&IVec3, &Chunk<T>)> {
        self.grid
            .chunks
            .keys()
            .zip(self.param.chunks.iter_many(self.grid.chunks.values()))
    }
}
