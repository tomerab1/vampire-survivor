//! Enemy archetypes and per-stage spawning (Jev picks reinforcements), steering
//! that executes the director's per-squad tactics, boss attack patterns (Jev
//! picks those too), enemy projectiles, burning, possessed allies, contact
//! damage, and the single damage path every weapon goes through.

use std::f32::consts::TAU;

use bevy::prelude::*;

use crate::anim::{Animated, HitFlash, Motion, YSort};
use crate::assets::{GameAssets, PlaySfx, Sfx};
use crate::config::*;
use crate::director::{HordeOrders, Tactic, sector_of};
use crate::effects::{
    FloatText, FxMaterials, FxMeshes, GlowMaterial, RingMaterial, ScreenFx, glow_bundle, spawn_float_text, spawn_particles,
    spawn_ring_burst,
};
use crate::hero::{Chilled, Health, Player, Stats};
use crate::rng::Rng;
use crate::stage::Stage;
use crate::world::{CameraShake, EnemyGrid, Gameplay, Obstacles, random_point};
use crate::{AppState, GameSet};

pub struct EnemiesPlugin;

impl Plugin for EnemiesPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<DamageEnemy>()
            .add_message::<Ignite>()
            .add_message::<EnemyKilled>()
            .init_resource::<SpawnClock>()
            .add_systems(OnEnter(AppState::Playing), reset_spawns)
            .add_systems(Update, (rebuild_grid, spawn_enemies, summon_minions).chain().in_set(GameSet::Ai))
            .add_systems(Update, (steer_enemies, steer_allies, boss_attacks, move_enemy_shots).in_set(GameSet::Movement))
            .add_systems(Update, (touch_player, ally_attacks, burn_enemies, expire_possession).in_set(GameSet::Combat))
            .add_systems(Update, (apply_ignites, apply_damage).chain().in_set(GameSet::Resolve));
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
    Goblin,
    Muddy,
    Imp,
    Wogol,
    MaskedOrc,
    OrcWarrior,
    OrcShaman,
    BigDemon,
    FrostKing,
    DemonLord,
}

pub struct EnemyDef {
    pub label: &'static str,
    /// Display name for boss banners.
    pub title: &'static str,
    pub anim: &'static str,
    /// Source-art height in pixels (for feet position / hit size).
    pub height: f32,
    pub hp: f32,
    pub speed: f32,
    pub damage: f32,
    pub xp: u32,
    pub role: Role,
    pub tint: Color,
    /// What a summoner raises.
    pub minion: Option<EnemyKind>,
    /// Shown to Jev when it picks reinforcements.
    pub pitch: &'static str,
}

const NO_TINT: Color = Color::WHITE;

impl EnemyKind {
    pub const ALL: [EnemyKind; 19] = [
        EnemyKind::TinyZombie,
        EnemyKind::Zombie,
        EnemyKind::Skeleton,
        EnemyKind::Swampy,
        EnemyKind::IceZombie,
        EnemyKind::Chort,
        EnemyKind::BigZombie,
        EnemyKind::Necromancer,
        EnemyKind::Ogre,
        EnemyKind::Goblin,
        EnemyKind::Muddy,
        EnemyKind::Imp,
        EnemyKind::Wogol,
        EnemyKind::MaskedOrc,
        EnemyKind::OrcWarrior,
        EnemyKind::OrcShaman,
        EnemyKind::BigDemon,
        EnemyKind::FrostKing,
        EnemyKind::DemonLord,
    ];

    pub fn def(self) -> EnemyDef {
        use EnemyKind::*;
        let d = |label, title, anim, height, hp, speed, damage, xp, role, tint, minion, pitch| EnemyDef {
            label,
            title,
            anim,
            height,
            hp,
            speed,
            damage,
            xp,
            role,
            tint,
            minion,
            pitch,
        };
        match self {
            TinyZombie => d("tiny_zombie", "", "tiny_zombie_run_anim", 16.0, 10.0, 110.0, 5.0, 1, Role::Swarm, NO_TINT, None, "Fast fragile swarmers; overwhelm with numbers"),
            Zombie => d("zombie", "", "zombie_anim", 16.0, 26.0, 72.0, 8.0, 1, Role::Swarm, NO_TINT, None, "Standard shambling zombies"),
            Skeleton => d("skeleton", "", "skelet_run_anim", 16.0, 22.0, 95.0, 7.0, 2, Role::Swarm, NO_TINT, None, "Quick skeletons that keep pace with a kiting survivor"),
            Swampy => d("swampy", "", "swampy_anim", 16.0, 60.0, 55.0, 10.0, 3, Role::Swarm, NO_TINT, None, "Slow, tanky swamp things that soak damage"),
            IceZombie => d("ice_zombie", "", "ice_zombie_anim", 16.0, 45.0, 82.0, 9.0, 3, Role::Swarm, Color::srgb(0.7, 0.9, 1.0), None, "Ice zombies whose touch chills the survivor"),
            Chort => d("chort", "", "chort_run_anim", 23.0, 34.0, 150.0, 9.0, 3, Role::Swarm, NO_TINT, None, "Very fast demons that punish standing still"),
            BigZombie => d("big_zombie", "", "big_zombie_run_anim", 36.0, 260.0, 62.0, 18.0, 12, Role::Elite, NO_TINT, None, "Hulking elite zombie; may drop a treasure chest"),
            Necromancer => d("necromancer", "", "necromancer_anim", 23.0, 110.0, 60.0, 6.0, 10, Role::Summoner, NO_TINT, Some(TinyZombie), "Keeps its distance and raises tiny zombies"),
            Ogre => d("ogre", "", "ogre_run_anim", 36.0, 420.0, 58.0, 22.0, 20, Role::Elite, Color::srgb(0.8, 0.9, 1.0), None, "Massive ogre elite that shrugs off knockback"),
            Goblin => d("goblin", "", "goblin_run_anim", 16.0, 20.0, 135.0, 7.0, 2, Role::Swarm, NO_TINT, None, "Twitchy goblins that sprint in packs"),
            Muddy => d("muddy", "", "muddy_anim", 16.0, 90.0, 50.0, 12.0, 4, Role::Swarm, NO_TINT, None, "Heavy mud golems that absorb punishment"),
            Imp => d("imp", "", "imp_run_anim", 16.0, 18.0, 160.0, 8.0, 2, Role::Swarm, NO_TINT, None, "Blistering fast imps that swarm from every side"),
            Wogol => d("wogol", "", "wogol_run_anim", 23.0, 55.0, 100.0, 11.0, 3, Role::Swarm, NO_TINT, None, "Aggressive brawlers, fast and hardy"),
            MaskedOrc => d("masked_orc", "", "masked_orc_run_anim", 23.0, 120.0, 70.0, 14.0, 4, Role::Swarm, NO_TINT, None, "Armoured orcs that walk through light fire"),
            OrcWarrior => d("orc_warrior", "", "orc_warrior_run_anim", 23.0, 520.0, 75.0, 24.0, 22, Role::Elite, Color::srgb(1.0, 0.85, 0.8), None, "Orc champion elite; hits like a truck"),
            OrcShaman => d("orc_shaman", "", "orc_shaman_run_anim", 23.0, 160.0, 62.0, 8.0, 12, Role::Summoner, NO_TINT, Some(Imp), "Hangs back and summons imps"),
            BigDemon => d("big_demon", "THE CRYPT DEMON", "big_demon_run_anim", 36.0, 3000.0, 78.0, 28.0, 120, Role::Boss, NO_TINT, Some(TinyZombie), "Boss"),
            FrostKing => d("frost_king", "THE FROST KING", "ogre_run_anim", 36.0, 9000.0, 84.0, 34.0, 250, Role::Boss, Color::srgb(0.55, 0.8, 1.0), Some(IceZombie), "Boss"),
            DemonLord => d("demon_lord", "THE DEMON LORD", "big_demon_run_anim", 36.0, 24000.0, 92.0, 45.0, 500, Role::Boss, Color::srgb(1.0, 0.45, 0.35), Some(Imp), "Boss"),
        }
    }

    pub fn from_label(label: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.def().label == label)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BossPattern {
    Nova,
    Spiral,
    Barrage,
    Summon,
}

impl BossPattern {
    pub const ALL: [BossPattern; 4] = [BossPattern::Nova, BossPattern::Spiral, BossPattern::Barrage, BossPattern::Summon];

    pub fn label(self) -> &'static str {
        match self {
            BossPattern::Nova => "nova",
            BossPattern::Spiral => "spiral",
            BossPattern::Barrage => "barrage",
            BossPattern::Summon => "summon",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            BossPattern::Nova => "A ring of projectiles in every direction; punishes a survivor standing close.",
            BossPattern::Spiral => "A rotating stream of projectiles that sweeps the area; hard to dodge in a crowd.",
            BossPattern::Barrage => "A fast aimed volley straight at the survivor; best when they are far and kiting.",
            BossPattern::Summon => "Call a pack of minions around the boss to body-block the survivor.",
        }
    }

    pub fn from_label(label: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.label() == label)
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
    speed_mult: f32,
    /// Stable per-enemy value in [0, 1) that spreads a squad around the ring when surrounding.
    slot: f32,
    dead: bool,
}

#[derive(Component)]
struct BossBrain {
    cooldown: f32,
    casting: f32,
    spiral_angle: f32,
    pattern: BossPattern,
    rotation: usize,
}

/// Taken over by Soul Bind: fights for the survivor, then detonates.
#[derive(Component)]
pub struct Possessed {
    pub remaining: f32,
    pub damage: f32,
    pub blast_damage: f32,
    pub blast_radius: f32,
    pub attack_cooldown: f32,
}

/// On fire: damage over time, a flame drawn on the enemy, and a chance to spread.
#[derive(Component)]
pub struct Burning {
    dps: f32,
    remaining: f32,
    tick: f32,
    flame: Entity,
}

pub const BURN_SECS: f32 = 2.5;

/// Set an enemy on fire (or refresh the fire it already has).
#[derive(Message)]
pub struct Ignite {
    pub entity: Entity,
    pub dps: f32,
}

#[derive(Component)]
struct EnemyShot {
    velocity: Vec2,
    damage: f32,
    life: f32,
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
    pub xp_mult: u32,
}

#[derive(Resource, Default)]
struct SpawnClock {
    budget: f32,
}

fn reset_spawns(mut clock: ResMut<SpawnClock>) {
    *clock = SpawnClock::default();
}

fn rebuild_grid(mut grid: ResMut<EnemyGrid>, enemies: Query<(Entity, &Transform, &Enemy), Without<Possessed>>) {
    grid.rebuild(enemies.iter().filter(|(_, _, e)| !e.dead).map(|(entity, t, _)| (entity, t.translation.truncate())));
}

pub fn spawn_enemy(commands: &mut Commands, assets: &GameAssets, rng: &mut Rng, stage: &Stage, kind: EnemyKind, pos: Vec2) {
    const BOSS_SCALE: f32 = 1.6;
    let def = kind.def();
    let stage_def = stage.def();
    let is_boss = def.role == Role::Boss;
    let hp = if is_boss { def.hp } else { def.hp * stage_def.hp_mult * (1.0 + HP_GROWTH_PER_MINUTE * stage.minute() as f32) };
    let scale = if is_boss { BOSS_SCALE } else { 1.0 };
    let frames = assets.anim(def.anim);
    let mut entity = commands.spawn((
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
            speed_mult: stage_def.speed_mult,
            slot: rng.f32(),
            dead: false,
        },
        Motion::default(),
        YSort { feet: -def.height / 2.0 * PIXEL_SCALE * scale },
        Sprite { image: frames[0].clone(), color: def.tint, ..default() },
        Animated::looping(frames, 7.0 + rng.range(0.0, 2.0)),
        Transform::from_translation(pos.extend(Z_ACTORS)).with_scale(Vec3::splat(PIXEL_SCALE * scale)),
    ));
    if is_boss {
        entity.insert(BossBrain { cooldown: 3.0, casting: 0.0, spiral_angle: 0.0, pattern: BossPattern::Nova, rotation: 0 });
    }
}

pub fn spawn_point(rng: &mut Rng, obstacles: &Obstacles, around: Vec2) -> Vec2 {
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
    stage: Res<Stage>,
    mut rng: ResMut<Rng>,
    mut clock: ResMut<SpawnClock>,
    enemies: Query<(), With<Enemy>>,
    player: Single<&Transform, With<Player>>,
) {
    let minute = stage.minute() as f32;
    let rate = (BASE_SPAWN_RATE + SPAWN_RATE_PER_MINUTE * minute + stage.def().extra_spawn_rate)
        * (1.0 + orders.pressure as f32 * PRESSURE_SPAWN_BOOST);
    clock.budget += rate * time.delta_secs();
    let alive = enemies.iter().count();
    let center = player.translation.truncate();
    let unlocked = stage.unlocked();
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
        spawn_enemy(&mut commands, &assets, &mut rng, &stage, kind, pos);
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
    unlocked.first().copied().unwrap_or(EnemyKind::Zombie)
}

fn summon_minions(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<GameAssets>,
    stage: Res<Stage>,
    mut rng: ResMut<Rng>,
    mut summoners: Query<(&mut Enemy, &Transform), Without<Possessed>>,
    count: Query<(), With<Enemy>>,
) {
    const SUMMON_EVERY: f32 = 5.0;
    const MINIONS: usize = 3;
    let alive = count.iter().count();
    let mut raised = Vec::new();
    for (mut enemy, transform) in &mut summoners {
        let def = enemy.kind.def();
        if def.role != Role::Summoner || enemy.dead {
            continue;
        }
        enemy.summon_cooldown -= time.delta_secs();
        if enemy.summon_cooldown > 0.0 || alive + raised.len() >= MAX_ENEMIES {
            continue;
        }
        enemy.summon_cooldown = SUMMON_EVERY;
        let at = transform.translation.truncate();
        let minion = def.minion.unwrap_or(EnemyKind::TinyZombie);
        raised.extend((0..MINIONS).map(|_| (minion, at + Vec2::from_angle(rng.angle()) * 40.0)));
    }
    for (kind, pos) in raised {
        spawn_particles(&mut commands, &mut rng, pos, Color::srgb(0.6, 0.3, 0.9), 6, 120.0);
        spawn_enemy(&mut commands, &assets, &mut rng, &stage, kind, pos);
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

/// Separation from neighbours plus obstacle avoidance, shared by foes and allies.
fn crowd_forces(grid: &EnemyGrid, obstacles: &Obstacles, pos: Vec2, heading: Vec2, scale: f32) -> Vec2 {
    const LOOK_AHEAD: f32 = 60.0;
    const AVOID_WEIGHT: f32 = 1.6;
    let radius = ENEMY_SEPARATION_RADIUS * scale;
    let mut separation = Vec2::ZERO;
    for (_, other) in grid.within(pos, radius) {
        let delta = pos - other;
        let d = delta.length();
        if d > 0.01 {
            separation += delta / d * (1.0 - d / radius);
        }
    }
    separation * ENEMY_SEPARATION_FORCE + obstacles.avoidance(pos, heading, LOOK_AHEAD) * AVOID_WEIGHT
}

fn tint_for(def: &EnemyDef, enraged: bool, burning: bool) -> Color {
    const ENRAGE_TINT: Color = Color::srgb(1.0, 0.8, 0.78);
    const BURN_TINT: Color = Color::srgb(1.0, 0.65, 0.35);
    match () {
        _ if burning => BURN_TINT,
        _ if enraged && def.role == Role::Swarm => ENRAGE_TINT,
        _ => def.tint,
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
    mut enemies: Query<(&mut Enemy, &mut Transform, &mut Motion, &mut Sprite, Has<Burning>), Without<Possessed>>,
) {
    const TURN_RATE: f32 = 7.0;
    const REGROUP_SPEED: f32 = 0.55;
    const REGROUP_SLOW_DIST: f32 = 240.0;

    let dt = time.delta_secs();
    let (player_transform, player_motion) = *player;
    let player_pos = player_transform.translation.truncate();
    let enrage_mult = if orders.enraged { ENRAGE_SPEED_MULT } else { 1.0 };

    for (mut enemy, mut transform, mut motion, mut sprite, burning) in &mut enemies {
        let pos = transform.translation.truncate();
        let def = enemy.kind.def();
        if pos.distance(player_pos) > DESPAWN_DIST && def.role != Role::Boss {
            let ahead = player_pos + player_motion.0.normalize_or(Vec2::from_angle(rng.angle())) * SPAWN_MIN_DIST;
            transform.translation = (ahead + random_point(&mut rng, 200.0)).extend(transform.translation.z);
            continue;
        }
        let sector = sector_of(player_pos, pos);
        let tactic = orders.tactics[sector];
        let desired = tactic_direction(tactic, &enemy, pos, player_pos, player_motion.0, orders.centroids[sector])
            + crowd_forces(&grid, &obstacles, pos, enemy.heading, enemy.scale);

        enemy.heading = enemy.heading.lerp(desired.normalize_or(enemy.heading), (TURN_RATE * dt).min(1.0)).normalize_or(Vec2::X);
        let tactic_speed = if tactic == Tactic::Regroup && pos.distance(player_pos) > REGROUP_SLOW_DIST { REGROUP_SPEED } else { 1.0 };
        let knock = enemy.knockback;
        enemy.knockback -= knock * (KNOCKBACK_DECAY * dt).min(1.0);
        let velocity = enemy.heading * def.speed * enemy.speed_mult * enrage_mult * tactic_speed + knock;
        let next = obstacles.resolve(pos + velocity * dt, ENEMY_RADIUS * enemy.scale);
        motion.0 = (next - pos) / dt.max(f32::EPSILON);
        transform.translation = next.extend(transform.translation.z);
        sprite.color = tint_for(&def, orders.enraged, burning);
    }
}

/// Possessed enemies hunt the nearest foe.
fn steer_allies(
    time: Res<Time>,
    grid: Res<EnemyGrid>,
    obstacles: Res<Obstacles>,
    player: Single<&Transform, (With<Player>, Without<Possessed>)>,
    mut allies: Query<(&mut Enemy, &mut Transform, &mut Motion, &mut Sprite), With<Possessed>>,
) {
    const HUNT_RANGE: f32 = 500.0;
    const ALLY_SPEED_BONUS: f32 = 1.3;
    const POSSESSED_TINT: Color = Color::srgb(0.75, 0.45, 1.0);
    let dt = time.delta_secs();
    let home = player.translation.truncate();
    for (mut ally, mut transform, mut motion, mut sprite) in &mut allies {
        let pos = transform.translation.truncate();
        let target = grid.nearest(pos, HUNT_RANGE).map_or(home, |(_, p)| p);
        let desired = (target - pos).normalize_or_zero() + crowd_forces(&grid, &obstacles, pos, ally.heading, ally.scale) * 0.3;
        ally.heading = ally.heading.lerp(desired.normalize_or(ally.heading), (8.0 * dt).min(1.0)).normalize_or(Vec2::X);
        let speed = ally.kind.def().speed * ALLY_SPEED_BONUS;
        let next = obstacles.resolve(pos + ally.heading * speed * dt, ENEMY_RADIUS * ally.scale);
        motion.0 = (next - pos) / dt.max(f32::EPSILON);
        transform.translation = next.extend(transform.translation.z);
        sprite.color = POSSESSED_TINT;
    }
}

fn ally_attacks(
    time: Res<Time>,
    grid: Res<EnemyGrid>,
    mut damage: MessageWriter<DamageEnemy>,
    mut allies: Query<(&mut Possessed, &Enemy, &Transform)>,
) {
    const REACH: f32 = 34.0;
    const ATTACK_EVERY: f32 = 0.45;
    for (mut possessed, ally, transform) in &mut allies {
        possessed.attack_cooldown -= time.delta_secs();
        if possessed.attack_cooldown > 0.0 {
            continue;
        }
        let pos = transform.translation.truncate();
        let hits: Vec<(Entity, Vec2)> = grid.within(pos, REACH * ally.scale).collect();
        if hits.is_empty() {
            continue;
        }
        possessed.attack_cooldown = ATTACK_EVERY;
        for (foe, at) in hits {
            damage.write(DamageEnemy { entity: foe, amount: possessed.damage, knockback: (at - pos).normalize_or_zero() * 80.0 });
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn expire_possession(
    mut commands: Commands,
    time: Res<Time>,
    grid: Res<EnemyGrid>,
    meshes: Res<FxMeshes>,
    stage: Res<Stage>,
    mut rings: ResMut<Assets<RingMaterial>>,
    mut rng: ResMut<Rng>,
    mut damage: MessageWriter<DamageEnemy>,
    mut killed: MessageWriter<EnemyKilled>,
    mut sfx: MessageWriter<PlaySfx>,
    mut allies: Query<(Entity, &mut Possessed, &mut Enemy, &Transform)>,
) {
    const SOUL_COLOR: Color = Color::srgb(0.75, 0.4, 1.0);
    for (entity, mut possessed, mut ally, transform) in &mut allies {
        possessed.remaining -= time.delta_secs();
        if possessed.remaining > 0.0 || ally.dead {
            continue;
        }
        ally.dead = true;
        let pos = transform.translation.truncate();
        for (foe, at) in grid.within(pos, possessed.blast_radius) {
            damage.write(DamageEnemy { entity: foe, amount: possessed.blast_damage, knockback: (at - pos).normalize_or_zero() * 220.0 });
        }
        spawn_ring_burst(&mut commands, &meshes, &mut rings, pos, SOUL_COLOR, possessed.blast_radius, 0.45);
        spawn_particles(&mut commands, &mut rng, pos, SOUL_COLOR, 16, 260.0);
        killed.write(EnemyKilled { pos, kind: ally.kind, xp_mult: stage.def().xp_mult });
        sfx.write(PlaySfx(Sfx::Bomb));
        commands.entity(entity).try_despawn();
    }
}

fn apply_ignites(
    mut commands: Commands,
    meshes: Res<FxMeshes>,
    fx: Res<FxMaterials>,
    mut ignites: MessageReader<Ignite>,
    mut burning: Query<&mut Burning>,
    enemies: Query<&Enemy, Without<Possessed>>,
) {
    const FLAME_SIZE: Vec3 = Vec3::new(9.0, 14.0, 1.0);
    let mut lit = Vec::new();
    for ignite in ignites.read() {
        if let Ok(mut burn) = burning.get_mut(ignite.entity) {
            burn.remaining = BURN_SECS;
            burn.dps = burn.dps.max(ignite.dps);
            continue;
        }
        let alive = enemies.get(ignite.entity).is_ok_and(|e| !e.dead);
        if !alive || lit.contains(&ignite.entity) {
            continue;
        }
        lit.push(ignite.entity);
        let flame = commands
            .spawn((
                Mesh2d(meshes.quad.clone()),
                MeshMaterial2d(fx.flame.clone()),
                Transform::from_xyz(0.0, 5.0, 0.2).with_scale(FLAME_SIZE),
                ChildOf(ignite.entity),
            ))
            .id();
        commands.entity(ignite.entity).try_insert(Burning { dps: ignite.dps, remaining: BURN_SECS, tick: 0.0, flame });
    }
}

fn burn_enemies(
    mut commands: Commands,
    time: Res<Time>,
    grid: Res<EnemyGrid>,
    mut rng: ResMut<Rng>,
    mut damage: MessageWriter<DamageEnemy>,
    mut ignite: MessageWriter<Ignite>,
    mut burning: Query<(Entity, &mut Burning, &Transform)>,
) {
    const TICK: f32 = 0.35;
    const EMBER_CHANCE: f32 = 0.3;
    const SPREAD_CHANCE: f32 = 0.15;
    const SPREAD_RANGE: f32 = 70.0;
    const SPREAD_FALLOFF: f32 = 0.7;
    let dt = time.delta_secs();
    for (entity, mut burn, transform) in &mut burning {
        burn.remaining -= dt;
        burn.tick -= dt;
        if burn.remaining <= 0.0 {
            commands.entity(burn.flame).try_despawn();
            commands.entity(entity).try_remove::<Burning>();
            continue;
        }
        if burn.tick > 0.0 {
            continue;
        }
        burn.tick = TICK;
        let pos = transform.translation.truncate();
        damage.write(DamageEnemy { entity, amount: burn.dps * TICK, knockback: Vec2::ZERO });
        if rng.chance(EMBER_CHANCE) {
            spawn_particles(&mut commands, &mut rng, pos, Color::srgb(1.0, 0.55, 0.15), 2, 60.0);
        }
        if rng.chance(SPREAD_CHANCE)
            && let Some((neighbour, _)) = grid.within(pos, SPREAD_RANGE).find(|(e, _)| *e != entity)
        {
            ignite.write(Ignite { entity: neighbour, dps: burn.dps * SPREAD_FALLOFF });
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn boss_attacks(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<GameAssets>,
    meshes: Res<FxMeshes>,
    orders: Res<HordeOrders>,
    stage: Res<Stage>,
    mut glow: ResMut<Assets<GlowMaterial>>,
    mut rng: ResMut<Rng>,
    mut sfx: MessageWriter<PlaySfx>,
    player: Single<&Transform, (With<Player>, Without<BossBrain>)>,
    mut bosses: Query<(&mut BossBrain, &Enemy, &Transform), Without<Possessed>>,
    count: Query<(), With<Enemy>>,
) {
    const CAST_EVERY: f32 = 3.4;
    const SPIRAL_SECS: f32 = 1.6;
    const SPIRAL_STEP: f32 = 0.45;
    const NOVA_SHOTS: usize = 18;
    const BARRAGE_SHOTS: usize = 7;
    const SUMMON_COUNT: usize = 6;
    let dt = time.delta_secs();
    let target = player.translation.truncate();
    let color = stage.def().shot_color;
    let alive = count.iter().count();

    for (mut brain, boss, transform) in &mut bosses {
        let origin = transform.translation.truncate();
        let damage = boss.kind.def().damage * 0.6;
        let mut shoot = |commands: &mut Commands, dir: Vec2, speed: f32| {
            let (mesh, material, glow_transform) = glow_bundle(&meshes, &mut glow, color, 34.0, 18.0);
            commands.spawn((
                Gameplay,
                EnemyShot { velocity: dir * speed, damage, life: 4.0 },
                mesh,
                material,
                glow_transform.with_translation(origin.extend(Z_PROJECTILE)),
            ));
        };

        if brain.casting > 0.0 {
            brain.casting -= dt;
            brain.spiral_angle += SPIRAL_STEP;
            for arm in 0..3 {
                shoot(&mut commands, Vec2::from_angle(brain.spiral_angle + arm as f32 * TAU / 3.0), 210.0);
            }
            continue;
        }
        brain.cooldown -= dt;
        if brain.cooldown > 0.0 {
            continue;
        }
        brain.cooldown = CAST_EVERY;
        // Jev's pick when it has one, else rotate through the patterns.
        brain.pattern = orders.boss_pattern.unwrap_or_else(|| {
            brain.rotation += 1;
            BossPattern::ALL[brain.rotation % BossPattern::ALL.len()]
        });
        match brain.pattern {
            BossPattern::Nova => {
                for i in 0..NOVA_SHOTS {
                    shoot(&mut commands, Vec2::from_angle(i as f32 * TAU / NOVA_SHOTS as f32), 190.0);
                }
            }
            BossPattern::Spiral => brain.casting = SPIRAL_SECS,
            BossPattern::Barrage => {
                let aim = (target - origin).normalize_or(Vec2::X);
                for i in 0..BARRAGE_SHOTS {
                    let spread = (i as f32 - (BARRAGE_SHOTS as f32 - 1.0) / 2.0) * 0.12;
                    shoot(&mut commands, Vec2::from_angle(spread).rotate(aim), 330.0);
                }
            }
            BossPattern::Summon => {
                let minion = boss.kind.def().minion.unwrap_or(EnemyKind::TinyZombie);
                for _ in 0..SUMMON_COUNT.min(MAX_ENEMIES.saturating_sub(alive)) {
                    let at = origin + Vec2::from_angle(rng.angle()) * 70.0;
                    spawn_particles(&mut commands, &mut rng, at, Color::srgb(0.6, 0.3, 0.9), 6, 140.0);
                    spawn_enemy(&mut commands, &assets, &mut rng, &stage, minion, at);
                }
            }
        }
        sfx.write(PlaySfx(Sfx::Magic));
    }
}

#[allow(clippy::too_many_arguments)]
fn move_enemy_shots(
    mut commands: Commands,
    time: Res<Time>,
    mut shake: ResMut<CameraShake>,
    mut screen: ResMut<ScreenFx>,
    mut sfx: MessageWriter<PlaySfx>,
    player: Single<(&Transform, &mut Health, &Stats), (With<Player>, Without<EnemyShot>)>,
    mut shots: Query<(Entity, &mut EnemyShot, &mut Transform)>,
) {
    const HIT_RADIUS: f32 = 24.0;
    let dt = time.delta_secs();
    let (player_transform, mut health, stats) = player.into_inner();
    let target = player_transform.translation.truncate();
    for (entity, mut shot, mut transform) in &mut shots {
        shot.life -= dt;
        let next = transform.translation.truncate() + shot.velocity * dt;
        transform.translation = next.extend(Z_PROJECTILE);
        let hit = next.distance(target) < HIT_RADIUS;
        if hit && health.invulnerable <= 0.0 {
            health.current -= (shot.damage - stats.armor).max(1.0);
            health.invulnerable = PLAYER_IFRAMES;
            shake.0 = SHAKE_ON_HURT;
            screen.hurt = 0.8;
            sfx.write(PlaySfx(Sfx::Hurt));
        }
        if hit || shot.life <= 0.0 || Obstacles::out_of_bounds(next) {
            commands.entity(entity).try_despawn();
        }
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

    for (entity, at) in grid.within(pos, TOUCH_RADIUS * 2.5) {
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
        if matches!(enemy.kind, EnemyKind::IceZombie | EnemyKind::FrostKing) {
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
    stage: Res<Stage>,
    mut rng: ResMut<Rng>,
    mut hits: MessageReader<DamageEnemy>,
    mut killed: MessageWriter<EnemyKilled>,
    mut sfx: MessageWriter<PlaySfx>,
    mut enemies: Query<(&mut Enemy, &Transform), Without<Possessed>>,
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
            let gore = if def.role == Role::Boss { 50 } else { 8 };
            spawn_particles(&mut commands, &mut rng, pos, Color::srgb(0.55, 0.85, 0.35), gore, 220.0);
            killed.write(EnemyKilled { pos, kind: enemy.kind, xp_mult: stage.def().xp_mult });
            sfx.write(PlaySfx(Sfx::Kill));
            commands.entity(hit.entity).try_despawn();
        }
    }
}

/// Attach the purple soul glow to a freshly possessed enemy.
pub fn possess(commands: &mut Commands, fx: &FxMaterials, meshes: &FxMeshes, entity: Entity, possessed: Possessed) {
    const GLOW_SIZE: f32 = 14.0;
    commands.entity(entity).try_insert(possessed).with_children(|parent| {
        parent.spawn((
            Mesh2d(meshes.quad.clone()),
            MeshMaterial2d(fx.possessed.clone()),
            Transform::from_xyz(0.0, -4.0, -0.1).with_scale(Vec3::splat(GLOW_SIZE)),
        ));
    });
}
