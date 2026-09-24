# Zombie Survivor — Jev Horde

A Vampire-Survivors-style horde game in Rust + [Bevy 0.19](https://bevyengine.org), native and WebAssembly.
Survive 15 minutes against an undead horde, level up, and pick your build. Bosses arrive at 5:00 and 10:00.

**Play in the browser:** https://tomerab1.github.io/zombie-survivor/

The horde is run by a **Horde Director**. Every two seconds it snapshots the fight and asks
[Jev](https://typesafe.ai) (TypeSafe's System One API) a batch of typed questions:

| Question | Type | Effect |
|---|---|---|
| `tactic_<SQUAD>` | choice | rush / flank_left / flank_right / surround / intercept / regroup for each squad, where squads are the six sectors around you |
| `pressure` | score 0–3 | spawn pacing |
| `enrage` | noul (yes/no) | a horde-wide speed burst |
| `reinforcements` | choice | which unlocked enemy type to send, chosen to counter your build |

If Jev isn't configured or a call fails, a local heuristic brain answers the same questions. The HUD panel (Tab)
shows which brain is active and what each squad is doing. The GitHub Pages build always uses the local brain,
because static hosting can't keep an API key secret.

## Controls

- **WASD / arrows**: move
- **Weapons fire automatically.** Hold the **left mouse button** to aim knives/arrows at the cursor.
- **1 / 2 / 3** or click: pick a level-up card
- **Tab**: toggle the Horde Director panel · **O**: squad tactics overlay
- **R / M**: retry / menu after a run

## Content

- **Heroes:** Knight (knives), Elf (bow), Wizard (homing bolts), Lizard (orbiting axes)
- **Weapons (5 levels each):** Throwing Knives, Longbow, Hex Staff, Orbit Axes, Blight Ward (aura),
  Storm Staff (chain lightning), Quake Hammer (shockwave)
- **Passives:** Might, Vitality, Swiftness, Haste, Reach, Magnet, Armor, Regen, Duplicator
- **Enemies:** tiny zombies, zombies, skeletons, swampies, ice zombies (their touch chills you), chorts,
  big zombies and ogres (elites that drop chests), necromancers (summoners), and the big demon boss
- **Drops:** XP gems, coins, flasks (heal / magnet / bomb), treasure chests, weapons lying on the floor,
  breakable crates
- **Shaders (WGSL):** glow (bolts, gems), animated rings (aura, shockwaves, drops), flickering lightning,
  and a screen vignette with a red hurt flash, a low-HP heartbeat, and a gold level-up bloom

## Run it

```sh
# Native. Set TYPESAFE_API_KEY to let Jev direct the horde; without it you get the local brain.
cargo run --release

# Web build + local server. `serve` hosts web/dist and proxies /jev to TypeSafe,
# so the key never reaches the browser.
./build-web.sh
TYPESAFE_API_KEY=... cargo run -p serve --release      # http://127.0.0.1:8080
cargo run -p serve -- --mock-jev                       # offline, fake Jev answers
```

### Verification hooks (native)

`ZS_AUTOPLAY=1` lets a bot play, pick level-ups and restart after dying. `ZS_EXIT_AFTER=<secs>` quits,
`ZS_SCREENSHOT=<png>` saves a screenshot just before quitting, `ZS_SPEED=<x>` fast-forwards the simulation,
and `ZS_NO_JEV=1` forces the local brain. Every 2 seconds the game logs a `[telemetry]` line
(time, level, kills, HP, build, and the director's last decisions).

## Credits

- Characters, monsters, weapons, props: **0x72 — DungeonTileset II** (CC0), https://0x72.itch.io/dungeontileset-ii
- Sound effects and cave ambience: **TomMusic — Free Fantasy 200 SFX Pack** (royalty-free),
  https://tommusic.itch.io/free-fantasy-200-sfx-pack
- UI/pickup sounds and fonts: **Kenney** (CC0), https://kenney.nl
