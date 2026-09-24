//! The dungeon arena: floor, wall border, pillars, breakable crates, decor,
//! the follow camera, and a spatial grid used for fast enemy queries.

use std::collections::HashMap;

use bevy::prelude::*;

use crate::anim::YSort;
use crate::assets::GameAssets;
use crate::config::*;
use crate::hero::Player;
use crate::rng::Rng;
use crate::stage::{STAGES, StageDef};
use crate::{AppState, GameSet};

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Obstacles>()
            .init_resource::<CameraShake>()
            .init_resource::<EnemyGrid>()
            .add_message::<CrateHit>()
            .add_message::<CrateBroken>()
            .add_systems(Startup, spawn_camera)
            .add_systems(OnEnter(AppState::Menu), despawn_gameplay)
            .add_systems(OnEnter(AppState::Playing), (despawn_gameplay, spawn_arena).chain())
            .add_systems(Update, damage_crates.in_set(GameSet::Resolve))
            .add_systems(Update, follow_camera.in_set(GameSet::Presentation));
    }
}

/// Everything spawned for one run; cleared when a new run starts or on return to the menu.
#[derive(Component)]
pub struct Gameplay;

#[derive(Component)]
pub struct Crate {
    hp: f32,
}

#[derive(Message)]
pub struct CrateHit {
    pub entity: Entity,
    pub damage: f32,
}

#[derive(Message)]
pub struct CrateBroken {
    pub pos: Vec2,
}

#[derive(Clone, Copy)]
pub struct Obstacle {
    pub center: Vec2,
    pub radius: f32,
    /// Set for breakable obstacles (crates).
    pub entity: Option<Entity>,
}

/// Static circle colliders, kept in a flat list for cheap per-frame queries.
#[derive(Resource, Default)]
pub struct Obstacles(pub Vec<Obstacle>);

#[derive(Resource, Default)]
pub struct CameraShake(pub f32);

impl Obstacles {
    /// Push a circle of `radius` at `pos` out of every obstacle and the arena walls.
    pub fn resolve(&self, pos: Vec2, radius: f32) -> Vec2 {
        let mut p = pos;
        for o in &self.0 {
            let delta = p - o.center;
            let min = o.radius + radius;
            let dist_sq = delta.length_squared();
            if dist_sq < min * min && dist_sq > f32::EPSILON {
                p = o.center + delta / dist_sq.sqrt() * min;
            }
        }
        let limit = ARENA_HALF - radius;
        p.clamp(Vec2::splat(-limit), Vec2::splat(limit))
    }

    /// The obstacle containing `pos`, if any.
    pub fn hit(&self, pos: Vec2) -> Option<&Obstacle> {
        self.0.iter().find(|o| o.center.distance_squared(pos) < o.radius * o.radius)
    }

    pub fn out_of_bounds(pos: Vec2) -> bool {
        pos.x.abs() > ARENA_HALF || pos.y.abs() > ARENA_HALF
    }

    /// Steering nudge that slides an agent around obstacles directly ahead.
    pub fn avoidance(&self, pos: Vec2, heading: Vec2, look_ahead: f32) -> Vec2 {
        let mut steer = Vec2::ZERO;
        for o in &self.0 {
            let to = o.center - pos;
            let ahead = to.dot(heading);
            if ahead <= 0.0 || ahead > look_ahead + o.radius {
                continue;
            }
            let lateral = to - heading * ahead;
            if lateral.length() < o.radius + ENEMY_RADIUS + 6.0 {
                let away = if lateral.length_squared() < 1.0 { heading.perp() } else { -lateral.normalize() };
                steer += away * (1.0 - ahead / (look_ahead + o.radius));
            }
        }
        steer
    }
}

/// Uniform-grid spatial hash of enemy positions, rebuilt every frame.
#[derive(Resource, Default)]
pub struct EnemyGrid {
    cells: HashMap<IVec2, Vec<(Entity, Vec2)>>,
}

impl EnemyGrid {
    fn cell(pos: Vec2) -> IVec2 {
        (pos / GRID_CELL).floor().as_ivec2()
    }

    pub fn rebuild(&mut self, entries: impl Iterator<Item = (Entity, Vec2)>) {
        self.cells.values_mut().for_each(Vec::clear);
        for (entity, pos) in entries {
            self.cells.entry(Self::cell(pos)).or_default().push((entity, pos));
        }
    }

    /// Every enemy within `radius` of `pos`.
    pub fn within(&self, pos: Vec2, radius: f32) -> impl Iterator<Item = (Entity, Vec2)> + '_ {
        let lo = Self::cell(pos - Vec2::splat(radius));
        let hi = Self::cell(pos + Vec2::splat(radius));
        let r2 = radius * radius;
        (lo.x..=hi.x)
            .flat_map(move |x| (lo.y..=hi.y).map(move |y| IVec2::new(x, y)))
            .filter_map(|c| self.cells.get(&c))
            .flatten()
            .filter(move |(_, p)| p.distance_squared(pos) <= r2)
            .copied()
    }

    /// Nearest enemy within `max_radius`, searching outward in growing rings.
    pub fn nearest(&self, pos: Vec2, max_radius: f32) -> Option<(Entity, Vec2)> {
        let mut radius = GRID_CELL * 2.0;
        loop {
            let r = radius.min(max_radius);
            let best = self
                .within(pos, r)
                .min_by(|a, b| a.1.distance_squared(pos).total_cmp(&b.1.distance_squared(pos)));
            if best.is_some() || r >= max_radius {
                return best;
            }
            radius *= 2.0;
        }
    }
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}

fn despawn_gameplay(mut commands: Commands, entities: Query<Entity, With<Gameplay>>) {
    for entity in &entities {
        commands.entity(entity).try_despawn();
    }
}

fn pixel_transform(pos: Vec2, z: f32) -> Transform {
    Transform::from_translation(pos.extend(z)).with_scale(Vec3::splat(PIXEL_SCALE))
}

fn spawn_arena(mut commands: Commands, assets: Res<GameAssets>, mut rng: ResMut<Rng>, mut obstacles: ResMut<Obstacles>) {
    build_arena(&mut commands, &assets, &mut rng, &mut obstacles, &STAGES[0]);
}

/// Lays out one stage's map: tinted floor and walls, pillars, crates and decor.
pub fn build_arena(commands: &mut Commands, assets: &GameAssets, rng: &mut Rng, obstacles: &mut Obstacles, stage: &StageDef) {
    let tiled = SpriteImageMode::Tiled { tile_x: true, tile_y: true, stretch_value: 1.0 };
    commands.spawn((
        Gameplay,
        Sprite {
            image: assets.img("floor_big"),
            custom_size: Some(Vec2::splat(ARENA_HALF * 2.0 / PIXEL_SCALE)),
            image_mode: tiled.clone(),
            color: stage.floor_tint,
            ..default()
        },
        pixel_transform(Vec2::ZERO, Z_FLOOR),
    ));
    spawn_walls(commands, assets, &tiled, stage.prop_tint);

    obstacles.0.clear();
    let place = |rng: &mut Rng, obstacles: &Obstacles, radius: f32| loop {
        let pos = random_point(rng, ARENA_HALF - 120.0);
        let crowded = obstacles.0.iter().any(|o| o.center.distance(pos) < o.radius + radius + 70.0);
        if pos.length() > OBSTACLE_CLEAR_RADIUS && !crowded {
            break pos;
        }
    };

    const COLUMN_RADIUS: f32 = 22.0;
    const COLUMN_LIFT: f32 = 60.0;
    for _ in 0..COLUMN_COUNT {
        let base = place(rng, obstacles, COLUMN_RADIUS);
        obstacles.0.push(Obstacle { center: base, radius: COLUMN_RADIUS, entity: None });
        commands.spawn((
            Gameplay,
            YSort { feet: -COLUMN_LIFT },
            Sprite { image: assets.img("column"), color: stage.prop_tint, ..default() },
            pixel_transform(base + Vec2::Y * COLUMN_LIFT, Z_ACTORS),
        ));
    }

    const CRATE_RADIUS: f32 = 22.0;
    const CRATE_LIFT: f32 = 18.0;
    for _ in 0..CRATE_COUNT {
        let base = place(rng, obstacles, CRATE_RADIUS);
        let entity = commands
            .spawn((
                Gameplay,
                Crate { hp: CRATE_HP },
                YSort { feet: -CRATE_LIFT },
                Sprite::from_image(assets.img("crate")),
                pixel_transform(base + Vec2::Y * CRATE_LIFT, Z_ACTORS),
            ))
            .id();
        obstacles.0.push(Obstacle { center: base, radius: CRATE_RADIUS, entity: Some(entity) });
    }

    let decor = ["skull", "hole"];
    for i in 0..DECOR_COUNT {
        let pos = random_point(rng, ARENA_HALF - 60.0);
        commands.spawn((
            Gameplay,
            Sprite { image: assets.img(decor[i % decor.len()]), color: stage.prop_tint, ..default() },
            pixel_transform(pos, Z_DECOR),
        ));
    }
}

fn spawn_walls(commands: &mut Commands, assets: &GameAssets, tiled: &SpriteImageMode, tint: Color) {
    const WALL_PX: f32 = 16.0;
    let thickness = WALL_PX * PIXEL_SCALE;
    let span = (ARENA_HALF * 2.0 + thickness * 2.0) / PIXEL_SCALE;
    let edge = ARENA_HALF + thickness / 2.0;
    for (pos, size, image) in [
        (Vec2::new(0.0, edge), Vec2::new(span, WALL_PX), "wall_top_mid"),
        (Vec2::new(0.0, -edge), Vec2::new(span, WALL_PX), "wall_mid"),
        (Vec2::new(edge, 0.0), Vec2::new(WALL_PX, span), "wall_mid"),
        (Vec2::new(-edge, 0.0), Vec2::new(WALL_PX, span), "wall_mid"),
    ] {
        commands.spawn((
            Gameplay,
            Sprite { image: assets.img(image), custom_size: Some(size), image_mode: tiled.clone(), color: tint, ..default() },
            pixel_transform(pos, Z_DECOR),
        ));
    }
}

pub fn random_point(rng: &mut Rng, half: f32) -> Vec2 {
    Vec2::new(rng.range(-half, half), rng.range(-half, half))
}

fn damage_crates(
    mut commands: Commands,
    mut hits: MessageReader<CrateHit>,
    mut broken: MessageWriter<CrateBroken>,
    mut obstacles: ResMut<Obstacles>,
    mut crates: Query<&mut Crate>,
) {
    for hit in hits.read() {
        let Ok(mut crate_) = crates.get_mut(hit.entity) else {
            continue;
        };
        if crate_.hp <= 0.0 {
            continue;
        }
        crate_.hp -= hit.damage;
        if crate_.hp > 0.0 {
            continue;
        }
        if let Some(index) = obstacles.0.iter().position(|o| o.entity == Some(hit.entity)) {
            let removed = obstacles.0.swap_remove(index);
            broken.write(CrateBroken { pos: removed.center });
        }
        commands.entity(hit.entity).try_despawn();
    }
}

fn follow_camera(
    time: Res<Time>,
    mut shake: ResMut<CameraShake>,
    mut rng: ResMut<Rng>,
    player: Single<&Transform, (With<Player>, Without<Camera2d>)>,
    mut camera: Single<&mut Transform, With<Camera2d>>,
) {
    let dt = time.delta_secs();
    let target = player.translation.truncate();
    let eased = camera.translation.truncate().lerp(target, (CAMERA_LERP * dt).min(1.0));
    let jitter = Vec2::new(rng.range(-1.0, 1.0), rng.range(-1.0, 1.0)) * shake.0;
    shake.0 = (shake.0 - shake.0 * SHAKE_DECAY * dt).max(0.0);
    camera.translation = (eased + jitter).extend(camera.translation.z);
}
