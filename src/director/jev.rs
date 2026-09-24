//! Wire client for Jev: `POST {base}/v1/systemone` with typed questions.
//! Requests run off the game loop via `ehttp` (a thread natively, `fetch` on
//! wasm); the reply lands in a shared inbox the director polls each frame.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use bevy::prelude::*;
use serde_json::{Map, Value, json};

use super::{Decision, SECTOR_LABELS, Snapshot, Tactic};
use crate::config::*;
use crate::director::DirectorStatus;
use crate::enemies::EnemyKind;

const BACKOFF_AFTER_FAILURES: u32 = 3;
const BACKOFF_SECS: f32 = 15.0;
const STUCK_GRACE_SECS: f32 = 2.0;
const MAX_PRESSURE: u8 = 3;
const ENRAGE_THRESHOLD: f64 = 0.5;
const ERROR_BODY_PREVIEW: usize = 160;

const GAME_BRIEF: &str = "Vampire-Survivors-style horde game. You are the director controlling an undead horde. \
The lone survivor auto-fires weapons (listed in their build) and levels up as they kill; your goal is to reach and \
overwhelm them before the 15 minute timer runs out. Coordinates are pixels, +x is east, +y is north. Squads are named \
by the side of the survivor they are on.";

const REINFORCE_TASK: &str = "Which enemy type should the horde send as reinforcements over the next few seconds? \
Counter the survivor's current build: e.g. fast units against slow single-target builds, tanky units against \
weak spray, swarms against slow heavy hitters, elites when the survivor is comfortable.";

const TACTIC_TASK: &str = "Pick the tactic this zombie squad should run for the next two seconds \
to maximize the chance of reaching the survivor. Consider squad size and composition, distance, how the survivor \
is moving, and what the other squads are doing (a coordinated pincer beats everyone rushing into gunfire).";

const PRESSURE_TASK: &str = "How hard should the horde press right now, i.e. how fast should new zombies pour in? \
Keep it fair: ease off when the survivor is nearly dead and push when they are healthy and comfortable.";

const PRESSURE_CRITERIA: [&str; 4] = [
    "Ease off: the survivor is overwhelmed or nearly dead.",
    "Steady: normal pacing.",
    "Press: the survivor is healthy and handling the current flow.",
    "Flood: the survivor is dominating; send everything.",
];

const ENRAGE_TASK: &str = "Should the whole horde enrage right now (a short burst of extra speed)? \
Say yes when a coordinated closing move is underway or the survivor is kiting too easily; otherwise no.";

#[derive(Resource)]
pub struct JevLink {
    next_due: f32,
    seq: u64,
    in_flight: Option<InFlight>,
    inbox: Arc<Mutex<Option<(u64, Result<Value, String>)>>>,
    consecutive_failures: u32,
    backoff_until: f32,
}

struct InFlight {
    seq: u64,
    sent_at: f32,
    snapshot: Snapshot,
}

pub struct Reply {
    pub decision: Decision,
    pub model: String,
    pub latency_ms: u32,
    pub input_tokens: u64,
}

impl Default for JevLink {
    fn default() -> Self {
        Self {
            next_due: 0.0,
            seq: 0,
            in_flight: None,
            inbox: Arc::new(Mutex::new(None)),
            consecutive_failures: 0,
            backoff_until: 0.0,
        }
    }
}

impl JevLink {
    pub fn reset_timer(&mut self) {
        self.next_due = 0.0;
    }

    pub fn due(&self, now: f32) -> bool {
        self.in_flight.is_none() && now >= self.next_due
    }

    pub fn schedule_next(&mut self, now: f32) {
        self.next_due = now + DIRECTOR_INTERVAL_SECS;
    }

    pub fn backing_off(&self, now: f32) -> bool {
        now < self.backoff_until
    }

    pub fn send(&mut self, base_url: &str, api_key: Option<&str>, snapshot: Snapshot, now: f32) {
        self.seq += 1;
        let seq = self.seq;
        let body = serde_json::to_vec(&build_request(&snapshot)).unwrap_or_default();
        let mut headers = vec![
            ("Content-Type".to_string(), "application/json".to_string()),
            ("Accept".to_string(), "application/json".to_string()),
        ];
        if let Some(key) = api_key {
            headers.push(("Authorization".to_string(), format!("Bearer {key}")));
        }
        let mut request = ehttp::Request::post(format!("{base_url}{SYSTEM_ONE_PATH}"), body);
        request.headers = ehttp::Headers { headers };
        request.timeout = Some(Duration::from_secs(DIRECTOR_TIMEOUT_SECS));

        let inbox = self.inbox.clone();
        ehttp::fetch(request, move |result| {
            let parsed = result.and_then(|response| {
                let text = response.text().unwrap_or_default().to_string();
                if !response.ok {
                    let preview: String = text.chars().take(ERROR_BODY_PREVIEW).collect();
                    return Err(format!("HTTP {} {}", response.status, preview));
                }
                serde_json::from_str::<Value>(&text).map_err(|e| format!("bad JSON: {e}"))
            });
            if let Ok(mut slot) = inbox.lock() {
                *slot = Some((seq, parsed));
            }
        });
        self.in_flight = Some(InFlight { seq, sent_at: now, snapshot });
    }

    /// The reply for the in-flight request, if it has arrived. Stale replies are dropped.
    pub fn take_reply(&mut self, now: f32) -> Option<(Snapshot, Result<Reply, String>)> {
        let (seq, result) = self.inbox.lock().ok()?.take()?;
        let in_flight = self.in_flight.take_if(|f| f.seq == seq)?;
        let latency_ms = ((now - in_flight.sent_at) * 1000.0) as u32;
        let reply = result.and_then(|body| parse_reply(&body, &in_flight.snapshot, latency_ms));
        self.record(reply.is_ok(), now);
        Some((in_flight.snapshot, reply))
    }

    /// Give up on a request the transport never resolved (e.g. a hung browser fetch).
    pub fn expire_if_stuck(&mut self, now: f32, status: &mut DirectorStatus) {
        let limit = DIRECTOR_TIMEOUT_SECS as f32 + STUCK_GRACE_SECS;
        if self.in_flight.take_if(|f| now - f.sent_at > limit).is_some() {
            status.failures += 1;
            status.last_error = Some("request never completed".to_string());
            self.record(false, now);
        }
    }

    fn record(&mut self, ok: bool, now: f32) {
        self.consecutive_failures = if ok { 0 } else { self.consecutive_failures + 1 };
        if self.consecutive_failures >= BACKOFF_AFTER_FAILURES {
            self.backoff_until = now + BACKOFF_SECS;
            self.consecutive_failures = 0;
        }
    }
}

fn squad_question(sector: usize) -> String {
    format!("tactic_{}", SECTOR_LABELS[sector])
}

pub fn build_request(snapshot: &Snapshot) -> Value {
    let round = |v: Vec2| [v.x.round() as i32, v.y.round() as i32];
    let squads: Vec<Value> = snapshot
        .squads
        .iter()
        .map(|s| {
            json!({
                "squad": SECTOR_LABELS[s.sector],
                "zombies": s.count,
                "composition": s.kinds.iter().map(|(k, n)| (k.def().label.to_string(), Value::from(*n))).collect::<Map<String, Value>>(),
                "center": round(s.centroid),
                "distance_px": s.distance.round(),
                "health_pct": s.health_pct,
            })
        })
        .collect();

    let tactic_criteria: Map<String, Value> =
        Tactic::ALL.iter().map(|t| (t.label().to_string(), Value::from(t.description()))).collect();
    let mut questions = Map::new();
    for squad in &snapshot.squads {
        questions.insert(
            squad_question(squad.sector),
            json!({
                "type": "choice",
                "instructions": { "task": TACTIC_TASK, "squad": SECTOR_LABELS[squad.sector] },
                "criteria": tactic_criteria,
            }),
        );
    }
    questions.insert(
        "pressure".into(),
        json!({ "type": "score", "instructions": PRESSURE_TASK, "criteria": PRESSURE_CRITERIA }),
    );
    questions.insert("enrage".into(), json!({ "type": "noul", "instructions": ENRAGE_TASK }));
    let reinforcements: Map<String, Value> =
        snapshot.unlocked.iter().map(|k| (k.def().label.to_string(), Value::from(k.def().pitch))).collect();
    if reinforcements.len() > 1 {
        questions.insert(
            "reinforcements".into(),
            json!({ "type": "choice", "instructions": REINFORCE_TASK, "criteria": reinforcements }),
        );
    }

    json!({
        "model": JEV_MODEL,
        "state": {
            "game": GAME_BRIEF,
            "minute": snapshot.minute,
            "survivor": {
                "position": round(snapshot.survivor_pos),
                "velocity": round(snapshot.survivor_vel),
                "hp": snapshot.survivor_hp.round(),
                "max_hp": snapshot.survivor_max_hp,
                "level": snapshot.level,
                "build": snapshot.build,
                "kills": snapshot.kills,
            },
            "squads": squads,
        },
        "questions": questions,
    })
}

fn parse_reply(body: &Value, snapshot: &Snapshot, latency_ms: u32) -> Result<Reply, String> {
    let answers = body.get("answers").ok_or("reply has no `answers`")?;
    let tactics = snapshot
        .squads
        .iter()
        .filter_map(|s| {
            let label = answers.get(squad_question(s.sector))?.get("choice")?.as_str()?;
            Some((s.sector, Tactic::from_label(label)?))
        })
        .collect::<Vec<_>>();
    if tactics.is_empty() {
        return Err("reply had no usable squad tactics".into());
    }
    let pressure = answers
        .get("pressure")
        .and_then(|a| a.get("score"))
        .and_then(Value::as_f64)
        .map_or(1, |s| (s.round().max(0.0) as u8).min(MAX_PRESSURE));
    let enrage = answers.get("enrage").and_then(|a| a.get("noul")).is_some_and(|v| {
        v.as_bool().unwrap_or_else(|| v.as_f64().is_some_and(|p| p >= ENRAGE_THRESHOLD))
    });

    let reinforcement = answers
        .get("reinforcements")
        .and_then(|a| a.get("choice"))
        .and_then(Value::as_str)
        .and_then(EnemyKind::from_label);

    Ok(Reply {
        decision: Decision { tactics, pressure, enrage, reinforcement },
        model: body.get("model").and_then(Value::as_str).unwrap_or(JEV_MODEL).to_string(),
        latency_ms,
        input_tokens: body.pointer("/usage/input_tokens").and_then(Value::as_u64).unwrap_or(0),
    })
}
