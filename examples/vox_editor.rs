//! Voxel editor for `.vox` files.
//!
//! Run with `cargo run --example vox_editor [path/to/file.vox]`. With a path,
//! the file is loaded on startup; otherwise an empty 64x64x64 grid is created.
//! Save / Load buttons (or Ctrl-S / Ctrl-L) round-trip through that path.
//!
//! Controls:
//! - WASD: horizontal movement, Space/Ctrl: up/down, mouse: look
//! - Tab: toggle cursor between camera (locked) and UI (free)
//! - Left click: place, Right click: erase (only while locked)
//! - 1..7 select material; Q/E/R select Single/Prism/Sphere; [ ] adjust size

use std::path::PathBuf;

use avian3d::prelude::*;
use bevy::{
    input::mouse::AccumulatedMouseMotion,
    prelude::*,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow, Window, WindowMode},
};
use dot_vox::{
    Color as VoxColor, DotVoxData, Model as VoxModel, SceneNode, ShapeModel, Size as VoxSize,
    Voxel as VoxVoxel,
};
use voxxelmaxx::{Grid, H, N, TerrainMaterial, VoxelPlugin};

const GRID_VOX: i32 = 64;
const CHUNKS: i32 = GRID_VOX / N as i32;
const WORLD: f32 = GRID_VOX as f32 * H;

const MOUSE_SENS: f32 = 0.0025;
const MOVE_SPEED: f32 = 4.0;
const FAST_MULT: f32 = 4.0;

// Editor materials. Tag bytes match the colors hard-coded in
// `voxxelmaxx::tag_color`, so the rendered mesh agrees with the swatches we
// show in the palette UI.
const MATERIALS: &[(u8, &str, [f32; 3])] = &[
    (0x80, "Stone", [0.5, 0.5, 0.52]),
    (0x81, "Dirt", [0.45, 0.3, 0.18]),
    (0x82, "Wood", [0.6, 0.42, 0.25]),
    (0x83, "Bark", [0.3, 0.2, 0.12]),
    (0x84, "Leaf", [0.25, 0.55, 0.2]),
    (0xc0, "Sand", [0.85, 0.78, 0.55]),
    (0x40, "Water", [0.15, 0.35, 0.7]),
];

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum Tool {
    Single,
    Prism,
    Sphere,
}

#[derive(Resource)]
struct Editor {
    path: Option<PathBuf>,
    tag: u8,
    tool: Tool,
    prism: IVec3,
    sphere_r: i32,
    locked: bool,
    status: String,
}

impl Default for Editor {
    fn default() -> Self {
        Self {
            path: None,
            tag: 0x80,
            tool: Tool::Single,
            prism: IVec3::splat(3),
            sphere_r: 3,
            locked: true,
            status: String::new(),
        }
    }
}

#[derive(Resource, Default)]
struct PendingLoad(Option<PathBuf>);

#[derive(Resource, Default)]
struct PendingSave(bool);

#[derive(Component)]
struct Freecam {
    yaw: f32,
    pitch: f32,
}

#[derive(Component)]
struct GridRoot;

#[derive(Component)]
struct GroundPlane;

#[derive(Component)]
struct MaterialBtn(u8);

#[derive(Component)]
struct ToolBtn(Tool);

#[derive(Component, Copy, Clone)]
enum SizeBtn {
    PrismX(i32),
    PrismY(i32),
    PrismZ(i32),
    Sphere(i32),
}

#[derive(Component)]
struct SaveBtn;

#[derive(Component)]
struct LoadBtn;

#[derive(Component)]
struct StatusText;

#[derive(Component)]
struct PrismRow;

#[derive(Component)]
struct SphereRow;

#[derive(Component)]
struct PrismValue(usize); // 0=x, 1=y, 2=z

#[derive(Component)]
struct SphereValue;

fn main() {
    let arg = std::env::args().nth(1).map(PathBuf::from);
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                mode: WindowMode::BorderlessFullscreen(MonitorSelection::Primary),
                title: "Voxxelmaxx Editor".into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(PhysicsPlugins::default())
        .add_plugins(VoxelPlugin)
        .insert_resource(Editor {
            path: arg.clone(),
            ..default()
        })
        .insert_resource(PendingLoad(arg))
        .insert_resource(PendingSave::default())
        .add_systems(Startup, setup)
        .add_systems(Update, (toggle_lock, camera_look, camera_move).chain())
        .add_systems(Update, (keyboard_shortcuts, ui_buttons))
        .add_systems(Update, (perform_load, perform_save))
        .add_systems(Update, do_click)
        .add_systems(Update, (refresh_button_styles, refresh_status, draw_overlays))
        .run();
}

fn setup(
    mut cmd: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut cursor_options: Single<&mut CursorOptions, With<PrimaryWindow>>,
) {
    cmd.insert_resource(TerrainMaterial(materials.add(Color::WHITE)));
    // Ambient light is a per-camera component in Bevy 0.18; it gets attached to the camera below.

    // Grid: spawn empty so chunks exist (Grid::set is a no-op outside loaded
    // chunks). Each chunk holds N=16 voxels per side; 4x4x4 chunks = 64^3 voxels.
    let mut grid = Grid::default();
    for cx in 0..CHUNKS {
        for cy in 0..CHUNKS {
            for cz in 0..CHUNKS {
                grid.add_chunk(IVec3::new(cx, cy, cz), || Box::new([0u8; N * N * N]));
            }
        }
    }
    cmd.spawn((grid, GridRoot, RigidBody::Static));

    // Ground plane: a thin sensor-less collider with its top at y=0 so rays
    // miss into empty space still hit something we can place onto.
    cmd.spawn((
        GroundPlane,
        RigidBody::Static,
        Collider::cuboid(WORLD, H * 0.5, WORLD),
        Transform::from_xyz(WORLD * 0.5, -H * 0.25, WORLD * 0.5),
    ));

    // Lights
    cmd.spawn((
        DirectionalLight {
            illuminance: 8000.0,
            ..default()
        },
        Transform::from_xyz(4.0, 8.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    cmd.spawn((
        DirectionalLight {
            illuminance: 4000.0,
            ..default()
        },
        Transform::from_xyz(-3.0, 5.0, -4.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // Freecam
    let cam_pos = Vec3::new(WORLD * 1.6, WORLD * 1.0, WORLD * 1.6);
    let look_to = (Vec3::splat(WORLD * 0.5) - cam_pos).normalize();
    let yaw = (-look_to.x).atan2(-look_to.z);
    let pitch = look_to.y.asin();
    cmd.spawn((
        Camera3d::default(),
        Transform::from_translation(cam_pos)
            .looking_at(Vec3::splat(WORLD * 0.5), Vec3::Y),
        Freecam { yaw, pitch },
    ));

    cursor_options.grab_mode = CursorGrabMode::Locked;
    cursor_options.visible = false;

    // a tiny placeholder mesh so we don't get warnings
    let _ = meshes.add(Cuboid::new(0.0, 0.0, 0.0));

    spawn_ui(&mut cmd);
}

fn spawn_ui(cmd: &mut Commands) {
    let panel_bg = BackgroundColor(Color::srgba(0.05, 0.05, 0.07, 0.85));
    let btn_bg = Color::srgba(0.18, 0.18, 0.22, 0.95);

    // Left palette panel
    cmd.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            bottom: Val::Px(0.0),
            width: Val::Px(160.0),
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(Val::Px(8.0)),
            row_gap: Val::Px(6.0),
            ..default()
        },
        panel_bg,
    ))
    .with_children(|p| {
        p.spawn((
            Text::new("Materials"),
            TextFont {
                font_size: 16.0,
                ..default()
            },
        ));
        for (tag, name, color) in MATERIALS {
            p.spawn((
                Button,
                MaterialBtn(*tag),
                Node {
                    height: Val::Px(28.0),
                    align_items: AlignItems::Center,
                    padding: UiRect::horizontal(Val::Px(6.0)),
                    column_gap: Val::Px(8.0),
                    border: UiRect::all(Val::Px(2.0)),
                    ..default()
                },
                BackgroundColor(btn_bg),
                BorderColor::all(Color::NONE),
            ))
            .with_children(|b| {
                b.spawn((
                    Node {
                        width: Val::Px(16.0),
                        height: Val::Px(16.0),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(color[0], color[1], color[2])),
                ));
                b.spawn((
                    Text::new(*name),
                    TextFont {
                        font_size: 14.0,
                        ..default()
                    },
                ));
            });
        }

        p.spawn(Node {
            height: Val::Px(12.0),
            ..default()
        });
        p.spawn((
            Text::new("File"),
            TextFont {
                font_size: 16.0,
                ..default()
            },
        ));
        for (label, is_save) in [("Save", true), ("Load", false)] {
            let mut e = p.spawn((
                Button,
                Node {
                    height: Val::Px(28.0),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                BackgroundColor(btn_bg),
            ));
            if is_save {
                e.insert(SaveBtn);
            } else {
                e.insert(LoadBtn);
            }
            e.with_child((
                Text::new(label),
                TextFont {
                    font_size: 14.0,
                    ..default()
                },
            ));
        }
        p.spawn((
            Text::new(""),
            TextFont {
                font_size: 12.0,
                ..default()
            },
            StatusText,
        ));
    });

    // Right tool panel
    cmd.spawn((
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(0.0),
            top: Val::Px(0.0),
            bottom: Val::Px(0.0),
            width: Val::Px(180.0),
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(Val::Px(8.0)),
            row_gap: Val::Px(6.0),
            ..default()
        },
        panel_bg,
    ))
    .with_children(|p| {
        p.spawn((
            Text::new("Tools"),
            TextFont {
                font_size: 16.0,
                ..default()
            },
        ));
        for (tool, name) in [
            (Tool::Single, "Single"),
            (Tool::Prism, "Prism"),
            (Tool::Sphere, "Sphere"),
        ] {
            p.spawn((
                Button,
                ToolBtn(tool),
                Node {
                    height: Val::Px(28.0),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    border: UiRect::all(Val::Px(2.0)),
                    ..default()
                },
                BackgroundColor(btn_bg),
                BorderColor::all(Color::NONE),
            ))
            .with_child((
                Text::new(name),
                TextFont {
                    font_size: 14.0,
                    ..default()
                },
            ));
        }

        // Prism size: three rows of -/value/+
        p.spawn((
            PrismRow,
            Node {
                margin: UiRect::top(Val::Px(8.0)),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(4.0),
                ..default()
            },
        ))
        .with_children(|p| {
            p.spawn((
                Text::new("Prism size"),
                TextFont {
                    font_size: 14.0,
                    ..default()
                },
            ));
            for (i, label) in ["X", "Y", "Z"].iter().enumerate() {
                p.spawn(Node {
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(6.0),
                    ..default()
                })
                .with_children(|row| {
                    row.spawn((
                        Text::new(*label),
                        TextFont {
                            font_size: 14.0,
                            ..default()
                        },
                        Node {
                            width: Val::Px(14.0),
                            ..default()
                        },
                    ));
                    spawn_step_btn(
                        row,
                        "-",
                        match i {
                            0 => SizeBtn::PrismX(-1),
                            1 => SizeBtn::PrismY(-1),
                            _ => SizeBtn::PrismZ(-1),
                        },
                    );
                    row.spawn((
                        Text::new("3"),
                        TextFont {
                            font_size: 14.0,
                            ..default()
                        },
                        PrismValue(i),
                        Node {
                            min_width: Val::Px(24.0),
                            ..default()
                        },
                    ));
                    spawn_step_btn(
                        row,
                        "+",
                        match i {
                            0 => SizeBtn::PrismX(1),
                            1 => SizeBtn::PrismY(1),
                            _ => SizeBtn::PrismZ(1),
                        },
                    );
                });
            }
        });

        // Sphere radius row
        p.spawn((
            SphereRow,
            Node {
                margin: UiRect::top(Val::Px(4.0)),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(4.0),
                ..default()
            },
        ))
        .with_children(|p| {
            p.spawn((
                Text::new("Sphere radius"),
                TextFont {
                    font_size: 14.0,
                    ..default()
                },
            ));
            p.spawn(Node {
                align_items: AlignItems::Center,
                column_gap: Val::Px(6.0),
                ..default()
            })
            .with_children(|row| {
                spawn_step_btn(row, "-", SizeBtn::Sphere(-1));
                row.spawn((
                    Text::new("3"),
                    TextFont {
                        font_size: 14.0,
                        ..default()
                    },
                    SphereValue,
                    Node {
                        min_width: Val::Px(24.0),
                        ..default()
                    },
                ));
                spawn_step_btn(row, "+", SizeBtn::Sphere(1));
            });
        });
    });

    // Crosshair
    cmd.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Percent(50.0),
            top: Val::Percent(50.0),
            width: Val::Px(8.0),
            height: Val::Px(8.0),
            margin: UiRect::all(Val::Px(-4.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.7)),
    ));
}

fn spawn_step_btn(parent: &mut ChildSpawnerCommands, label: &str, kind: SizeBtn) {
    parent
        .spawn((
            Button,
            kind,
            Node {
                width: Val::Px(22.0),
                height: Val::Px(22.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.25, 0.25, 0.3, 1.0)),
        ))
        .with_child((
            Text::new(label),
            TextFont {
                font_size: 14.0,
                ..default()
            },
        ));
}

fn toggle_lock(
    keys: Res<ButtonInput<KeyCode>>,
    mut editor: ResMut<Editor>,
    mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>,
) {
    if keys.just_pressed(KeyCode::Tab) {
        editor.locked = !editor.locked;
        if editor.locked {
            cursor.grab_mode = CursorGrabMode::Locked;
            cursor.visible = false;
        } else {
            cursor.grab_mode = CursorGrabMode::None;
            cursor.visible = true;
        }
    }
}

fn camera_look(
    editor: Res<Editor>,
    mouse: Res<AccumulatedMouseMotion>,
    mut cam: Single<(&mut Transform, &mut Freecam)>,
) {
    if !editor.locked || mouse.delta == Vec2::ZERO {
        return;
    }
    let (tf, fc) = &mut *cam;
    fc.yaw -= mouse.delta.x * MOUSE_SENS;
    fc.pitch = (fc.pitch - mouse.delta.y * MOUSE_SENS).clamp(-1.54, 1.54);
    tf.rotation = Quat::from_axis_angle(Vec3::Y, fc.yaw) * Quat::from_axis_angle(Vec3::X, fc.pitch);
}

fn camera_move(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut cam: Single<&mut Transform, With<Freecam>>,
) {
    let mut dir = Vec3::ZERO;
    let f = cam.forward();
    let r = cam.right();
    let fwd = Vec3::new(f.x, 0.0, f.z).normalize_or_zero();
    let right = Vec3::new(r.x, 0.0, r.z).normalize_or_zero();
    if keys.pressed(KeyCode::KeyW) {
        dir += fwd;
    }
    if keys.pressed(KeyCode::KeyS) {
        dir -= fwd;
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
    if keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ControlLeft) {
        dir -= Vec3::Y;
    }
    let speed = if keys.pressed(KeyCode::AltLeft) {
        MOVE_SPEED * FAST_MULT
    } else {
        MOVE_SPEED
    };
    cam.translation += dir.normalize_or_zero() * speed * time.delta_secs();
}

fn keyboard_shortcuts(
    keys: Res<ButtonInput<KeyCode>>,
    mut editor: ResMut<Editor>,
    mut load: ResMut<PendingLoad>,
    mut save: ResMut<PendingSave>,
) {
    let nums = [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
        KeyCode::Digit7,
    ];
    for (i, k) in nums.iter().enumerate() {
        if keys.just_pressed(*k) && i < MATERIALS.len() {
            editor.tag = MATERIALS[i].0;
        }
    }
    if keys.just_pressed(KeyCode::KeyQ) {
        editor.tool = Tool::Single;
    }
    if keys.just_pressed(KeyCode::KeyE) {
        editor.tool = Tool::Prism;
    }
    if keys.just_pressed(KeyCode::KeyR) {
        editor.tool = Tool::Sphere;
    }
    let step = if keys.pressed(KeyCode::ShiftLeft) {
        4
    } else {
        1
    };
    if keys.just_pressed(KeyCode::BracketRight) {
        match editor.tool {
            Tool::Prism => editor.prism = (editor.prism + IVec3::splat(step)).min(IVec3::splat(32)),
            Tool::Sphere => editor.sphere_r = (editor.sphere_r + step).min(32),
            Tool::Single => {}
        }
    }
    if keys.just_pressed(KeyCode::BracketLeft) {
        match editor.tool {
            Tool::Prism => editor.prism = (editor.prism - IVec3::splat(step)).max(IVec3::ONE),
            Tool::Sphere => editor.sphere_r = (editor.sphere_r - step).max(1),
            Tool::Single => {}
        }
    }
    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::SuperLeft);
    if ctrl && keys.just_pressed(KeyCode::KeyS) {
        save.0 = true;
    }
    if ctrl && keys.just_pressed(KeyCode::KeyL) {
        load.0 = editor.path.clone();
    }
}

#[allow(clippy::too_many_arguments)]
fn ui_buttons(
    mut editor: ResMut<Editor>,
    mut load: ResMut<PendingLoad>,
    mut save: ResMut<PendingSave>,
    mat_q: Query<(&Interaction, &MaterialBtn), Changed<Interaction>>,
    tool_q: Query<(&Interaction, &ToolBtn), Changed<Interaction>>,
    size_q: Query<(&Interaction, &SizeBtn), Changed<Interaction>>,
    save_q: Query<&Interaction, (Changed<Interaction>, With<SaveBtn>)>,
    load_q: Query<&Interaction, (Changed<Interaction>, With<LoadBtn>)>,
) {
    for (i, m) in &mat_q {
        if *i == Interaction::Pressed {
            editor.tag = m.0;
        }
    }
    for (i, t) in &tool_q {
        if *i == Interaction::Pressed {
            editor.tool = t.0;
        }
    }
    for (i, s) in &size_q {
        if *i != Interaction::Pressed {
            continue;
        }
        match *s {
            SizeBtn::PrismX(d) => editor.prism.x = (editor.prism.x + d).clamp(1, 32),
            SizeBtn::PrismY(d) => editor.prism.y = (editor.prism.y + d).clamp(1, 32),
            SizeBtn::PrismZ(d) => editor.prism.z = (editor.prism.z + d).clamp(1, 32),
            SizeBtn::Sphere(d) => editor.sphere_r = (editor.sphere_r + d).clamp(1, 32),
        }
    }
    for i in &save_q {
        if *i == Interaction::Pressed {
            save.0 = true;
        }
    }
    for i in &load_q {
        if *i == Interaction::Pressed {
            load.0 = editor.path.clone();
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn refresh_button_styles(
    editor: Res<Editor>,
    mut mat_q: Query<(&MaterialBtn, &mut BorderColor)>,
    mut tool_q: Query<(&ToolBtn, &mut BorderColor), Without<MaterialBtn>>,
    mut prism_row: Query<&mut Node, (With<PrismRow>, Without<SphereRow>)>,
    mut sphere_row: Query<&mut Node, (With<SphereRow>, Without<PrismRow>)>,
    mut prism_vals: Query<(&PrismValue, &mut Text), Without<SphereValue>>,
    mut sphere_val: Query<&mut Text, (With<SphereValue>, Without<PrismValue>)>,
) {
    for (m, mut border) in &mut mat_q {
        let on = m.0 == editor.tag;
        *border = BorderColor::all(if on {
            Color::WHITE
        } else {
            Color::NONE
        });
    }
    for (t, mut border) in &mut tool_q {
        let on = t.0 == editor.tool;
        *border = BorderColor::all(if on {
            Color::WHITE
        } else {
            Color::NONE
        });
    }
    let show_prism = editor.tool == Tool::Prism;
    let show_sphere = editor.tool == Tool::Sphere;
    if let Ok(mut n) = prism_row.single_mut() {
        n.display = if show_prism {
            Display::Flex
        } else {
            Display::None
        };
    }
    if let Ok(mut n) = sphere_row.single_mut() {
        n.display = if show_sphere {
            Display::Flex
        } else {
            Display::None
        };
    }
    for (p, mut t) in &mut prism_vals {
        let v = match p.0 {
            0 => editor.prism.x,
            1 => editor.prism.y,
            _ => editor.prism.z,
        };
        *t = Text::new(v.to_string());
    }
    if let Ok(mut t) = sphere_val.single_mut() {
        *t = Text::new(editor.sphere_r.to_string());
    }
}

fn refresh_status(editor: Res<Editor>, mut q: Query<&mut Text, With<StatusText>>) {
    let Ok(mut text) = q.single_mut() else {
        return;
    };
    let path = editor
        .path
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "(no file)".into());
    let lock = if editor.locked { "cam" } else { "ui" };
    let s = format!("{}\n[Tab: {}]\n{}", path, lock, editor.status);
    *text = Text::new(s);
}

fn draw_overlays(mut gizmos: Gizmos) {
    let lo = Vec3::ZERO;
    let hi = Vec3::splat(WORLD);
    let grid_color = Color::srgba(0.4, 0.4, 0.4, 0.6);
    for i in 0..=GRID_VOX {
        let a = i as f32 * H;
        gizmos.line(Vec3::new(a, 0.0, 0.0), Vec3::new(a, 0.0, WORLD), grid_color);
        gizmos.line(Vec3::new(0.0, 0.0, a), Vec3::new(WORLD, 0.0, a), grid_color);
    }
    let edge = Color::srgba(1.0, 0.85, 0.3, 0.9);
    let corners = [
        Vec3::new(lo.x, lo.y, lo.z),
        Vec3::new(hi.x, lo.y, lo.z),
        Vec3::new(hi.x, lo.y, hi.z),
        Vec3::new(lo.x, lo.y, hi.z),
        Vec3::new(lo.x, hi.y, lo.z),
        Vec3::new(hi.x, hi.y, lo.z),
        Vec3::new(hi.x, hi.y, hi.z),
        Vec3::new(lo.x, hi.y, hi.z),
    ];
    let edges = [
        (0, 1),
        (1, 2),
        (2, 3),
        (3, 0),
        (4, 5),
        (5, 6),
        (6, 7),
        (7, 4),
        (0, 4),
        (1, 5),
        (2, 6),
        (3, 7),
    ];
    for (a, b) in edges {
        gizmos.line(corners[a], corners[b], edge);
    }
}

#[allow(clippy::too_many_arguments)]
fn do_click(
    editor: Res<Editor>,
    mouse: Res<ButtonInput<MouseButton>>,
    spatial: SpatialQuery,
    cam: Single<&GlobalTransform, With<Camera3d>>,
    parents: Query<&ChildOf>,
    grid_q: Query<(), With<GridRoot>>,
    ground_q: Query<(), With<GroundPlane>>,
    mut grids: Query<&mut Grid>,
) {
    if !editor.locked {
        return;
    }
    let break_ = mouse.just_pressed(MouseButton::Left);
    let place = mouse.just_pressed(MouseButton::Right);
    if break_ == place {
        return;
    }

    let origin = cam.translation();
    let dir = cam.forward();
    let filter = SpatialQueryFilter::default();
    let Some(hit) = spatial.cast_ray(origin, dir, 100.0, true, &filter) else {
        return;
    };

    // Resolve the entity to either ground or a chunk-of-grid hit.
    let on_ground = ground_q.get(hit.entity).is_ok();
    let mut grid = if on_ground {
        let Ok(g) = grids.single_mut() else {
            return;
        };
        g
    } else if let Ok(child_of) = parents.get(hit.entity) {
        let parent = child_of.parent();
        if grid_q.get(parent).is_ok() {
            let Ok(g) = grids.get_mut(parent) else {
                return;
            };
            g
        } else {
            return;
        }
    } else {
        return;
    };

    let p = origin + *dir * hit.distance;
    let (anchor, can_break) = if on_ground {
        let v = (p / H).floor().as_ivec3();
        (IVec3::new(v.x, 0, v.z), false)
    } else {
        let voxel = ((p - hit.normal * (H * 0.5)) / H).floor().as_ivec3();
        if break_ {
            (voxel, true)
        } else {
            // step out along the hit face
            let face = dominant_axis(hit.normal);
            (voxel + face, true)
        }
    };
    if break_ && !can_break {
        return;
    }

    let val = if break_ { 0 } else { editor.tag };
    apply_tool(&mut grid, anchor, val, editor.tool, editor.prism, editor.sphere_r);
}

fn dominant_axis(n: Vec3) -> IVec3 {
    let a = n.abs();
    if a.x >= a.y && a.x >= a.z {
        IVec3::new(n.x.signum() as i32, 0, 0)
    } else if a.y >= a.z {
        IVec3::new(0, n.y.signum() as i32, 0)
    } else {
        IVec3::new(0, 0, n.z.signum() as i32)
    }
}

fn apply_tool(grid: &mut Grid, anchor: IVec3, val: u8, tool: Tool, prism: IVec3, r: i32) {
    match tool {
        Tool::Single => {
            set_in_bounds(grid, anchor, val);
        }
        Tool::Prism => {
            let half = prism / 2;
            // include both ends so size N actually paints N voxels per axis
            let lo = anchor - half;
            let hi = anchor + (prism - half - IVec3::ONE);
            for z in lo.z..=hi.z {
                for y in lo.y..=hi.y {
                    for x in lo.x..=hi.x {
                        set_in_bounds(grid, IVec3::new(x, y, z), val);
                    }
                }
            }
        }
        Tool::Sphere => {
            let r2 = r * r;
            for dz in -r..=r {
                for dy in -r..=r {
                    for dx in -r..=r {
                        if dx * dx + dy * dy + dz * dz <= r2 {
                            set_in_bounds(grid, anchor + IVec3::new(dx, dy, dz), val);
                        }
                    }
                }
            }
        }
    }
}

fn set_in_bounds(grid: &mut Grid, v: IVec3, val: u8) {
    if v.x < 0 || v.y < 0 || v.z < 0 || v.x >= GRID_VOX || v.y >= GRID_VOX || v.z >= GRID_VOX {
        return;
    }
    grid.set(v, val);
}

fn perform_load(
    mut load: ResMut<PendingLoad>,
    mut editor: ResMut<Editor>,
    mut grids: Query<&mut Grid>,
) {
    let Some(path) = load.0.take() else {
        return;
    };
    let Ok(mut grid) = grids.single_mut() else {
        return;
    };
    // Clear existing voxels
    for z in 0..GRID_VOX {
        for y in 0..GRID_VOX {
            for x in 0..GRID_VOX {
                grid.set(IVec3::new(x, y, z), 0);
            }
        }
    }
    let path_str = path.to_string_lossy().to_string();
    match dot_vox::load(&path_str) {
        Ok(data) => {
            let n_loaded = load_into_grid(&data, &mut grid);
            editor.path = Some(path);
            editor.status = format!("loaded {} voxels", n_loaded);
        }
        Err(e) => {
            editor.status = format!("load failed: {}", e);
        }
    }
}

fn load_into_grid(data: &DotVoxData, grid: &mut Grid) -> usize {
    let Some(model) = data.models.first() else {
        return 0;
    };
    let palette = if data.palette.is_empty() {
        dot_vox::DEFAULT_PALETTE.clone()
    } else {
        data.palette.clone()
    };
    let mut count = 0usize;
    for v in &model.voxels {
        let pal_idx = data.index_map.get(v.i as usize).copied().unwrap_or(v.i + 1);
        let color = palette
            .get(pal_idx.saturating_sub(1) as usize)
            .copied()
            .unwrap_or(VoxColor {
                r: 255,
                g: 0,
                b: 255,
                a: 255,
            });
        let tag = nearest_tag(color);
        // .vox is Z-up; remap to our Y-up.
        let pos = IVec3::new(v.x as i32, v.z as i32, v.y as i32);
        if pos.x < GRID_VOX && pos.y < GRID_VOX && pos.z < GRID_VOX {
            grid.set(pos, tag);
            count += 1;
        }
    }
    count
}

fn nearest_tag(c: VoxColor) -> u8 {
    let target = [c.r as f32 / 255.0, c.g as f32 / 255.0, c.b as f32 / 255.0];
    let mut best = MATERIALS[0].0;
    let mut best_d = f32::MAX;
    for (tag, _, col) in MATERIALS {
        let d = (col[0] - target[0]).powi(2)
            + (col[1] - target[1]).powi(2)
            + (col[2] - target[2]).powi(2);
        if d < best_d {
            best_d = d;
            best = *tag;
        }
    }
    best
}

fn perform_save(mut save: ResMut<PendingSave>, mut editor: ResMut<Editor>, grids: Query<&Grid>) {
    if !save.0 {
        return;
    }
    save.0 = false;
    let Ok(grid) = grids.single() else {
        return;
    };
    let path = editor.path.clone().unwrap_or_else(|| PathBuf::from("out.vox"));

    let mut voxels: Vec<VoxVoxel> = Vec::new();
    for z in 0..GRID_VOX {
        for y in 0..GRID_VOX {
            for x in 0..GRID_VOX {
                let p = IVec3::new(x, y, z);
                let tag = grid.get(p);
                if tag == 0 {
                    continue;
                }
                // Y-up -> Z-up for .vox
                voxels.push(VoxVoxel {
                    x: x as u8,
                    y: z as u8,
                    z: y as u8,
                    i: tag.wrapping_sub(1),
                });
            }
        }
    }

    let palette = build_palette();
    let model = VoxModel {
        size: VoxSize {
            x: GRID_VOX as u32,
            y: GRID_VOX as u32,
            z: GRID_VOX as u32,
        },
        voxels,
    };
    let data = DotVoxData {
        version: 150,
        index_map: dot_vox::DEFAULT_INDEX_MAP.to_vec(),
        models: vec![model],
        palette,
        materials: vec![],
        scenes: vec![
            SceneNode::Transform {
                attributes: Default::default(),
                frames: vec![Default::default()],
                child: 1,
                layer_id: 0,
            },
            SceneNode::Group {
                attributes: Default::default(),
                children: vec![2],
            },
            SceneNode::Transform {
                attributes: Default::default(),
                frames: vec![Default::default()],
                child: 3,
                layer_id: 0,
            },
            SceneNode::Shape {
                attributes: Default::default(),
                models: vec![ShapeModel {
                    model_id: 0,
                    attributes: Default::default(),
                }],
            },
        ],
        layers: vec![],
    };

    match std::fs::File::create(&path).and_then(|mut f| data.write_vox(&mut f)) {
        Ok(()) => {
            editor.path = Some(path.clone());
            editor.status = format!("saved {}", path.display());
        }
        Err(e) => {
            editor.status = format!("save failed: {}", e);
        }
    }
}

fn build_palette() -> Vec<VoxColor> {
    let mut p: Vec<VoxColor> = dot_vox::DEFAULT_PALETTE.clone();
    while p.len() < 256 {
        p.push(VoxColor {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        });
    }
    for (tag, _, col) in MATERIALS {
        let idx = tag.wrapping_sub(1) as usize;
        p[idx] = VoxColor {
            r: (col[0] * 255.0) as u8,
            g: (col[1] * 255.0) as u8,
            b: (col[2] * 255.0) as u8,
            a: 255,
        };
    }
    p
}
