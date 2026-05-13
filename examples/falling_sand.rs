use bevy::prelude::*;

use crate::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(FreeCameraPlugin)
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut cmd: Commands) {
    // player
    commands.spawn((
        Camera3d,
        FreeCamera {
            sensitivity: 0.2,
            friction: 25.0,
            walk_speed: 3.0,
            run_speed: 9.0,
            ..default()
        },
    ));

    // world
commands.spawn((
    Grid<u8>,
))
}
