//! Stages: each map has its own look, roster, ambience and boss. Killing the
//! boss opens a portal; stepping in warps the survivor (and their build) to the
//! next, harder map. Beating the final boss wins the run.

use bevy::prelude::*;

use crate::assets::{Ambience, GameAssets, PlaySfx, Sfx, swap_ambience};
use crate::config::*;
use crate::effects::{FxMaterials, FxMeshes, ScreenFx, spawn_ring_burst};
use crate::effects::RingMaterial;
use crate::enemies::{EnemyKilled, EnemyKind, Role, spawn_enemy, spawn_point};
use crate::hero::Player;
use crate::launch::LaunchOptions;
use crate::rng::Rng;
use crate::world::{CameraShake, Gameplay, Obstacles, build_arena};
use crate::{AppState, GameSet};

pub struct StagePlugin;

impl Plugin for StagePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Stage>()
            .init_resource::<Banner>()
            .add_systems(OnEnter(AppState::Playing), start_run)
            .add_systems(Update, (tick_stage, spawn_boss).chain().in_set(GameSet::Ai))
            .add_systems(Update, (open_portal, enter_portal).chain().in_set(GameSet::Resolve))
            .add_systems(Update, tick_banner.in_set(GameSet::Presentation));
    }
}

pub struct StageDef {
    pub name: &'static str,
    pub floor_tint: Color,
    pub prop_tint: Color,
    pub clear_color: Color,
    pub ambience: &'static str,
    /// (enemy, minute within the stage it starts appearing).
    pub roster: &'static [(EnemyKind, u32)],
    pub boss: EnemyKind,
    pub boss_escorts: (EnemyKind, usize),
    pub hp_mult: f32,
    pub speed_mult: f32,
    pub extra_spawn_rate: f32,
    pub xp_mult: u32,
    pub shot_color: Color,
}

pub const STAGES: [StageDef; 3] = [
    StageDef {
        name: "THE CRYPT",
        floor_tint: Color::WHITE,
        prop_tint: Color::WHITE,
        clear_color: Color::srgb(0.05, 0.04, 0.06),
        ambience: "sounds/ambience_cave.ogg",
        roster: &[
            (EnemyKind::TinyZombie, 0),
            (EnemyKind::Zombie, 0),
            (EnemyKind::Skeleton, 1),
            (EnemyKind::Swampy, 2),
            (EnemyKind::BigZombie, 2),
            (EnemyKind::Necromancer, 3),
            (EnemyKind::Chort, 3),
        ],
        boss: EnemyKind::BigDemon,
        boss_escorts: (EnemyKind::BigZombie, 2),
        hp_mult: 1.0,
        speed_mult: 1.0,
        extra_spawn_rate: 0.0,
        xp_mult: 1,
        shot_color: Color::srgb(1.0, 0.35, 0.3),
    },
    StageDef {
        name: "FROZEN CATACOMBS",
        floor_tint: Color::srgb(0.62, 0.78, 1.0),
        prop_tint: Color::srgb(0.75, 0.88, 1.0),
        clear_color: Color::srgb(0.03, 0.05, 0.09),
        ambience: "sounds/ambience_rain.ogg",
        roster: &[
            (EnemyKind::IceZombie, 0),
            (EnemyKind::Skeleton, 0),
            (EnemyKind::Goblin, 0),
            (EnemyKind::Muddy, 1),
            (EnemyKind::Ogre, 2),
            (EnemyKind::Necromancer, 2),
            (EnemyKind::Chort, 3),
        ],
        boss: EnemyKind::FrostKing,
        boss_escorts: (EnemyKind::Ogre, 3),
        hp_mult: 2.4,
        speed_mult: 1.1,
        extra_spawn_rate: 1.5,
        xp_mult: 2,
        shot_color: Color::srgb(0.45, 0.8, 1.0),
    },
    StageDef {
        name: "HELLFORGE",
        floor_tint: Color::srgb(1.0, 0.55, 0.45),
        prop_tint: Color::srgb(1.0, 0.7, 0.6),
        clear_color: Color::srgb(0.09, 0.02, 0.02),
        ambience: "sounds/ambience_storm.ogg",
        roster: &[
            (EnemyKind::Imp, 0),
            (EnemyKind::Chort, 0),
            (EnemyKind::Wogol, 1),
            (EnemyKind::MaskedOrc, 1),
            (EnemyKind::OrcWarrior, 2),
            (EnemyKind::OrcShaman, 2),
        ],
        boss: EnemyKind::DemonLord,
        boss_escorts: (EnemyKind::OrcWarrior, 4),
        hp_mult: 5.0,
        speed_mult: 1.2,
        extra_spawn_rate: 3.0,
        xp_mult: 3,
        shot_color: Color::srgb(1.0, 0.55, 0.15),
    },
];

#[derive(Resource, Default)]
pub struct Stage {
    pub index: usize,
    pub elapsed: f32,
    pub boss_spawned: bool,
    pub portal_open: bool,
}

impl Stage {
    pub fn def(&self) -> &'static StageDef {
        &STAGES[self.index]
    }

    pub fn minute(&self) -> u32 {
        (self.elapsed / 60.0) as u32
    }

    pub fn unlocked(&self) -> Vec<EnemyKind> {
        let minute = self.minute();
        self.def().roster.iter().filter(|(_, m)| *m <= minute).map(|(k, _)| *k).collect()
    }

    pub fn is_final(&self) -> bool {
        self.index + 1 == STAGES.len()
    }
}

/// Big centered announcement ("STAGE 2 — FROZEN CATACOMBS").
#[derive(Resource, Default)]
pub struct Banner {
    pub text: String,
    pub remaining: f32,
}

impl Banner {
    pub fn show(&mut self, text: impl Into<String>) {
        const BANNER_SECS: f32 = 3.5;
        self.text = text.into();
        self.remaining = BANNER_SECS;
    }
}

#[derive(Component)]
pub struct Portal;

fn start_run(
    mut commands: Commands,
    server: Res<AssetServer>,
    mut stage: ResMut<Stage>,
    mut banner: ResMut<Banner>,
    mut clear: ResMut<ClearColor>,
    ambience: Query<Entity, With<Ambience>>,
) {
    *stage = Stage::default();
    clear.0 = stage.def().clear_color;
    swap_ambience(&mut commands, &server, &ambience, stage.def().ambience);
    banner.show(format!("STAGE 1 — {}", stage.def().name));
}

fn tick_stage(time: Res<Time>, mut stage: ResMut<Stage>) {
    stage.elapsed += time.delta_secs();
}

#[allow(clippy::too_many_arguments)]
fn spawn_boss(
    mut commands: Commands,
    assets: Res<GameAssets>,
    obstacles: Res<Obstacles>,
    mut stage: ResMut<Stage>,
    mut rng: ResMut<Rng>,
    mut banner: ResMut<Banner>,
    mut shake: ResMut<CameraShake>,
    mut sfx: MessageWriter<PlaySfx>,
    options: Res<LaunchOptions>,
    player: Single<&Transform, With<Player>>,
) {
    if stage.boss_spawned || stage.elapsed < options.boss_at {
        return;
    }
    stage.boss_spawned = true;
    let def = stage.def();
    let center = player.translation.truncate();
    let pos = spawn_point(&mut rng, &obstacles, center);
    spawn_enemy(&mut commands, &assets, &mut rng, &stage, def.boss, pos);
    let (escort, count) = def.boss_escorts;
    for _ in 0..count {
        let at = spawn_point(&mut rng, &obstacles, center);
        spawn_enemy(&mut commands, &assets, &mut rng, &stage, escort, at);
    }
    banner.show(format!("{} AWAKENS", def.boss.def().title));
    shake.0 = 16.0;
    sfx.write(PlaySfx(Sfx::Boss));
    info!("[telemetry] boss spawned: stage {} {}", stage.index + 1, def.boss.def().label);
}

#[allow(clippy::too_many_arguments)]
fn open_portal(
    mut commands: Commands,
    meshes: Res<FxMeshes>,
    fx: Res<FxMaterials>,
    mut rings: ResMut<Assets<RingMaterial>>,
    mut stage: ResMut<Stage>,
    mut banner: ResMut<Banner>,
    mut next: ResMut<NextState<AppState>>,
    mut sfx: MessageWriter<PlaySfx>,
    mut killed: MessageReader<EnemyKilled>,
    player: Single<&Transform, With<Player>>,
) {
    const PORTAL_SIZE: f32 = 190.0;
    const PORTAL_OFFSET: f32 = 220.0;
    let Some(_) = killed.read().find(|k| k.kind.def().role == Role::Boss) else {
        return;
    };
    if stage.is_final() {
        info!("[telemetry] final boss defeated");
        next.set(AppState::Victory);
        return;
    }
    if stage.portal_open {
        return;
    }
    stage.portal_open = true;
    let limit = ARENA_HALF - PORTAL_SIZE;
    let pos = (player.translation.truncate() + Vec2::Y * PORTAL_OFFSET).clamp(Vec2::splat(-limit), Vec2::splat(limit));
    commands.spawn((
        Gameplay,
        Portal,
        Mesh2d(meshes.quad.clone()),
        MeshMaterial2d(fx.portal.clone()),
        Transform::from_translation(pos.extend(Z_GROUND_FX + 0.5)).with_scale(Vec3::splat(PORTAL_SIZE)),
    ));
    spawn_ring_burst(&mut commands, &meshes, &mut rings, pos, Color::srgb(0.7, 0.4, 1.0), 260.0, 0.8);
    banner.show("A PORTAL HAS OPENED");
    sfx.write(PlaySfx(Sfx::Portal));
    info!("[telemetry] portal opened on stage {}", stage.index + 1);
}

/// Everything that belongs to the current map (not the player, UI, or audio).
type StageScoped = (With<Gameplay>, Without<Player>, Without<Node>);

#[allow(clippy::too_many_arguments)]
fn enter_portal(
    mut commands: Commands,
    assets: Res<GameAssets>,
    server: Res<AssetServer>,
    mut stage: ResMut<Stage>,
    mut rng: ResMut<Rng>,
    mut obstacles: ResMut<Obstacles>,
    mut banner: ResMut<Banner>,
    mut screen: ResMut<ScreenFx>,
    mut clear: ResMut<ClearColor>,
    mut sfx: MessageWriter<PlaySfx>,
    portal: Query<&Transform, (With<Portal>, Without<Player>)>,
    player: Single<&mut Transform, With<Player>>,
    scoped: Query<Entity, StageScoped>,
    ambience: Query<Entity, With<Ambience>>,
) {
    const ENTER_RADIUS: f32 = 70.0;
    let mut player_transform = player.into_inner();
    let Some(portal_transform) = portal.iter().next() else {
        return;
    };
    if portal_transform.translation.truncate().distance(player_transform.translation.truncate()) > ENTER_RADIUS {
        return;
    }

    for entity in &scoped {
        commands.entity(entity).try_despawn();
    }
    let next_index = stage.index + 1;
    *stage = Stage { index: next_index, ..default() };
    let def = stage.def();
    build_arena(&mut commands, &assets, &mut rng, &mut obstacles, def);
    player_transform.translation = Vec3::new(0.0, 0.0, player_transform.translation.z);
    clear.0 = def.clear_color;

    swap_ambience(&mut commands, &server, &ambience, def.ambience);

    screen.warp = 1.0;
    banner.show(format!("STAGE {} — {}", next_index + 1, def.name));
    sfx.write(PlaySfx(Sfx::Portal));
    info!("[telemetry] entered stage {}: {}", next_index + 1, def.name);
}

fn tick_banner(time: Res<Time>, mut banner: ResMut<Banner>) {
    banner.remaining = (banner.remaining - time.delta_secs()).max(0.0);
}
