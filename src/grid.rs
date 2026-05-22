use bevy::math::*;
use bevy::prelude::*;

use std::collections::*;
use std::ops::*;

use crate::prelude::*;

pub const N: usize = 16;
pub const DIMS: IVec3 = IVec3::splat(N as i32);

/* --------------------------- chunk --------------------------- */

#[derive(Default)]
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
    pub chunks: HashMap<IVec3, Box<Chunk<T>>>,
    // dirty_chunks: HashSet<IVec3>,
    // children: HashMap<IVec3, Entity>,
}

impl<T: Default + Copy + PartialEq> Grid<T> {
    /// This will allocate a chunk if `idx` is out of bounds.
    pub fn set(&mut self, idx: IVec3, val: T) {
        let major = idx.div_euclid(DIMS);
        let minor = idx.rem_euclid(DIMS);

        self.chunks.entry(major).or_default()[minor] = val;
    }

    /// counts the number of cells that are `!= T::default()`
    pub fn count(&self) -> usize {
        let mut count = 0;
        for chunk in self.chunks.values() {
            for idx in prism(IVec3::ZERO, DIMS) {
                if chunk[idx] != T::default() {
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
            // dirty_chunks: HashSet::new(),
            // children: HashMap::new(),
        }
    }

    pub fn get(&self, idx: IVec3) -> Option<&T> {
        let major = idx.div_euclid(DIMS);
        let minor = idx.rem_euclid(DIMS);

        self.chunks.get(&major).map(|chunk| &chunk[minor])
    }

    pub fn get_mut(&mut self, idx: IVec3) -> Option<&mut T> {
        let major = idx.div_euclid(DIMS);
        let minor = idx.rem_euclid(DIMS);

        self.chunks.get_mut(&major).map(|chunk| &mut chunk[minor])
    }
}
