//! The flashy weapons:
//! Inferno (aimable flamethrower that ignites), Meteor Storm (telegraphed
//! impacts that leave burning ground) and Soul Bind (possession).

use std::collections::HashMap;

use bevy::prelude::*;

use crate::GameSet;
use crate::assets::{PlaySfx, Sfx};
use crate::config::*;
use crate::effects::{
    BoltMaterial, FxMaterials, FxMeshes, GlowMaterial, RingMaterial, spawn_bolt, spawn_flash, spawn_particles, spawn_ring,
    spawn_ring_burst,
};
use crate::enemies::{DamageEnemy, Enemy, Ignite, Possessed, Role, possess};
use crate::hero::{Loadout, Player, Stats};
use crate::rng::Rng;
use crate::weapons::WeaponKind;
use crate::world::{CameraShake, EnemyGrid, Gameplay};

pub struct SpecialsPlugin;

impl Plugin for SpecialsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, fire_specials.in_set(GameSet::Combat)).add_systems(
            Update,
            (update_meteors, update_ground_fire).in_set(GameSet::Combat).after(fire_specials),
        );
    }
}

const FIRE_COLOR: Color = Color::srgb(1.0, 0.55, 0.15);
const SOUL_COLOR: Color = Color::srgb(0.75, 0.4, 1.0);

#[derive(Component)]
struct MeteorFall {
    from: Vec2,
    target: Vec2,
    age: f32,
    fall_secs: f32,
    damage: f32,
    radius: f32,
    burn_secs: f32,
}

#[derive(Component)]
struct GroundFire {
    life: f32,
    radius: f32,
    dps: f32,
    tick: f32,
}

#[derive(Default)]
struct Clocks(HashMap<WeaponKind, f32>);

const FIRE_BOLT: Color = Color::srgb(1.0, 0.5, 0.12);

#[allow(clippy::too_many_arguments)]
fn fire_specials(
    mut commands: Commands,
    time: Res<Time>,
    grid: Res<EnemyGrid>,
    meshes: Res<FxMeshes>,
    fx: Res<FxMaterials>,
    mut rings: ResMut<Assets<RingMaterial>>,
    mut bolts: ResMut<Assets<BoltMaterial>>,
    mut rng: ResMut<Rng>,
    mut glow: ResMut<Assets<GlowMaterial>>,
    mut clocks: Local<Clocks>,
    mut sfx: MessageWriter<PlaySfx>,
    mut damage: MessageWriter<DamageEnemy>,
    mut ignite: MessageWriter<Ignite>,
    player: Single<(&Transform, &Loadout, &Stats), With<Player>>,
    enemies: Query<&Enemy, Without<Possessed>>,
) {
    let dt = time.delta_secs();
    let (transform, loadout, stats) = *player;
    let pos = transform.translation.truncate();

    for &(kind, level) in loadout.weapons.iter().filter(|(k, _)| k.is_special()) {
        let t = kind.table();
        let i = (level.clamp(1, MAX_LEVEL) - 1) as usize;
        let timer = clocks.0.entry(kind).or_insert(0.5);
        *timer -= dt;
        if *timer > 0.0 {
            continue;
        }
        *timer = t.cooldown[i] * stats.cooldown;
        let dmg = t.damage[i] * stats.might;
        let count = t.count[i] + if kind == WeaponKind::SoulBind { 0 } else { stats.amount };

        match kind {
            WeaponKind::Inferno => {
                // Fire arcs leap to random foes in range, chain once, and set everything they touch alight.
                const CHAIN_RANGE: f32 = 160.0;
                const CHAIN_FALLOFF: f32 = 0.7;
                let burn_dps = t.extra[i] as f32 * stats.might;
                let mut targets: Vec<(Entity, Vec2)> = grid.within(pos, t.area[i] * stats.area).collect();
                if targets.is_empty() {
                    *timer = 0.25;
                    continue;
                }
                for _ in 0..count.min(targets.len() as u32) {
                    let (first, at) = targets.swap_remove(rng.index(targets.len()));
                    spawn_bolt(&mut commands, &meshes, &mut bolts, &mut rng, pos, at, FIRE_BOLT, 40.0);
                    spawn_flash(&mut commands, &meshes, &mut glow, at, FIRE_BOLT, 70.0);
                    damage.write(DamageEnemy { entity: first, amount: dmg, knockback: Vec2::ZERO });
                    ignite.write(Ignite { entity: first, dps: burn_dps });
                    if let Some((next, next_at)) = grid
                        .within(at, CHAIN_RANGE)
                        .filter(|(e, _)| *e != first)
                        .min_by(|a, b| a.1.distance_squared(at).total_cmp(&b.1.distance_squared(at)))
                    {
                        spawn_bolt(&mut commands, &meshes, &mut bolts, &mut rng, at, next_at, FIRE_BOLT, 28.0);
                        damage.write(DamageEnemy { entity: next, amount: dmg * CHAIN_FALLOFF, knockback: Vec2::ZERO });
                        ignite.write(Ignite { entity: next, dps: burn_dps * CHAIN_FALLOFF });
                    }
                }
                sfx.write(PlaySfx(Sfx::Flame));
            }
            WeaponKind::Meteor => {
                const RANGE: f32 = 520.0;
                const FALL_SECS: f32 = 0.75;
                let targets: Vec<Vec2> = grid.within(pos, RANGE).map(|(_, p)| p).collect();
                if targets.is_empty() {
                    *timer = 0.3;
                    continue;
                }
                let radius = t.area[i] * stats.area;
                for _ in 0..count {
                    let target = targets[rng.index(targets.len())];
                    spawn_ring(&mut commands, &meshes, &mut rings, target, Color::srgb(1.0, 0.25, 0.15), radius, radius * 0.2, FALL_SECS);
                    let from = target + Vec2::new(rng.range(-220.0, -120.0), 620.0);
                    commands.spawn((
                        Gameplay,
                        MeteorFall { from, target, age: 0.0, fall_secs: FALL_SECS, damage: dmg, radius, burn_secs: t.extra[i] as f32 },
                        Mesh2d(meshes.quad.clone()),
                        MeshMaterial2d(fx.meteor.clone()),
                        Transform::from_translation(from.extend(Z_OVERHEAD_FX)).with_scale(Vec3::splat(70.0)),
                    ));
                }
                sfx.write(PlaySfx(Sfx::Meteor));
            }
            WeaponKind::SoulBind => {
                const RANGE: f32 = 450.0;
                let mut candidates: Vec<(Entity, Vec2)> = grid
                    .within(pos, RANGE)
                    .filter(|(e, _)| enemies.get(*e).is_ok_and(|en| en.kind.def().role != Role::Boss))
                    .collect();
                if candidates.is_empty() {
                    *timer = 0.5;
                    continue;
                }
                for _ in 0..count.min(candidates.len() as u32) {
                    let (entity, at) = candidates.swap_remove(rng.index(candidates.len()));
                    possess(
                        &mut commands,
                        &fx,
                        &meshes,
                        entity,
                        Possessed {
                            remaining: t.extra[i] as f32,
                            damage: dmg * 0.4,
                            blast_damage: dmg,
                            blast_radius: t.area[i] * stats.area,
                            attack_cooldown: 0.0,
                        },
                    );
                    spawn_bolt(&mut commands, &meshes, &mut bolts, &mut rng, pos, at, SOUL_COLOR, 40.0);
                    spawn_ring_burst(&mut commands, &meshes, &mut rings, at, SOUL_COLOR, 70.0, 0.35);
                }
                sfx.write(PlaySfx(Sfx::Soul));
            }
            _ => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn update_meteors(
    mut commands: Commands,
    time: Res<Time>,
    grid: Res<EnemyGrid>,
    meshes: Res<FxMeshes>,
    fx: Res<FxMaterials>,
    mut rings: ResMut<Assets<RingMaterial>>,
    mut glow: ResMut<Assets<GlowMaterial>>,
    mut rng: ResMut<Rng>,
    mut shake: ResMut<CameraShake>,
    mut damage: MessageWriter<DamageEnemy>,
    mut ignite: MessageWriter<Ignite>,
    mut sfx: MessageWriter<PlaySfx>,
    mut meteors: Query<(Entity, &mut MeteorFall, &mut Transform), Without<Player>>,
    player: Single<&Transform, With<Player>>,
) {
    let player_pos = player.translation.truncate();
    const IMPACT_SHAKE_NEAR: f32 = 9.0;
    const IMPACT_SHAKE_FAR: f32 = 2.5;
    const SHAKE_FALLOFF_DIST: f32 = 600.0;
    const BURN_DPS_FRACTION: f32 = 0.25;
    for (entity, mut meteor, mut transform) in &mut meteors {
        meteor.age += time.delta_secs();
        let t = (meteor.age / meteor.fall_secs).min(1.0);
        let pos = meteor.from.lerp(meteor.target, t * t);
        transform.translation = pos.extend(Z_OVERHEAD_FX);
        if rng.chance(0.6) {
            spawn_particles(&mut commands, &mut rng, pos, FIRE_COLOR, 1, 60.0);
        }
        if t < 1.0 {
            continue;
        }
        commands.entity(entity).try_despawn();
        let at = meteor.target;
        for (foe, p) in grid.within(at, meteor.radius) {
            damage.write(DamageEnemy { entity: foe, amount: meteor.damage, knockback: (p - at).normalize_or_zero() * 200.0 });
            ignite.write(Ignite { entity: foe, dps: meteor.damage * BURN_DPS_FRACTION });
        }
        spawn_ring_burst(&mut commands, &meshes, &mut rings, at, FIRE_COLOR, meteor.radius * 1.2, 0.4);
        spawn_flash(&mut commands, &meshes, &mut glow, at, Color::srgb(1.0, 0.7, 0.3), meteor.radius * 1.4);
        spawn_particles(&mut commands, &mut rng, at, FIRE_COLOR, 8, 280.0);
        commands.spawn((
            Gameplay,
            GroundFire { life: meteor.burn_secs * 0.6, radius: meteor.radius * 0.55, dps: meteor.damage * BURN_DPS_FRACTION, tick: 0.0 },
            Mesh2d(meshes.quad.clone()),
            MeshMaterial2d(fx.ground_fire.clone()),
            Transform::from_translation(at.extend(Z_GROUND_FX)).with_scale(Vec3::splat(meteor.radius * 1.15)),
        ));
        // Closer impacts kick harder; distant ones still rumble a little.
        let closeness = 1.0 - (at.distance(player_pos) / SHAKE_FALLOFF_DIST).min(1.0);
        shake.0 = shake.0.max(IMPACT_SHAKE_FAR + (IMPACT_SHAKE_NEAR - IMPACT_SHAKE_FAR) * closeness);
        sfx.write(PlaySfx(Sfx::Slam));
    }
}

fn update_ground_fire(
    mut commands: Commands,
    time: Res<Time>,
    grid: Res<EnemyGrid>,
    mut damage: MessageWriter<DamageEnemy>,
    mut ignite: MessageWriter<Ignite>,
    mut fires: Query<(Entity, &mut GroundFire, &mut Transform)>,
) {
    const TICK: f32 = 0.35;
    let dt = time.delta_secs();
    for (entity, mut fire, mut transform) in &mut fires {
        fire.life -= dt;
        if fire.life <= 0.0 {
            commands.entity(entity).try_despawn();
            continue;
        }
        if fire.life < 0.4 {
            transform.scale *= 1.0 - dt * 2.5;
        }
        fire.tick -= dt;
        if fire.tick > 0.0 {
            continue;
        }
        fire.tick = TICK;
        for (foe, _) in grid.within(transform.translation.truncate(), fire.radius) {
            damage.write(DamageEnemy { entity: foe, amount: fire.dps * TICK, knockback: Vec2::ZERO });
            ignite.write(Ignite { entity: foe, dps: fire.dps });
        }
    }
}
