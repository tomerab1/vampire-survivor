//! Visual effects: the WGSL materials (glow, ring, bolt, vignette), expanding
//! rings, lightning bolts, particles, floating combat text, and screen feedback.

use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;
use bevy::sprite_render::{AlphaMode2d, Material2d, Material2dPlugin};
use bevy::window::PrimaryWindow;

use crate::GameSet;
use crate::assets::GameAssets;
use crate::config::*;
use crate::rng::Rng;
use crate::world::Gameplay;

pub struct EffectsPlugin;

impl Plugin for EffectsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            Material2dPlugin::<GlowMaterial>::default(),
            Material2dPlugin::<RingMaterial>::default(),
            Material2dPlugin::<BoltMaterial>::default(),
            Material2dPlugin::<VignetteMaterial>::default(),
        ))
        .init_resource::<ScreenFx>()
        .add_systems(Startup, (init_fx_meshes, spawn_vignette).chain())
        .add_systems(Update, (update_rings, update_bolts, update_flashes, update_particles, update_float_text).in_set(GameSet::Presentation))
        .add_systems(PostUpdate, update_vignette);
    }
}

/// Every effect material shares the same uniform layout: a color and four shader-specific params.
macro_rules! fx_material {
    ($name:ident, $path:literal) => {
        #[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
        pub struct $name {
            #[uniform(0)]
            pub color: LinearRgba,
            #[uniform(1)]
            pub params: Vec4,
        }

        impl Material2d for $name {
            fn fragment_shader() -> ShaderRef {
                $path.into()
            }

            fn alpha_mode(&self) -> AlphaMode2d {
                AlphaMode2d::Blend
            }
        }
    };
}

fx_material!(GlowMaterial, "shaders/glow.wgsl");
fx_material!(RingMaterial, "shaders/ring.wgsl");
fx_material!(BoltMaterial, "shaders/bolt.wgsl");
fx_material!(VignetteMaterial, "shaders/vignette.wgsl");

/// Shared unit meshes; scale them with `Transform`.
#[derive(Resource)]
pub struct FxMeshes {
    pub quad: Handle<Mesh>,
    pub rhombus: Handle<Mesh>,
}

/// Screen-level feedback driven by gameplay (decays each frame).
#[derive(Resource, Default)]
pub struct ScreenFx {
    pub hurt: f32,
    pub levelup: f32,
    pub low_hp: f32,
}

#[derive(Component)]
struct Vignette;

/// An expanding, fading ring (shockwaves, bomb blasts, level-up bursts).
#[derive(Component)]
pub struct RingFx {
    pub age: f32,
    pub duration: f32,
    pub from: f32,
    pub to: f32,
}

#[derive(Component)]
pub struct BoltFx {
    pub age: f32,
    pub duration: f32,
}

/// A glow that pops and fades (lightning strike points, explosions).
#[derive(Component)]
pub struct FlashFx {
    pub age: f32,
    pub duration: f32,
    pub size: f32,
}

#[derive(Component)]
struct Particle {
    velocity: Vec2,
    life: f32,
    max_life: f32,
}

#[derive(Component)]
pub struct FloatText {
    life: f32,
    max_life: f32,
}

fn init_fx_meshes(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    commands.insert_resource(FxMeshes {
        quad: meshes.add(Rectangle::new(1.0, 1.0)),
        rhombus: meshes.add(Rhombus::new(1.0, 1.4)),
    });
}

fn spawn_vignette(mut commands: Commands, meshes: Res<FxMeshes>, mut materials: ResMut<Assets<VignetteMaterial>>) {
    commands.spawn((
        Vignette,
        Mesh2d(meshes.quad.clone()),
        MeshMaterial2d(materials.add(VignetteMaterial {
            color: LinearRgba::new(0.0, 0.0, 0.0, 0.55),
            params: Vec4::new(0.0, 0.0, 0.0, 16.0 / 9.0),
        })),
        Transform::from_xyz(0.0, 0.0, Z_VIGNETTE),
    ));
}

fn update_vignette(
    time: Res<Time>,
    mut fx: ResMut<ScreenFx>,
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<&Transform, (With<Camera2d>, Without<Vignette>)>,
    vignette: Single<(&mut Transform, &MeshMaterial2d<VignetteMaterial>), With<Vignette>>,
    mut materials: ResMut<Assets<VignetteMaterial>>,
) {
    const HURT_DECAY: f32 = 3.0;
    const LEVELUP_DECAY: f32 = 1.2;
    let dt = time.delta_secs();
    fx.hurt = (fx.hurt - dt * HURT_DECAY).max(0.0);
    fx.levelup = (fx.levelup - dt * LEVELUP_DECAY).max(0.0);

    let (mut transform, material) = vignette.into_inner();
    let size = window.size();
    transform.translation = camera.translation.truncate().extend(Z_VIGNETTE);
    transform.scale = size.extend(1.0);
    if let Some(mut material) = materials.get_mut(&material.0) {
        material.params = Vec4::new(fx.hurt, fx.low_hp, fx.levelup, size.x / size.y.max(1.0));
    }
}

/// A soft glowing orb of `size` pixels.
pub fn glow_bundle(
    meshes: &FxMeshes,
    materials: &mut Assets<GlowMaterial>,
    color: Color,
    size: f32,
    pulse: f32,
) -> (Mesh2d, MeshMaterial2d<GlowMaterial>, Transform) {
    (
        Mesh2d(meshes.quad.clone()),
        MeshMaterial2d(materials.add(GlowMaterial {
            color: color.to_linear(),
            params: Vec4::new(pulse, 2.2, 0.25, 0.0),
        })),
        Transform::from_scale(Vec3::splat(size)),
    )
}

pub fn ring_material(color: Color, inner: f32, swirl: f32, fill: f32) -> RingMaterial {
    RingMaterial { color: color.to_linear(), params: Vec4::new(inner, 0.08, swirl, fill) }
}

pub fn spawn_ring_burst(
    commands: &mut Commands,
    meshes: &FxMeshes,
    materials: &mut Assets<RingMaterial>,
    pos: Vec2,
    color: Color,
    to: f32,
    duration: f32,
) {
    commands.spawn((
        Gameplay,
        RingFx { age: 0.0, duration, from: 10.0, to },
        Mesh2d(meshes.quad.clone()),
        MeshMaterial2d(materials.add(ring_material(color, 0.82, 4.0, 0.12))),
        Transform::from_translation(pos.extend(Z_GROUND_FX)).with_scale(Vec3::splat(20.0)),
    ));
}

fn update_rings(
    mut commands: Commands,
    time: Res<Time>,
    mut rings: Query<(Entity, &mut RingFx, &mut Transform, &MeshMaterial2d<RingMaterial>)>,
    mut materials: ResMut<Assets<RingMaterial>>,
) {
    for (entity, mut ring, mut transform, material) in &mut rings {
        ring.age += time.delta_secs();
        let t = (ring.age / ring.duration).min(1.0);
        if t >= 1.0 {
            commands.entity(entity).try_despawn();
            continue;
        }
        let eased = 1.0 - (1.0 - t).powi(3);
        transform.scale = Vec3::splat((ring.from + (ring.to - ring.from) * eased) * 2.0);
        if let Some(mut m) = materials.get_mut(&material.0) {
            m.color.alpha = 1.0 - t;
        }
    }
}

/// A lightning bolt between two world points.
pub fn spawn_bolt(
    commands: &mut Commands,
    meshes: &FxMeshes,
    materials: &mut Assets<BoltMaterial>,
    rng: &mut Rng,
    from: Vec2,
    to: Vec2,
    color: Color,
    width: f32,
) {
    const SEGMENTS_PER_PX: f32 = 1.0 / 28.0;
    let delta = to - from;
    let length = delta.length().max(1.0);
    commands.spawn((
        Gameplay,
        BoltFx { age: 0.0, duration: 0.28 },
        Mesh2d(meshes.quad.clone()),
        MeshMaterial2d(materials.add(BoltMaterial {
            color: color.to_linear(),
            params: Vec4::new(rng.range(0.0, 100.0), 1.0, 0.16, (length * SEGMENTS_PER_PX).max(3.0).round()),
        })),
        Transform::from_translation(((from + to) / 2.0).extend(Z_OVERHEAD_FX))
            .with_rotation(Quat::from_rotation_z(delta.y.atan2(delta.x)))
            .with_scale(Vec3::new(length, width, 1.0)),
    ));
}

fn update_bolts(
    mut commands: Commands,
    time: Res<Time>,
    mut bolts: Query<(Entity, &mut BoltFx, &MeshMaterial2d<BoltMaterial>)>,
    mut materials: ResMut<Assets<BoltMaterial>>,
) {
    for (entity, mut bolt, material) in &mut bolts {
        bolt.age += time.delta_secs();
        let t = bolt.age / bolt.duration;
        if t >= 1.0 {
            commands.entity(entity).try_despawn();
        } else if let Some(mut m) = materials.get_mut(&material.0) {
            m.params.y = 1.0 - t * t;
        }
    }
}

pub fn spawn_flash(
    commands: &mut Commands,
    meshes: &FxMeshes,
    materials: &mut Assets<GlowMaterial>,
    pos: Vec2,
    color: Color,
    size: f32,
) {
    let (mesh, material, transform) = glow_bundle(meshes, materials, color, size, 0.0);
    commands.spawn((
        Gameplay,
        FlashFx { age: 0.0, duration: 0.3, size },
        mesh,
        material,
        transform.with_translation(pos.extend(Z_OVERHEAD_FX - 0.5)),
    ));
}

fn update_flashes(
    mut commands: Commands,
    time: Res<Time>,
    mut flashes: Query<(Entity, &mut FlashFx, &mut Transform, &MeshMaterial2d<GlowMaterial>)>,
    mut materials: ResMut<Assets<GlowMaterial>>,
) {
    for (entity, mut flash, mut transform, material) in &mut flashes {
        flash.age += time.delta_secs();
        let t = flash.age / flash.duration;
        if t >= 1.0 {
            commands.entity(entity).try_despawn();
            continue;
        }
        transform.scale = Vec3::splat(flash.size * (0.6 + 0.6 * t));
        if let Some(mut m) = materials.get_mut(&material.0) {
            m.color.alpha = (1.0 - t).powi(2);
        }
    }
}

pub fn spawn_particles(commands: &mut Commands, rng: &mut Rng, pos: Vec2, color: Color, count: usize, speed: f32) {
    const PARTICLE_SIZE: f32 = 5.0;
    for _ in 0..count {
        let life = rng.range(0.25, 0.55);
        commands.spawn((
            Gameplay,
            Particle { velocity: Vec2::from_angle(rng.angle()) * rng.range(0.3, 1.0) * speed, life, max_life: life },
            Sprite::from_color(color, Vec2::splat(PARTICLE_SIZE)),
            Transform::from_translation(pos.extend(Z_OVERHEAD_FX)),
        ));
    }
}

fn update_particles(
    mut commands: Commands,
    time: Res<Time>,
    mut particles: Query<(Entity, &mut Particle, &mut Transform, &mut Sprite)>,
) {
    const DRAG: f32 = 4.0;
    let dt = time.delta_secs();
    for (entity, mut p, mut transform, mut sprite) in &mut particles {
        p.life -= dt;
        if p.life <= 0.0 {
            commands.entity(entity).try_despawn();
            continue;
        }
        let v = p.velocity;
        p.velocity -= v * DRAG * dt;
        transform.translation += (v * dt).extend(0.0);
        let alpha = p.life / p.max_life;
        sprite.color = sprite.color.with_alpha(alpha);
        transform.scale = Vec3::splat(0.5 + alpha * 0.5);
    }
}

/// Floating combat text (damage numbers, pickup toasts). Skipped when too many are alive.
pub fn spawn_float_text(
    commands: &mut Commands,
    assets: &GameAssets,
    existing: usize,
    pos: Vec2,
    text: impl Into<String>,
    color: Color,
    size: f32,
) {
    if existing >= MAX_DAMAGE_NUMBERS {
        return;
    }
    const LIFE: f32 = 0.7;
    commands.spawn((
        Gameplay,
        FloatText { life: LIFE, max_life: LIFE },
        Text2d::new(text),
        TextFont { font: assets.pixel_font.clone().into(), font_size: FontSize::Px(size), ..default() },
        TextColor(color),
        Transform::from_translation(pos.extend(Z_WORLD_TEXT)),
    ));
}

fn update_float_text(
    mut commands: Commands,
    time: Res<Time>,
    mut texts: Query<(Entity, &mut FloatText, &mut Transform, &mut TextColor)>,
) {
    const RISE_SPEED: f32 = 45.0;
    let dt = time.delta_secs();
    for (entity, mut text, mut transform, mut color) in &mut texts {
        text.life -= dt;
        if text.life <= 0.0 {
            commands.entity(entity).try_despawn();
            continue;
        }
        transform.translation.y += RISE_SPEED * dt;
        color.0 = color.0.with_alpha((text.life / text.max_life * 1.5).min(1.0));
    }
}
