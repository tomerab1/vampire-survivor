//! Tunables. World units are screen pixels; 16px source art is drawn at `PIXEL_SCALE`.

use bevy::prelude::*;

pub const WINDOW_TITLE: &str = "Zombie Survivor — Jev Horde";
pub const WINDOW_WIDTH: u32 = 1280;
pub const WINDOW_HEIGHT: u32 = 720;
pub const BACKGROUND: Color = Color::srgb(0.05, 0.04, 0.06);
pub const PIXEL_SCALE: f32 = 3.0;

// Run
pub const RUN_LENGTH_SECS: f32 = 15.0 * 60.0;
pub const BOSS_MINUTES: [u32; 2] = [5, 10];

// Arena
pub const ARENA_HALF: f32 = 2200.0;
pub const COLUMN_COUNT: usize = 34;
pub const CRATE_COUNT: usize = 40;
pub const DECOR_COUNT: usize = 140;
pub const OBSTACLE_CLEAR_RADIUS: f32 = 300.0;
pub const CRATE_HP: f32 = 20.0;

// Z layers (actors are y-sorted inside their band)
pub const Z_FLOOR: f32 = 0.0;
pub const Z_DECOR: f32 = 1.0;
pub const Z_GROUND_FX: f32 = 2.0;
pub const Z_PICKUP: f32 = 3.0;
pub const Z_ACTORS: f32 = 10.0;
pub const Z_PROJECTILE: f32 = 30.0;
pub const Z_OVERHEAD_FX: f32 = 40.0;
pub const Z_WORLD_TEXT: f32 = 50.0;
pub const Z_VIGNETTE: f32 = 90.0;

// Player
pub const PLAYER_RADIUS: f32 = 18.0;
pub const PLAYER_BASE_SPEED: f32 = 210.0;
pub const PLAYER_IFRAMES: f32 = 0.45;
pub const BASE_MAGNET: f32 = 120.0;
pub const MAX_WEAPONS: usize = 6;
pub const MAX_PASSIVES: usize = 6;
pub const MAX_LEVEL: u32 = 5;

// Enemies
pub const ENEMY_RADIUS: f32 = 18.0;
pub const ENEMY_TOUCH_COOLDOWN: f32 = 0.7;
pub const ENEMY_SEPARATION_RADIUS: f32 = 34.0;
pub const ENEMY_SEPARATION_FORCE: f32 = 1.4;
pub const SPAWN_MIN_DIST: f32 = 700.0;
pub const SPAWN_MAX_DIST: f32 = 950.0;
pub const DESPAWN_DIST: f32 = 1700.0;
pub const MAX_ENEMIES: usize = 320;
pub const BASE_SPAWN_RATE: f32 = 1.1;
pub const SPAWN_RATE_PER_MINUTE: f32 = 0.55;
pub const HP_GROWTH_PER_MINUTE: f32 = 0.12;
pub const PRESSURE_SPAWN_BOOST: f32 = 0.3;
pub const REINFORCEMENT_SHARE: f32 = 0.5;
pub const ENRAGE_SPEED_MULT: f32 = 1.3;
pub const KNOCKBACK_DECAY: f32 = 10.0;
pub const HIT_FLASH_SECS: f32 = 0.1;
pub const GRID_CELL: f32 = 64.0;

// Pickups
pub const GEM_TIERS: [(u32, Color); 3] = [
    (1, Color::srgb(0.35, 0.65, 1.0)),
    (4, Color::srgb(0.35, 1.0, 0.5)),
    (15, Color::srgb(1.0, 0.35, 0.45)),
];
pub const MAGNET_PULL_SPEED: f32 = 520.0;
pub const PICKUP_RADIUS: f32 = 22.0;
pub const FLASK_DROP_CHANCE: f32 = 0.006;
pub const COIN_DROP_CHANCE: f32 = 0.03;
pub const ELITE_CHEST_CHANCE: f32 = 0.35;
pub const ELITE_WEAPON_DROP_CHANCE: f32 = 0.25;
pub const FLOOR_WEAPON_INTERVAL: f32 = 40.0;
pub const HEAL_FLASK_AMOUNT: f32 = 35.0;
pub const BOMB_DAMAGE: f32 = 250.0;

// Feel
pub const CAMERA_LERP: f32 = 9.0;
pub const SHAKE_DECAY: f32 = 6.0;
pub const SHAKE_ON_HURT: f32 = 8.0;
pub const MAX_DAMAGE_NUMBERS: usize = 70;

// Director (Jev)
pub const DIRECTOR_INTERVAL_SECS: f32 = 2.0;
pub const DIRECTOR_TIMEOUT_SECS: u64 = 6;
pub const SQUAD_SECTORS: usize = 6;
pub const JEV_MODEL: &str = "jev-latest";
#[cfg(not(target_arch = "wasm32"))]
pub const JEV_DEFAULT_BASE_URL: &str = "https://api.typesafe.ai";
#[cfg(target_arch = "wasm32")]
pub const JEV_WEB_PROXY_PATH: &str = "/jev";
pub const SYSTEM_ONE_PATH: &str = "/v1/systemone";
