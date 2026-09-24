//! Local heuristic brain: answers the same questions as Jev when Jev is
//! unconfigured, backing off after failures, or a request fails.

use super::{Decision, Snapshot, Tactic};
use crate::enemies::{EnemyKind, Role};

const BIG_SQUAD: usize = 5;
const FAR_PX: f32 = 380.0;
const FAST_SURVIVOR: f32 = 150.0;
const LOW_HP_FRACTION: f32 = 0.35;
const HIGH_HP_FRACTION: f32 = 0.8;
const CROWDED_FIELD: usize = 25;

pub fn decide(snapshot: &Snapshot) -> Decision {
    let moving_fast = snapshot.survivor_vel.length() > FAST_SURVIVOR;
    let tactics = snapshot
        .squads
        .iter()
        .map(|s| {
            let tactic = match () {
                _ if s.count >= BIG_SQUAD && s.distance > FAR_PX => Tactic::Surround,
                _ if s.count <= 2 && s.distance > FAR_PX && snapshot.squads.len() > 1 => Tactic::Regroup,
                _ if moving_fast => Tactic::Intercept,
                _ if s.distance > FAR_PX && s.sector % 2 == 0 => Tactic::FlankLeft,
                _ if s.distance > FAR_PX => Tactic::FlankRight,
                _ => Tactic::Rush,
            };
            (s.sector, tactic)
        })
        .collect();

    let hp = snapshot.survivor_hp / snapshot.survivor_max_hp;
    let alive: usize = snapshot.squads.iter().map(|s| s.count).sum();
    let pressure = match () {
        _ if hp < LOW_HP_FRACTION => 0,
        _ if alive > CROWDED_FIELD => 1,
        _ if hp > HIGH_HP_FRACTION => 3,
        _ => 2,
    };
    let enrage = moving_fast && snapshot.squads.len() >= 3 && hp > LOW_HP_FRACTION;

    // Send the fastest unit at a kiting survivor, the toughest elite at a comfortable one, else swarm.
    let reinforcement = if moving_fast {
        snapshot.unlocked.iter().copied().max_by(|a, b| a.def().speed.total_cmp(&b.def().speed))
    } else if hp > HIGH_HP_FRACTION {
        snapshot.unlocked.iter().copied().find(|k| k.def().role == Role::Elite)
    } else {
        Some(EnemyKind::TinyZombie)
    };

    Decision { tactics, pressure, enrage, reinforcement }
}
