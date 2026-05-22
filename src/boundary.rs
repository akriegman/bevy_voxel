use bevy::prelude::*;

/// for now this is just a marker for Grids that should be
/// treated as infinite. later we'll add functionality.
#[derive(Component)]
pub struct Boundary;
