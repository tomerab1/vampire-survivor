//! Enemy archetypes, the spawn timeline (unlocks, bosses, Jev reinforcements),
//! steering that executes the director's per-squad tactics, contact damage,
//! and the single damage path every weapon goes through.

use std::f32::consts::TAU;

use bevy::prelude::*;

use crate::anim::{Animated, HitFlash, Motion, YSort};
use crate::assets::{GameAssets, PlaySfx, Sfx};
use crate::config::*;
use crate::director::{HordeOrders, Tactic, sector_of};
use crate::effects::{FloatText, ScreenFx, spawn_float_text, spawn_particles};
use crate::hero::{Chilled, Health, Player, Stats};
use crate::progression::RunStats;
use crate::rng::Rng;
use crate::world::{CameraShake, EnemyGrid, Gameplay, Obstacles, random_point};
use crate::{AppState, GameSet};

pub struct EnemiesPlugin;

impl Plugin for EnemiesPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<DamageEnemy>()
            .add_message::<EnemyKilled>()
            .init_resource::<SpawnClock>()
            .add_systems(OnEnter(AppState::Playing), reset_spawns)
            .add_systems(Update, (rebuild_grid, spawn_enemies, spawn_bosses, summon_minions).chain().in_set(GameSet::Ai))
            .add_systems(Update, steer_enemies.in_set(GameSet::Movement))
            .add_systems(Update, touch_player.in_set(GameSet::Combat))
            .add_systems(Update, apply_damage.in_set(GameSet::Resolve));
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Role {
    Swarm,
    Elite,
    Summoner,
    Boss,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum EnemyKind {
    TinyZombie,
    Zombie,
    Skeleton,
    Swampy,
    IceZombie,
    Chort,
    BigZombie,
    Necromancer,
    Ogre,
    BigDemon,
}

pub struct EnemyDef {
    pub label: &'static str,
    pub anim: &'static str,
    /// Source-art height in pixels (for feet position / hit size).
    pub height: f32,
    pub hp: f32,
    pub speed: f32,
    pub damage: f32,
    pub xp: u32,
    pub unlock_minute: u32,
    pub role: Role,
    /// Shown to Jev when it picks reinforcements.
    pub pitch: &'static str,
}

impl EnemyKind {
    pub const ALL: [EnemyKind; 10] = [
        EnemyKind::TinyZombie,
        EnemyKind::Zombie,
        EnemyKind::Skeleton,
        EnemyKind::Swampy,
        EnemyKind::IceZombie,
        EnemyKind::Chort,
        EnemyKind::BigZombie,
        EnemyKind::Necromancer,
        EnemyKind::Ogre,
        EnemyKind::BigDemon,
    ];

    pub fn def(self) -> EnemyDef {
        let (label, anim, height, hp, speed, damage, xp, unlock_minute, role, pitch) = match self {
            EnemyKind::TinyZombie => ("tiny_zombie", "tiny_zombie_run_anim", 16.0, 10.0, 110.0, 5.0, 1, 0, Role::Swarm, "Fast fragile swarmers; overwhelm with numbers"),
            EnemyKind::Zombie => ("zombie", "zombie_anim", 16.0, 26.0, 72.0, 8.0, 1, 0, Role::Swarm, "Standard shambling zombies"),
            EnemyKind::Skeleton => ("skeleton", "skelet_run_anim", 16.0, 22.0, 95.0, 7.0, 2, 1, Role::Swarm, "Quick skeletons that keep pace with a kiting survivor"),
            EnemyKind::Swampy => ("swampy", "swampy_anim", 16.0, 60.0, 55.0, 10.0, 3, 2, Role::Swarm, "Slow, tanky swamp things that soak damage"),
            EnemyKind::IceZombie => ("ice_zombie", "ice_zombie_anim", 16.0, 45.0, 82.0, 9.0, 3, 3, Role::Swarm, "Ice zombies whose touch chills the survivor"),
            EnemyKind::Chort => ("chort", "chort_run_anim", 23.0, 34.0, 150.0, 9.0, 3, 4, Role::Swarm, "Very fast demons that punish standing still"),
            EnemyKind::BigZombie => ("big_zombie", "big_zombie_run_anim", 36.0, 260.0, 62.0, 18.0, 12, 3, Role::Elite, "Hulking elite zombie; may drop a treasure chest"),
            EnemyKind::Necromancer => ("necromancer", "necromancer_anim", 23.0, 110.0, 60.0, 6.0, 10, 5, Role::Summoner, "Keeps its distance and raises tiny zombies"),
            EnemyKind::Ogre => ("ogre", "ogre_run_anim", 36.0, 420.0, 58.0, 22.0, 20, 7, Role::Elite, "Massive ogre elite that shrugs off knockback"),
            EnemyKind::BigDemon => ("big_demon", "big_demon_run_anim", 36.0, 3000.0, 78.0, 28.0, 120, 99, Role::Boss, "Boss"),
        };
        EnemyDef { label, anim, height, hp, speed, damage, xp, unlock_minute, role, pitch }
    }

    pub fn from_label(label: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.def().label == label)
    }

    pub fn unlocked(minute: u32) -> impl Iterator<Item = EnemyKind> {
        Self::ALL.into_iter().filter(move |k| k.def().role != Role::Boss && k.def().unlock_minute <= minute)
    }
}

#[derive(Component)]
pub struct Enemy {
    pub kind: EnemyKind,
    pub hp: f32,
    pub max_hp: f32,
    pub scale: f32,
    heading: Vec2,
    knockback: Vec2,
    touch_cooldown: f32,
    summon_cooldown: f32,
    /// Stable per-enemy value in [0, 1) that spreads a squad around the ring when surrounding.
    slot: f32,
    dead: bool,
}

#[derive(Message)]
pub struct DamageEnemy {
    pub entity: Entity,
    pub amount: f32,
    pub knockback: Vec2,
}

#[derive(Message)]
pub struct EnemyKilled {
    pub pos: Vec2,
    pub kind: EnemyKind,
}

#[derive(Resource, Default)]
struct SpawnClock {
    budget: f32,
    bosses_spawned: Vec<u32>,
}

fn reset_spawns(mut clock: ResMut<SpawnClock>) {
    *clock = SpawnClock::default();
}

fn rebuild_grid(mut grid: ResMut<EnemyGrid>, enemies: Query<(Entity, &Transform, &Enemy)>) {
    grid.rebuild(enemies.iter().filter(|(_, _, e)| !e.dead).map(|(entity, t, _)| (entity, t.translation.truncate())));
}

pub fn spawn_enemy(
    commands: &mut Commands,
    assets: &GameAssets,
    rng: &mut Rng,
    kind: EnemyKind,
    pos: Vec2,
    minute: u32,
    boss_scale: f32,
) {
    let def = kind.def();
    let hp = def.hp * (1.0 + HP_GROWTH_PER_MINUTE * minute as f32) * boss_scale;
    let scale = if def.role == Role::Boss { 1.5 } else { 1.0 };
    let frames = assets.anim(def.anim);
    commands.spawn((
        Gameplay,
        Enemy {
            kind,
            hp,
            max_hp: hp,
            scale,
            heading: Vec2::from_angle(rng.angle()),
            knockback: Vec2::ZERO,
            touch_cooldown: 0.0,
            summon_cooldown: 3.0,
            slot: rng.f32(),
            dead: false,
        },
        Motion::default(),
        YSort { feet: -def.height / 2.0 * PIXEL_SCALE * scale },
        Sprite::from_image(frames[0].clone()),
        Animated::looping(frames, 7.0 + rng.range(0.0, 2.0)),
        Transform::from_translation(pos.extend(Z_ACTORS)).with_scale(Vec3::splat(PIXEL_SCALE * scale)),
    ));
}

fn spawn_point(rng: &mut Rng, obstacles: &Obstacles, around: Vec2) -> Vec2 {
    const ATTEMPTS: usize = 8;
    let limit = ARENA_HALF - 40.0;
    for _ in 0..ATTEMPTS {
        let p = (around + Vec2::from_angle(rng.angle()) * rng.range(SPAWN_MIN_DIST, SPAWN_MAX_DIST))
            .clamp(Vec2::splat(-limit), Vec2::splat(limit));
        if obstacles.hit(p).is_none() && p.distance(around) > SPAWN_MIN_DIST * 0.6 {
            return p;
        }
    }
    around + Vec2::from_angle(rng.angle()) * SPAWN_MIN_DIST
}

#[allow(clippy::too_many_arguments)]
fn spawn_enemies(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<GameAssets>,
    orders: Res<HordeOrders>,
    obstacles: Res<Obstacles>,
    run: Res<RunStats>,
    mut rng: ResMut<Rng>,
    mut clock: ResMut<SpawnClock>,
    enemies: Query<(), With<Enemy>>,
    player: Single<&Transform, With<Player>>,
) {
    let minute = run.minute();
    let rate = (BASE_SPAWN_RATE + SPAWN_RATE_PER_MINUTE * minute as f32) * (1.0 + orders.pressure as f32 * PRESSURE_SPAWN_BOOST);
    clock.budget += rate * time.delta_secs();
    let alive = enemies.iter().count();
    let center = player.translation.truncate();
    let unlocked: Vec<EnemyKind> = EnemyKind::unlocked(minute).collect();
    let mut spawned = 0;
    while clock.budget >= 1.0 {
        clock.budget -= 1.0;
        if alive + spawned >= MAX_ENEMIES {
            continue;
        }
        let kind = match orders.reinforcement {
            Some(k) if unlocked.contains(&k) && rng.chance(REINFORCEMENT_SHARE) => k,
            _ => pick_weighted(&mut rng, &unlocked),
        };
        let pos = spawn_point(&mut rng, &obstacles, center);
        spawn_enemy(&mut commands, &assets, &mut rng, kind, pos, minute, 1.0);
        spawned += 1;
    }
}

/// Swarmers are common, elites and summoners rare.
fn pick_weighted(rng: &mut Rng, unlocked: &[EnemyKind]) -> EnemyKind {
    let weight = |k: &EnemyKind| match k.def().role {
        Role::Swarm => 10.0,
        Role::Summoner => 1.2,
        Role::Elite => 0.8,
        Role::Boss => 0.0,
    };
    let total: f32 = unlocked.iter().map(weight).sum();
    let mut roll = rng.range(0.0, total);
    for k in unlocked {
        roll -= weight(k);
        if roll <= 0.0 {
            return *k;
        }
    }
    EnemyKind::Zombie
}

#[allow(clippy::too_many_arguments)]
fn spawn_bosses(
    mut commands: Commands,
    assets: Res<GameAssets>,
    obstacles: Res<Obstacles>,
    run: Res<RunStats>,
    mut rng: ResMut<Rng>,
    mut clock: ResMut<SpawnClock>,
    mut sfx: MessageWriter<PlaySfx>,
    mut shake: ResMut<CameraShake>,
    player: Single<&Transform, With<Player>>,
) {
    let minute = run.minute();
    for (n, boss_minute) in BOSS_MINUTES.iter().enumerate() {
        if minute < *boss_minute || clock.bosses_spawned.contains(boss_minute) {
            continue;
        }
        clock.bosses_spawned.push(*boss_minute);
        let pos = spawn_point(&mut rng, &obstacles, player.translation.truncate());
        spawn_enemy(&mut commands, &assets, &mut rng, EnemyKind::BigDemon, pos, 0, 1.0 + n as f32 * 1.4);
        for _ in 0..n * 2 {
            let escort = spawn_point(&mut rng, &obstacles, player.translation.truncate());
            spawn_enemy(&mut commands, &assets, &mut rng, EnemyKind::Ogre, escort, minute, 1.0);
        }
        sfx.write(PlaySfx(Sfx::Boss));
        shake.0 = 14.0;
        info!("[telemetry] boss spawned at minute {minute}");
    }
}

fn summon_minions(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<GameAssets>,
    run: Res<RunStats>,
    mut rng: ResMut<Rng>,
    mut summoners: Query<(&mut Enemy, &Transform)>,
    count: Query<(), With<Enemy>>,
) {
    const SUMMON_EVERY: f32 = 5.0;
    const MINIONS: usize = 3;
    let alive = count.iter().count();
    let mut raised = Vec::new();
    for (mut enemy, transform) in &mut summoners {
        if enemy.kind.def().role != Role::Summoner || enemy.dead {
            continue;
        }
        enemy.summon_cooldown -= time.delta_secs();
        if enemy.summon_cooldown > 0.0 || alive + raised.len() >= MAX_ENEMIES {
            continue;
        }
        enemy.summon_cooldown = SUMMON_EVERY;
        let at = transform.translation.truncate();
        raised.extend((0..MINIONS).map(|_| at + Vec2::from_angle(rng.angle()) * 40.0));
    }
    for pos in raised {
        spawn_particles(&mut commands, &mut rng, pos, Color::srgb(0.6, 0.3, 0.9), 6, 120.0);
        spawn_enemy(&mut commands, &assets, &mut rng, EnemyKind::TinyZombie, pos, run.minute(), 1.0);
    }
}

/// Where an enemy wants to go under its squad's tactic, before separation and avoidance.
fn tactic_direction(tactic: Tactic, enemy: &Enemy, pos: Vec2, player: Vec2, player_vel: Vec2, squad_center: Vec2) -> Vec2 {
    const ENGAGE_DIST: f32 = 150.0;
    const FLANK_ANGLE: f32 = 0.95;
    const FLANK_FADE: f32 = 320.0;
    const RING_RADIUS: f32 = 170.0;
    const MAX_LEAD_SECS: f32 = 1.4;
    const REGROUP_PULL: f32 = 0.7;
    const SUMMONER_RANGE: f32 = 280.0;

    let to_player = player - pos;
    let dist = to_player.length();
    let direct = to_player.normalize_or_zero();
    match enemy.kind.def().role {
        Role::Boss | Role::Elite => return direct,
        Role::Summoner => return if dist < SUMMONER_RANGE { -direct } else { direct * 0.3 },
        Role::Swarm => {}
    }
    if dist < ENGAGE_DIST {
        return direct;
    }
    match tactic {
        Tactic::Rush => direct,
        Tactic::FlankLeft | Tactic::FlankRight => {
            let sign = if tactic == Tactic::FlankLeft { 1.0 } else { -1.0 };
            let bend = sign * FLANK_ANGLE * ((dist - ENGAGE_DIST) / FLANK_FADE).clamp(0.0, 1.0);
            Vec2::from_angle(bend).rotate(direct)
        }
        Tactic::Surround => {
            let ring_point = player + Vec2::from_angle(enemy.slot * TAU) * RING_RADIUS;
            (ring_point - pos).normalize_or(direct)
        }
        Tactic::Intercept => {
            let lead = (dist / enemy.kind.def().speed).min(MAX_LEAD_SECS);
            (player + player_vel * lead - pos).normalize_or(direct)
        }
        Tactic::Regroup => ((squad_center - pos).normalize_or_zero() * REGROUP_PULL + direct * (1.0 - REGROUP_PULL)).normalize_or(direct),
    }
}

#[allow(clippy::too_many_arguments)]
fn steer_enemies(
    time: Res<Time>,
    orders: Res<HordeOrders>,
    obstacles: Res<Obstacles>,
    grid: Res<EnemyGrid>,
    mut rng: ResMut<Rng>,
    player: Single<(&Transform, &Motion), (With<Player>, Without<Enemy>)>,
    mut enemies: Query<(&mut Enemy, &mut Transform, &mut Motion, &mut Sprite)>,
) {
    const LOOK_AHEAD: f32 = 60.0;
    const AVOID_WEIGHT: f32 = 1.6;
    const TURN_RATE: f32 = 7.0;
    const REGROUP_SPEED: f32 = 0.55;
    const REGROUP_SLOW_DIST: f32 = 240.0;
    const ICE_TINT: Color = Color::srgb(0.7, 0.9, 1.0);
    const ENRAGE_TINT: Color = Color::srgb(1.0, 0.8, 0.78);

    let dt = time.delta_secs();
    let (player_transform, player_motion) = *player;
    let player_pos = player_transform.translation.truncate();
    let speed_mult = if orders.enraged { ENRAGE_SPEED_MULT } else { 1.0 };

    for (mut enemy, mut transform, mut motion, mut sprite) in &mut enemies {
        let pos = transform.translation.truncate();
        if pos.distance(player_pos) > DESPAWN_DIST && enemy.kind.def().role != Role::Boss {
            let ahead = player_pos + player_motion.0.normalize_or(Vec2::from_angle(rng.angle())) * SPAWN_MIN_DIST;
            transform.translation = (ahead + random_point(&mut rng, 200.0)).extend(transform.translation.z);
            continue;
        }
        let sector = sector_of(player_pos, pos);
        let tactic = orders.tactics[sector];
        let mut desired = tactic_direction(tactic, &enemy, pos, player_pos, player_motion.0, orders.centroids[sector]);

        let mut separation = Vec2::ZERO;
        for (_, other) in grid.within(pos, ENEMY_SEPARATION_RADIUS * enemy.scale) {
            let delta = pos - other;
            let d = delta.length();
            if d > 0.01 {
                separation += delta / d * (1.0 - d / (ENEMY_SEPARATION_RADIUS * enemy.scale));
            }
        }
        desired += separation * ENEMY_SEPARATION_FORCE;
        desired += obstacles.avoidance(pos, enemy.heading, LOOK_AHEAD) * AVOID_WEIGHT;

        enemy.heading = enemy.heading.lerp(desired.normalize_or(enemy.heading), (TURN_RATE * dt).min(1.0)).normalize_or(Vec2::X);
        let def = enemy.kind.def();
        let tactic_speed = if tactic == Tactic::Regroup && pos.distance(player_pos) > REGROUP_SLOW_DIST { REGROUP_SPEED } else { 1.0 };
        let knock = enemy.knockback;
        enemy.knockback -= knock * (KNOCKBACK_DECAY * dt).min(1.0);
        let velocity = enemy.heading * def.speed * speed_mult * tactic_speed + knock;
        let next = obstacles.resolve(pos + velocity * dt, ENEMY_RADIUS * enemy.scale);
        motion.0 = (next - pos) / dt.max(f32::EPSILON);
        transform.translation = next.extend(transform.translation.z);

        sprite.color = match () {
            _ if orders.enraged && def.role == Role::Swarm => ENRAGE_TINT,
            _ if enemy.kind == EnemyKind::IceZombie => ICE_TINT,
            _ => Color::WHITE,
        };
    }
}

#[allow(clippy::too_many_arguments)]
fn touch_player(
    mut commands: Commands,
    time: Res<Time>,
    grid: Res<EnemyGrid>,
    mut shake: ResMut<CameraShake>,
    mut screen: ResMut<ScreenFx>,
    mut sfx: MessageWriter<PlaySfx>,
    mut next_state: ResMut<NextState<AppState>>,
    player: Single<(Entity, &Transform, &mut Health, &Stats), With<Player>>,
    mut enemies: Query<&mut Enemy>,
) {
    const CHILL_SECS: f32 = 1.5;
    const TOUCH_RADIUS: f32 = 30.0;
    const MIN_DAMAGE: f32 = 1.0;
    const LOW_HP: f32 = 0.3;
    let dt = time.delta_secs();
    let (player_entity, transform, mut health, stats) = player.into_inner();
    health.invulnerable -= dt;
    let pos = transform.translation.truncate();

    for (entity, at) in grid.within(pos, TOUCH_RADIUS * 2.2) {
        let Ok(mut enemy) = enemies.get_mut(entity) else {
            continue;
        };
        enemy.touch_cooldown -= dt;
        let in_reach = at.distance(pos) <= TOUCH_RADIUS * enemy.scale;
        if enemy.dead || !in_reach || enemy.touch_cooldown > 0.0 || health.invulnerable > 0.0 {
            continue;
        }
        enemy.touch_cooldown = ENEMY_TOUCH_COOLDOWN;
        health.current -= (enemy.kind.def().damage - stats.armor).max(MIN_DAMAGE);
        health.invulnerable = PLAYER_IFRAMES;
        shake.0 = SHAKE_ON_HURT;
        screen.hurt = 0.8;
        sfx.write(PlaySfx(Sfx::Hurt));
        if enemy.kind == EnemyKind::IceZombie {
            commands.entity(player_entity).try_insert(Chilled(CHILL_SECS));
        }
    }
    screen.low_hp = (1.0 - health.current / health.max / LOW_HP).clamp(0.0, 1.0);
    if health.current <= 0.0 {
        health.current = 0.0;
        next_state.set(AppState::GameOver);
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_damage(
    mut commands: Commands,
    assets: Res<GameAssets>,
    mut rng: ResMut<Rng>,
    mut hits: MessageReader<DamageEnemy>,
    mut killed: MessageWriter<EnemyKilled>,
    mut sfx: MessageWriter<PlaySfx>,
    mut enemies: Query<(&mut Enemy, &Transform)>,
    texts: Query<(), With<FloatText>>,
) {
    let mut live_texts = texts.iter().count();
    for hit in hits.read() {
        let Ok((mut enemy, transform)) = enemies.get_mut(hit.entity) else {
            continue;
        };
        if enemy.dead {
            continue;
        }
        let def = enemy.kind.def();
        let resist = match def.role {
            Role::Boss => 0.1,
            Role::Elite => 0.35,
            _ => 1.0,
        };
        enemy.hp -= hit.amount;
        enemy.knockback += hit.knockback * resist;
        let pos = transform.translation.truncate();
        commands.entity(hit.entity).try_insert(HitFlash(HIT_FLASH_SECS));
        spawn_float_text(&mut commands, &assets, live_texts, pos + Vec2::new(rng.range(-10.0, 10.0), 30.0), format!("{:.0}", hit.amount), Color::WHITE, 20.0);
        live_texts += 1;
        sfx.write(PlaySfx(Sfx::Hit));

        if enemy.hp <= 0.0 {
            enemy.dead = true;
            let gore = if def.role == Role::Boss { 40 } else { 8 };
            spawn_particles(&mut commands, &mut rng, pos, Color::srgb(0.55, 0.85, 0.35), gore, 220.0);
            killed.write(EnemyKilled { pos, kind: enemy.kind });
            sfx.write(PlaySfx(Sfx::Kill));
            commands.entity(hit.entity).try_despawn();
        }
    }
}
