//! Playable heroes, passive stats, the player's loadout, input and the autoplay bot.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::anim::{Animated, Motion, YSort};
use crate::assets::GameAssets;
use crate::blink::Blink;
use crate::stage::Portal;
use crate::config::*;
use crate::launch::LaunchOptions;
use crate::pickups::Gem;
use crate::weapons::WeaponKind;
use crate::world::{EnemyGrid, Gameplay, Obstacles};
use crate::{AppState, GameSet};

pub struct HeroPlugin;

impl Plugin for HeroPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SelectedHero>()
            .add_systems(OnEnter(AppState::Playing), spawn_player)
            .add_systems(
                Update,
                (human_intent.run_if(not(autoplay)), bot_intent.run_if(autoplay)).in_set(GameSet::Input),
            )
            .add_systems(Update, (recompute_stats, move_player, regenerate).chain().in_set(GameSet::Movement))
            .add_systems(Update, update_hp_bar.in_set(GameSet::Presentation));
    }
}

pub fn autoplay(options: Res<LaunchOptions>) -> bool {
    options.autoplay
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HeroKind {
    Knight,
    Elf,
    Wizard,
    Lizard,
}

pub struct HeroDef {
    pub name: &'static str,
    pub sprite: &'static str,
    pub blurb: &'static str,
    pub max_hp: f32,
    pub speed: f32,
    pub might: f32,
    pub cooldown: f32,
    pub armor: f32,
    pub regen: f32,
    pub start: WeaponKind,
}

impl HeroKind {
    pub const ALL: [HeroKind; 4] = [HeroKind::Knight, HeroKind::Elf, HeroKind::Wizard, HeroKind::Lizard];

    pub fn def(self) -> &'static HeroDef {
        const KNIGHT: HeroDef = HeroDef {
            name: "Knight",
            sprite: "knight_m",
            blurb: "Sturdy. Throwing knives. +1 armor",
            max_hp: 120.0,
            speed: 1.0,
            might: 1.0,
            cooldown: 1.0,
            armor: 1.0,
            regen: 0.0,
            start: WeaponKind::Knives,
        };
        const ELF: HeroDef = HeroDef {
            name: "Elf",
            sprite: "elf_f",
            blurb: "Fast. Piercing bow",
            max_hp: 90.0,
            speed: 1.15,
            might: 1.0,
            cooldown: 1.0,
            armor: 0.0,
            regen: 0.0,
            start: WeaponKind::Bow,
        };
        const WIZARD: HeroDef = HeroDef {
            name: "Wizard",
            sprite: "wizzard_m",
            blurb: "Fragile. Homing bolts. +10% might, -10% cooldown",
            max_hp: 80.0,
            speed: 1.0,
            might: 1.1,
            cooldown: 0.9,
            armor: 0.0,
            regen: 0.0,
            start: WeaponKind::MagicBolt,
        };
        const LIZARD: HeroDef = HeroDef {
            name: "Lizard",
            sprite: "lizard_m",
            blurb: "Regenerates. Orbiting axes",
            max_hp: 110.0,
            speed: 1.05,
            might: 1.0,
            cooldown: 1.0,
            armor: 0.0,
            regen: 0.3,
            start: WeaponKind::Axes,
        };
        match self {
            HeroKind::Knight => &KNIGHT,
            HeroKind::Elf => &ELF,
            HeroKind::Wizard => &WIZARD,
            HeroKind::Lizard => &LIZARD,
        }
    }
}

#[derive(Resource)]
pub struct SelectedHero(pub HeroKind);

impl Default for SelectedHero {
    fn default() -> Self {
        Self(HeroKind::Knight)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum PassiveKind {
    Might,
    Vitality,
    Swiftness,
    Haste,
    Reach,
    Magnet,
    Armor,
    Regen,
    Duplicator,
}

pub struct PassiveDef {
    pub name: &'static str,
    pub icon: &'static str,
    pub per_level: &'static str,
    pub max_level: u32,
}

impl PassiveKind {
    pub const ALL: [PassiveKind; 9] = [
        PassiveKind::Might,
        PassiveKind::Vitality,
        PassiveKind::Swiftness,
        PassiveKind::Haste,
        PassiveKind::Reach,
        PassiveKind::Magnet,
        PassiveKind::Armor,
        PassiveKind::Regen,
        PassiveKind::Duplicator,
    ];

    pub fn def(self) -> PassiveDef {
        let (name, icon, per_level, max_level) = match self {
            PassiveKind::Might => ("Might", "weapon_golden_sword", "+10% damage", MAX_LEVEL),
            PassiveKind::Vitality => ("Vitality", "ui_heart_full", "+20 max HP", MAX_LEVEL),
            PassiveKind::Swiftness => ("Swiftness", "flask_big_green", "+8% move speed", MAX_LEVEL),
            PassiveKind::Haste => ("Haste", "flask_big_blue", "-8% weapon cooldowns", MAX_LEVEL),
            PassiveKind::Reach => ("Reach", "flask_big_yellow", "+10% area", MAX_LEVEL),
            PassiveKind::Magnet => ("Magnet", "coin_anim", "+30% pickup range", MAX_LEVEL),
            PassiveKind::Armor => ("Armor", "weapon_knight_sword", "-1 damage taken", MAX_LEVEL),
            PassiveKind::Regen => ("Regen", "flask_big_red", "+0.4 HP per second", MAX_LEVEL),
            PassiveKind::Duplicator => ("Duplicator", "flask_yellow", "+1 projectile", 2),
        };
        PassiveDef { name, icon, per_level, max_level }
    }
}

#[derive(Component)]
pub struct Player {
    pub hero: HeroKind,
}

#[derive(Component)]
pub struct Health {
    pub current: f32,
    pub max: f32,
    pub invulnerable: f32,
}

/// Everything the player owns this run.
#[derive(Component, Default)]
pub struct Loadout {
    pub weapons: Vec<(WeaponKind, u32)>,
    pub passives: Vec<(PassiveKind, u32)>,
}

impl Loadout {
    pub fn weapon_level(&self, kind: WeaponKind) -> u32 {
        self.weapons.iter().find(|(k, _)| *k == kind).map_or(0, |(_, l)| *l)
    }

    pub fn passive_level(&self, kind: PassiveKind) -> u32 {
        self.passives.iter().find(|(k, _)| *k == kind).map_or(0, |(_, l)| *l)
    }

    /// Adds the weapon or raises its level; returns the resulting level, or `None` if it can't.
    pub fn grant_weapon(&mut self, kind: WeaponKind) -> Option<u32> {
        if let Some((_, level)) = self.weapons.iter_mut().find(|(k, _)| *k == kind) {
            (*level < MAX_LEVEL).then(|| {
                *level += 1;
                *level
            })
        } else {
            (self.weapons.len() < MAX_WEAPONS).then(|| {
                self.weapons.push((kind, 1));
                1
            })
        }
    }

    pub fn grant_passive(&mut self, kind: PassiveKind) -> Option<u32> {
        let max = kind.def().max_level;
        if let Some((_, level)) = self.passives.iter_mut().find(|(k, _)| *k == kind) {
            (*level < max).then(|| {
                *level += 1;
                *level
            })
        } else {
            (self.passives.len() < MAX_PASSIVES).then(|| {
                self.passives.push((kind, 1));
                1
            })
        }
    }
}

/// Derived multipliers used by weapons and movement; recomputed from hero + passives.
#[derive(Component, Clone, Copy)]
pub struct Stats {
    pub might: f32,
    pub speed: f32,
    pub cooldown: f32,
    pub area: f32,
    pub magnet: f32,
    pub armor: f32,
    pub regen: f32,
    pub amount: u32,
}

#[derive(Component, Default)]
pub struct Intent {
    pub movement: Vec2,
    /// World point the player is actively aiming at (mouse held); `None` = auto-target.
    pub aim: Option<Vec2>,
    pub facing: Vec2,
}

#[derive(Component)]
struct HpBarFill;

/// Slowed by an ice zombie's touch for the remaining seconds.
#[derive(Component)]
pub struct Chilled(pub f32);

const CHILL_SPEED: f32 = 0.6;

fn spawn_player(mut commands: Commands, assets: Res<GameAssets>, selected: Res<SelectedHero>, options: Res<LaunchOptions>) {
    const HP_BAR_SIZE: Vec2 = Vec2::new(16.0, 2.0);
    const HP_BAR_Y: f32 = -17.0;
    const FEET: f32 = -36.0;
    let def = selected.0.def();
    let mut weapons = vec![(def.start, 1)];
    weapons.extend(
        WeaponKind::ALL.into_iter().filter(|k| options.grant.iter().any(|g| g.eq_ignore_ascii_case(&format!("{k:?}")))).map(|k| (k, MAX_LEVEL)),
    );
    commands.spawn((
        Gameplay,
        Player { hero: selected.0 },
        Health { current: def.max_hp, max: def.max_hp, invulnerable: 0.0 },
        Loadout { weapons, passives: Vec::new() },
        stats_for(def, &Loadout::default()),
        Intent { facing: Vec2::X, ..default() },
        Blink::default(),
        Motion::default(),
        YSort { feet: FEET },
        Animated::idle_run(
            assets.anim(&format!("{}_idle_anim", def.sprite)),
            assets.anim(&format!("{}_run_anim", def.sprite)),
            8.0,
        ),
        Sprite::from_image(assets.anim(&format!("{}_idle_anim", def.sprite))[0].clone()),
        Transform::from_xyz(0.0, 0.0, Z_ACTORS).with_scale(Vec3::splat(PIXEL_SCALE)),
        children![
            (Sprite::from_color(Color::srgb(0.25, 0.05, 0.05), HP_BAR_SIZE), Transform::from_xyz(0.0, HP_BAR_Y, 0.1)),
            (
                HpBarFill,
                Sprite::from_color(Color::srgb(0.9, 0.2, 0.25), HP_BAR_SIZE),
                Transform::from_xyz(0.0, HP_BAR_Y, 0.2)
            ),
        ],
    ));
}

pub fn stats_for(def: &HeroDef, loadout: &Loadout) -> Stats {
    let level = |kind| loadout.passive_level(kind) as f32;
    Stats {
        might: def.might * (1.0 + 0.10 * level(PassiveKind::Might)),
        speed: def.speed * (1.0 + 0.08 * level(PassiveKind::Swiftness)),
        cooldown: def.cooldown * (1.0 - 0.08 * level(PassiveKind::Haste)),
        area: 1.0 + 0.10 * level(PassiveKind::Reach),
        magnet: 1.0 + 0.30 * level(PassiveKind::Magnet),
        armor: def.armor + level(PassiveKind::Armor),
        regen: def.regen + 0.4 * level(PassiveKind::Regen),
        amount: loadout.passive_level(PassiveKind::Duplicator),
    }
}

fn recompute_stats(mut player: Single<(&Player, &Loadout, &mut Stats, &mut Health), Changed<Loadout>>) {
    const VITALITY_HP: f32 = 20.0;
    let (p, loadout, stats, health) = &mut *player;
    let def = p.hero.def();
    **stats = stats_for(def, loadout);
    let max = def.max_hp + VITALITY_HP * loadout.passive_level(PassiveKind::Vitality) as f32;
    health.current += (max - health.max).max(0.0);
    health.max = max;
}

fn regenerate(time: Res<Time>, player: Single<(&Stats, &mut Health), With<Player>>) {
    let (stats, mut health) = player.into_inner();
    health.current = (health.current + stats.regen * time.delta_secs()).min(health.max);
}

fn human_intent(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform)>,
    mut intent: Single<&mut Intent, With<Player>>,
) {
    let axis = |neg: [KeyCode; 2], pos: [KeyCode; 2]| f32::from(keys.any_pressed(pos)) - f32::from(keys.any_pressed(neg));
    intent.movement = Vec2::new(
        axis([KeyCode::KeyA, KeyCode::ArrowLeft], [KeyCode::KeyD, KeyCode::ArrowRight]),
        axis([KeyCode::KeyS, KeyCode::ArrowDown], [KeyCode::KeyW, KeyCode::ArrowUp]),
    )
    .normalize_or_zero();
    if intent.movement != Vec2::ZERO {
        intent.facing = intent.movement;
    }

    let (camera, camera_transform) = *camera;
    intent.aim = mouse
        .pressed(MouseButton::Left)
        .then(|| window.cursor_position())
        .flatten()
        .and_then(|cursor| camera.viewport_to_world_2d(camera_transform, cursor).ok());
}

/// Kites away from nearby threats while circle-strafing, and hoovers up gems when it's safe.
fn bot_intent(
    grid: Res<EnemyGrid>,
    obstacles: Res<Obstacles>,
    gems: Query<&Transform, (With<Gem>, Without<Player>)>,
    portal: Query<&Transform, (With<Portal>, Without<Player>)>,
    player: Single<(&Transform, &mut Intent, &Health), With<Player>>,
) {
    const THREAT_RADIUS: f32 = 260.0;
    const GEM_SEEK_RADIUS: f32 = 500.0;
    const STRAFE: f32 = 0.8;
    const LOOK_AHEAD: f32 = 80.0;
    const CENTER_PULL: f32 = 1.5;
    const GEM_WEIGHT_UNDER_THREAT: f32 = 0.6;

    let (transform, mut intent, _) = player.into_inner();
    let pos = transform.translation.truncate();
    let threat: Vec2 = grid.within(pos, THREAT_RADIUS).map(|(_, e)| (pos - e) / (pos.distance(e).max(1.0)).powi(2)).sum();

    let gem_pull = gems
        .iter()
        .map(|t| t.translation.truncate())
        .filter(|g| g.distance(pos) < GEM_SEEK_RADIUS)
        .min_by(|a, b| a.distance_squared(pos).total_cmp(&b.distance_squared(pos)))
        .map_or(Vec2::ZERO, |g| (g - pos).normalize_or_zero());
    let mut dir = if threat.length_squared() > 1e-6 {
        let away = threat.normalize();
        away + away.perp() * STRAFE + gem_pull * GEM_WEIGHT_UNDER_THREAT
    } else {
        gem_pull
    };
    // An open portal beats everything: head straight for the next stage.
    if let Some(p) = portal.iter().next() {
        dir = (p.translation.truncate() - pos).normalize_or_zero() * 2.0 + dir * 0.3;
    }
    let edge = (pos / ARENA_HALF).length().powi(4);
    dir -= pos.normalize_or_zero() * edge * CENTER_PULL;
    dir += obstacles.avoidance(pos, dir.normalize_or_zero(), LOOK_AHEAD) * 2.0;
    intent.movement = dir.normalize_or_zero();
    if intent.movement != Vec2::ZERO {
        intent.facing = intent.movement;
    }
    intent.aim = None;
}

fn move_player(
    time: Res<Time>,
    obstacles: Res<Obstacles>,
    player: Single<(&mut Transform, &mut Motion, &Intent, &Stats, Option<&mut Chilled>), With<Player>>,
) {
    let dt = time.delta_secs();
    let (mut transform, mut motion, intent, stats, chilled) = player.into_inner();
    let chill = match chilled {
        Some(mut c) if c.0 > 0.0 => {
            c.0 -= dt;
            CHILL_SPEED
        }
        _ => 1.0,
    };
    let pos = transform.translation.truncate();
    let next = obstacles.resolve(pos + intent.movement * PLAYER_BASE_SPEED * stats.speed * chill * dt, PLAYER_RADIUS);
    motion.0 = (next - pos) / dt.max(f32::EPSILON);
    transform.translation = next.extend(transform.translation.z);
}

fn update_hp_bar(
    player: Single<(&Health, &Children), With<Player>>,
    mut fills: Query<(&mut Transform, &mut Sprite), With<HpBarFill>>,
) {
    const BAR_WIDTH: f32 = 16.0;
    let (health, children) = *player;
    let fraction = (health.current / health.max).clamp(0.0, 1.0);
    for child in children.iter() {
        if let Ok((mut transform, mut sprite)) = fills.get_mut(child) {
            transform.scale.x = fraction;
            transform.translation.x = -BAR_WIDTH * (1.0 - fraction) / 2.0;
            sprite.color = if health.invulnerable > 0.0 { Color::WHITE } else { Color::srgb(0.9, 0.2, 0.25) };
        }
    }
}
