//! Serves the web build and proxies `POST /jev/v1/systemone` to TypeSafe,
//! adding `TYPESAFE_API_KEY` server-side so the key never reaches the browser.
//!
//! Usage: serve [--port 8080] [--root web/dist] [--mock-jev]
//! `--mock-jev` answers Jev requests locally (for offline testing of the wire path).

use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tiny_http::{Header, Method, Request, Response, Server};

const DEFAULT_PORT: u16 = 8080;
const DEFAULT_ROOT: &str = "web/dist";
const DEFAULT_UPSTREAM: &str = "https://api.typesafe.ai";
const JEV_ROUTE: &str = "/jev/v1/systemone";
const UPSTREAM_PATH: &str = "/v1/systemone";
const UPSTREAM_TIMEOUT: Duration = Duration::from_secs(10);
const MOCK_MODEL: &str = "mock-jev";

struct Config {
    port: u16,
    root: PathBuf,
    mock: bool,
    upstream: String,
    api_key: Option<String>,
}

fn main() {
    let config = parse_args();
    let server = Server::http(("127.0.0.1", config.port)).unwrap_or_else(|e| panic!("bind :{}: {e}", config.port));
    let jev_mode = match (config.mock, config.api_key.is_some()) {
        (true, _) => "MOCK (local answers)".to_string(),
        (false, true) => format!("proxy → {}{UPSTREAM_PATH}", config.upstream),
        (false, false) => "UNAVAILABLE (no TYPESAFE_API_KEY; game will use its local brain)".to_string(),
    };
    println!("[serve] http://127.0.0.1:{}/  root={}  jev={jev_mode}", config.port, config.root.display());

    let mut counter: u64 = 0;
    for request in server.incoming_requests() {
        counter += 1;
        let result = if request.method() == &Method::Post && request.url() == JEV_ROUTE {
            handle_jev(request, &config, counter)
        } else {
            handle_static(request, &config.root)
        };
        if let Err(e) = result {
            eprintln!("[serve] response error: {e}");
        }
    }
}

fn parse_args() -> Config {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let value_of = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned();
    let env = |name: &str| std::env::var(name).ok().filter(|v| !v.trim().is_empty());
    Config {
        port: value_of("--port").and_then(|p| p.parse().ok()).unwrap_or(DEFAULT_PORT),
        root: PathBuf::from(value_of("--root").unwrap_or_else(|| DEFAULT_ROOT.to_string())),
        mock: args.iter().any(|a| a == "--mock-jev"),
        upstream: env("TYPESAFE_BASE_URL").unwrap_or_else(|| DEFAULT_UPSTREAM.to_string()),
        api_key: env("TYPESAFE_API_KEY"),
    }
}

fn json_response(status: u16, body: String) -> Response<std::io::Cursor<Vec<u8>>> {
    Response::from_data(body.into_bytes()).with_status_code(status).with_header(header("Content-Type", "application/json"))
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("static header is valid")
}

fn handle_jev(mut request: Request, config: &Config, counter: u64) -> std::io::Result<()> {
    let started = Instant::now();
    let mut body = String::new();
    request.as_reader().read_to_string(&mut body)?;
    let questions = serde_json::from_str::<Value>(&body)
        .ok()
        .and_then(|v| v.get("questions").and_then(Value::as_object).map(|q| q.len()))
        .unwrap_or(0);

    let (status, reply) = if config.mock {
        mock_answer(&body, counter)
    } else if let Some(key) = &config.api_key {
        forward(&config.upstream, key, &body)
    } else {
        (503, json!({ "error": "serve has no TYPESAFE_API_KEY; restart it with the key or --mock-jev" }).to_string())
    };
    println!("[serve] jev #{counter} questions={questions} -> {status} in {}ms", started.elapsed().as_millis());
    request.respond(json_response(status, reply))
}

fn forward(upstream: &str, key: &str, body: &str) -> (u16, String) {
    let agent = ureq::AgentBuilder::new().timeout(UPSTREAM_TIMEOUT).build();
    let result = agent
        .post(&format!("{upstream}{UPSTREAM_PATH}"))
        .set("Authorization", &format!("Bearer {key}"))
        .set("Content-Type", "application/json")
        .set("Accept", "application/json")
        .send_string(body);
    match result {
        Ok(response) => (response.status(), response.into_string().unwrap_or_default()),
        Err(ureq::Error::Status(code, response)) => (code, response.into_string().unwrap_or_default()),
        Err(e) => (502, json!({ "error": format!("upstream unreachable: {e}") }).to_string()),
    }
}

/// Deterministic, varied fake answers in the System One reply shape.
fn mock_answer(body: &str, counter: u64) -> (u16, String) {
    let Ok(request) = serde_json::from_str::<Value>(body) else {
        return (400, json!({ "error": "body is not JSON" }).to_string());
    };
    let Some(questions) = request.get("questions").and_then(Value::as_object) else {
        return (422, json!({ "error": "missing questions" }).to_string());
    };
    let answers: serde_json::Map<String, Value> = questions
        .iter()
        .enumerate()
        .map(|(i, (name, q))| {
            let pick = counter as usize + i;
            let answer = match q.get("type").and_then(Value::as_str) {
                Some("choice") => {
                    let labels: Vec<&String> = q.get("criteria").and_then(Value::as_object).map(|c| c.keys().collect()).unwrap_or_default();
                    json!({ "choice": labels.get(pick % labels.len().max(1)).map(|s| s.as_str()).unwrap_or("rush") })
                }
                Some("score") => json!({ "score": (pick % 3) as f64 + 0.5 }),
                _ => json!({ "noul": if pick % 4 == 0 { 0.8 } else { 0.2 } }),
            };
            (name.clone(), answer)
        })
        .collect();
    (200, json!({ "answers": answers, "model": MOCK_MODEL, "usage": { "input_tokens": body.len() / 4 } }).to_string())
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript",
        "wasm" => "application/wasm",
        "png" => "image/png",
        "ogg" => "audio/ogg",
        "wav" => "audio/wav",
        "ttf" => "font/ttf",
        "json" => "application/json",
        "css" => "text/css",
        _ => "application/octet-stream",
    }
}

fn handle_static(request: Request, root: &Path) -> std::io::Result<()> {
    let url_path = request.url().split(['?', '#']).next().unwrap_or("/");
    let relative = Path::new(url_path.trim_start_matches('/'));
    if relative.components().any(|c| !matches!(c, Component::Normal(_))) {
        return request.respond(Response::from_string("bad path").with_status_code(400));
    }
    let mut path = root.join(relative);
    if path.is_dir() {
        path = path.join("index.html");
    }
    match fs::read(&path) {
        Ok(bytes) => request.respond(
            Response::from_data(bytes)
                .with_header(header("Content-Type", content_type(&path)))
                .with_header(header("Cache-Control", "no-cache")),
        ),
        Err(_) => request.respond(Response::from_string("not found").with_status_code(404)),
    }
}
