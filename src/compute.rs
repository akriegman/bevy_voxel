use bevy::app::*;
use bevy::ecs::schedule::*;
use bevy::prelude::*;
use bevy::render::renderer::*;

#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
struct Compute;

pub struct ComputePlugin;
impl Plugin for ComputePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PendingCommandBuffers>();

        app.init_schedule(Compute);
        app.world_mut()
            .resource_mut::<FixedMainScheduleOrder>()
            .insert_before(FixedPreUpdate, Compute);

        app.add_systems(FixedPreUpdate, submit);

        app.configure_sets(
            PreUpdate,
            ComputeStartup.run_if(resource_changed::<RenderDevice>),
        );
    }
}

fn submit(mut flush: FlushCommands) {
    flush.flush();
}

/* ------------------ resource initialization ------------------ */

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
struct ComputeStartup;

/// Constructs a `T` resource with `from_world` and inserts it.
pub fn init_gpu_resource<R: Resource + FromWorld>(world: &mut World) {
    let res = R::from_world(world);
    world.insert_resource(res);
}

/// Convenience methods for render-recovery-aware resource initialization.
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
