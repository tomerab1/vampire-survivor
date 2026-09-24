//! Launch options (env vars natively, `?query` params on the web) plus the
//! automation hooks used for end-to-end verification: autoplay, screenshots,
//! periodic telemetry lines and a timed exit.

use bevy::prelude::*;

use crate::AppState;
use crate::director::DirectorStatus;
use crate::enemies::Enemy;
use crate::hero::{Health, Loadout, Player};
use crate::progression::RunStats;

const TELEMETRY_INTERVAL_SECS: f32 = 2.0;
#[cfg(not(target_arch = "wasm32"))]
const DEFAULT_SEED: u64 = 0x5eed_2026;

#[derive(Resource, Clone, Debug, Default)]
pub struct LaunchOptions {
    /// A bot drives the survivor (kites and shoots) and restarts after death.
    pub autoplay: bool,
    /// Skip the title screen.
    pub skip_menu: bool,
    /// Quit after this many seconds (native only).
    #[cfg(not(target_arch = "wasm32"))]
    pub exit_after: Option<f32>,
    /// Save a screenshot at `exit_after - 1s` to this path (native only).
    #[cfg(not(target_arch = "wasm32"))]
    pub screenshot: Option<String>,
    pub seed: u64,
    /// Simulation speed multiplier (native `ZS_SPEED`), for fast-forwarding verification runs.
    pub speed: f32,
    /// Extra starting weapons for testing (native `ZS_GRANT=Lightning,Aura`).
    pub grant: Vec<String>,
    /// Jev endpoint root; `None` disables Jev and uses the local fallback brain.
    pub jev_base_url: Option<String>,
    pub jev_api_key: Option<String>,
    pub jev_disabled_reason: Option<String>,
}

impl LaunchOptions {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn detect() -> Self {
        use crate::config::JEV_DEFAULT_BASE_URL;

        let env = |name: &str| std::env::var(name).ok().filter(|v| !v.trim().is_empty());
        let flag = |name: &str| env(name).is_some_and(|v| v != "0");
        let api_key = env("TYPESAFE_API_KEY");
        let jev_off = flag("ZS_NO_JEV");
        let jev_disabled_reason = match (jev_off, api_key.is_some()) {
            (true, _) => Some("disabled by ZS_NO_JEV".to_string()),
            (false, false) => Some("TYPESAFE_API_KEY not set".to_string()),
            (false, true) => None,
        };
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(DEFAULT_SEED, |d| d.as_nanos() as u64);

        Self {
            autoplay: flag("ZS_AUTOPLAY"),
            skip_menu: flag("ZS_AUTOPLAY") || flag("ZS_SKIP_MENU"),
            exit_after: env("ZS_EXIT_AFTER").and_then(|v| v.parse().ok()),
            screenshot: env("ZS_SCREENSHOT"),
            seed: env("ZS_SEED").and_then(|v| v.parse().ok()).unwrap_or(seed),
            speed: env("ZS_SPEED").and_then(|v| v.parse().ok()).unwrap_or(1.0),
            grant: env("ZS_GRANT").map(|v| v.split(',').map(str::to_string).collect()).unwrap_or_default(),
            jev_base_url: jev_disabled_reason
                .is_none()
                .then(|| env("TYPESAFE_BASE_URL").unwrap_or_else(|| JEV_DEFAULT_BASE_URL.to_string())),
            jev_api_key: api_key,
            jev_disabled_reason,
        }
    }

    /// On the web the API key lives in the `serve` proxy, never in the page.
    #[cfg(target_arch = "wasm32")]
    pub fn detect() -> Self {
        use crate::config::JEV_WEB_PROXY_PATH;

        let query = web_sys::window()
            .and_then(|w| w.location().search().ok())
            .unwrap_or_default();
        let has = |name: &str| query.trim_start_matches('?').split('&').any(|kv| kv == name);
        let host = web_sys::window().and_then(|w| w.location().hostname().ok()).unwrap_or_default();
        // The Jev proxy only exists where `serve` hosts the game (it holds the API key);
        // static hosts like GitHub Pages fall back to the local brain.
        let local_host = matches!(host.as_str(), "localhost" | "127.0.0.1");
        let jev_disabled_reason = match (has("nojev"), local_host || has("jev")) {
            (true, _) => Some("disabled by ?nojev".to_string()),
            (false, false) => Some("static hosting (no Jev proxy)".to_string()),
            (false, true) => None,
        };

        Self {
            autoplay: has("autoplay"),
            skip_menu: has("autoplay") || has("play"),
            seed: js_sys::Date::now() as u64,
            speed: 1.0,
            grant: query
                .trim_start_matches('?')
                .split('&')
                .find_map(|kv| kv.strip_prefix("grant="))
                .map(|v| v.split(',').map(str::to_string).collect())
                .unwrap_or_default(),
            jev_base_url: jev_disabled_reason.is_none().then(|| JEV_WEB_PROXY_PATH.to_string()),
            jev_api_key: None,
            jev_disabled_reason,
        }
    }
}

pub struct LaunchPlugin;

impl Plugin for LaunchPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, apply_speed)
            .add_systems(Update, telemetry.run_if(in_state(AppState::Playing)));

        #[cfg(not(target_arch = "wasm32"))]
        app.add_systems(Update, timed_exit_and_screenshot);
    }
}

fn apply_speed(options: Res<LaunchOptions>, mut time: ResMut<Time<Virtual>>) {
    const MAX_DELTA_SECS: f32 = 0.25;
    if options.speed != 1.0 {
        time.set_relative_speed(options.speed);
        time.set_max_delta(std::time::Duration::from_secs_f32(MAX_DELTA_SECS * options.speed));
    }
}

/// One grep-able line every couple of seconds so runs can be verified from logs.
fn telemetry(
    time: Res<Time>,
    mut since: Local<f32>,
    run: Res<RunStats>,
    director: Res<DirectorStatus>,
    player: Single<(&Health, &Loadout), With<Player>>,
    enemies: Query<(), With<Enemy>>,
) {
    *since += time.delta_secs();
    if *since < TELEMETRY_INTERVAL_SECS {
        return;
    }
    *since = 0.0;
    let (health, loadout) = *player;
    let weapons: Vec<String> = loadout.weapons.iter().map(|(k, l)| format!("{:?}{l}", k)).collect();
    let passives: Vec<String> = loadout.passives.iter().map(|(k, l)| format!("{:?}{l}", k)).collect();
    info!(
        "[telemetry] t={:.0}s lvl={} kills={} hp={:.0}/{:.0} alive={} weapons=[{}] passives=[{}] director={} model={} ok={} fail={} last=[{}]",
        run.elapsed,
        run.level,
        run.kills,
        health.current,
        health.max,
        enemies.iter().count(),
        weapons.join(" "),
        passives.join(" "),
        director.source_label(),
        director.model.as_deref().unwrap_or("-"),
        director.successes,
        director.failures,
        director.last_summary,
    );
}

#[cfg(not(target_arch = "wasm32"))]
fn timed_exit_and_screenshot(
    mut commands: Commands,
    time: Res<Time>,
    options: Res<LaunchOptions>,
    mut shot_taken: Local<bool>,
    mut exit: MessageWriter<AppExit>,
) {
    use bevy::render::view::screenshot::{Screenshot, save_to_disk};
    const SCREENSHOT_LEAD_SECS: f32 = 1.0;

    let Some(exit_after) = options.exit_after else {
        return;
    };
    let now = time.elapsed_secs();
    if let Some(path) = &options.screenshot
        && !*shot_taken
        && now >= exit_after - SCREENSHOT_LEAD_SECS
    {
        *shot_taken = true;
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path.clone()));
    }
    if now >= exit_after {
        info!("[telemetry] exiting after {exit_after}s");
        exit.write(AppExit::Success);
    }
}
