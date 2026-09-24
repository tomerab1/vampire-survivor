//! In-run HUD: XP bar, timer, kills/gold, owned weapons and passives, boss bar,
//! and the live Horde Director panel. Tab toggles the director panel; O toggles
//! the world overlay that draws every squad's tactic.

use bevy::prelude::*;

use crate::assets::GameAssets;
use crate::config::SQUAD_SECTORS;
use crate::director::{DirectorStatus, HordeOrders, SECTOR_LABELS, Source};
use crate::enemies::{Enemy, Role};
use crate::hero::{Loadout, Player};
use crate::blink::Blink;
use crate::config::BLINK_COOLDOWN;
use crate::progression::RunStats;
use crate::stage::{Banner, Stage};
use crate::world::Gameplay;
use crate::{AppState, GameSet};

const PANEL_BG: Color = Color::srgba(0.04, 0.03, 0.05, 0.78);
const TEXT_MAIN: Color = Color::srgb(0.96, 0.93, 0.88);
const TEXT_DIM: Color = Color::srgb(0.62, 0.6, 0.56);
const XP_COLOR: Color = Color::srgb(0.3, 0.6, 1.0);
const BOSS_COLOR: Color = Color::srgb(0.85, 0.15, 0.2);
const GOLD: Color = Color::srgb(1.0, 0.82, 0.3);
const JEV_ACCENT: Color = Color::srgb(0.98, 0.55, 0.35);
const LOCAL_ACCENT: Color = Color::srgb(0.6, 0.7, 0.95);
const ICON_PX: f32 = 36.0;
const OVERLAY_RADIUS: f32 = 26.0;

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(DirectorVisible(true))
            .insert_resource(OverlayVisible(false))
            .add_systems(OnEnter(AppState::Playing), spawn_hud)
            .add_systems(Update, toggle_director.run_if(in_state(AppState::Playing)))
            .add_systems(
                Update,
                (update_top_bar, update_inventory, update_boss_bar, update_director_panel, draw_overlay, update_banner, update_blink)
                    .in_set(GameSet::Presentation),
            );
    }
}

#[derive(Resource)]
struct DirectorVisible(bool);

/// Debug-style world overlay of squad tactics; off by default, toggled with O.
#[derive(Resource)]
struct OverlayVisible(bool);

#[derive(Component)]
struct XpFill;

#[derive(Component)]
struct LevelText;

#[derive(Component)]
struct ClockText;

#[derive(Component)]
struct StatsText;

#[derive(Component)]
struct Inventory;

#[derive(Component)]
struct BossBar;

#[derive(Component)]
struct BannerText;

#[derive(Component)]
struct BlinkFill;

#[derive(Component)]
struct BlinkLabel;

#[derive(Component)]
struct BossFill;

#[derive(Component)]
struct DirectorPanel;

#[derive(Component)]
struct DirectorHeader;

#[derive(Component)]
struct DirectorDetail;

#[derive(Component)]
struct SquadLine(usize);

fn text(font: &Handle<Font>, size: f32, color: Color, content: impl Into<String>) -> impl Bundle {
    (
        Text::new(content),
        TextFont { font: font.clone().into(), font_size: FontSize::Px(size), ..default() },
        TextColor(color),
    )
}

fn spawn_hud(mut commands: Commands, assets: Res<GameAssets>) {
    let pixel = &assets.pixel_font;
    let title = &assets.title_font;
    let mono = &assets.mono_font;

    // XP bar across the top edge.
    commands.spawn((
        Gameplay,
        Node { position_type: PositionType::Absolute, top: Val::Px(0.0), left: Val::Px(0.0), width: Val::Percent(100.0), height: Val::Px(18.0), ..default() },
        BackgroundColor(Color::srgb(0.08, 0.08, 0.14)),
        children![
            (XpFill, Node { width: Val::Percent(0.0), height: Val::Percent(100.0), ..default() }, BackgroundColor(XP_COLOR)),
            (
                LevelText,
                text(pixel, 22.0, TEXT_MAIN, "LV 1"),
                Node { position_type: PositionType::Absolute, right: Val::Px(10.0), top: Val::Px(-4.0), ..default() }
            ),
        ],
    ));

    commands.spawn((
        Gameplay,
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(24.0),
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            ..default()
        },
        children![
            (ClockText, text(title, 30.0, TEXT_MAIN, "00:00")),
            (StatsText, text(pixel, 22.0, TEXT_DIM, "")),
            (
                BossBar,
                Node { width: Val::Px(420.0), height: Val::Px(14.0), margin: UiRect::top(Val::Px(8.0)), display: Display::None, ..default() },
                BackgroundColor(Color::srgb(0.2, 0.04, 0.06)),
                children![(BossFill, Node { width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, BackgroundColor(BOSS_COLOR))],
            ),
        ],
    ));

    commands.spawn((
        Gameplay,
        Inventory,
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(26.0),
            left: Val::Px(10.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(4.0),
            ..default()
        },
    ));

    let squad_lines: Vec<Entity> = (0..SQUAD_SECTORS).map(|i| commands.spawn((SquadLine(i), text(mono, 13.0, TEXT_DIM, ""))).id()).collect();
    commands
        .spawn((
            Gameplay,
            DirectorPanel,
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(26.0),
                right: Val::Px(10.0),
                padding: UiRect::all(Val::Px(10.0)),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(4.0),
                min_width: Val::Px(290.0),
                border_radius: BorderRadius::all(Val::Px(6.0)),
                ..default()
            },
            BackgroundColor(PANEL_BG),
            children![(DirectorHeader, text(title, 14.0, JEV_ACCENT, "HORDE DIRECTOR")), (DirectorDetail, text(mono, 13.0, TEXT_DIM, "")),],
        ))
        .add_children(&squad_lines);

    commands.spawn((
        Gameplay,
        Node { position_type: PositionType::Absolute, bottom: Val::Px(6.0), left: Val::Px(10.0), ..default() },
        children![text(pixel, 20.0, TEXT_DIM, "WASD move · hold mouse to aim · SPACE / right-click blink · Tab director · O overlay")],
    ));

    commands.spawn((
        Gameplay,
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            top: Val::Percent(24.0),
            justify_content: JustifyContent::Center,
            ..default()
        },
        children![(BannerText, text(title, 40.0, GOLD, ""), TextLayout::justify(Justify::Center))],
    ));

    commands.spawn((
        Gameplay,
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(34.0),
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: Val::Px(3.0),
            ..default()
        },
        children![
            (BlinkLabel, text(pixel, 20.0, TEXT_MAIN, "BLINK")),
            (
                Node { width: Val::Px(140.0), height: Val::Px(8.0), ..default() },
                BackgroundColor(Color::srgb(0.1, 0.12, 0.2)),
                children![(BlinkFill, Node { width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, BackgroundColor(Color::srgb(0.5, 0.75, 1.0)))],
            ),
        ],
    ));
}

fn update_banner(banner: Res<Banner>, mut text: Single<(&mut Text, &mut TextColor), With<BannerText>>) {
    const FADE_SECS: f32 = 0.8;
    text.0.0 = if banner.remaining > 0.0 { banner.text.clone() } else { String::new() };
    text.1.0 = GOLD.with_alpha((banner.remaining / FADE_SECS).min(1.0));
}

fn update_blink(
    player: Single<&Blink, With<Player>>,
    mut fill: Single<&mut Node, With<BlinkFill>>,
    mut label: Single<(&mut Text, &mut TextColor), With<BlinkLabel>>,
) {
    let ready = player.cooldown <= 0.0;
    fill.width = Val::Percent((1.0 - player.cooldown / BLINK_COOLDOWN).clamp(0.0, 1.0) * 100.0);
    label.0.0 = if ready { "BLINK READY [SPACE]".to_string() } else { format!("BLINK {:.1}s", player.cooldown) };
    label.1.0 = if ready { Color::srgb(0.6, 0.85, 1.0) } else { TEXT_DIM };
}

fn update_top_bar(
    run: Res<RunStats>,
    stage: Res<Stage>,
    mut xp: Single<&mut Node, With<XpFill>>,
    mut level: Single<&mut Text, (With<LevelText>, Without<ClockText>, Without<StatsText>)>,
    mut clock: Single<&mut Text, (With<ClockText>, Without<StatsText>)>,
    mut stats: Single<&mut Text, With<StatsText>>,
) {
    xp.width = Val::Percent(run.xp as f32 / run.next_xp.max(1) as f32 * 100.0);
    level.0 = format!("LV {}", run.level);
    let secs = run.elapsed as u32;
    clock.0 = format!("{:02}:{:02}", secs / 60, secs % 60);
    stats.0 = format!("STAGE {} · {}   ·   {} kills   {} gold", stage.index + 1, stage.def().name, run.kills, run.gold);
}

/// Rebuilds the weapon/passive icon rows whenever the loadout changes.
fn update_inventory(
    mut commands: Commands,
    assets: Res<GameAssets>,
    player: Single<&Loadout, (With<Player>, Changed<Loadout>)>,
    inventory: Single<Entity, With<Inventory>>,
) {
    let loadout = *player;
    commands.entity(*inventory).despawn_related::<Children>();
    let slot = |icon: Handle<Image>, level: u32| {
        (
            Node {
                width: Val::Px(ICON_PX + 8.0),
                height: Val::Px(ICON_PX + 8.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(Val::Px(2.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(PANEL_BG),
            BorderColor::all(if level >= crate::config::MAX_LEVEL { GOLD } else { Color::srgb(0.3, 0.28, 0.34) }),
            children![
                (ImageNode::new(icon), Node { max_width: Val::Px(ICON_PX), max_height: Val::Px(ICON_PX), ..default() }),
                (
                    text(&assets.pixel_font, 18.0, GOLD, level.to_string()),
                    Node { position_type: PositionType::Absolute, right: Val::Px(2.0), bottom: Val::Px(-4.0), ..default() }
                ),
            ],
        )
    };
    let weapons: Vec<Entity> = loadout.weapons.iter().map(|(k, l)| commands.spawn(slot(assets.icon(k.def().icon), *l)).id()).collect();
    let passives: Vec<Entity> =
        loadout.passives.iter().map(|(k, l)| commands.spawn(slot(assets.icon(k.def().icon), *l)).id()).collect();
    let row = || Node { column_gap: Val::Px(4.0), ..default() };
    let weapon_row = commands.spawn(row()).add_children(&weapons).id();
    let passive_row = commands.spawn(row()).add_children(&passives).id();
    commands.entity(*inventory).add_children(&[weapon_row, passive_row]);
}

fn update_boss_bar(
    enemies: Query<&Enemy>,
    mut bar: Single<&mut Node, (With<BossBar>, Without<BossFill>)>,
    mut fill: Single<&mut Node, With<BossFill>>,
) {
    let boss = enemies.iter().find(|e| e.kind.def().role == Role::Boss);
    bar.display = if boss.is_some() { Display::Flex } else { Display::None };
    if let Some(boss) = boss {
        fill.width = Val::Percent((boss.hp / boss.max_hp).clamp(0.0, 1.0) * 100.0);
    }
}

fn toggle_director(
    keys: Res<ButtonInput<KeyCode>>,
    mut visible: ResMut<DirectorVisible>,
    mut overlay: ResMut<OverlayVisible>,
    mut panel: Query<&mut Node, With<DirectorPanel>>,
) {
    if keys.just_pressed(KeyCode::Tab) {
        visible.0 = !visible.0;
    }
    if keys.just_pressed(KeyCode::KeyO) {
        overlay.0 = !overlay.0;
    }
    for mut node in &mut panel {
        node.display = if visible.0 { Display::Flex } else { Display::None };
    }
}

fn update_director_panel(
    status: Res<DirectorStatus>,
    orders: Res<HordeOrders>,
    mut header: Single<(&mut Text, &mut TextColor), (With<DirectorHeader>, Without<DirectorDetail>)>,
    mut detail: Single<&mut Text, (With<DirectorDetail>, Without<SquadLine>)>,
    mut lines: Query<(&SquadLine, &mut Text, &mut TextColor), (Without<DirectorHeader>, Without<DirectorDetail>)>,
) {
    let (brain, accent) = match status.source {
        Source::Jev => (format!("JEV · {}", status.model.as_deref().unwrap_or("jev")), JEV_ACCENT),
        Source::Local => ("LOCAL BRAIN".to_string(), LOCAL_ACCENT),
    };
    header.0.0 = format!("HORDE DIRECTOR — {brain}");
    header.1.0 = accent;

    let link = if !status.jev_enabled {
        format!("jev off: {}", status.disabled_reason.as_deref().unwrap_or("-"))
    } else if let Some(error) = &status.last_error {
        format!("jev error: {}", error.chars().take(38).collect::<String>())
    } else {
        format!(
            "jev ok {} · fail {} · {}ms · {}k tok",
            status.successes,
            status.failures,
            status.last_latency_ms.unwrap_or(0),
            status.input_tokens / 1000
        )
    };
    detail.0 = format!(
        "{link}\npressure {}/3 · enrage {}\nsending: {}",
        orders.pressure,
        if orders.enraged { "YES" } else { "no" },
        orders.reinforcement.map_or("-", |k| k.def().label).to_uppercase(),
    );

    for (line, mut text, mut color) in &mut lines {
        match status.squads.iter().find(|s| s.sector == line.0) {
            Some(squad) => {
                text.0 = format!("{:<4}{:>4}  {}", SECTOR_LABELS[squad.sector], squad.count, squad.tactic.label().to_uppercase());
                color.0 = squad.tactic.color();
            }
            None => {
                text.0 = format!("{:<4}   —", SECTOR_LABELS[line.0]);
                color.0 = TEXT_DIM;
            }
        }
    }
}

/// A ring on each squad's center, colored by tactic, with a faint line to the survivor.
fn draw_overlay(
    visible: Res<OverlayVisible>,
    status: Res<DirectorStatus>,
    orders: Res<HordeOrders>,
    player: Single<&Transform, With<Player>>,
    mut gizmos: Gizmos,
) {
    if !visible.0 {
        return;
    }
    let target = player.translation.truncate();
    for squad in &status.squads {
        let center = orders.centroids[squad.sector];
        let color = squad.tactic.color().with_alpha(0.5);
        gizmos.circle_2d(center, OVERLAY_RADIUS + (squad.count as f32).sqrt() * 6.0, color);
        gizmos.line_2d(center, target, color.with_alpha(0.12));
    }
}
