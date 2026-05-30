# bevy_voxel

This is a package for working with voxels in Bevy. This is not the first package for working with voxels in Bevy. My hope is that we can make a general enough API that we can serve a large number of use cases, and hopefully start to deduplicate some of the work in this area.

The API so far:
- `Grid<T>` is a component that holds a grid of data in chunks.
- `Boundary` is a marker component for `Grid`s that should be treated as if there is more outside their boundary. Later we'll add some functionality to this, so it can be used eg in procedural generation for finding which chunks to generate next.
- `BodyTracker` tracks the connectivity of the `Grid` it's attached to. You can use it to iterate over the connected components of the `Grid`, and then to iterate over the voxels in each component. If the entity has a `Boundary`, then any voxels connected to the boundary will be considered connected to each other.

So far most of the interesting stuff is in `connectivity.rs` and the example `falling_sand.rs`.

One big goal is to make it easy to set up compute shaders to run on these grids. If it's not too inflexible, we can handle the passing of the chunks to the GPU, and provide a wgsl library with some helpers for accessing the voxels from the correct chunk.

One big decision is how to store the chunks. One option is to make the chunks components and put them on child entities. We'll need a child per chunk anyways for the colliders and meshes, and this way we also get change detection. I tried this in the `chunks_are_components` branch but I don't think it's the move.
