use std::collections::*;

use bevy::math::*;
use bevy::prelude::*;

use bitvec::prelude::*;

use crate::prelude::*;

// terminology:
// body = connected component
// piece = chunk-body intersection

// we need to iterate over bodies in three ways:
// 1) all bodies in a chunk. here we can use the two pass algorithm.
// 2) all bodies in the grid.
// 3) one body. here we want to expose an iterator. this will have to be stackful.
//
// on second thought the two pass algorithm has to allocate and branch anyways, so let's just do DFS
//
// we also want to support finding all bodies that do not contain the boundary.
// we can treat the boundary as special and make it come first when iterating...

pub trait Connector: Send + Sync + 'static {
    type Item: Send + Sync + 'static;
    fn solid(item: &Self::Item) -> bool;
}

/* ------------------------- component ------------------------- */

type Piece = Option<(IVec3, i16)>;
type Liberties<const N_SQUARE: usize = 256> = BitArray<[usize; N_SQUARE], Lsb0>;

#[derive(Component)]
pub struct BodyTracker<C: Connector, const N: usize = 16, const N_SQUARE: usize = 256> {
    /// face_bodies[chunk][face] is a list of (piece_id, cross_chunk_connection_points) pairs.
    face_bodies: HashMap<IVec3, [Vec<(i16, Liberties<N_SQUARE>)>; 6]>,
    /// graph[piece] is the list of pieces it touches
    graph: HashMap<Piece, Vec<Piece>>,
    /// reps[chunk][id] is a voxel representative for the piece
    reps: HashMap<IVec3, Vec<IVec3>>,
    marker: std::marker::PhantomData<C>,
}

impl<C: Connector, const N: usize, const N_SQUARE: usize> BodyTracker<C, N, N_SQUARE> {
    pub fn new() -> Self {
        Self {
            face_bodies: HashMap::new(),
            graph: HashMap::new(),
            reps: HashMap::new(),
            marker: std::marker::PhantomData,
        }
    }

    pub fn bodies<'a>(&'a self, grid: &'a Grid<C::Item, N>) -> Bodies<'a, C, N, N_SQUARE> {
        Bodies {
            tracker: self,
            grid,
            unvisited: self.graph.keys().copied().collect(),
        }
    }
}

/* --------------------------- system -------------------------- */

fn check_connectivity<C: Connector, const N: usize, const N_SQUARE: usize>(
    grids: Query<
        (
            &Grid<C::Item, N>,
            &mut BodyTracker<C, N, N_SQUARE>,
            Option<&Boundary>,
        ),
        Changed<Grid<C::Item, N>>,
    >,
) {
    let mut stack = Vec::new();
    for (grid, bodies, boundary) in grids {
        let bodies = bodies.into_inner();
        // I think it might be best if we rebuild this from scratch...
        bodies.graph.clear();
        if let Some(_) = boundary {
            bodies.graph.insert(None, Vec::new());
        }

        // todo add dirty chunking
        for (chunk_idx, chunk) in &grid.chunks {
            bodies.face_bodies.insert(*chunk_idx, Default::default());
            bodies.reps.entry(*chunk_idx).or_default().clear();

            let mut body_ids = Chunk::<i16, N>::new(-1);
            let mut current_id = 0;
            for idx in prism(IVec3::ZERO, Chunk::<C::Item, N>::DIMS) {
                // we check solidity before pushing, because if we decide to
                // check connectivity that will have have to happen before pushing too.
                // we check if it's already in a body after popping because
                // that can change between pushing and popping.
                // we check if it's already in a body before starting the DFS
                // because we need to initialize the new body.
                if !C::solid(&chunk[idx]) || body_ids[idx] != -1 {
                    continue;
                }

                bodies
                    .graph
                    .insert(Some((*chunk_idx, current_id)), Vec::new());
                bodies.reps.get_mut(&chunk_idx).unwrap().push(idx);
                // assert! reps[chunk_idx].index_of(idx) == current_id

                /* -------------------- DFS -------------------- */

                // assert!(stack.len() == 0);
                stack.push(idx);
                while let Some(node) = stack.pop() {
                    if body_ids[node] != -1 {
                        continue;
                    }
                    body_ids[node] = current_id;
                    for face in Element::FACES.iter().rev() {
                        if let Some(nbr) = chunk.get(node + *face)
                            && C::solid(nbr)
                        {
                            stack.push(node + *face)
                        }
                    }
                }

                current_id += 1;
            }

            // collect a bit vector for the exposed faces of each piece
            for [i, j, k] in Element::FRAMES {
                for (sign, bound) in [(-1, 0), (1, N as i32 - 1)] {
                    let mut liberties = vec![Liberties::ZERO; current_id as usize];
                    for u in 0..N as i32 {
                        for v in 0..N as i32 {
                            let id = body_ids
                                [i.as_ivec3() * bound + j.as_ivec3() * u + k.as_ivec3() * v];
                            if id != -1 {
                                *liberties[id as usize]
                                    .get_mut(N * u as usize + v as usize)
                                    .unwrap() = true;
                            }
                        }
                    }
                    for (id, libs) in liberties.iter().enumerate() {
                        if libs.any() {
                            bodies.face_bodies.entry(*chunk_idx).or_default()
                                [(i * sign).trailing_zeros() as usize]
                                .push((id as i16, *libs));
                        }
                    }
                }
            }
        }

        // link chunks to their neighbors
        // tbd if we should keep the graph for clean chunks and just do this for dirty chunks
        for chunk_idx in grid.chunks.keys() {
            for face in Element::FACES {
                let Some(face_bodies) = bodies.face_bodies.get(&chunk_idx) else {
                    continue;
                };
                let Some(other_face_bodies) = bodies.face_bodies.get(&(*chunk_idx + face)) else {
                    match boundary {
                        // we need to make sure that face_bodies.get(chunk) == None
                        // implies that the chunk is not in the grid,
                        // ie face_bodies.keys() == grid.chunks.keys()
                        Some(_) => {
                            for (id, _) in &face_bodies[face.trailing_zeros() as usize] {
                                let a = Some((*chunk_idx, *id));
                                let b = None;
                                bodies.graph.get_mut(&a).unwrap().push(b);
                                bodies.graph.get_mut(&b).unwrap().push(a);
                            }
                        }
                        None => (),
                    }
                    continue;
                };
                for (id, libs) in &face_bodies[face.trailing_zeros() as usize] {
                    for (other_id, other_libs) in
                        &other_face_bodies[(-face).trailing_zeros() as usize]
                    {
                        if (*libs & other_libs).any() {
                            let a = Some((*chunk_idx, *id));
                            let b = Some((*chunk_idx + face, *other_id));
                            bodies.graph.get_mut(&a).unwrap().push(b);
                            bodies.graph.get_mut(&b).unwrap().push(a);
                        }
                    }
                }
            }
        }

        // todo: should we DFS the graph here and store the list of bodies, or do DFS when
        // the user iterates over the bodies? because if they want to iterate over the voxels
        // then we'll have to run DFS then. so we shouldn't duplicate the work. on the other hand
        // if the grid is only changing infrequently and the user checks for new bodies each frame...
        // but a) that's the cold path, and b) the user should use Changed to avoid this. so let's
        // leave it for the iterator.
    }
}

/* ------------------------- iterators ------------------------- */

pub struct Bodies<'a, C: Connector, const N: usize = 16, const N_SQUARE: usize = 256> {
    tracker: &'a BodyTracker<C, N, N_SQUARE>,
    grid: &'a Grid<C::Item, N>,
    unvisited: HashSet<Piece>,
}

impl<'a, C: Connector, const N: usize, const N_SQUARE: usize> Iterator
    for Bodies<'a, C, N, N_SQUARE>
{
    type Item = Body<'a, C, N, N_SQUARE>;

    fn next(&mut self) -> Option<Self::Item> {
        let &start = if self.unvisited.contains(&None) {
            &None
        } else {
            self.unvisited.iter().next()?
        };
        let mut pieces = Vec::new();
        let mut stack = vec![start];
        while let Some(node) = stack.pop() {
            if !self.unvisited.remove(&node) {
                continue;
            }
            stack.extend_from_slice(self.tracker.graph.get(&node).unwrap());
            pieces.push(node);
        }
        Some(Body {
            tracker: self.tracker,
            grid: self.grid,
            pieces,
            current_chunk: IVec3::ZERO,
            stack: Vec::new(),
            visited: Chunk::new(false),
        })
    }
}

pub struct Body<'a, C: Connector, const N: usize, const N_SQUARE: usize> {
    tracker: &'a BodyTracker<C, N, N_SQUARE>,
    grid: &'a Grid<C::Item, N>,
    pieces: Vec<Piece>,
    current_chunk: IVec3,
    stack: Vec<IVec3>,
    visited: Chunk<bool, N>,
}

impl<'a, C: Connector, const N: usize, const N_SQUARE: usize> Iterator
    for Body<'a, C, N, N_SQUARE>
{
    type Item = IVec3;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(vox) = self.stack.pop() {
            if self.visited[vox] {
                continue;
            }
            self.visited[vox] = true;
            let chunk = self.grid.chunks.get(&self.current_chunk).unwrap();
            for face in Element::FACES.iter().rev() {
                if let Some(nbr) = chunk.get(vox + *face)
                    && C::solid(nbr)
                {
                    self.stack.push(vox + *face)
                }
            }
            return Some(self.current_chunk * Chunk::<C::Item, N>::DIMS + vox);
        }

        match self.pieces.pop()? {
            Some((chunk_idx, piece_id)) => {
                self.current_chunk = chunk_idx;
                self.stack
                    .push(self.tracker.reps.get(&chunk_idx).unwrap()[piece_id as usize]);
                self.visited = Chunk::new(false);
            }
            None => (), // boundary piece: no items to yield
        }

        // should never recurse more than twice: once for each new piece, and we
        // can only traverse two pieces between yields if one is empty, which should
        // only be the boundary piece.
        return self.next();
    }
}

/* --------------------------- plugin -------------------------- */

pub struct ConnectivityPlugin<C: Connector, const N: usize, const N_SQUARE: usize> {
    marker: std::marker::PhantomData<C>,
}

impl<C: Connector, const N: usize, const N_SQUARE: usize> Plugin
    for ConnectivityPlugin<C, N, N_SQUARE>
{
    fn build(&self, app: &mut App) {
        app.add_systems(FixedUpdate, check_connectivity::<C, N, N_SQUARE>);
    }
}

impl<C: Connector, const N: usize, const N_SQUARE: usize> Default
    for ConnectivityPlugin<C, N, N_SQUARE>
{
    fn default() -> Self {
        Self {
            marker: std::marker::PhantomData,
        }
    }
}
