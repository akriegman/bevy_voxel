#![feature(const_array)]
#![feature(const_trait_impl)]
#![feature(const_closures)]

use bevy::asset::RenderAssetUsages;
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::mesh::*;
use bevy::picking::mesh_picking::ray_cast::{MeshRayCast, MeshRayCastSettings};
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions};

use avian3d::prelude::*;

use voxxelmaxx::prelude::*;

/* -------------------------- setup ---------------------------- */

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                // resizable: false,
                mode: bevy::window::WindowMode::BorderlessFullscreen(MonitorSelection::Current),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(PhysicsPlugins::default())
        .add_plugins(MeshPickingPlugin)
        .add_plugins(ConnectivityPlugin::<SandConnector>::default())
        .insert_resource(Gravity::default())
        .add_systems(Startup, setup)
        .add_systems(FixedUpdate, falling_sand)
        .add_systems(FixedUpdate, check_islands)
        .add_systems(FixedUpdate, colliders)
        .add_systems(Update, movement)
        .add_systems(Update, interact)
        .add_systems(Update, mesh)
        .run();
}

fn setup(
    mut cmd: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut cursor: Query<&mut CursorOptions>,
) {
    /* ------------------------ player ------------------------- */

    cmd.spawn((
        children![Camera3d::default()],
        Player {
            pitch: -std::f32::consts::TAU * 1. / 8.,
            yaw: std::f32::consts::TAU * 1. / 8.,
            material: 0xc0,
            action_timer: Timer::from_seconds(1. / 12., TimerMode::Repeating),
        },
        Transform::from_xyz(1.5, 0.5, 1.5),
        RigidBody::Kinematic,
        Collider::capsule(0.5, 1.),
    ));
    println!("");
    println!("controls:");
    println!("  w a s d shift space |            move");
    println!("  click               |    place voxels");
    println!("  1 2 3 4             | select material");
    println!("");

    if let Ok(mut cursor) = cursor.single_mut() {
        cursor.grab_mode = CursorGrabMode::Locked;
        cursor.visible = false;
    }

    /* ------------------------ world -------------------------- */

    cmd.spawn((
        Grid::<u8>::new(),
        BodyTracker::<SandConnector>::new(),
        Boundary,
        MeshMaterial3d(materials.add(StandardMaterial {
            reflectance: 0.,
            ..default()
        })),
        Transform::from_scale(Vec3::splat(1. / N as f32)),
        RigidBody::Static,
    ))
    .queue(|mut entity: EntityWorldMut| {
        let mut grid = entity.get_mut::<Grid<u8>>().unwrap();
        for idx in prism(IVec3::splat(-2 * N as i32), IVec3::splat(2 * N as i32)) {
            grid.set(idx, if idx.y >= -(N as i32) { 0x00 } else { 0x80 })
        }
    });

    /* ------------------------ light -------------------------- */

    cmd.spawn((
        DirectionalLight {
            illuminance: 1000.,
            ..default()
        },
        Transform::from_xyz(-4., 5., 3.).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    cmd.spawn((
        DirectionalLight {
            illuminance: 1000.,
            ..default()
        },
        Transform::from_xyz(4., 5., -3.).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

#[derive(Component, Default)]
#[require(Visibility)]
struct Player {
    pitch: f32,
    yaw: f32,
    material: u8,
    action_timer: Timer,
}

/* -------------------- cellular automata ---------------------- */

fn falling_sand(mut grid: Single<&mut Grid<u8>, With<Boundary>>, mut ticks: Local<usize>) {
    *ticks += 1;
    if *ticks % 2 != 0 {
        return;
    }

    'voxel: for idx in prism(IVec3::splat(-2 * N as i32), IVec3::splat(2 * N as i32)) {
        let vox = *grid.get(idx).unwrap();
        // powder or liquid
        if vox & 0x40 != 0 {
            for delta in [
                IVec3::NEG_Y,
                IVec3::NEG_Y + IVec3::X,
                IVec3::NEG_Y - IVec3::X,
                IVec3::NEG_Y + IVec3::Z,
                IVec3::NEG_Y - IVec3::Z,
            ] {
                if let Some(cell) = grid.get_mut(idx + delta) {
                    if *cell == 0x00 {
                        *cell = vox;
                        *grid.get_mut(idx).unwrap() = 0x00;
                        continue 'voxel;
                    }
                }
            }
            // liquid
            if vox & 0x80 == 0 {
                use rand::prelude::*;

                for delta in
                    [IVec3::X, IVec3::NEG_X, IVec3::Z, IVec3::NEG_Z].sample(&mut rand::rng(), 4)
                {
                    if let Some(cell) = grid.get_mut(idx + delta)
                        && *cell == 0x00
                    {
                        *cell = vox;
                        *grid.get_mut(idx).unwrap() = 0x00;
                        continue 'voxel;
                    }
                }
            }
        }
    }
}

/* -------------------- island detection ----------------------- */

struct SandConnector;
impl Connector for SandConnector {
    type Item = u8;
    fn solid(voxel: &Self::Item) -> bool {
        voxel & 0xc0 == 0x80
    }
}

fn check_islands(
    mut cmd: Commands,
    grids: Query<
        (
            &mut Grid<u8>,
            &BodyTracker<SandConnector>,
            &GlobalTransform,
            &MeshMaterial3d<StandardMaterial>,
        ),
        Changed<BodyTracker<SandConnector>>,
    >,
) {
    for (mut grid, bodies, gtf, material) in grids {
        // the first body is the boundary component, so we skip it.
        //
        // in order to save on memory, BodyTracker needs a reference to the grid
        // when it iterates over the bodies. the compromise is that we cannot iterate
        // and modify the grid at the same time, so we end up allocating here anyways,
        // but hopefully it's smaller and shorter lived.
        let islands: Vec<Vec<IVec3>> = bodies
            .bodies(&grid)
            .skip(1)
            .map(Iterator::collect)
            .collect();
        for island in islands {
            let popped = island
                .into_iter()
                .map(|idx| {
                    let vox = *grid.get(idx).unwrap();
                    *grid.get_mut(idx).unwrap() = 0x00;
                    (idx, vox)
                })
                .collect::<Vec<_>>();
            cmd.spawn((
                Grid::<u8>::new(),
                BodyTracker::<SandConnector>::new(),
                RigidBody::Dynamic,
                Collider::sphere(1.), // todo: we shouldn't have to put a placeholder here
                gtf.compute_transform(),
                material.clone(),
            ))
            .queue(|mut entity: EntityWorldMut| {
                let mut grid = entity.get_mut::<Grid<u8>>().unwrap();
                for (idx, vox) in popped {
                    grid.set(idx, vox);
                }
            });
        }
    }
}

/* --------------------------- mesh ---------------------------- */

fn mesh(
    mut cmd: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    grids: Query<(Entity, &Grid<u8>), Changed<Grid<u8>>>,
) {
    for (entity, grid) in grids {
        let mut positions = Vec::new();
        let mut normals = Vec::new();
        let mut colors = Vec::new();
        let mut indices = Vec::new();

        for idx in prism(IVec3::splat(-2 * N as i32 + 1), IVec3::splat(2 * N as i32)) {
            let Some(vox) = grid.get(idx) else {
                continue;
            };
            for (axis, swizzle) in [
                (IVec3::X, Vec3::xyz as fn(Vec3) -> Vec3),
                (IVec3::Y, Vec3::zxy as fn(Vec3) -> Vec3),
                (IVec3::Z, Vec3::yzx as fn(Vec3) -> Vec3),
            ] {
                let Some(cell) = grid.get(idx - axis) else {
                    continue;
                };
                let flip = *vox == 0;
                match (vox, cell) {
                    (0, 1..) | (1.., 0) => {
                        indices.extend_from_slice(
                            &if !flip {
                                [0, 2, 1, 0, 3, 2]
                            } else {
                                [0, 1, 2, 0, 2, 3]
                            }
                            .map(|i| i + positions.len() as u16),
                        );

                        positions.extend_from_slice(
                            &[
                                vec3(0., 0., 0.),
                                vec3(0., 1., 0.),
                                vec3(0., 1., 1.),
                                vec3(0., 0., 1.),
                            ]
                            .map(|p| swizzle(p) + idx.as_vec3()),
                        );

                        normals.extend_from_slice(
                            &[vec3(if flip { 1. } else { -1. }, 0., 0.); 4].map(swizzle),
                        );

                        use std::ops::*;
                        let idx = if flip { idx - axis } else { idx };
                        let a = idx.dot(IVec3::splat(1)).add(1).rem_euclid(2);
                        let b = idx.rem_euclid(IVec3::splat(2)).dot(IVec3::splat(1)).rem(3) == 0;
                        let c = idx.shr(1i32).dot(IVec3::splat(1)).rem_euclid(2);

                        let dither = a | (b as i32).bitand(c);
                        let dither = 1. + (dither as f32 / 1. - 0.5) / 3.;
                        colors.extend_from_slice(&[PALETTE[(vox + cell) as usize] * dither; 4]);
                    }
                    _ => (),
                }
            }
        }

        if positions.len() == 0 {
            continue;
        }

        cmd.entity(entity).try_insert(Mesh3d(
            meshes.add(
                Mesh::new(
                    PrimitiveTopology::TriangleList,
                    RenderAssetUsages::default(),
                )
                .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
                .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
                .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
                .with_inserted_indices(Indices::U16(indices)),
            ),
        ));
    }
}

const PALETTE: [Vec4; 256] = {
    let mut palette = [uvec4(0, 0, 0, 0xff); 256];
    palette[0x40] = uvec4(0x00, 0x33, 0x99, 0xff);
    palette[0x80] = uvec4(0x66, 0x66, 0x66, 0xff);
    palette[0xc0] = uvec4(0x99, 0x66, 0x11, 0xff);
    palette.map(const |c| {
        vec4(
            c.x as f32 / 255.,
            c.y as f32 / 255.,
            c.z as f32 / 255.,
            c.w as f32 / 255.,
        )
    })
    // Vec4 is Div<f32>, but not const Div<f32>
    // one day...
};

/* ------------------------- colliders ------------------------- */

fn colliders(mut cmd: Commands, grids: Query<(Entity, &Grid<u8>), Changed<Grid<u8>>>) {
    for (entity, grid) in grids {
        let voxels = grid
            .iter()
            .map(|(k, v)| {
                prism(IVec3::ZERO, DIMS).filter_map(move |idx| {
                    if v[idx] & 0x80 == 0 {
                        None
                    } else {
                        Some(k * DIMS + idx)
                    }
                })
            })
            .flatten()
            .collect::<Vec<_>>();

        if voxels.len() == 0 {
            cmd.entity(entity).despawn();
        } else {
            cmd.entity(entity)
                .try_insert(Collider::voxels(Vec3::splat(1.), &voxels));
        }
    }
}

/* ------------------------- interact -------------------------- */

fn interact(
    head: Single<&GlobalTransform, With<Camera3d>>,
    player: Single<&mut Player>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut ray_cast: MeshRayCast,
    mut grids: Query<(&mut Grid<u8>, &GlobalTransform)>,
    mut gizmos: Gizmos,
    time: Res<Time>,
) {
    let head_tf = head.into_inner();
    let mut player = player.into_inner();
    let origin = head_tf.translation();
    let dir = head_tf.forward();
    let ray = Ray3d::new(origin, dir);
    let hits = ray_cast.cast_ray(ray, &MeshRayCastSettings::default());
    let Some((entity, hit)) = hits.first() else {
        return;
    };
    let pos = hit.point;
    let normal = hit.normal;
    let Ok((mut grid, tf)) = grids.get_mut(*entity) else {
        return;
    };
    let inv = tf.affine().inverse();
    let local = inv.transform_point3(pos);
    let axis = match inv.transform_vector3(normal).abs().round().as_ivec3() {
        IVec3::X => 0,
        IVec3::Y => 1,
        IVec3::Z => 2,
        _ => panic!(),
    };
    let u = (axis + 1) % 3;
    let v = (axis + 2) % 3;
    let mut base = local.floor();
    base[axis] = local[axis].round();
    let corner = |du: f32, dv: f32| {
        let mut p = base;
        p[u] += du;
        p[v] += dv;
        tf.transform_point(p)
    };
    let corners = [
        corner(0., 0.),
        corner(1., 0.),
        corner(1., 1.),
        corner(0., 1.),
    ];
    for i in 0..4 {
        gizmos.line(corners[i], corners[(i + 1) % 4], Color::WHITE);
    }

    let base = base.as_ivec3();
    let diam = 5;
    if mouse.pressed(MouseButton::Left) && player.action_timer.tick(time.delta()).just_finished() {
        for idx in prism(base - diam / 2, base + diam / 2 + 1) {
            if idx.distance_squared(base) > (diam * diam) / 4 {
                continue;
            }
            if let Some(cell) = grid.get_mut(idx) {
                if let (0, 1..) | (1.., 0) = (player.material, *cell) {
                    *cell = player.material;
                }
            }
        }
    }
}

/* --------------------------- input --------------------------- */

fn movement(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<AccumulatedMouseMotion>,
    head: Single<&mut Transform, With<Camera3d>>,
    player: Single<(Entity, &mut Player, &mut Transform, &Collider), Without<Camera3d>>,
    mover: MoveAndSlide,
) {
    let mut head_tf = head.into_inner();
    let (entity, mut player, mut tf, collider) = player.into_inner();

    let sensitivity = 0.002;
    player.yaw -= mouse.delta.x * sensitivity;
    player.pitch = (player.pitch - mouse.delta.y * sensitivity)
        .clamp(-std::f32::consts::FRAC_PI_2, std::f32::consts::FRAC_PI_2);
    head_tf.rotation = Quat::from_euler(EulerRot::YXZ, player.yaw, player.pitch, 0.);

    let yaw = Quat::from_rotation_y(player.yaw);
    let forward = yaw * Vec3::NEG_Z;
    let right = yaw * Vec3::X;

    let mut dir = Vec3::ZERO;
    if keys.pressed(KeyCode::KeyW) {
        dir += forward;
    }
    if keys.pressed(KeyCode::KeyS) {
        dir -= forward;
    }
    if keys.pressed(KeyCode::KeyD) {
        dir += right;
    }
    if keys.pressed(KeyCode::KeyA) {
        dir -= right;
    }
    if keys.pressed(KeyCode::Space) {
        dir += Vec3::Y;
    }
    if keys.pressed(KeyCode::ShiftLeft) {
        dir -= Vec3::Y;
    }

    let move_output = mover.move_and_slide(
        collider,
        tf.translation,
        tf.rotation,
        dir.normalize() * 2.,
        time.delta(),
        &MoveAndSlideConfig::default(),
        &SpatialQueryFilter::from_excluded_entities([entity]),
        |_| MoveAndSlideHitResponse::Accept,
    );
    tf.translation = move_output.position;

    if keys.pressed(KeyCode::Digit1) {
        player.material = 0x00;
    }
    if keys.pressed(KeyCode::Digit2) {
        player.material = 0x40;
    }
    if keys.pressed(KeyCode::Digit3) {
        player.material = 0xc0;
    }
    if keys.pressed(KeyCode::Digit4) {
        player.material = 0x80;
    }
}
