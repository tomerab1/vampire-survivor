//! Drops and pickups: XP gems, coins, flasks (heal / magnet / bomb), treasure
//! chests from elites and bosses, and weapons lying on the floor.

use bevy::prelude::*;

use crate::anim::Animated;
use crate::assets::{GameAssets, PlaySfx, Sfx};
use crate::config::*;
use crate::effects::{FloatText, FxMeshes, GlowMaterial, RingMaterial, ScreenFx, ring_material, spawn_float_text, spawn_particles, spawn_ring_burst};
use crate::enemies::{DamageEnemy, EnemyKilled, Role};
use crate::hero::{Health, Loadout, Player, Stats};
use crate::progression::{GainXp, RunStats, apply_offer, roll_offers};
use crate::rng::Rng;
use crate::weapons::WeaponKind;
use crate::world::{CrateBroken, EnemyGrid, Gameplay, Obstacles, random_point};
use crate::{AppState, GameSet};

pub struct PickupsPlugin;

impl Plugin for PickupsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, init_gem_materials)
            .add_systems(OnEnter(AppState::Playing), reset_floor_weapon_clock)
            .add_systems(Update, (drop_loot, drop_crate_loot, spawn_floor_weapons).in_set(GameSet::Resolve))
            .add_systems(Update, (attract_pickups, collect_pickups).chain().in_set(GameSet::Movement));
    }
}

/// Marks XP gems (the value lives in `Pickup(Loot::Gem(xp))`).
#[derive(Component)]
pub struct Gem;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Loot {
    Gem(u32),
    Coin,
    Heal,
    Magnet,
    Bomb,
    Chest { rewards: u32 },
    Weapon(WeaponKind),
}

#[derive(Component)]
pub struct Pickup(pub Loot);

/// Flying toward the player.
#[derive(Component)]
struct Attracted {
    speed: f32,
}

#[derive(Resource)]
struct FloorWeaponClock(f32);

/// One shared glow material per gem tier so hundreds of gems batch together.
#[derive(Resource)]
pub struct GemMaterials(Vec<Handle<GlowMaterial>>);

fn init_gem_materials(mut commands: Commands, mut glow: ResMut<Assets<GlowMaterial>>) {
    const PULSE: f32 = 6.0;
    commands.insert_resource(GemMaterials(
        GEM_TIERS
            .iter()
            .map(|(_, color)| glow.add(GlowMaterial { color: color.to_linear(), params: Vec4::new(PULSE, 2.2, 0.25, 0.0) }))
            .collect(),
    ));
}

fn reset_floor_weapon_clock(mut commands: Commands) {
    commands.insert_resource(FloorWeaponClock(FLOOR_WEAPON_INTERVAL / 2.0));
}

fn gem_tier(xp: u32) -> usize {
    GEM_TIERS.iter().rposition(|(value, _)| xp >= *value).unwrap_or(0)
}

pub struct LootSpawner<'a, 'w, 's> {
    pub commands: &'a mut Commands<'w, 's>,
    pub assets: &'a GameAssets,
    pub meshes: &'a FxMeshes,
    pub gems: &'a GemMaterials,
    pub rings: &'a mut Assets<RingMaterial>,
}

impl LootSpawner<'_, '_, '_> {
    pub fn spawn(&mut self, loot: Loot, pos: Vec2) {
        let at = pos.extend(Z_PICKUP);
        let pixel = Transform::from_translation(at).with_scale(Vec3::splat(PIXEL_SCALE));
        let mut entity = self.commands.spawn((Gameplay, Pickup(loot)));
        match loot {
            Loot::Gem(xp) => {
                let tier = gem_tier(xp);
                entity.insert((
                    Gem,
                    Mesh2d(self.meshes.rhombus.clone()),
                    MeshMaterial2d(self.gems.0[tier].clone()),
                    Transform::from_translation(at).with_scale(Vec3::splat(16.0 + 5.0 * tier as f32)),
                ));
            }
            Loot::Coin => {
                let frames = self.assets.anim("coin_anim");
                entity.insert((Sprite::from_image(frames[0].clone()), Animated::looping(frames, 10.0), pixel));
            }
            Loot::Heal | Loot::Magnet | Loot::Bomb => {
                let image = match loot {
                    Loot::Heal => "flask_red",
                    Loot::Magnet => "flask_blue",
                    _ => "flask_yellow",
                };
                entity.insert((Sprite::from_image(self.assets.img(image)), pixel));
            }
            Loot::Chest { .. } => {
                let frames = self.assets.anim("chest_full_open_anim");
                entity.insert((Sprite::from_image(frames[0].clone()), pixel));
            }
            Loot::Weapon(kind) => {
                const HALO: f32 = 80.0;
                entity.insert((
                    Mesh2d(self.meshes.quad.clone()),
                    MeshMaterial2d(self.rings.add(ring_material(Color::srgba(1.0, 0.85, 0.3, 0.9), 0.7, 5.0, 0.25))),
                    Transform::from_translation(at).with_scale(Vec3::splat(HALO)),
                    children![(
                        Sprite::from_image(self.assets.img(kind.def().icon)),
                        Transform::from_xyz(0.0, 0.0, 0.1).with_scale(Vec3::splat(PIXEL_SCALE / HALO)),
                    )],
                ));
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn drop_loot(
    mut commands: Commands,
    assets: Res<GameAssets>,
    meshes: Res<FxMeshes>,
    gems: Res<GemMaterials>,
    mut rings: ResMut<Assets<RingMaterial>>,
    mut rng: ResMut<Rng>,
    mut killed: MessageReader<EnemyKilled>,
) {
    const BOSS_CHEST_REWARDS: u32 = 3;
    let mut spawner = LootSpawner { commands: &mut commands, assets: &assets, meshes: &meshes, gems: &gems, rings: &mut rings };
    for kill in killed.read() {
        let def = kill.kind.def();
        spawner.spawn(Loot::Gem(def.xp * kill.xp_mult), kill.pos);
        let scatter = |rng: &mut Rng| kill.pos + random_point(rng, 30.0);
        if rng.chance(COIN_DROP_CHANCE) {
            spawner.spawn(Loot::Coin, scatter(&mut rng));
        }
        if rng.chance(FLASK_DROP_CHANCE) {
            let flask = [Loot::Heal, Loot::Magnet, Loot::Bomb][rng.index(3)];
            spawner.spawn(flask, scatter(&mut rng));
        }
        match def.role {
            Role::Boss => spawner.spawn(Loot::Chest { rewards: BOSS_CHEST_REWARDS }, scatter(&mut rng)),
            Role::Elite if rng.chance(ELITE_CHEST_CHANCE) => spawner.spawn(Loot::Chest { rewards: 1 }, scatter(&mut rng)),
            Role::Elite if rng.chance(ELITE_WEAPON_DROP_CHANCE) => {
                let kind = WeaponKind::ALL[rng.index(WeaponKind::ALL.len())];
                spawner.spawn(Loot::Weapon(kind), scatter(&mut rng));
            }
            _ => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn drop_crate_loot(
    mut commands: Commands,
    assets: Res<GameAssets>,
    meshes: Res<FxMeshes>,
    gems: Res<GemMaterials>,
    mut rings: ResMut<Assets<RingMaterial>>,
    mut rng: ResMut<Rng>,
    mut broken: MessageReader<CrateBroken>,
) {
    const FLASK_CHANCE: f32 = 0.45;
    let mut spawner = LootSpawner { commands: &mut commands, assets: &assets, meshes: &meshes, gems: &gems, rings: &mut rings };
    for b in broken.read() {
        spawn_particles(spawner.commands, &mut rng, b.pos, Color::srgb(0.6, 0.42, 0.25), 10, 200.0);
        let loot = if rng.chance(FLASK_CHANCE) { [Loot::Heal, Loot::Magnet, Loot::Bomb][rng.index(3)] } else { Loot::Coin };
        spawner.spawn(loot, b.pos);
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_floor_weapons(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<GameAssets>,
    meshes: Res<FxMeshes>,
    obstacles: Res<Obstacles>,
    mut clock: ResMut<FloorWeaponClock>,
    gems: Res<GemMaterials>,
    mut rings: ResMut<Assets<RingMaterial>>,
    mut rng: ResMut<Rng>,
    player: Single<&Transform, With<Player>>,
) {
    const MIN_DIST: f32 = 250.0;
    const MAX_DIST: f32 = 450.0;
    clock.0 -= time.delta_secs();
    if clock.0 > 0.0 {
        return;
    }
    clock.0 = FLOOR_WEAPON_INTERVAL;
    let limit = ARENA_HALF - 80.0;
    let pos = (player.translation.truncate() + Vec2::from_angle(rng.angle()) * rng.range(MIN_DIST, MAX_DIST))
        .clamp(Vec2::splat(-limit), Vec2::splat(limit));
    let pos = obstacles.resolve(pos, 30.0);
    let kind = WeaponKind::ALL[rng.index(WeaponKind::ALL.len())];
    LootSpawner { commands: &mut commands, assets: &assets, meshes: &meshes, gems: &gems, rings: &mut rings }
        .spawn(Loot::Weapon(kind), pos);
    info!("[telemetry] floor weapon dropped: {}", kind.def().name);
}

fn attract_pickups(
    mut commands: Commands,
    time: Res<Time>,
    player: Single<(&Transform, &Stats), With<Player>>,
    mut pickups: Query<(Entity, &Pickup, &mut Transform, Option<&mut Attracted>), Without<Player>>,
) {
    const ACCEL: f32 = 900.0;
    let dt = time.delta_secs();
    let (player_transform, stats) = *player;
    let target = player_transform.translation.truncate();
    let magnet = BASE_MAGNET * stats.magnet;
    for (entity, pickup, mut transform, attracted) in &mut pickups {
        let pos = transform.translation.truncate();
        match attracted {
            Some(mut a) => {
                a.speed += ACCEL * dt;
                let step = (target - pos).normalize_or_zero() * a.speed * dt;
                transform.translation += step.extend(0.0);
            }
            None if matches!(pickup.0, Loot::Gem(_) | Loot::Coin) && pos.distance(target) < magnet => {
                commands.entity(entity).try_insert(Attracted { speed: MAGNET_PULL_SPEED * 0.4 });
            }
            None => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn collect_pickups(
    mut commands: Commands,
    assets: Res<GameAssets>,
    meshes: Res<FxMeshes>,
    grid: Res<EnemyGrid>,
    mut rings: ResMut<Assets<RingMaterial>>,
    mut rng: ResMut<Rng>,
    mut run: ResMut<RunStats>,
    mut screen: ResMut<ScreenFx>,
    mut xp: MessageWriter<GainXp>,
    mut damage: MessageWriter<DamageEnemy>,
    mut sfx: MessageWriter<PlaySfx>,
    player: Single<(&Transform, &mut Health, &mut Loadout), With<Player>>,
    pickups: Query<(Entity, &Pickup, &Transform), Without<Player>>,
    texts: Query<(), With<FloatText>>,
) {
    const BOMB_RADIUS: f32 = 900.0;
    const COIN_GOLD: u32 = 10;
    const FULL_SLOT_GOLD: u32 = 50;
    let (player_transform, mut health, mut loadout) = player.into_inner();
    let pos = player_transform.translation.truncate();
    let toast_at = pos + Vec2::Y * 70.0;
    let live_texts = texts.iter().count();
    let mut pull_all_gems = false;

    for (entity, pickup, transform) in &pickups {
        if transform.translation.truncate().distance(pos) > PICKUP_RADIUS + PLAYER_RADIUS {
            continue;
        }
        commands.entity(entity).try_despawn();
        match pickup.0 {
            Loot::Gem(value) => {
                xp.write(GainXp(value));
                sfx.write(PlaySfx(Sfx::Gem));
            }
            Loot::Coin => {
                run.gold += COIN_GOLD;
                sfx.write(PlaySfx(Sfx::Coin));
            }
            Loot::Heal => {
                health.current = (health.current + HEAL_FLASK_AMOUNT).min(health.max);
                spawn_float_text(&mut commands, &assets, live_texts, toast_at, format!("+{HEAL_FLASK_AMOUNT:.0} HP"), Color::srgb(1.0, 0.4, 0.45), 26.0);
                sfx.write(PlaySfx(Sfx::Potion));
            }
            Loot::Magnet => {
                pull_all_gems = true;
                spawn_float_text(&mut commands, &assets, live_texts, toast_at, "MAGNET!", Color::srgb(0.5, 0.75, 1.0), 26.0);
                sfx.write(PlaySfx(Sfx::Potion));
            }
            Loot::Bomb => {
                for (enemy, at) in grid.within(pos, BOMB_RADIUS) {
                    damage.write(DamageEnemy { entity: enemy, amount: BOMB_DAMAGE, knockback: (at - pos).normalize_or_zero() * 300.0 });
                }
                spawn_ring_burst(&mut commands, &meshes, &mut rings, pos, Color::srgb(1.0, 0.9, 0.5), BOMB_RADIUS, 0.6);
                screen.levelup = 0.6;
                sfx.write(PlaySfx(Sfx::Bomb));
            }
            Loot::Chest { rewards } => {
                for (n, offer) in roll_offers(&loadout, &mut rng, rewards as usize).into_iter().enumerate() {
                    let text = apply_offer(&mut loadout, &mut run, offer);
                    spawn_float_text(&mut commands, &assets, live_texts, toast_at + Vec2::Y * 28.0 * n as f32, text, Color::srgb(1.0, 0.85, 0.3), 24.0);
                }
                spawn_ring_burst(&mut commands, &meshes, &mut rings, pos, Color::srgb(1.0, 0.85, 0.3), 220.0, 0.5);
                screen.levelup = 0.8;
                sfx.write(PlaySfx(Sfx::Chest));
            }
            Loot::Weapon(kind) => {
                let text = match loadout.grant_weapon(kind) {
                    Some(level) => format!("{} Lv{level}", kind.def().name),
                    None => {
                        run.gold += FULL_SLOT_GOLD;
                        format!("+{FULL_SLOT_GOLD} gold")
                    }
                };
                spawn_float_text(&mut commands, &assets, live_texts, toast_at, text, Color::srgb(1.0, 0.85, 0.3), 26.0);
                spawn_ring_burst(&mut commands, &meshes, &mut rings, pos, Color::srgb(1.0, 0.85, 0.3), 160.0, 0.4);
                sfx.write(PlaySfx(Sfx::Chest));
            }
        }
    }

    if pull_all_gems {
        for (entity, pickup, _) in &pickups {
            if matches!(pickup.0, Loot::Gem(_)) {
                commands.entity(entity).try_insert(Attracted { speed: MAGNET_PULL_SPEED });
            }
        }
    }
}
