//! Zombie Survivor: a Vampire-Survivors-style Bevy game whose horde is directed by Jev.

mod anim;
mod assets;
mod config;
mod director;
mod effects;
mod enemies;
mod hero;
mod hud;
mod launch;
mod pickups;
mod progression;
mod rng;
mod screens;
mod weapons;
mod world;

use bevy::prelude::*;
use bevy::window::WindowResolution;

use config::{WINDOW_HEIGHT, WINDOW_TITLE, WINDOW_WIDTH};

#[derive(States, Debug, Clone, Copy, Default, Eq, PartialEq, Hash)]
pub enum AppState {
    #[default]
    Menu,
    Playing,
    GameOver,
    Victory,
}

/// Whether the world simulates or is frozen behind the level-up picker.
#[derive(States, Debug, Clone, Copy, Default, Eq, PartialEq, Hash)]
pub enum Phase {
    #[default]
    Running,
    LevelUp,
}

/// Ordering for the per-frame gameplay pipeline; only runs while playing and not paused.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum GameSet {
    Input,
    Ai,
    Movement,
    Combat,
    Resolve,
    Presentation,
}

fn main() {
    let options = launch::LaunchOptions::detect();

    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: WINDOW_TITLE.into(),
                        resolution: WindowResolution::new(WINDOW_WIDTH, WINDOW_HEIGHT),
                        canvas: Some("#game".into()),
                        fit_canvas_to_parent: true,
                        prevent_default_event_handling: true,
                        ..default()
                    }),
                    ..default()
                })
                .set(ImagePlugin::default_nearest()),
        )
        .insert_resource(ClearColor(config::BACKGROUND))
        .insert_resource(options.clone())
        .insert_resource(rng::Rng::seeded(options.seed))
        .init_state::<AppState>()
        .init_state::<Phase>()
        .configure_sets(
            Update,
            (
                GameSet::Input,
                GameSet::Ai,
                GameSet::Movement,
                GameSet::Combat,
                GameSet::Resolve,
                GameSet::Presentation,
            )
                .chain()
                .run_if(in_state(AppState::Playing).and_then(in_state(Phase::Running))),
        )
        .add_plugins((
            assets::AssetsPlugin,
            effects::EffectsPlugin,
            anim::AnimPlugin,
            world::WorldPlugin,
            hero::HeroPlugin,
            weapons::WeaponsPlugin,
            enemies::EnemiesPlugin,
            pickups::PickupsPlugin,
            progression::ProgressionPlugin,
            director::DirectorPlugin,
            hud::HudPlugin,
            screens::ScreensPlugin,
            launch::LaunchPlugin,
        ))
        .run();
}
