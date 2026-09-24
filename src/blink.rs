//! Blink: the one active ability. Space / right-click teleports toward the aim
//! (cursor, else movement), tearing a rift at both ends and blasting foes where
//! you land. Brief invulnerability; cooldown shown on the HUD.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::GameSet;
use crate::assets::{PlaySfx, Sfx};
use crate::config::*;
use crate::effects::{FxMeshes, RingMaterial, SwirlMaterial, spawn_particles, spawn_rift, spawn_ring_burst};
use crate::enemies::DamageEnemy;
use crate::hero::{Health, Intent, Player, Stats, autoplay};
use crate::rng::Rng;
use crate::world::{EnemyGrid, Obstacles};

pub struct BlinkPlugin;

impl Plugin for BlinkPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                (human_blink.run_if(not(autoplay)), bot_blink.run_if(autoplay)).in_set(GameSet::Input),
                perform_blink.in_set(GameSet::Movement),
            ),
        );
    }
}

/// Cooldown state lives on the player; `requested` is set by input and consumed on use.
#[derive(Component, Default)]
pub struct Blink {
    pub cooldown: f32,
    pub requested: Option<Vec2>,
}

const RIFT_COLOR: Color = Color::srgb(0.5, 0.75, 1.0);

fn human_blink(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform)>,
    player: Single<(&Transform, &Intent, &mut Blink), With<Player>>,
) {
    if !(keys.just_pressed(KeyCode::Space) || mouse.just_pressed(MouseButton::Right)) {
        return;
    }
    let (transform, intent, mut blink) = player.into_inner();
    let pos = transform.translation.truncate();
    let (camera, camera_transform) = *camera;
    let cursor_dir = window
        .cursor_position()
        .and_then(|c| camera.viewport_to_world_2d(camera_transform, c).ok())
        .map(|p| (p - pos).normalize_or_zero());
    let dir = if intent.movement != Vec2::ZERO { intent.movement } else { cursor_dir.unwrap_or(intent.facing) };
    blink.requested = Some(dir.normalize_or(Vec2::X));
}

/// The bot blinks out when it gets swarmed.
fn bot_blink(grid: Res<EnemyGrid>, player: Single<(&Transform, &mut Blink), With<Player>>) {
    const DANGER_RADIUS: f32 = 110.0;
    const DANGER_COUNT: usize = 7;
    let (transform, mut blink) = player.into_inner();
    let pos = transform.translation.truncate();
    let near: Vec<Vec2> = grid.within(pos, DANGER_RADIUS).map(|(_, p)| p).collect();
    if blink.cooldown > 0.0 || near.len() < DANGER_COUNT {
        return;
    }
    let crowd = near.iter().copied().sum::<Vec2>() / near.len() as f32;
    blink.requested = Some((pos - crowd).normalize_or(Vec2::X));
}

#[allow(clippy::too_many_arguments)]
fn perform_blink(
    mut commands: Commands,
    time: Res<Time>,
    grid: Res<EnemyGrid>,
    obstacles: Res<Obstacles>,
    meshes: Res<FxMeshes>,
    mut swirls: ResMut<Assets<SwirlMaterial>>,
    mut rings: ResMut<Assets<RingMaterial>>,
    mut rng: ResMut<Rng>,
    mut damage: MessageWriter<DamageEnemy>,
    mut sfx: MessageWriter<PlaySfx>,
    player: Single<(&mut Transform, &mut Blink, &mut Health, &Stats), With<Player>>,
) {
    const DISTANCE: f32 = 280.0;
    const BLAST_RADIUS: f32 = 120.0;
    const BLAST_DAMAGE: f32 = 30.0;
    const IFRAMES: f32 = 0.6;
    let (mut transform, mut blink, mut health, stats) = player.into_inner();
    blink.cooldown = (blink.cooldown - time.delta_secs()).max(0.0);
    let Some(dir) = blink.requested.take() else {
        return;
    };
    if blink.cooldown > 0.0 {
        return;
    }
    blink.cooldown = BLINK_COOLDOWN * stats.cooldown;

    let from = transform.translation.truncate();
    let to = obstacles.resolve(from + dir * DISTANCE, PLAYER_RADIUS);
    transform.translation = to.extend(transform.translation.z);
    health.invulnerable = health.invulnerable.max(IFRAMES);

    spawn_rift(&mut commands, &meshes, &mut swirls, from, RIFT_COLOR, 130.0);
    spawn_rift(&mut commands, &meshes, &mut swirls, to, RIFT_COLOR, 170.0);
    spawn_ring_burst(&mut commands, &meshes, &mut rings, to, RIFT_COLOR, BLAST_RADIUS, 0.35);
    for step in 0..8 {
        let p = from.lerp(to, step as f32 / 7.0);
        spawn_particles(&mut commands, &mut rng, p, RIFT_COLOR, 2, 90.0);
    }
    for (foe, at) in grid.within(to, BLAST_RADIUS) {
        damage.write(DamageEnemy { entity: foe, amount: BLAST_DAMAGE * stats.might, knockback: (at - to).normalize_or_zero() * 260.0 });
    }
    sfx.write(PlaySfx(Sfx::Blink));
}
