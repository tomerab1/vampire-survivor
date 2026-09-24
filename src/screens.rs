//! Title screen with hero select, plus the game-over and victory screens.

use bevy::prelude::*;

use crate::AppState;
use crate::anim::Animated;
use crate::assets::{GameAssets, PlaySfx, Sfx};
use crate::hero::{HeroKind, SelectedHero};
use crate::launch::LaunchOptions;
use crate::progression::RunStats;

const TITLE_COLOR: Color = Color::srgb(0.93, 0.35, 0.28);
const BODY_COLOR: Color = Color::srgb(0.92, 0.9, 0.85);
const DIM_COLOR: Color = Color::srgb(0.6, 0.58, 0.54);
const GOLD: Color = Color::srgb(1.0, 0.82, 0.3);
const CARD_BG: Color = Color::srgb(0.12, 0.1, 0.14);
const CARD_SELECTED: Color = Color::srgb(0.24, 0.17, 0.2);
const OVERLAY: Color = Color::srgba(0.0, 0.0, 0.0, 0.7);
const AUTOPLAY_RESTART_SECS: f32 = 3.0;

pub struct ScreensPlugin;

impl Plugin for ScreensPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Menu), spawn_menu)
            .add_systems(Update, (select_hero, animate_ui_heroes, start_from_menu).chain().run_if(in_state(AppState::Menu)))
            .add_systems(OnEnter(AppState::GameOver), |c: Commands, a: Res<GameAssets>, r: Res<RunStats>| spawn_end_screen(c, a, r, false))
            .add_systems(OnEnter(AppState::Victory), |c: Commands, a: Res<GameAssets>, r: Res<RunStats>| spawn_end_screen(c, a, r, true))
            .add_systems(Update, leave_end_screen.run_if(in_state(AppState::GameOver).or_else(in_state(AppState::Victory))));
    }
}

#[derive(Component)]
struct HeroCard(HeroKind);

/// Cycles an `ImageNode` through the hero's idle frames.
#[derive(Component)]
struct UiHeroAnim(Animated);

fn centered_column() -> Node {
    Node {
        width: Val::Percent(100.0),
        height: Val::Percent(100.0),
        flex_direction: FlexDirection::Column,
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        row_gap: Val::Px(16.0),
        ..default()
    }
}

fn line(font: &Handle<Font>, size: f32, color: Color, content: impl Into<String>) -> impl Bundle {
    (
        Text::new(content),
        TextFont { font: font.clone().into(), font_size: FontSize::Px(size), ..default() },
        TextColor(color),
        TextLayout::justify(Justify::Center),
    )
}

fn spawn_menu(mut commands: Commands, assets: Res<GameAssets>, options: Res<LaunchOptions>, selected: Res<SelectedHero>) {
    let brain = match &options.jev_disabled_reason {
        None => "The horde is directed by Jev (TypeSafe System One)".to_string(),
        Some(reason) => format!("Horde on local brain — {reason}"),
    };
    let cards: Vec<Entity> = HeroKind::ALL
        .iter()
        .enumerate()
        .map(|(i, hero)| {
            let def = hero.def();
            let frames = assets.anim(&format!("{}_idle_anim", def.sprite));
            commands
                .spawn((
                    HeroCard(*hero),
                    Button,
                    Node {
                        width: Val::Px(210.0),
                        padding: UiRect::all(Val::Px(14.0)),
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        row_gap: Val::Px(8.0),
                        border: UiRect::all(Val::Px(3.0)),
                        border_radius: BorderRadius::all(Val::Px(10.0)),
                        ..default()
                    },
                    BackgroundColor(if selected.0 == *hero { CARD_SELECTED } else { CARD_BG }),
                    BorderColor::all(if selected.0 == *hero { GOLD } else { Color::srgb(0.3, 0.28, 0.34) }),
                    children![
                        line(&assets.pixel_font, 22.0, DIM_COLOR, format!("[{}]", i + 1)),
                        (
                            UiHeroAnim(Animated::looping(frames.clone(), 6.0)),
                            ImageNode::new(frames[0].clone()),
                            Node { width: Val::Px(64.0), height: Val::Px(112.0), ..default() },
                        ),
                        line(&assets.title_font, 22.0, BODY_COLOR, def.name),
                        line(&assets.pixel_font, 22.0, DIM_COLOR, def.blurb),
                        (ImageNode::new(assets.icon(def.start.def().icon)), Node { height: Val::Px(48.0), ..default() }),
                    ],
                ))
                .id()
        })
        .collect();

    commands
        .spawn((DespawnOnExit(AppState::Menu), centered_column(), BackgroundColor(Color::srgb(0.05, 0.04, 0.06))))
        .with_children(|root| {
            root.spawn(line(&assets.title_font, 60.0, TITLE_COLOR, "ZOMBIE SURVIVOR"));
            root.spawn(line(&assets.pixel_font, 28.0, BODY_COLOR, "Survive 15 minutes. Level up. Pick your build."));
            root.spawn(line(&assets.mono_font, 16.0, DIM_COLOR, brain));
            root.spawn(Node { column_gap: Val::Px(18.0), margin: UiRect::vertical(Val::Px(12.0)), ..default() }).add_children(&cards);
            root.spawn(line(&assets.title_font, 24.0, GOLD, "ENTER TO START"));
            root.spawn(line(&assets.pixel_font, 22.0, DIM_COLOR, "1-4 or click to choose a hero · WASD move · hold mouse to aim"));
        });
}

fn select_hero(
    keys: Res<ButtonInput<KeyCode>>,
    mut selected: ResMut<SelectedHero>,
    mut sfx: MessageWriter<PlaySfx>,
    mut cards: Query<(&HeroCard, &Interaction, &mut BackgroundColor, &mut BorderColor)>,
) {
    let digits = [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4];
    let by_key = digits.iter().position(|k| keys.just_pressed(*k)).map(|i| HeroKind::ALL[i]);
    let by_click = cards.iter().find(|(_, i, ..)| **i == Interaction::Pressed).map(|(c, ..)| c.0);
    if let Some(hero) = by_key.or(by_click)
        && hero != selected.0
    {
        selected.0 = hero;
        sfx.write(PlaySfx(Sfx::Select));
    }
    for (card, interaction, mut bg, mut border) in &mut cards {
        let chosen = card.0 == selected.0;
        bg.0 = match (chosen, interaction) {
            (true, _) => CARD_SELECTED,
            (false, Interaction::Hovered) => Color::srgb(0.18, 0.15, 0.2),
            _ => CARD_BG,
        };
        *border = BorderColor::all(if chosen { GOLD } else { Color::srgb(0.3, 0.28, 0.34) });
    }
}

fn animate_ui_heroes(time: Res<Time>, mut heroes: Query<(&mut UiHeroAnim, &mut ImageNode)>) {
    for (mut anim, mut image) in &mut heroes {
        anim.0.clock += time.delta_secs();
        let frames = &anim.0.idle;
        let index = (anim.0.clock * anim.0.fps) as usize % frames.len();
        image.image = frames[index].clone();
    }
}

fn start_from_menu(
    keys: Res<ButtonInput<KeyCode>>,
    options: Res<LaunchOptions>,
    mut next: ResMut<NextState<AppState>>,
    mut sfx: MessageWriter<PlaySfx>,
) {
    if options.skip_menu || keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::Space) {
        sfx.write(PlaySfx(Sfx::Select));
        next.set(AppState::Playing);
    }
}

fn spawn_end_screen(mut commands: Commands, assets: Res<GameAssets>, run: Res<RunStats>, won: bool) {
    let secs = run.elapsed as u32;
    info!(
        "[telemetry] {} at {:02}:{:02} level={} kills={} gold={}",
        if won { "victory" } else { "game over" },
        secs / 60,
        secs % 60,
        run.level,
        run.kills,
        run.gold
    );
    let state = if won { AppState::Victory } else { AppState::GameOver };
    commands.spawn((
        DespawnOnExit(state),
        centered_column(),
        BackgroundColor(OVERLAY),
        GlobalZIndex(20),
        children![
            line(&assets.title_font, 60.0, if won { GOLD } else { TITLE_COLOR }, if won { "YOU SURVIVED" } else { "YOU WERE EATEN" }),
            line(
                &assets.pixel_font,
                32.0,
                BODY_COLOR,
                format!("{:02}:{:02} survived · level {} · {} kills · {} gold", secs / 60, secs % 60, run.level, run.kills, run.gold)
            ),
            line(&assets.title_font, 24.0, BODY_COLOR, "R TO RETRY    M FOR MENU"),
        ],
    ));
}

fn leave_end_screen(
    time: Res<Time<Real>>,
    keys: Res<ButtonInput<KeyCode>>,
    options: Res<LaunchOptions>,
    mut waited: Local<f32>,
    mut next: ResMut<NextState<AppState>>,
) {
    *waited += time.delta_secs();
    let autoplay_restart = options.autoplay && *waited > AUTOPLAY_RESTART_SECS;
    if autoplay_restart || keys.just_pressed(KeyCode::KeyR) {
        *waited = 0.0;
        next.set(AppState::Playing);
    } else if keys.just_pressed(KeyCode::KeyM) {
        *waited = 0.0;
        next.set(AppState::Menu);
    }
}
