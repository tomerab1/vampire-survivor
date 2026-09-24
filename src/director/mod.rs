//! The Horde Director. Every couple of seconds it snapshots the fight, groups
//! zombies into squads by the side of the survivor they are on, and asks Jev
//! (TypeSafe System One) one batched set of questions:
//!   - a `choice` per squad: which tactic that squad should run,
//!   - a `score` for horde pressure (spawn pacing),
//!   - a `noul` for whether the horde enrages (speed burst),
//!   - a `choice` of which unlocked enemy type to send as reinforcements.
//! Enemy steering and spawning (`enemies.rs`) execute the answers. When Jev
//! is unconfigured or failing, a local heuristic brain answers the same questions.

mod fallback;
mod jev;

use std::f32::consts::TAU;

use bevy::prelude::*;

use crate::config::*;
use crate::launch::LaunchOptions;
use crate::anim::Motion;
use crate::enemies::{Enemy, EnemyKind};
use crate::hero::{Health, Loadout, Player};
use crate::progression::RunStats;
use crate::{AppState, GameSet};

pub struct DirectorPlugin;

impl Plugin for DirectorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HordeOrders>()
            .init_resource::<DirectorStatus>()
            .init_resource::<jev::JevLink>()
            .add_systems(Startup, describe_backend)
            .add_systems(OnEnter(AppState::Playing), reset_orders)
            .add_systems(Update, (receive_decisions, request_decisions).chain().in_set(GameSet::Ai));
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Tactic {
    #[default]
    Rush,
    FlankLeft,
    FlankRight,
    Surround,
    Intercept,
    Regroup,
}

impl Tactic {
    pub const ALL: [Tactic; 6] =
        [Tactic::Rush, Tactic::FlankLeft, Tactic::FlankRight, Tactic::Surround, Tactic::Intercept, Tactic::Regroup];

    pub fn label(self) -> &'static str {
        match self {
            Tactic::Rush => "rush",
            Tactic::FlankLeft => "flank_left",
            Tactic::FlankRight => "flank_right",
            Tactic::Surround => "surround",
            Tactic::Intercept => "intercept",
            Tactic::Regroup => "regroup",
        }
    }

    /// Criteria text sent to Jev for this option.
    pub fn description(self) -> &'static str {
        match self {
            Tactic::Rush => "Charge straight at the survivor by the shortest path.",
            Tactic::FlankLeft => "Curve around to the survivor's left side, then close in from the flank.",
            Tactic::FlankRight => "Curve around to the survivor's right side, then close in from the flank.",
            Tactic::Surround => "Spread out into a ring around the survivor to cut off every escape route, then collapse.",
            Tactic::Intercept => "Aim ahead of the survivor's movement to cut them off where they are running to.",
            Tactic::Regroup => "Slow down and bunch up with the squad so it can attack together as a bigger mass.",
        }
    }

    pub fn from_label(label: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.label() == label)
    }

    pub fn color(self) -> Color {
        match self {
            Tactic::Rush => Color::srgb(0.95, 0.3, 0.25),
            Tactic::FlankLeft | Tactic::FlankRight => Color::srgb(1.0, 0.7, 0.2),
            Tactic::Surround => Color::srgb(0.7, 0.45, 1.0),
            Tactic::Intercept => Color::srgb(0.3, 0.8, 1.0),
            Tactic::Regroup => Color::srgb(0.6, 0.9, 0.5),
        }
    }
}

pub const SECTOR_LABELS: [&str; SQUAD_SECTORS] = ["ENE", "N", "WNW", "WSW", "S", "ESE"];

/// Which squad (sector around the survivor) a position belongs to.
pub fn sector_of(player: Vec2, pos: Vec2) -> usize {
    let d = pos - player;
    let angle = d.y.atan2(d.x).rem_euclid(TAU);
    ((angle / (TAU / SQUAD_SECTORS as f32)) as usize).min(SQUAD_SECTORS - 1)
}

/// What the zombies are currently executing.
#[derive(Resource, Default)]
pub struct HordeOrders {
    pub tactics: [Tactic; SQUAD_SECTORS],
    pub centroids: [Vec2; SQUAD_SECTORS],
    pub pressure: u8,
    pub enraged: bool,
    pub reinforcement: Option<EnemyKind>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Source {
    Jev,
    Local,
}

#[derive(Resource)]
pub struct DirectorStatus {
    pub jev_enabled: bool,
    pub disabled_reason: Option<String>,
    pub source: Source,
    pub model: Option<String>,
    pub successes: u32,
    pub failures: u32,
    pub consecutive_failures: u32,
    pub last_error: Option<String>,
    pub last_latency_ms: Option<u32>,
    pub input_tokens: u64,
    pub last_summary: String,
    pub squads: Vec<SquadReport>,
}

impl Default for DirectorStatus {
    fn default() -> Self {
        Self {
            jev_enabled: false,
            disabled_reason: None,
            source: Source::Local,
            model: None,
            successes: 0,
            failures: 0,
            consecutive_failures: 0,
            last_error: None,
            last_latency_ms: None,
            input_tokens: 0,
            last_summary: String::new(),
            squads: Vec::new(),
        }
    }
}

impl DirectorStatus {
    pub fn source_label(&self) -> &'static str {
        match self.source {
            Source::Jev => "jev",
            Source::Local => "local",
        }
    }
}

#[derive(Clone)]
pub struct SquadReport {
    pub sector: usize,
    pub count: usize,
    pub tactic: Tactic,
}

/// Everything the director knows when it asks for decisions.
#[derive(Clone)]
pub struct Snapshot {
    pub minute: u32,
    pub level: u32,
    pub kills: u32,
    pub survivor_pos: Vec2,
    pub survivor_vel: Vec2,
    pub survivor_hp: f32,
    pub survivor_max_hp: f32,
    pub build: Vec<String>,
    pub squads: Vec<Squad>,
    pub unlocked: Vec<EnemyKind>,
}

#[derive(Clone)]
pub struct Squad {
    pub sector: usize,
    pub count: usize,
    pub kinds: Vec<(EnemyKind, usize)>,
    pub centroid: Vec2,
    pub distance: f32,
    pub health_pct: f32,
}

pub struct Decision {
    pub tactics: Vec<(usize, Tactic)>,
    pub pressure: u8,
    pub enrage: bool,
    pub reinforcement: Option<EnemyKind>,
}

fn describe_backend(options: Res<LaunchOptions>, mut status: ResMut<DirectorStatus>) {
    status.jev_enabled = options.jev_base_url.is_some();
    status.disabled_reason = options.jev_disabled_reason.clone();
    match &options.jev_base_url {
        Some(url) => info!("[director] Jev enabled at {url}{SYSTEM_ONE_PATH} model={JEV_MODEL}"),
        None => info!(
            "[director] Jev disabled ({}); using local brain",
            options.jev_disabled_reason.as_deref().unwrap_or("unknown")
        ),
    }
}

fn reset_orders(mut orders: ResMut<HordeOrders>, mut link: ResMut<jev::JevLink>) {
    *orders = HordeOrders::default();
    link.reset_timer();
}

fn take_snapshot(
    run: &RunStats,
    player: (&Transform, &Motion, &Health, &Loadout),
    enemies: &Query<(&Enemy, &Transform), Without<Player>>,
) -> Snapshot {
    let (transform, motion, health, loadout) = player;
    let pos = transform.translation.truncate();
    let mut squads: Vec<Squad> = (0..SQUAD_SECTORS)
        .map(|sector| Squad { sector, count: 0, kinds: Vec::new(), centroid: Vec2::ZERO, distance: 0.0, health_pct: 0.0 })
        .collect();

    for (enemy, et) in enemies {
        let ep = et.translation.truncate();
        let squad = &mut squads[sector_of(pos, ep)];
        squad.count += 1;
        match squad.kinds.iter_mut().find(|(k, _)| *k == enemy.kind) {
            Some((_, n)) => *n += 1,
            None => squad.kinds.push((enemy.kind, 1)),
        }
        squad.centroid += ep;
        squad.health_pct += enemy.hp / enemy.max_hp;
    }
    for squad in squads.iter_mut().filter(|s| s.count > 0) {
        let n = squad.count as f32;
        squad.centroid /= n;
        squad.distance = squad.centroid.distance(pos);
        squad.health_pct = (squad.health_pct / n * 100.0).round();
    }
    squads.retain(|s| s.count > 0);

    let build = loadout
        .weapons
        .iter()
        .map(|(k, l)| format!("{} Lv{l}", k.def().name))
        .chain(loadout.passives.iter().map(|(k, l)| format!("{} Lv{l}", k.def().name)))
        .collect();

    Snapshot {
        minute: run.minute(),
        level: run.level,
        kills: run.kills,
        survivor_pos: pos,
        survivor_vel: motion.0,
        survivor_hp: health.current,
        survivor_max_hp: health.max,
        build,
        squads,
        unlocked: EnemyKind::unlocked(run.minute()).collect(),
    }
}

fn request_decisions(
    time: Res<Time>,
    run: Res<RunStats>,
    options: Res<LaunchOptions>,
    mut link: ResMut<jev::JevLink>,
    mut orders: ResMut<HordeOrders>,
    mut status: ResMut<DirectorStatus>,
    player: Single<(&Transform, &Motion, &Health, &Loadout), With<Player>>,
    enemies: Query<(&Enemy, &Transform), Without<Player>>,
) {
    let now = time.elapsed_secs();
    link.expire_if_stuck(now, &mut status);
    if !link.due(now) {
        return;
    }
    link.schedule_next(now);
    let snapshot = take_snapshot(&run, *player, &enemies);
    if snapshot.squads.is_empty() {
        return;
    }

    match (&options.jev_base_url, link.backing_off(now)) {
        (Some(base_url), false) => link.send(base_url, options.jev_api_key.as_deref(), snapshot, now),
        _ => {
            let decision = fallback::decide(&snapshot);
            apply(&decision, &snapshot, Source::Local, &mut orders, &mut status);
        }
    }
}

fn receive_decisions(
    time: Res<Time>,
    mut link: ResMut<jev::JevLink>,
    mut orders: ResMut<HordeOrders>,
    mut status: ResMut<DirectorStatus>,
) {
    let Some((snapshot, result)) = link.take_reply(time.elapsed_secs()) else {
        return;
    };
    match result {
        Ok(reply) => {
            status.successes += 1;
            status.consecutive_failures = 0;
            status.last_error = None;
            status.model = Some(reply.model);
            status.last_latency_ms = Some(reply.latency_ms);
            status.input_tokens += reply.input_tokens;
            apply(&reply.decision, &snapshot, Source::Jev, &mut orders, &mut status);
        }
        Err(error) => {
            status.failures += 1;
            status.consecutive_failures += 1;
            warn!("[director] Jev request failed: {error}");
            status.last_error = Some(error);
            let decision = fallback::decide(&snapshot);
            apply(&decision, &snapshot, Source::Local, &mut orders, &mut status);
        }
    }
}

fn apply(decision: &Decision, snapshot: &Snapshot, source: Source, orders: &mut HordeOrders, status: &mut DirectorStatus) {
    for squad in &snapshot.squads {
        orders.centroids[squad.sector] = squad.centroid;
    }
    for (sector, tactic) in &decision.tactics {
        orders.tactics[*sector] = *tactic;
    }
    orders.pressure = decision.pressure;
    orders.enraged = decision.enrage;
    orders.reinforcement = decision.reinforcement;

    status.source = source;
    status.squads = snapshot
        .squads
        .iter()
        .map(|s| SquadReport { sector: s.sector, count: s.count, tactic: orders.tactics[s.sector] })
        .collect();
    let squads: Vec<String> = status
        .squads
        .iter()
        .map(|s| format!("{}:{}({})", SECTOR_LABELS[s.sector], s.tactic.label(), s.count))
        .collect();
    status.last_summary = format!(
        "{} | pressure {} | enrage {} | send {}",
        squads.join(" "),
        decision.pressure,
        if decision.enrage { "yes" } else { "no" },
        decision.reinforcement.map_or("-", |k| k.def().label),
    );
    info!("[director] {} decided: {}", status.source_label(), status.last_summary);
}
