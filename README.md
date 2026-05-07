# voxxelmaxx

A 3D falling sand game engine. And by game engine, I mean Bevy plugin.

The core idea is the `Grid` component, which can be thought of as an ECS for a uniform grid of entities with local systems, meaning systems that act on voxels and can only touch neighboring voxels.
> There's two ways to do this: we can individually install systems on each `Grid` and let the `Grid` handle iteration, or we can put the systems in the actual Bevy ECS, somehow make them query `Grid`s based on the types in the tuple they hold for each voxel, and let the system handle iteration, like how systems iterate over `Query`s.

## Engineering choices

We have some decisions to make. For each of these, we can either make a choice, or make the engine generic over the choices.

ways to prevent race conditions:
- atomics
- process non-adjacent chunks concurrently, one thread per chunk
- process non-adjacent voxels concurrently
- use a stateless update rule

ways to make adjacent voxels move together:
- process bottom to top
  - helps with common automata like water and sand, not fully general
- * two buffers
- just make it random

ways to render
- rasterize
- raymarch
  - people tell me this scales better with voxels. not sure it'll work with non cubes though. I guess we'll support both, bevy is already generic over rendering methods.

Conclusion: we'll use two buffers, leave race conditions up to the user so that they can either make things stateless or use atomics, and provide both rendering pipelines.

## Design

- Written in Rust with Bevy

- Chunks are 1x1x1 units, with variable voxels per chunk.

- Chunks live inside grids. A grid is a lattice of chunks that moves as one rigid body.

- Chunks have multiple buffers. Eg one buffer for the material tag, another buffer for water level.
  - > we can either have the buffers be type parameters, or use an entity component system to do "for each chunk with an X buffer do Y"

- Chunk -> render mesh and chunk -> collision mesh are overridable functions, but we provide sensible defaults and shared machinery such as triangle combining passes.

## Examples

Some systems we would like to support:

- procedural generation
  - this could be in a separate subapp...
- body detection
- rasterizing one grid onto another, conserving voxels
  - this is a stable matching problem. it could also be made an optimal transport problem, but stable matching is simpler and I think more natural.
- chunk -> collider
- chunk -> mesh
- cellular automota
  - margolus neighborhoods

I would also like to support some weird geometry.

- Instead of cube voxels, rhombic dodecahedral (rad) voxels. There's a few ways to achieve this:
  - Use a skewed lattice, where the origin and the three axis generators make a regular simplex
  - Use every other voxel of the cubic grid. Ie you checker the cubic grid, then you cut each white cell into 6 pyramids and glue them on to the 6 neighboring black cells
    - This would require either leaving half the elements unused in the buffers, or having the dimensions of the buffers not be all the same length
  - have four rad voxel per cell of the cubic grid

If we do not want to compromise on grids having orthonormal lattices and buffers being nxnxn, then we would have to use the third option.

- Instead of building on the cells of the grid, building on the faces.
  - Instead of a material tag per cell, we would need either:
    - Three buffers of material tags for the three orientations of faces
    - One buffer of structs holding three material tags each
