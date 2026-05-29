# bevy_voxel

This is a package for working with voxels in Bevy. This is not the first package for working with voxels in Bevy. My hope is that we can make a general enough API that we can serve a large number of use cases, and hopefully start to deduplicate some of the work in this area.

The API so far:
- `Grid<T>` is a component that holds a grid of data in chunks.
- `Boundary` is a marker component for `Grid`s that should be treated as if there is more outside their boundary. Later we'll add some functionality to this, so it can be used eg in procedural generation for finding which chunks to generate next.
- `BodyTracker` tracks the connectivity of the `Grid` it's attached to. You can use it to iterate over the connected components of the `Grid`, and then to iterate over the voxels in each component. If the entity has a `Boundary`, then any voxels connected to the boundary will be considered connected to each other.

So far most of the interesting stuff is in `connectivity.rs` and the example `falling_sand.rs`.

One big goal is to make it easy to set up compute shaders to run on these grids. If it's not too inflexible, we can handle the passing of the chunks to the GPU, and provide a wgsl library with some helpers for accessing the voxels from the correct chunk.

One big decision is how to store the chunks. One option is to make the chunks components and put them on child entities. We'll need a child per chunk anyways for the colliders and meshes, and this way we also get change detection. This is what I tried at first but I didn't like having to pass around a `Query<&mut Chunk>` to every function that deals with chunks. But I didn't know about `SystemParam`s then. So maybe we can try that again with a `Grids` or `Chunks` `SystemParam`. It's also unclear if Bevy change detection is the right thing here. Many grid based systems need to update every _neighbor_ of every mutated voxel. So we can manually `deref_mut` the neighboring chunks when we modify a voxel on a chunk boundary. Noita apparently tracks a dirty rectangle in each chunk [citation needed] to avoid iterating over the entirety of every dirty chunk. Some systems need to act on voxels whose face neighbors have changed, others whose edge and corner neighbors have changed, etc. Maybe no storage scheme is general enough. Idk. Chunks as components is probably the move, we should try it.

Actually, probably how we should do change detection is just vanilla bevy change detection, and then let systems handle propogating dirt to neighboring chunks.
