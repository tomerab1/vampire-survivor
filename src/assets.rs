//! Handles to every sprite, sound and font. Art is CC0: 0x72's DungeonTileset II
//! (0x72.itch.io) for characters/props and Kenney (kenney.nl) for audio and fonts.

use std::collections::HashMap;

use bevy::audio::Volume;
use bevy::prelude::*;


pub struct AssetsPlugin;

impl Plugin for AssetsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameAssets>()
            .add_message::<PlaySfx>()
            .add_systems(PostUpdate, play_sounds)
            .add_systems(Startup, start_ambience);
    }
}

#[derive(Component)]
struct Ambience;

/// Looping cave ambience for the whole session. The source is mastered quiet (~-31 dB mean),
/// so it is amplified rather than attenuated.
fn start_ambience(mut commands: Commands, assets: Res<GameAssets>) {
    const AMBIENCE_VOLUME: f32 = 2.5;
    commands.spawn((
        Ambience,
        AudioPlayer(assets.ambience.clone()),
        PlaybackSettings::LOOP.with_volume(Volume::Linear(AMBIENCE_VOLUME)),
    ));
}

/// Request a sound effect; duplicates within `SFX_MIN_GAP_SECS` are dropped so hordes don't deafen.
#[derive(Message, Clone, Copy)]
pub struct PlaySfx(pub Sfx);

const SFX_MIN_GAP_SECS: f32 = 0.06;

fn play_sounds(
    mut commands: Commands,
    time: Res<Time<Real>>,
    assets: Res<GameAssets>,
    mut requests: MessageReader<PlaySfx>,
    mut last_played: Local<HashMap<Sfx, f32>>,
    mut rotation: Local<usize>,
) {
    let now = time.elapsed_secs();
    for PlaySfx(sfx) in requests.read() {
        let last = last_played.get(sfx).copied().unwrap_or(f32::NEG_INFINITY);
        if now - last < SFX_MIN_GAP_SECS {
            continue;
        }
        last_played.insert(*sfx, now);
        let (variants, volume) = &assets.sounds[sfx];
        *rotation = rotation.wrapping_add(1);
        let handle = variants[*rotation % variants.len()].clone();
        commands.spawn((AudioPlayer(handle), PlaybackSettings::DESPAWN.with_volume(Volume::Linear(*volume))));
    }
}

/// Animated families: (file prefix, frame count). Files are `dungeon/{prefix}_f{i}.png`.
const ANIMATIONS: &[(&str, usize)] = &[
    ("knight_m_idle_anim", 4),
    ("knight_m_run_anim", 4),
    ("elf_f_idle_anim", 4),
    ("elf_f_run_anim", 4),
    ("wizzard_m_idle_anim", 4),
    ("wizzard_m_run_anim", 4),
    ("lizard_m_idle_anim", 4),
    ("lizard_m_run_anim", 4),
    ("tiny_zombie_run_anim", 4),
    ("zombie_anim", 4),
    ("skelet_run_anim", 4),
    ("swampy_anim", 4),
    ("ice_zombie_anim", 4),
    ("chort_run_anim", 4),
    ("big_zombie_run_anim", 4),
    ("necromancer_anim", 4),
    ("ogre_run_anim", 4),
    ("big_demon_run_anim", 4),
    ("coin_anim", 4),
    ("chest_full_open_anim", 3),
];

/// Single-frame images: `dungeon/{name}.png`.
const IMAGES: &[&str] = &[
    "floor_big",
    "wall_mid",
    "wall_top_mid",
    "column",
    "crate",
    "skull",
    "hole",
    "weapon_knife",
    "weapon_bow",
    "weapon_arrow",
    "weapon_red_magic_staff",
    "weapon_green_magic_staff",
    "weapon_throwing_axe",
    "weapon_big_hammer",
    "weapon_golden_sword",
    "weapon_knight_sword",
    "flask_red",
    "flask_blue",
    "flask_yellow",
    "flask_big_red",
    "flask_big_blue",
    "flask_big_green",
    "flask_big_yellow",
    "ui_heart_full",
];

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Sfx {
    Swing,
    Bow,
    Magic,
    Zap,
    Slam,
    Hit,
    Kill,
    Gem,
    LevelUp,
    Chest,
    Coin,
    Hurt,
    Potion,
    Boss,
    Select,
    Bomb,
}

impl Sfx {
    /// (effect, file stem, variant count, volume). Files are `sounds/{stem}_{n}.ogg`, n from 1.
    const ALL: [(Sfx, &'static str, usize, f32); 16] = [
        (Sfx::Swing, "swing", 3, 0.35),
        (Sfx::Bow, "bow", 2, 0.4),
        (Sfx::Magic, "magic", 2, 0.3),
        (Sfx::Zap, "zap", 3, 0.45),
        (Sfx::Slam, "slam", 2, 0.6),
        (Sfx::Hit, "hit", 3, 0.28),
        (Sfx::Kill, "kill", 4, 0.4),
        (Sfx::Gem, "gem", 1, 0.2),
        (Sfx::LevelUp, "levelup", 1, 0.7),
        (Sfx::Chest, "chest", 1, 0.8),
        (Sfx::Coin, "coin", 1, 0.5),
        (Sfx::Hurt, "hurt", 1, 0.6),
        (Sfx::Potion, "potion", 1, 0.7),
        (Sfx::Boss, "boss", 1, 0.9),
        (Sfx::Select, "select", 1, 0.6),
        (Sfx::Bomb, "bomb", 1, 0.8),
    ];
}

#[derive(Resource)]
pub struct GameAssets {
    animations: HashMap<&'static str, Vec<Handle<Image>>>,
    images: HashMap<&'static str, Handle<Image>>,
    sounds: HashMap<Sfx, (Vec<Handle<AudioSource>>, f32)>,
    pub ambience: Handle<AudioSource>,
    pub pixel_font: Handle<Font>,
    pub mono_font: Handle<Font>,
    pub title_font: Handle<Font>,
}

impl GameAssets {
    pub fn anim(&self, prefix: &str) -> Vec<Handle<Image>> {
        self.animations
            .get(prefix)
            .cloned()
            .unwrap_or_else(|| panic!("animation `{prefix}` is not registered in ANIMATIONS"))
    }

    /// A single image, or the first frame of an animation of that name.
    pub fn icon(&self, name: &str) -> Handle<Image> {
        self.images.get(name).cloned().unwrap_or_else(|| self.anim(name)[0].clone())
    }

    pub fn img(&self, name: &str) -> Handle<Image> {
        self.images
            .get(name)
            .cloned()
            .unwrap_or_else(|| panic!("image `{name}` is not registered in IMAGES"))
    }
}

impl FromWorld for GameAssets {
    fn from_world(world: &mut World) -> Self {
        let server = world.resource::<AssetServer>();
        Self {
            animations: ANIMATIONS
                .iter()
                .map(|(prefix, count)| {
                    let frames = (0..*count).map(|i| server.load(format!("dungeon/{prefix}_f{i}.png"))).collect();
                    (*prefix, frames)
                })
                .collect(),
            images: IMAGES.iter().map(|name| (*name, server.load(format!("dungeon/{name}.png")))).collect(),
            sounds: Sfx::ALL
                .iter()
                .map(|(sfx, stem, variants, volume)| {
                    let handles = (1..=*variants).map(|n| server.load(format!("sounds/{stem}_{n}.ogg"))).collect();
                    (*sfx, (handles, *volume))
                })
                .collect(),
            ambience: server.load("sounds/ambience_cave.ogg"),
            pixel_font: server.load("fonts/pixel.ttf"),
            mono_font: server.load("fonts/mono.ttf"),
            title_font: server.load("fonts/future.ttf"),
        }
    }
}
