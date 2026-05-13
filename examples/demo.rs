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
            binding_types::{storage_buffer, storage_buffer_read_only, texture_3d, uniform_buffer},
            encase::UniformBuffer as EncaseUniform,
            *,
        },
        renderer::{RenderDevice, RenderQueue},
    },
    window::{CursorGrabMode, CursorOptions, PrimaryWindow, Window, WindowMode},
};
use noisy_bevy::NoisyShaderPlugin;

use voxxelmaxx::{BoundaryCollider, Grid, N, TerrainMaterial, VoxelPlugin};

const LOAD_RADIUS: f32 = 32.0;
const MOUSE_SENS: f32 = 0.002;
const MOVE_FORCE: f32 = 4.0;
const JUMP_IMPULSE: f32 = 1.0;
const PLACE_TAG: u8 = 0x80;
const BRUSH_RADIUS: i32 = 8;

const CHUNK_VOX: usize = N * N * N;
const MAX_CHUNKS_PER_DISPATCH: usize = 48;
/// One byte per voxel, packed into u32 words by the shader.
const CHUNK_BYTES: u64 = CHUNK_VOX as u64;
const OUTPUT_BYTES: u64 = CHUNK_BYTES * MAX_CHUNKS_PER_DISPATCH as u64;
const INDICES_BYTES: u64 = (MAX_CHUNKS_PER_DISPATCH * 16) as u64; // vec4<i32> stride
const SHADER_PATH: &str = "examples/proc_gen.wgsl";
const VOX_PATH: &str = "assets/examples/monu1.vox";

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
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut cursor_options: Single<&mut CursorOptions, With<PrimaryWindow>>,
) {
    // player
    cmd.spawn((
        Player::default(),
        Transform::from_xyz(0., 48., 0.),
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
    let palette = load_vox_palette(VOX_PATH).expect("failed to load .vox palette");
    cmd.spawn((
        Grid::default(),
        BoundaryCollider::default(),
        RigidBody::Static,
        ProcGen {
            seed: Vec3::ZERO,
            palette,
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

struct VoxPalette {
    size: UVec3,
    /// Dense `size.x * size.y * size.z` bytes, voxel.i for filled cells, 0 elsewhere.
    data: Vec<u8>,
}

#[derive(Component)]
pub struct ProcGen {
    seed: Vec3,
    palette: VoxPalette,
}

fn load_vox_palette(path: &str) -> Result<VoxPalette, &'static str> {
    let data = dot_vox::load(path)?;
    let model = data.models.first().ok_or("no models in .vox")?;
    let size = UVec3::new(model.size.x, model.size.y, model.size.z);
    let len = (size.x * size.y * size.z) as usize;
    let mut dense = vec![0u8; len];
    for v in &model.voxels {
        let x = v.x as u32;
        let y = v.y as u32;
        let z = v.z as u32;
        let i = (x + size.x * (y + size.y * z)) as usize;
        // dot_vox stores 0 as a valid palette index; bias by 1 so 0 means empty.
        dense[i] = v.i.wrapping_add(1);
    }
    Ok(VoxPalette { size, data: dense })
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
    mut grids: Query<(&mut Grid, &BoundaryCollider, &ProcGen)>,
    player: Single<&Transform, With<Player>>,
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
    let radius_sq = LOAD_RADIUS * LOAD_RADIUS;

    for (mut grid, boundary, proc) in &mut grids {
        gpu.ensure_palette(&device, &queue, &proc.palette);

        let mut candidates: Vec<IVec3> = if grid.len() == 0 {
            vec![player_chunk]
        } else {
            let mut scored: Vec<(f32, IVec3)> = boundary
                .chunks
                .iter()
                .filter_map(|&idx| {
                    let center = idx.as_vec3() + Vec3::splat(0.5);
                    let d2 = (center - player_pos).length_squared();
                    (d2 <= radius_sq).then_some((d2, idx))
                })
                .collect();
            scored.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
            scored
                .into_iter()
                .take(MAX_CHUNKS_PER_DISPATCH)
                .map(|(_, idx)| idx)
                .collect()
        };
        if candidates.is_empty() {
            continue;
        }
        candidates.truncate(MAX_CHUNKS_PER_DISPATCH);

        let params = GpuProcParams {
            palette_size: proc.palette.size,
            chunk_count: candidates.len() as u32,
            seed_u: proc.seed + Vec3::splat(101.0),
            _pad0: 0,
            seed_v: proc.seed + Vec3::splat(307.0),
            _pad1: 0,
            seed_w: proc.seed + Vec3::splat(523.0),
            _pad2: 0,
        };
        let chunks = gpu.dispatch(&device, &queue, &pipeline.0, &params, &candidates);
        for (idx, tags) in candidates.into_iter().zip(chunks.into_iter()) {
            grid.add_chunk(idx, move || tags);
        }
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
    palette_size: UVec3,
    chunk_count: u32,
    seed_u: Vec3,
    _pad0: u32,
    seed_v: Vec3,
    _pad1: u32,
    seed_w: Vec3,
    _pad2: u32,
}

#[derive(Resource)]
struct ProcGenPipeline(ComputePipeline);

#[derive(Resource)]
struct ProcGenGpu {
    layout: BindGroupLayout,
    output_buf: Buffer,
    staging_buf: Buffer,
    params_buf: Buffer,
    indices_buf: Buffer,
    palette_view: Option<TextureView>,
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
        let indices_buf = device.create_buffer(&BufferDescriptor {
            label: Some("proc_gen_indices"),
            size: INDICES_BYTES,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            layout,
            output_buf,
            staging_buf,
            params_buf,
            indices_buf,
            palette_view: None,
        }
    }

    /// Upload the .vox palette as an R8Uint 3D texture the first time it
    /// shows up.
    fn ensure_palette(&mut self, device: &RenderDevice, queue: &RenderQueue, pal: &VoxPalette) {
        if self.palette_view.is_some() {
            return;
        }
        let texture = device.create_texture(&TextureDescriptor {
            label: Some("proc_gen_palette"),
            size: Extent3d {
                width: pal.size.x,
                height: pal.size.y,
                depth_or_array_layers: pal.size.z,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D3,
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
            &pal.data,
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(pal.size.x),
                rows_per_image: Some(pal.size.y),
            },
            Extent3d {
                width: pal.size.x,
                height: pal.size.y,
                depth_or_array_layers: pal.size.z,
            },
        );
        self.palette_view = Some(texture.create_view(&TextureViewDescriptor::default()));
    }

    fn dispatch(
        &self,
        device: &RenderDevice,
        queue: &RenderQueue,
        pipeline: &ComputePipeline,
        params: &GpuProcParams,
        chunks: &[IVec3],
    ) -> Vec<Box<[u8; CHUNK_VOX]>> {
        let mut bytes = EncaseUniform::new(Vec::<u8>::new());
        bytes.write(params).unwrap();
        queue.write_buffer(&self.params_buf, 0, &bytes.into_inner());

        // vec4<i32> stride per chunk index.
        let mut indices = vec![0i32; chunks.len() * 4];
        for (slot, c) in chunks.iter().enumerate() {
            indices[slot * 4] = c.x;
            indices[slot * 4 + 1] = c.y;
            indices[slot * 4 + 2] = c.z;
        }
        let indices_bytes: &[u8] =
            unsafe { std::slice::from_raw_parts(indices.as_ptr() as *const u8, indices.len() * 4) };
        queue.write_buffer(&self.indices_buf, 0, indices_bytes);

        let bind_group = device.create_bind_group(
            "proc_gen",
            &self.layout,
            &BindGroupEntries::sequential((
                self.params_buf.as_entire_buffer_binding(),
                self.palette_view.as_ref().unwrap().into_binding(),
                self.indices_buf.as_entire_buffer_binding(),
                self.output_buf.as_entire_buffer_binding(),
            )),
        );

        let dispatch_bytes = chunks.len() as u64 * CHUNK_BYTES;
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
            // Workgroup is (4,4,4); shader uses gid.x as the x-word index
            // (4 voxels per thread along x) and packs (chunk_id, local_z)
            // into gid.z.
            pass.dispatch_workgroups(1, N as u32 / 4, chunks.len() as u32 * (N as u32 / 4));
        }
        encoder.copy_buffer_to_buffer(&self.output_buf, 0, &self.staging_buf, 0, dispatch_bytes);
        let submission = queue.submit([encoder.finish()]);

        let slice = self.staging_buf.slice(..dispatch_bytes);
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
        let mut out: Vec<Box<[u8; CHUNK_VOX]>> = Vec::with_capacity(chunks.len());
        for k in 0..chunks.len() {
            let mut tags: Box<[u8; CHUNK_VOX]> = Box::new([0; CHUNK_VOX]);
            let base = k * CHUNK_VOX;
            tags.copy_from_slice(&mapped[base..base + CHUNK_VOX]);
            out.push(tags);
        }
        drop(mapped);
        self.staging_buf.unmap();
        out
    }
}

fn proc_gen_layout_entries() -> Vec<BindGroupLayoutEntry> {
    BindGroupLayoutEntries::sequential(
        ShaderStages::COMPUTE,
        (
            uniform_buffer::<GpuProcParams>(false),
            texture_3d(TextureSampleType::Uint),
            storage_buffer_read_only::<Vec<IVec4>>(false),
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
