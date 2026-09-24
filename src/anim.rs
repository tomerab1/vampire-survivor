//! Frame animation, facing (sprite flip from motion), depth y-sorting and hit flashes.

use bevy::prelude::*;

use crate::GameSet;
use crate::config::{ARENA_HALF, Z_ACTORS};

pub struct AnimPlugin;

impl Plugin for AnimPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (animate, face_motion, y_sort, hit_flash).in_set(GameSet::Presentation));
    }
}

/// Cycles `frames` at `fps`; swaps to `moving` frames while `Motion` is non-zero, if provided.
#[derive(Component)]
pub struct Animated {
    pub idle: Vec<Handle<Image>>,
    pub moving: Option<Vec<Handle<Image>>>,
    pub fps: f32,
    pub clock: f32,
}

impl Animated {
    pub fn looping(frames: Vec<Handle<Image>>, fps: f32) -> Self {
        Self { idle: frames, moving: None, fps, clock: 0.0 }
    }

    pub fn idle_run(idle: Vec<Handle<Image>>, run: Vec<Handle<Image>>, fps: f32) -> Self {
        Self { idle, moving: Some(run), fps, clock: 0.0 }
    }
}

/// World-space velocity of an actor this frame (pixels / second).
#[derive(Component, Default, Clone, Copy)]
pub struct Motion(pub Vec2);

/// Keeps actors drawn back-to-front by their feet's y position.
#[derive(Component)]
pub struct YSort {
    /// Offset from the transform origin to the feet, in world pixels (negative = below).
    pub feet: f32,
}

#[derive(Component)]
pub struct HitFlash(pub f32);

/// Multiplying texels by this saturates them to white on an LDR target.
const FLASH_COLOR: Color = Color::linear_rgb(6.0, 6.0, 6.0);
const MOVING_EPSILON_SQ: f32 = 4.0;

fn animate(time: Res<Time>, mut query: Query<(&mut Animated, &mut Sprite, Option<&Motion>)>) {
    for (mut anim, mut sprite, motion) in &mut query {
        anim.clock += time.delta_secs();
        let moving = motion.is_some_and(|m| m.0.length_squared() > MOVING_EPSILON_SQ);
        let frames = match (&anim.moving, moving) {
            (Some(run), true) => run,
            _ => &anim.idle,
        };
        let index = (anim.clock * anim.fps) as usize % frames.len();
        if sprite.image != frames[index] {
            sprite.image = frames[index].clone();
        }
    }
}

fn face_motion(mut query: Query<(&Motion, &mut Sprite)>) {
    const TURN_THRESHOLD: f32 = 1.0;
    for (motion, mut sprite) in &mut query {
        if motion.0.x.abs() > TURN_THRESHOLD {
            sprite.flip_x = motion.0.x < 0.0;
        }
    }
}

fn y_sort(mut query: Query<(&YSort, &mut Transform)>) {
    for (sort, mut transform) in &mut query {
        let feet_y = transform.translation.y + sort.feet;
        transform.translation.z = Z_ACTORS + (ARENA_HALF - feet_y) / (ARENA_HALF * 2.0) * 10.0;
    }
}

fn hit_flash(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(Entity, &mut HitFlash, &mut Sprite)>,
) {
    for (entity, mut flash, mut sprite) in &mut query {
        flash.0 -= time.delta_secs();
        if flash.0 > 0.0 {
            sprite.color = FLASH_COLOR;
        } else {
            sprite.color = Color::WHITE;
            commands.entity(entity).try_remove::<HitFlash>();
        }
    }
}
