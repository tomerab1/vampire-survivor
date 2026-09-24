//! Auto weapons. Each `WeaponKind` has a per-level stat table; the player's
//! `Loadout` says which ones fire. Knives and bow aim at the cursor while the
//! mouse is held and auto-target otherwise; everything else is automatic.

use std::collections::HashMap;
use std::f32::consts::{FRAC_PI_2, TAU};

use bevy::prelude::*;

use crate::GameSet;
use crate::assets::{GameAssets, PlaySfx, Sfx};
use crate::config::*;
use crate::effects::{BoltMaterial, FxMeshes, GlowMaterial, RingMaterial, glow_bundle, ring_material, spawn_bolt, spawn_flash, spawn_ring_burst};
use crate::enemies::DamageEnemy;
use crate::hero::{Intent, Loadout, Player, Stats};
use crate::rng::Rng;
use crate::world::{CrateHit, EnemyGrid, Gameplay, Obstacles};

pub struct WeaponsPlugin;

impl Plugin for WeaponsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (fire_weapons, sync_orbiters, sync_aura).in_set(GameSet::Combat),
        )
        .add_systems(Update, (move_projectiles, spin_orbiters, aura_damage).in_set(GameSet::Combat).after(fire_weapons));
    }
}

const LIGHTNING_COLOR: Color = Color::srgb(0.45, 0.75, 1.0);

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum WeaponKind {
    Knives,
    Bow,
    MagicBolt,
    Axes,
    Aura,
    Lightning,
    Hammer,
}

pub struct WeaponDef {
    pub name: &'static str,
    pub icon: &'static str,
    pub blurb: &'static str,
}

/// Per-level numbers; index 0 is level 1.
struct Table {
    cooldown: [f32; 5],
    damage: [f32; 5],
    count: [u32; 5],
    /// Pierce for projectiles, chain jumps for lightning, unused otherwise.
    extra: [u32; 5],
    /// Radius for area weapons.
    area: [f32; 5],
}

impl WeaponKind {
    pub const ALL: [WeaponKind; 7] = [
        WeaponKind::Knives,
        WeaponKind::Bow,
        WeaponKind::MagicBolt,
        WeaponKind::Axes,
        WeaponKind::Aura,
        WeaponKind::Lightning,
        WeaponKind::Hammer,
    ];

    pub fn def(self) -> WeaponDef {
        let (name, icon, blurb) = match self {
            WeaponKind::Knives => ("Throwing Knives", "weapon_knife", "Fast knives at your aim / the nearest foe"),
            WeaponKind::Bow => ("Longbow", "weapon_bow", "Heavy arrows that pierce through lines"),
            WeaponKind::MagicBolt => ("Hex Staff", "weapon_red_magic_staff", "Homing magic bolts"),
            WeaponKind::Axes => ("Orbit Axes", "weapon_throwing_axe", "Axes circle you, cutting all they touch"),
            WeaponKind::Aura => ("Blight Ward", "flask_big_green", "A burning ring that damages nearby foes"),
            WeaponKind::Lightning => ("Storm Staff", "weapon_green_magic_staff", "Chain lightning strikes random foes"),
            WeaponKind::Hammer => ("Quake Hammer", "weapon_big_hammer", "Periodic shockwave that blasts foes back"),
        };
        WeaponDef { name, icon, blurb }
    }

    fn table(self) -> Table {
        match self {
            WeaponKind::Knives => Table {
                cooldown: [1.0, 0.95, 0.85, 0.75, 0.6],
                damage: [12.0, 12.0, 16.0, 16.0, 20.0],
                count: [1, 2, 2, 3, 4],
                extra: [1, 1, 2, 2, 3],
                area: [0.0; 5],
            },
            WeaponKind::Bow => Table {
                cooldown: [1.4, 1.35, 1.3, 1.2, 1.1],
                damage: [20.0, 26.0, 26.0, 32.0, 40.0],
                count: [1, 1, 2, 3, 3],
                extra: [3, 4, 4, 5, 8],
                area: [0.0; 5],
            },
            WeaponKind::MagicBolt => Table {
                cooldown: [1.2, 1.1, 1.0, 0.9, 0.75],
                damage: [15.0, 15.0, 20.0, 22.0, 28.0],
                count: [1, 2, 2, 3, 3],
                extra: [1, 1, 1, 2, 2],
                area: [0.0; 5],
            },
            WeaponKind::Axes => Table {
                cooldown: [0.5; 5],
                damage: [10.0, 12.0, 14.0, 16.0, 20.0],
                count: [1, 2, 3, 4, 5],
                extra: [0; 5],
                area: [95.0, 100.0, 105.0, 110.0, 120.0],
            },
            WeaponKind::Aura => Table {
                cooldown: [0.4; 5],
                damage: [4.0, 5.0, 6.0, 7.0, 9.0],
                count: [0; 5],
                extra: [0; 5],
                area: [70.0, 80.0, 90.0, 105.0, 120.0],
            },
            WeaponKind::Lightning => Table {
                cooldown: [2.4, 2.1, 1.8, 1.5, 1.2],
                damage: [22.0, 26.0, 30.0, 36.0, 44.0],
                count: [1, 2, 2, 3, 4],
                extra: [0, 1, 2, 2, 3],
                area: [520.0; 5],
            },
            WeaponKind::Hammer => Table {
                cooldown: [3.2, 3.0, 2.7, 2.4, 2.0],
                damage: [26.0, 32.0, 40.0, 48.0, 60.0],
                count: [0; 5],
                extra: [0; 5],
                area: [150.0, 170.0, 190.0, 220.0, 260.0],
            },
        }
    }

    /// What the next level adds, for level-up cards.
    pub fn upgrade_text(self, next_level: u32) -> String {
        if next_level <= 1 {
            return self.def().blurb.to_string();
        }
        let (now, prev) = (self.table(), (next_level - 2) as usize);
        let next = prev + 1;
        let mut parts = Vec::new();
        if now.count[next] > now.count[prev] {
            parts.push(format!("+{} {}", now.count[next] - now.count[prev], if self == WeaponKind::Lightning { "strike" } else { "projectile" }));
        }
        if now.damage[next] > now.damage[prev] {
            parts.push(format!("+{:.0} damage", now.damage[next] - now.damage[prev]));
        }
        if now.extra[next] > now.extra[prev] {
            parts.push(if self == WeaponKind::Lightning { "+chain".to_string() } else { "+pierce".to_string() });
        }
        if now.area[next] > now.area[prev] {
            parts.push("+area".to_string());
        }
        if now.cooldown[next] < now.cooldown[prev] {
            parts.push("faster".to_string());
        }
        if parts.is_empty() { "stronger".to_string() } else { parts.join(", ") }
    }
}

#[derive(Component)]
pub struct Projectile {
    damage: f32,
    velocity: Vec2,
    life: f32,
    pierce: u32,
    homing: f32,
    radius: f32,
    knockback: f32,
    already_hit: Vec<Entity>,
    /// Sprite art points "up"; rotate to face travel.
    orient: bool,
}

#[derive(Component)]
struct Orbiter {
    index: u32,
    damage: f32,
    radius: f32,
    level: u32,
    cooldowns: HashMap<Entity, f32>,
}

#[derive(Component)]
struct AuraFx {
    level: u32,
}

#[derive(Default)]
struct Timers(HashMap<WeaponKind, f32>);

/// Where aimed weapons should shoot: the held cursor, else the nearest foe, else facing.
fn aim_direction(pos: Vec2, intent: &Intent, grid: &EnemyGrid) -> Vec2 {
    const AUTO_AIM_RANGE: f32 = 700.0;
    intent
        .aim
        .map(|p| p - pos)
        .or_else(|| grid.nearest(pos, AUTO_AIM_RANGE).map(|(_, e)| e - pos))
        .unwrap_or(intent.facing)
        .normalize_or(Vec2::X)
}

#[allow(clippy::too_many_arguments)]
fn fire_weapons(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<GameAssets>,
    grid: Res<EnemyGrid>,
    meshes: Res<FxMeshes>,
    mut glow: ResMut<Assets<GlowMaterial>>,
    mut rings: ResMut<Assets<RingMaterial>>,
    mut bolts: ResMut<Assets<BoltMaterial>>,
    mut rng: ResMut<Rng>,
    mut timers: Local<Timers>,
    mut damage: MessageWriter<DamageEnemy>,
    mut crate_hits: MessageWriter<CrateHit>,
    mut sfx: MessageWriter<PlaySfx>,
    obstacles: Res<Obstacles>,
    player: Single<(&Transform, &Intent, &Loadout, &Stats), With<Player>>,
) {
    let dt = time.delta_secs();
    let (transform, intent, loadout, stats) = *player;
    let pos = transform.translation.truncate();

    for &(kind, level) in &loadout.weapons {
        let t = kind.table();
        let i = (level.clamp(1, MAX_LEVEL) - 1) as usize;
        let timer = timers.0.entry(kind).or_insert(0.0);
        *timer -= dt;
        if *timer > 0.0 || matches!(kind, WeaponKind::Axes | WeaponKind::Aura) {
            continue;
        }
        *timer = t.cooldown[i] * stats.cooldown;
        let dmg = t.damage[i] * stats.might;
        let count = t.count[i] + stats.amount;

        match kind {
            WeaponKind::Knives | WeaponKind::Bow => {
                let is_bow = kind == WeaponKind::Bow;
                let dir = aim_direction(pos, intent, &grid);
                let spread = if is_bow { 0.16 } else { 0.1 };
                let (image, speed, knockback) =
                    if is_bow { ("weapon_arrow", 900.0, 70.0) } else { ("weapon_knife", 760.0, 35.0) };
                for n in 0..count {
                    let offset = (n as f32 - (count as f32 - 1.0) / 2.0) * spread;
                    let v = Vec2::from_angle(offset).rotate(dir) * speed;
                    commands.spawn((
                        Gameplay,
                        Projectile {
                            damage: dmg,
                            velocity: v,
                            life: 0.9,
                            pierce: t.extra[i],
                            homing: 0.0,
                            radius: 10.0,
                            knockback,
                            already_hit: Vec::new(),
                            orient: true,
                        },
                        Sprite::from_image(assets.img(image)),
                        Transform::from_translation(pos.extend(Z_PROJECTILE))
                            .with_rotation(Quat::from_rotation_z(v.y.atan2(v.x) - FRAC_PI_2))
                            .with_scale(Vec3::splat(PIXEL_SCALE)),
                    ));
                }
                sfx.write(PlaySfx(if is_bow { Sfx::Bow } else { Sfx::Swing }));
            }
            WeaponKind::MagicBolt => {
                for n in 0..count {
                    let v = Vec2::from_angle(n as f32 * TAU / count as f32 + rng.range(0.0, 0.6)) * 420.0;
                    let (mesh, material, glow_transform) =
                        glow_bundle(&meshes, &mut glow, Color::srgb(1.0, 0.35, 0.8), 34.0, 14.0);
                    commands.spawn((
                        Gameplay,
                        Projectile {
                            damage: dmg,
                            velocity: v,
                            life: 2.2,
                            pierce: t.extra[i],
                            homing: 7.0,
                            radius: 14.0,
                            knockback: 50.0,
                            already_hit: Vec::new(),
                            orient: false,
                        },
                        mesh,
                        material,
                        glow_transform.with_translation(pos.extend(Z_PROJECTILE)),
                    ));
                }
                sfx.write(PlaySfx(Sfx::Magic));
            }
            WeaponKind::Lightning => {
                let range = t.area[i];
                let candidates: Vec<(Entity, Vec2)> = grid.within(pos, range).collect();
                if candidates.is_empty() {
                    *timer = 0.2;
                    continue;
                }
                for _ in 0..count {
                    let (first, mut at) = candidates[rng.index(candidates.len())];
                    let sky = at + Vec2::new(rng.range(-60.0, 60.0), 420.0);
                    spawn_bolt(&mut commands, &meshes, &mut bolts, &mut rng, sky, at, LIGHTNING_COLOR, 70.0);
                    spawn_flash(&mut commands, &meshes, &mut glow, at, LIGHTNING_COLOR, 110.0);
                    damage.write(DamageEnemy { entity: first, amount: dmg, knockback: Vec2::ZERO });
                    let mut struck = vec![first];
                    for _ in 0..t.extra[i] {
                        const CHAIN_RANGE: f32 = 170.0;
                        let Some((next, next_at)) = grid
                            .within(at, CHAIN_RANGE)
                            .filter(|(e, _)| !struck.contains(e))
                            .min_by(|a, b| a.1.distance_squared(at).total_cmp(&b.1.distance_squared(at)))
                        else {
                            break;
                        };
                        spawn_bolt(&mut commands, &meshes, &mut bolts, &mut rng, at, next_at, LIGHTNING_COLOR, 46.0);
                        spawn_flash(&mut commands, &meshes, &mut glow, next_at, LIGHTNING_COLOR, 70.0);
                        damage.write(DamageEnemy { entity: next, amount: dmg * 0.7, knockback: Vec2::ZERO });
                        struck.push(next);
                        at = next_at;
                    }
                }
                sfx.write(PlaySfx(Sfx::Zap));
            }
            WeaponKind::Hammer => {
                let radius = t.area[i] * stats.area;
                spawn_ring_burst(&mut commands, &meshes, &mut rings, pos, Color::srgb(1.0, 0.7, 0.3), radius, 0.45);
                for (entity, at) in grid.within(pos, radius) {
                    let push = (at - pos).normalize_or(Vec2::X) * 260.0;
                    damage.write(DamageEnemy { entity, amount: dmg, knockback: push });
                }
                for entity in obstacles.0.iter().filter(|o| o.center.distance(pos) < radius).filter_map(|o| o.entity) {
                    crate_hits.write(CrateHit { entity, damage: dmg });
                }
                sfx.write(PlaySfx(Sfx::Slam));
            }
            WeaponKind::Axes | WeaponKind::Aura => {}
        }
    }
}

fn move_projectiles(
    mut commands: Commands,
    time: Res<Time>,
    grid: Res<EnemyGrid>,
    obstacles: Res<Obstacles>,
    mut damage: MessageWriter<DamageEnemy>,
    mut crate_hits: MessageWriter<CrateHit>,
    mut projectiles: Query<(Entity, &mut Projectile, &mut Transform)>,
) {
    const HOMING_RANGE: f32 = 420.0;
    const ENEMY_HIT_RADIUS: f32 = 22.0;
    let dt = time.delta_secs();
    for (entity, mut p, mut transform) in &mut projectiles {
        p.life -= dt;
        let pos = transform.translation.truncate();
        if p.homing > 0.0
            && let Some((_, target)) = grid.nearest(pos, HOMING_RANGE)
        {
            let speed = p.velocity.length();
            let desired = (target - pos).normalize_or_zero() * speed;
            p.velocity = p.velocity.lerp(desired, (p.homing * dt).min(1.0)).normalize_or_zero() * speed;
        }
        let next = pos + p.velocity * dt;
        let blocked = obstacles.hit(next);
        if let Some(entity) = blocked.and_then(|o| o.entity) {
            crate_hits.write(CrateHit { entity, damage: p.damage });
        }
        if p.life <= 0.0 || blocked.is_some() || Obstacles::out_of_bounds(next) {
            commands.entity(entity).try_despawn();
            continue;
        }
        transform.translation = next.extend(Z_PROJECTILE);
        if p.orient {
            transform.rotation = Quat::from_rotation_z(p.velocity.y.atan2(p.velocity.x) - FRAC_PI_2);
        }

        let hits: Vec<Entity> = grid
            .within(next, p.radius + ENEMY_HIT_RADIUS)
            .map(|(e, _)| e)
            .filter(|e| !p.already_hit.contains(e))
            .collect();
        for enemy in hits {
            let knockback = p.velocity.normalize_or_zero() * p.knockback;
            damage.write(DamageEnemy { entity: enemy, amount: p.damage, knockback });
            p.already_hit.push(enemy);
            if p.pierce == 0 {
                commands.entity(entity).try_despawn();
                break;
            }
            p.pierce -= 1;
        }
    }
}

fn sync_orbiters(
    mut commands: Commands,
    assets: Res<GameAssets>,
    player: Single<(&Loadout, &Stats), With<Player>>,
    orbiters: Query<(Entity, &Orbiter)>,
) {
    let (loadout, stats) = *player;
    let level = loadout.weapon_level(WeaponKind::Axes);
    let wanted = if level == 0 { 0 } else { WeaponKind::Axes.table().count[(level - 1) as usize] + stats.amount };
    let current = orbiters.iter().count() as u32;
    let stale = orbiters.iter().any(|(_, o)| o.level != level);
    if current == wanted && !stale {
        return;
    }
    for (entity, _) in &orbiters {
        commands.entity(entity).try_despawn();
    }
    if level == 0 {
        return;
    }
    let t = WeaponKind::Axes.table();
    let i = (level - 1) as usize;
    for index in 0..wanted {
        commands.spawn((
            Gameplay,
            Orbiter { index, damage: t.damage[i], radius: t.area[i], level, cooldowns: HashMap::new() },
            Sprite::from_image(assets.img("weapon_throwing_axe")),
            Transform::from_xyz(0.0, 0.0, Z_PROJECTILE).with_scale(Vec3::splat(PIXEL_SCALE)),
        ));
    }
}

fn spin_orbiters(
    time: Res<Time>,
    grid: Res<EnemyGrid>,
    obstacles: Res<Obstacles>,
    mut damage: MessageWriter<DamageEnemy>,
    mut crate_hits: MessageWriter<CrateHit>,
    player: Single<(&Transform, &Stats), (With<Player>, Without<Orbiter>)>,
    mut orbiters: Query<(&mut Orbiter, &mut Transform)>,
) {
    const ANGULAR_SPEED: f32 = 3.4;
    const SELF_SPIN: f32 = 14.0;
    const HIT_RADIUS: f32 = 30.0;
    const HIT_COOLDOWN: f32 = 0.5;
    let dt = time.delta_secs();
    let (player_transform, stats) = *player;
    let center = player_transform.translation.truncate();
    let total = orbiters.iter().count().max(1) as f32;
    let now = time.elapsed_secs();
    for (mut orbiter, mut transform) in &mut orbiters {
        let angle = now * ANGULAR_SPEED + orbiter.index as f32 * TAU / total;
        let pos = center + Vec2::from_angle(angle) * orbiter.radius * stats.area;
        transform.translation = pos.extend(Z_PROJECTILE);
        transform.rotation = Quat::from_rotation_z(now * SELF_SPIN);

        orbiter.cooldowns.retain(|_, cd| {
            *cd -= dt;
            *cd > 0.0
        });
        let dmg = orbiter.damage * stats.might;
        for (enemy, at) in grid.within(pos, HIT_RADIUS) {
            if orbiter.cooldowns.contains_key(&enemy) {
                continue;
            }
            orbiter.cooldowns.insert(enemy, HIT_COOLDOWN);
            damage.write(DamageEnemy { entity: enemy, amount: dmg, knockback: (at - center).normalize_or_zero() * 90.0 });
        }
        if let Some(entity) = obstacles.hit(pos).and_then(|o| o.entity) {
            crate_hits.write(CrateHit { entity, damage: dmg * dt * 4.0 });
        }
    }
}

fn sync_aura(
    mut commands: Commands,
    meshes: Res<FxMeshes>,
    mut materials: ResMut<Assets<RingMaterial>>,
    player: Single<(&Transform, &Loadout, &Stats), With<Player>>,
    mut auras: Query<(&mut AuraFx, &mut Transform), Without<Player>>,
) {
    let (player_transform, loadout, stats) = *player;
    let level = loadout.weapon_level(WeaponKind::Aura);
    if level == 0 {
        return;
    }
    let radius = WeaponKind::Aura.table().area[(level - 1) as usize] * stats.area;
    let pos = player_transform.translation.truncate().extend(Z_GROUND_FX);
    if let Some((mut aura, mut transform)) = auras.iter_mut().next() {
        aura.level = level;
        transform.translation = pos;
        transform.scale = Vec3::splat(radius * 2.0);
        return;
    }
    commands.spawn((
        Gameplay,
        AuraFx { level },
        Mesh2d(meshes.quad.clone()),
        MeshMaterial2d(materials.add(ring_material(Color::srgba(0.45, 1.0, 0.45, 0.8), 0.86, 3.0, 0.18))),
        Transform::from_translation(pos).with_scale(Vec3::splat(radius * 2.0)),
    ));
}

fn aura_damage(
    time: Res<Time>,
    grid: Res<EnemyGrid>,
    mut clock: Local<f32>,
    mut damage: MessageWriter<DamageEnemy>,
    player: Single<(&Transform, &Loadout, &Stats), With<Player>>,
) {
    const KNOCKBACK: f32 = 40.0;
    let (transform, loadout, stats) = *player;
    let level = loadout.weapon_level(WeaponKind::Aura);
    if level == 0 {
        return;
    }
    let t = WeaponKind::Aura.table();
    let i = (level - 1) as usize;
    *clock -= time.delta_secs();
    if *clock > 0.0 {
        return;
    }
    *clock = t.cooldown[i] * stats.cooldown;
    let center = transform.translation.truncate();
    for (entity, at) in grid.within(center, t.area[i] * stats.area) {
        damage.write(DamageEnemy {
            entity,
            amount: t.damage[i] * stats.might,
            knockback: (at - center).normalize_or_zero() * KNOCKBACK,
        });
    }
}
