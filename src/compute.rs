use bevy::app::*;
use bevy::ecs::schedule::*;
use bevy::prelude::*;
use bevy::render::render_resource::*;
use bevy::render::renderer::*;

/// Schedule between [`FixedFirst`] and [`FixedPreUpdate`]. Use this to
/// submit compute graph commands. Also use this for any systems that
/// need to happen after reading back from last tick's compute shaders
/// but before sending data to the gpu for this tick, ie for mutating
/// data that is also mutated on the GPU.
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
struct Compute;

/// Sets up the infrastructure needed to run compute shaders in the main
/// world the same way that you can in the render world, and in particular
/// inside [`FixedMain`].
pub struct ComputePlugin;
impl Plugin for ComputePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PendingCommandBuffers>();

        app.init_schedule(Compute);
        app.world_mut()
            .resource_mut::<FixedMainScheduleOrder>()
            .insert_before(FixedPreUpdate, Compute);

        // do we need to enforce FixedFirst running before Compute?
        // also does this even need to run every FixedUpdate?
        app.add_systems(FixedFirst, process_pipeline_queue);
        app.add_systems(FixedPreUpdate, submit);

        app.configure_sets(
            PreUpdate,
            ComputeStartup.run_if(resource_changed::<RenderDevice>),
        );
    }
}

// PipelineCache::process_pipeline_queue_system is private :(
fn process_pipeline_queue(mut pipeline_cache: ResMut<PipelineCache>) {
    pipeline_cache.process_queue();
}

fn submit(mut flush: FlushCommands) {
    flush.flush();
}

/* ------------------ resource initialization ------------------ */

/// Runs every time a new [`RenderDevice`] is acquired. Used by
/// [`init_gpu_resource`](ComputeResourceAppExt::init_gpu_resource).
// this should probably be a schedule shouldn't it...
// (and single threaded like RenderStartup)
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
struct ComputeStartup;

/// Constructs a `T` resource with `from_world` and inserts it.
pub fn init_gpu_resource<R: Resource + FromWorld>(world: &mut World) {
    let res = R::from_world(world);
    world.insert_resource(res);
}

/// Convenience methods for render-recovery-aware resource initialization.
// wait, most GPU resources don't need &mut World to initialize. init_gpu_resource
// forces all the initialization to serialize for no reason...
// I think the rationale for FromWorld is that you need &mut World to insert the
// resource anyways, so taking SystemParams instead of &mut World would let you serialize
// the initialization, but that's generally cheap, and then you'd have to box the
// resource for later insertion, which would probably not be worth it.
pub trait ComputeResourceAppExt {
    /// Causes the provided GPU resource to be re-initialized during [`ComputeStartup`].
    ///
    /// This is useful when recovering from lost render devices.
    ///
    /// Shorthand for:
    /// ```ignore
    /// app.add_systems(
    ///     PreUpdate,
    ///     init_gpu_resource::<R>
    ///         .in_set(ComputeStartup)
    ///         .ambiguous_with_all(),
    /// );
    /// ```
    fn init_gpu_resource<R: Resource + FromWorld>(&mut self) -> &mut Self;
}

impl ComputeResourceAppExt for App {
    fn init_gpu_resource<R: Resource + FromWorld>(&mut self) -> &mut Self {
        self.add_systems(
            PreUpdate,
            init_gpu_resource::<R>
                .in_set(ComputeStartup)
                .ambiguous_with_all(),
        )
    }
}
