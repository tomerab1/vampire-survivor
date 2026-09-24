//! Run clock, XP and levels, and the level-up picker: the world freezes
//! (`Phase::LevelUp`) while the player chooses one of three upgrades.

use bevy::prelude::*;

use crate::assets::{GameAssets, PlaySfx, Sfx};
use crate::config::*;
use crate::effects::ScreenFx;
use crate::enemies::EnemyKilled;
use crate::hero::{Health, Loadout, PassiveKind, Player, autoplay};
use crate::rng::Rng;
use crate::weapons::WeaponKind;
use crate::{AppState, GameSet, Phase};

pub struct ProgressionPlugin;

impl Plugin for ProgressionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RunStats>()
            .init_resource::<Offers>()
            .add_message::<GainXp>()
            .add_systems(OnEnter(AppState::Playing), reset_run)
            .add_systems(Update, tick_clock.in_set(GameSet::Ai))
            .add_systems(Update, (count_kills, gain_xp).in_set(GameSet::Resolve))
            .add_systems(OnEnter(Phase::LevelUp), open_picker)
            .add_systems(OnExit(Phase::LevelUp), close_picker)
            .add_systems(
                Update,
                (keyboard_pick, click_pick, bot_pick.run_if(autoplay), hover_cards)
                    .run_if(in_state(Phase::LevelUp).and_then(in_state(AppState::Playing))),
            );
    }
}

#[derive(Resource, Default)]
pub struct RunStats {
    pub elapsed: f32,
    pub level: u32,
    pub xp: u32,
    pub next_xp: u32,
    pub kills: u32,
    pub gold: u32,
    pending_levels: u32,
}

impl RunStats {
    pub fn minute(&self) -> u32 {
        (self.elapsed / 60.0) as u32
    }
}

#[derive(Message)]
pub struct GainXp(pub u32);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Offer {
    Weapon(WeaponKind),
    Passive(PassiveKind),
    Gold,
    Heal,
}

#[derive(Resource, Default)]
struct Offers(Vec<Offer>);

#[derive(Component)]
struct Picker;

#[derive(Component)]
struct Card(usize);

fn xp_for_level(level: u32) -> u32 {
    const BASE: u32 = 4;
    const STEP: u32 = 4;
    BASE + STEP * level.saturating_sub(1) + level * level / 5
}

fn reset_run(mut run: ResMut<RunStats>, mut phase: ResMut<NextState<Phase>>) {
    *run = RunStats { level: 1, next_xp: xp_for_level(1), ..default() };
    phase.set(Phase::Running);
}

fn tick_clock(time: Res<Time>, mut run: ResMut<RunStats>, mut next: ResMut<NextState<AppState>>) {
    run.elapsed += time.delta_secs();
    if run.elapsed >= RUN_LENGTH_SECS {
        next.set(AppState::Victory);
    }
}

fn count_kills(mut killed: MessageReader<EnemyKilled>, mut run: ResMut<RunStats>) {
    run.kills += killed.read().count() as u32;
}

fn gain_xp(
    mut gains: MessageReader<GainXp>,
    mut run: ResMut<RunStats>,
    mut phase: ResMut<NextState<Phase>>,
) {
    for GainXp(amount) in gains.read() {
        run.xp += amount;
        while run.xp >= run.next_xp {
            run.xp -= run.next_xp;
            run.level += 1;
            run.next_xp = xp_for_level(run.level);
            run.pending_levels += 1;
        }
    }
    if run.pending_levels > 0 {
        phase.set(Phase::LevelUp);
    }
}

/// Up to `n` distinct upgrade offers the loadout can still take; gold/heal when maxed out.
pub fn roll_offers(loadout: &Loadout, rng: &mut Rng, n: usize) -> Vec<Offer> {
    let weapons = WeaponKind::ALL.into_iter().filter(|k| {
        let level = loadout.weapon_level(*k);
        if level == 0 { loadout.weapons.len() < MAX_WEAPONS } else { level < MAX_LEVEL }
    });
    let passives = PassiveKind::ALL.into_iter().filter(|k| {
        let level = loadout.passive_level(*k);
        if level == 0 { loadout.passives.len() < MAX_PASSIVES } else { level < k.def().max_level }
    });
    let mut pool: Vec<Offer> = weapons.map(Offer::Weapon).chain(passives.map(Offer::Passive)).collect();
    let mut picked = Vec::new();
    while picked.len() < n && !pool.is_empty() {
        picked.push(pool.swap_remove(rng.index(pool.len())));
    }
    for filler in [Offer::Gold, Offer::Heal] {
        if picked.len() < n {
            picked.push(filler);
        }
    }
    picked
}

/// Applies an offer and returns a short toast describing it.
pub fn apply_offer(loadout: &mut Loadout, run: &mut RunStats, offer: Offer) -> String {
    const GOLD_OFFER: u32 = 50;
    match offer {
        Offer::Weapon(kind) => match loadout.grant_weapon(kind) {
            Some(level) => format!("{} Lv{level}", kind.def().name),
            None => "weapon slots full".to_string(),
        },
        Offer::Passive(kind) => match loadout.grant_passive(kind) {
            Some(level) => format!("{} Lv{level}", kind.def().name),
            None => "passive slots full".to_string(),
        },
        Offer::Gold => {
            run.gold += GOLD_OFFER;
            format!("+{GOLD_OFFER} gold")
        }
        Offer::Heal => "healed".to_string(),
    }
}

struct CardText {
    icon: String,
    title: String,
    tag: String,
    body: String,
}

fn describe(offer: Offer, loadout: &Loadout) -> CardText {
    match offer {
        Offer::Weapon(kind) => {
            let next = loadout.weapon_level(kind) + 1;
            CardText {
                icon: kind.def().icon.to_string(),
                title: kind.def().name.to_string(),
                tag: if next == 1 { "NEW WEAPON".to_string() } else { format!("LEVEL {next}") },
                body: kind.upgrade_text(next),
            }
        }
        Offer::Passive(kind) => {
            let def = kind.def();
            let next = loadout.passive_level(kind) + 1;
            CardText {
                icon: def.icon.to_string(),
                title: def.name.to_string(),
                tag: if next == 1 { "NEW PASSIVE".to_string() } else { format!("LEVEL {next}") },
                body: def.per_level.to_string(),
            }
        }
        Offer::Gold => CardText { icon: "coin_anim".into(), title: "Gold".into(), tag: "BONUS".into(), body: "+50 gold".into() },
        Offer::Heal => CardText { icon: "flask_big_red".into(), title: "Chicken".into(), tag: "BONUS".into(), body: "Restore 30 HP".into() },
    }
}

const CARD_BG: Color = Color::srgb(0.13, 0.11, 0.15);
const CARD_HOVER: Color = Color::srgb(0.22, 0.18, 0.26);
const CARD_BORDER: Color = Color::srgb(0.95, 0.75, 0.3);
const TEXT_MAIN: Color = Color::srgb(0.96, 0.93, 0.88);
const TEXT_DIM: Color = Color::srgb(0.68, 0.64, 0.6);

#[allow(clippy::too_many_arguments)]
fn open_picker(
    mut commands: Commands,
    assets: Res<GameAssets>,
    mut rng: ResMut<Rng>,
    mut offers: ResMut<Offers>,
    mut screen: ResMut<ScreenFx>,
    mut sfx: MessageWriter<PlaySfx>,
    run: Res<RunStats>,
    loadout: Single<&Loadout, With<Player>>,
) {
    const CARDS: usize = 3;
    offers.0 = roll_offers(&loadout, &mut rng, CARDS);
    screen.levelup = 1.0;
    sfx.write(PlaySfx(Sfx::LevelUp));

    let text = |font: &Handle<Font>, size: f32, color: Color, s: String| {
        (Text::new(s), TextFont { font: font.clone().into(), font_size: FontSize::Px(size), ..default() }, TextColor(color))
    };
    let cards: Vec<Entity> = offers
        .0
        .iter()
        .enumerate()
        .map(|(i, offer)| {
            let d = describe(*offer, &loadout);
            let icon = assets.icon(&d.icon);
            commands
                .spawn((
                    Card(i),
                    Button,
                    Node {
                        width: Val::Px(250.0),
                        min_height: Val::Px(250.0),
                        padding: UiRect::all(Val::Px(16.0)),
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        row_gap: Val::Px(10.0),
                        border: UiRect::all(Val::Px(3.0)),
                        border_radius: BorderRadius::all(Val::Px(10.0)),
                        ..default()
                    },
                    BackgroundColor(CARD_BG),
                    BorderColor::all(CARD_BORDER),
                    children![
                        text(&assets.pixel_font, 22.0, CARD_BORDER, format!("[{}]  {}", i + 1, d.tag)),
                        (ImageNode::new(icon), Node { width: Val::Px(64.0), height: Val::Px(64.0), ..default() }),
                        text(&assets.title_font, 22.0, TEXT_MAIN, d.title),
                        text(&assets.pixel_font, 24.0, TEXT_DIM, d.body),
                    ],
                ))
                .id()
        })
        .collect();

    commands
        .spawn((
            Picker,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                row_gap: Val::Px(24.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
            GlobalZIndex(10),
        ))
        .with_children(|root| {
            root.spawn(text(&assets.title_font, 44.0, CARD_BORDER, format!("LEVEL {}!", run.level)));
            root.spawn(Node { column_gap: Val::Px(22.0), ..default() }).add_children(&cards);
            root.spawn(text(&assets.pixel_font, 24.0, TEXT_DIM, "Press 1 / 2 / 3 or click a card".to_string()));
        });
}

fn close_picker(mut commands: Commands, pickers: Query<Entity, With<Picker>>, cards: Query<Entity, With<Card>>) {
    for entity in pickers.iter().chain(cards.iter()) {
        commands.entity(entity).try_despawn();
    }
}

fn choose(
    index: usize,
    offers: &Offers,
    loadout: &mut Loadout,
    health: &mut Health,
    run: &mut RunStats,
    phase: &mut NextState<Phase>,
) {
    const HEAL_OFFER: f32 = 30.0;
    let Some(offer) = offers.0.get(index).copied() else {
        return;
    };
    let toast = apply_offer(loadout, run, offer);
    if offer == Offer::Heal {
        health.current = (health.current + HEAL_OFFER).min(health.max);
    }
    info!("[telemetry] level {} pick: {toast}", run.level);
    run.pending_levels = run.pending_levels.saturating_sub(1);
    // Leaving and re-entering LevelUp re-rolls cards for any remaining queued levels.
    phase.set(Phase::Running);
}

type PickerPlayer<'w, 's> = Single<'w, 's, (&'static mut Loadout, &'static mut Health), With<Player>>;

fn keyboard_pick(
    keys: Res<ButtonInput<KeyCode>>,
    offers: Res<Offers>,
    mut run: ResMut<RunStats>,
    mut phase: ResMut<NextState<Phase>>,
    mut sfx: MessageWriter<PlaySfx>,
    player: PickerPlayer,
) {
    let slots = [(KeyCode::Digit1, 0), (KeyCode::Digit2, 1), (KeyCode::Digit3, 2)];
    let Some(index) = slots.iter().find(|(k, _)| keys.just_pressed(*k)).map(|(_, i)| *i) else {
        return;
    };
    let (mut loadout, mut health) = player.into_inner();
    sfx.write(PlaySfx(Sfx::Select));
    choose(index, &offers, &mut loadout, &mut health, &mut run, &mut phase);
}

fn click_pick(
    cards: Query<(&Card, &Interaction), Changed<Interaction>>,
    offers: Res<Offers>,
    mut run: ResMut<RunStats>,
    mut phase: ResMut<NextState<Phase>>,
    mut sfx: MessageWriter<PlaySfx>,
    player: PickerPlayer,
) {
    let Some((card, _)) = cards.iter().find(|(_, i)| **i == Interaction::Pressed) else {
        return;
    };
    let (mut loadout, mut health) = player.into_inner();
    sfx.write(PlaySfx(Sfx::Select));
    choose(card.0, &offers, &mut loadout, &mut health, &mut run, &mut phase);
}

fn bot_pick(
    time: Res<Time<Real>>,
    mut waited: Local<f32>,
    mut rng: ResMut<Rng>,
    offers: Res<Offers>,
    mut run: ResMut<RunStats>,
    mut phase: ResMut<NextState<Phase>>,
    player: PickerPlayer,
) {
    const THINK_SECS: f32 = 0.6;
    *waited += time.delta_secs();
    if *waited < THINK_SECS || offers.0.is_empty() {
        return;
    }
    *waited = 0.0;
    let (mut loadout, mut health) = player.into_inner();
    let index = rng.index(offers.0.len());
    choose(index, &offers, &mut loadout, &mut health, &mut run, &mut phase);
}

fn hover_cards(mut cards: Query<(&Interaction, &mut BackgroundColor), (With<Card>, Changed<Interaction>)>) {
    for (interaction, mut bg) in &mut cards {
        bg.0 = if *interaction == Interaction::None { CARD_BG } else { CARD_HOVER };
    }
}
