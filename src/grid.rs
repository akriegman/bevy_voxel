use bevy::math::*;
use bevy::prelude::*;

use std::collections::*;
use std::ops::*;

use crate::prelude::*;
/* --------------------------- chunk --------------------------- */

pub struct Chunk<T, const N: usize>([[[T; N]; N]; N]);

impl<T: Copy, const N: usize> Chunk<T, N> {
    pub const fn new(fill: T) -> Self {
        Self([[[fill; N]; N]; N])
    }
}

impl<T, const N: usize> Chunk<T, N> {
    pub const N: usize = N;
    pub const DIMS: IVec3 = IVec3::splat(N as i32);
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

impl<T, const N: usize> Index<IVec3> for Chunk<T, N> {
    type Output = T;

    fn index(&self, idx: IVec3) -> &Self::Output {
        &self.0[idx.y as usize][idx.z as usize][idx.x as usize]
    }
}

impl<T, const N: usize> IndexMut<IVec3> for Chunk<T, N> {
    fn index_mut(&mut self, idx: IVec3) -> &mut Self::Output {
        &mut self.0[idx.y as usize][idx.z as usize][idx.x as usize]
    }
}

/* --------------------------- grid ---------------------------- */

#[derive(Component, Default)]
pub struct Grid<T, const N: usize> {
    pub chunks: HashMap<IVec3, Box<Chunk<T, N>>>,
    // dirty_chunks: HashSet<IVec3>,
    // children: HashMap<IVec3, Entity>,
}

impl<T: Default + Copy + PartialEq, const N: usize> Grid<T, N> {
    /// This will allocate a chunk if `idx` is out of bounds.
    pub fn set(&mut self, idx: IVec3, val: T) {
        let major = idx.div_euclid(Chunk::<T, N>::DIMS);
        let minor = idx.rem_euclid(Chunk::<T, N>::DIMS);

        self.chunks
            .entry(major)
            .or_insert(Box::new(Chunk::new(T::default())))[minor] = val;
    }

    /// counts the number of cells that are `!= T::default()`
    pub fn count(&self) -> usize {
        let mut count = 0;
        for chunk in self.chunks.values() {
            for idx in prism(IVec3::ZERO, Chunk::<T, N>::DIMS) {
                if chunk[idx] != T::default() {
                    count += 1;
                }
            }
        }
        count
    }
}

impl<T, const N: usize> Grid<T, N> {
    pub fn new() -> Self {
        Self {
            chunks: HashMap::new(),
            // dirty_chunks: HashSet::new(),
            // children: HashMap::new(),
        }
    }

    pub fn get(&self, idx: IVec3) -> Option<&T> {
        let major = idx.div_euclid(Chunk::<T, N>::DIMS);
        let minor = idx.rem_euclid(Chunk::<T, N>::DIMS);

        self.chunks.get(&major).map(|chunk| &chunk[minor])
    }

    pub fn get_mut(&mut self, idx: IVec3) -> Option<&mut T> {
        let major = idx.div_euclid(Chunk::<T, N>::DIMS);
        let minor = idx.rem_euclid(Chunk::<T, N>::DIMS);

        self.chunks.get_mut(&major).map(|chunk| &mut chunk[minor])
    }
}
