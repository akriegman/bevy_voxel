//! This example showcases a few things because it was the first
//! example. Procedural generation, a first person dynamic grid
//! player, and breaking / placing voxels. Uses Minecraft controls.

use std::sync::mpsc;

use avian3d::prelude::*;
use bevy::{
    input::mouse::AccumulatedMouseMotion,
    prelude::*,
    render::{
        MainWorld, RenderApp, RenderStartup,
        render_resource::{
            binding_types::{storage_buffer, texture_2d, uniform_buffer},
            encase::UniformBuffer as EncaseUniform,
            *,
        },
        renderer::{RenderDevice, RenderQueue},
    },
    window::{CursorGrabMode, CursorOptions, PrimaryWindow, Window, WindowMode},
};
use noisy_bevy::NoisyShaderPlugin;

use voxxelmaxx::{BoundaryCollider, Grid, N, TerrainMaterial, VoxelPlugin};

const LOAD_RADIUS: f32 = 8.0;
const MOUSE_SENS: f32 = 0.002;
const MOVE_FORCE: f32 = 4.0;
const JUMP_IMPULSE: f32 = 1.0;
const PLACE_TAG: u8 = 0x80;
const BRUSH_RADIUS: i32 = 8;

const CHUNK_VOX: usize = N * N * N;
const OUTPUT_BYTES: u64 = (CHUNK_VOX * 4) as u64;
const SHADER_PATH: &str = "examples/proc_gen.wgsl";

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                mode: WindowMode::BorderlessFullscreen(MonitorSelection::Primary),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(PhysicsPlugins::default())
        .add_plugins(VoxelPlugin)
        .add_plugins(NoisyShaderPlugin)
        .add_plugins(ProcGenGpuPlugin)
        .insert_resource(Gravity::default())
        .add_systems(Startup, setup)
        .add_systems(Update, movement)
        .add_systems(Update, build_break)
        .add_systems(Update, proc_gen)
        .run();
}

fn setup(
    mut cmd: Commands,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut cursor_options: Single<&mut CursorOptions, With<PrimaryWindow>>,
) {
    // player
    cmd.spawn((
        Player::default(),
        Transform::from_xyz(0., 3., 0.),
        Visibility::default(),
        RigidBody::Dynamic,
        Collider::cylinder(0.25, 1.),
        Mesh3d(meshes.add(Capsule3d::new(0.25, 0.5))),
        MeshMaterial3d(materials.add(Color::srgb(0.8, 0.7, 0.6))),
        LockedAxes::ROTATION_LOCKED,
        Friction::new(0.1),
        LinearDamping(2.0),
    ))
    .with_child((Camera3d::default(), Transform::from_xyz(0., 0.25, 0.)));

    // world
    cmd.spawn((
        Grid::default(),
        BoundaryCollider::default(),
        RigidBody::Static,
        ProcGen {
            seed: Vec3::ZERO,
            image: asset_server.load("gen_map.png"),
        },
    ));

    // light
    cmd.spawn((
        DirectionalLight::default(),
        Transform::from_xyz(2., 4., 1.).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    cmd.spawn((
        DirectionalLight::default(),
        Transform::from_xyz(-2., 4., -1.).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    cmd.insert_resource(TerrainMaterial(materials.add(Color::WHITE)));

    cursor_options.grab_mode = CursorGrabMode::Locked;
    cursor_options.visible = false;
}

#[derive(Component, Default)]
struct Player {
    yaw: f32,
    pitch: f32,
}

#[derive(Component)]
pub struct ProcGen {
    seed: Vec3,
    image: Handle<Image>,
}

fn movement(
    mut player: Single<(Forces, &mut Player, &mut Transform), With<Player>>,
    mut cam: Single<&mut Transform, (With<Camera3d>, Without<Player>)>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<AccumulatedMouseMotion>,
) {
    let (forces, player, body_tf) = &mut *player;

    if mouse.delta != Vec2::ZERO {
        player.yaw -= mouse.delta.x * MOUSE_SENS;
        player.pitch = (player.pitch - mouse.delta.y * MOUSE_SENS).clamp(-1.54, 1.54);
        body_tf.rotation = Quat::from_axis_angle(Vec3::Y, player.yaw);
        cam.rotation = Quat::from_axis_angle(Vec3::X, player.pitch);
    }

    let forward = body_tf.forward();
    let forward_h = Vec3::new(forward.x, 0., forward.z).normalize_or_zero();
    let right = body_tf.right();
    let right_h = Vec3::new(right.x, 0., right.z).normalize_or_zero();

    let mut dir = Vec3::ZERO;
    if keys.pressed(KeyCode::KeyW) {
        dir += forward_h;
    }
    if keys.pressed(KeyCode::KeyS) {
        dir -= forward_h;
    }
    if keys.pressed(KeyCode::KeyD) {
        dir += right_h;
    }
    if keys.pressed(KeyCode::KeyA) {
        dir -= right_h;
    }

    forces.apply_force(dir.normalize_or_zero() * MOVE_FORCE);

    if keys.just_pressed(KeyCode::Space) {
        forces.apply_linear_impulse(Vec3::Y * JUMP_IMPULSE);
    }
}

fn build_break(
    mouse: Res<ButtonInput<MouseButton>>,
    spatial: SpatialQuery,
    cam: Single<&GlobalTransform, With<Camera3d>>,
    player_e: Single<Entity, With<Player>>,
    parents: Query<&ChildOf>,
    mut grids: Query<&mut Grid>,
) {
    let break_ = mouse.pressed(MouseButton::Left);
    let place = mouse.pressed(MouseButton::Right);
    if !(break_ ^ place) {
        return;
    }

    let origin = cam.translation();
    let dir = cam.forward();
    let filter = SpatialQueryFilter::from_excluded_entities([*player_e]);
    let Some(hit) = spatial.cast_ray(origin, dir, f32::INFINITY, true, &filter) else {
        return;
    };
    // Hit lands on a chunk render-child entity; the grid is its parent.
    let Ok(child_of) = parents.get(hit.entity) else {
        return;
    };
    let Ok(mut grid) = grids.get_mut(child_of.parent()) else {
        return;
    };

    let (vox, face) = grid.rayhit_face(origin, *dir, &hit);
    let center = if break_ { vox } else { vox + face };
    let tag = if break_ { 0 } else { PLACE_TAG };
    let r = BRUSH_RADIUS;
    let r_sq = (r * r) as f32;
    for dz in -r..=r {
        for dy in -r..=r {
            for dx in -r..=r {
                let offset = IVec3::new(dx, dy, dz);
                if offset.as_vec3().length_squared() > r_sq {
                    continue;
                }
                grid.set(center + offset, tag);
            }
        }
    }
}

fn proc_gen(
    mut grids: Query<(&mut Grid, &ProcGen)>,
    boundary_q: Query<(), With<BoundaryCollider>>,
    spatial: SpatialQuery,
    player: Single<&Transform, With<Player>>,
    images: Res<Assets<Image>>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    pipeline: Option<Res<ProcGenPipeline>>,
    mut gpu: ResMut<ProcGenGpu>,
) {
    let Some(pipeline) = pipeline else {
        return;
    };
    let player_pos = player.translation;
    let player_chunk = player_pos.floor().as_ivec3();

    for (mut grid, proc) in &mut grids {
        let Some(map) = images.get(&proc.image) else {
            continue;
        };
        if !gpu.ensure_gen_map(&device, &queue, map) {
            continue;
        }

        let idx = if grid.len() == 0 {
            player_chunk
        } else {
            let Some(proj) = spatial.project_point_predicate(
                player_pos,
                true,
                &SpatialQueryFilter::default(),
                &|e| boundary_q.contains(e),
            ) else {
                continue;
            };
            if (proj.point - player_pos).length() > LOAD_RADIUS {
                continue;
            }
            let step = (proj.point - player_pos).normalize_or_zero() * 0.001;
            (proj.point + step).floor().as_ivec3()
        };

        let params = GpuProcParams {
            chunk_idx: idx,
            map_w: gpu.gen_map_size.x,
            seed_u: proc.seed + Vec3::splat(101.0),
            map_h: gpu.gen_map_size.y,
            seed_v: proc.seed + Vec3::splat(307.0),
            _pad: 0,
        };
        let tags = gpu.dispatch(&device, &queue, &pipeline.0, &params);
        grid.add_chunk(idx, move || tags);
    }
}

// --- GPU plumbing --------------------------------------------------------
//
// PipelineCache lives in the render world and handles shader loading +
// `#import` preprocessing + async pipeline compilation. We let it do that,
// then ship the compiled `ComputePipeline` into the main world via
// ExtractSchedule. From then on the main-world `proc_gen` system drives
// dispatch + readback synchronously through a separate command encoder.

#[derive(ShaderType, Clone, Default)]
struct GpuProcParams {
    chunk_idx: IVec3,
    map_w: u32,
    seed_u: Vec3,
    map_h: u32,
    seed_v: Vec3,
    _pad: u32,
}

#[derive(Resource)]
struct ProcGenPipeline(ComputePipeline);

#[derive(Resource)]
struct ProcGenGpu {
    layout: BindGroupLayout,
    output_buf: Buffer,
    staging_buf: Buffer,
    params_buf: Buffer,
    gen_map_view: Option<TextureView>,
    gen_map_size: UVec2,
}

impl ProcGenGpu {
    fn new(device: &RenderDevice) -> Self {
        let layout = device.create_bind_group_layout("proc_gen", &proc_gen_layout_entries());
        let output_buf = device.create_buffer(&BufferDescriptor {
            label: Some("proc_gen_output"),
            size: OUTPUT_BYTES,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let staging_buf = device.create_buffer(&BufferDescriptor {
            label: Some("proc_gen_staging"),
            size: OUTPUT_BYTES,
            usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let params_buf = device.create_buffer(&BufferDescriptor {
            label: Some("proc_gen_params"),
            size: GpuProcParams::min_size().get(),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            layout,
            output_buf,
            staging_buf,
            params_buf,
            gen_map_view: None,
            gen_map_size: UVec2::ZERO,
        }
    }

    /// Upload the source image as an R8Uint texture the first time it
    /// shows up. Returns true once the texture is ready to bind.
    fn ensure_gen_map(
        &mut self,
        device: &RenderDevice,
        queue: &RenderQueue,
        map: &Image,
    ) -> bool {
        if self.gen_map_view.is_some() {
            return true;
        }
        let Some(data) = map.data.as_deref() else {
            return false;
        };
        let w = map.width();
        let h = map.height();
        let pixels = (w * h) as usize;
        let bpp = data.len() / pixels;
        let red: Vec<u8> = data.chunks_exact(bpp).map(|c| c[0]).collect();

        let texture = device.create_texture(&TextureDescriptor {
            label: Some("proc_gen_genmap"),
            size: Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::R8Uint,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            &red,
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w),
                rows_per_image: Some(h),
            },
            Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        self.gen_map_view = Some(texture.create_view(&TextureViewDescriptor::default()));
        self.gen_map_size = UVec2::new(w, h);
        true
    }

    fn dispatch(
        &self,
        device: &RenderDevice,
        queue: &RenderQueue,
        pipeline: &ComputePipeline,
        params: &GpuProcParams,
    ) -> Box<[u8; CHUNK_VOX]> {
        let mut bytes = EncaseUniform::new(Vec::<u8>::new());
        bytes.write(params).unwrap();
        queue.write_buffer(&self.params_buf, 0, &bytes.into_inner());

        let bind_group = device.create_bind_group(
            "proc_gen",
            &self.layout,
            &BindGroupEntries::sequential((
                self.params_buf.as_entire_buffer_binding(),
                self.gen_map_view.as_ref().unwrap().into_binding(),
                self.output_buf.as_entire_buffer_binding(),
            )),
        );

        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("proc_gen"),
        });
        {
            let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
                label: Some("proc_gen"),
                timestamp_writes: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(N as u32 / 4, N as u32 / 4, N as u32 / 4);
        }
        encoder.copy_buffer_to_buffer(&self.output_buf, 0, &self.staging_buf, 0, OUTPUT_BYTES);
        let submission = queue.submit([encoder.finish()]);

        let slice = self.staging_buf.slice(..);
        let (tx, rx) = mpsc::channel();
        slice.map_async(MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        device
            .poll(PollType::Wait {
                submission_index: Some(submission),
                timeout: None,
            })
            .unwrap();
        rx.recv().unwrap().unwrap();

        let mapped = slice.get_mapped_range();
        let mut tags: Box<[u8; CHUNK_VOX]> = Box::new([0; CHUNK_VOX]);
        for i in 0..CHUNK_VOX {
            tags[i] = mapped[i * 4];
        }
        drop(mapped);
        self.staging_buf.unmap();
        tags
    }
}

fn proc_gen_layout_entries() -> Vec<BindGroupLayoutEntry> {
    BindGroupLayoutEntries::sequential(
        ShaderStages::COMPUTE,
        (
            uniform_buffer::<GpuProcParams>(false),
            texture_2d(TextureSampleType::Uint),
            storage_buffer::<Vec<u32>>(false),
        ),
    )
    .to_vec()
}

struct ProcGenGpuPlugin;

impl Plugin for ProcGenGpuPlugin {
    fn build(&self, _app: &mut App) {}

    fn finish(&self, app: &mut App) {
        let render_app = app.sub_app_mut(RenderApp);
        render_app
            .add_systems(RenderStartup, queue_pipeline)
            .add_systems(ExtractSchedule, export_pipeline);
        let device = render_app.world().resource::<RenderDevice>().clone();
        app.insert_resource(ProcGenGpu::new(&device));
    }
}

#[derive(Resource)]
struct ProcGenPipelineId(CachedComputePipelineId);

fn queue_pipeline(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    pipeline_cache: Res<PipelineCache>,
) {
    let layout = BindGroupLayoutDescriptor {
        label: "proc_gen".into(),
        entries: proc_gen_layout_entries(),
    };
    let shader = asset_server.load(SHADER_PATH);
    let id = pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
        label: Some("proc_gen".into()),
        layout: vec![layout],
        shader,
        ..default()
    });
    commands.insert_resource(ProcGenPipelineId(id));
}

fn export_pipeline(
    pipeline_id: Option<Res<ProcGenPipelineId>>,
    pipeline_cache: Res<PipelineCache>,
    mut main_world: ResMut<MainWorld>,
) {
    if main_world.contains_resource::<ProcGenPipeline>() {
        return;
    }
    let Some(id) = pipeline_id else {
        return;
    };
    if let Some(p) = pipeline_cache.get_compute_pipeline(id.0) {
        main_world.insert_resource(ProcGenPipeline(p.clone()));
    }
}
