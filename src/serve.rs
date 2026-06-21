//! Live preview server — `cadspec serve`.
//!
//! Watches the project files and serves an auto-reloading SVG preview in the
//! browser. The vibecoding loop: an agent (or human) edits `.cf` files, the
//! browser refreshes instantly, build errors show as an overlay.
//!
//! Viewer features: pan/zoom, click-to-inspect any entity (shows its source
//! TOML block, copyable for targeted agent edits), per-layer visibility with
//! a ghost mode for tracing over other floors, and an extruded 3D view.
//!
//! Plain `std::net` HTTP — this is a localhost dev server, no framework needed.

use crate::parser::parse_project;
use crate::render3d::render_scene_3d;
use crate::svg::{layer_display_color, load_project_layers, render_scene_from};
use anyhow::{bail, Context, Result};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

const SVG_WIDTH: u32 = 1600;
const DEBOUNCE: Duration = Duration::from_millis(80);
const SSE_KEEPALIVE: Duration = Duration::from_secs(15);

struct LiveState {
    /// Arc so request handlers serve the SVG without copying it.
    svg: Arc<String>,
    /// Extruded axonometric 3D render of the same scene.
    svg3d: Arc<String>,
    error: Option<String>,
    version: u64,
    project_name: String,
    /// (name, color) per layer, for the layer panel.
    layers: Vec<(String, String)>,
    /// (name, view, title) per plano, for the planos panel.
    planos: Vec<(String, String, String)>,
}

/// Shared state plus a condvar so SSE clients are woken the instant a rebuild
/// lands, instead of polling.
struct Live {
    state: Mutex<LiveState>,
    changed: Condvar,
    project_dir: PathBuf,
}

type Shared = Arc<Live>;

/// Start the live preview server (blocks until killed).
pub fn serve_project(project_dir: &Path, port: u16, open: bool) -> Result<()> {
    let project = parse_project(&project_dir.join("project.toml"))?;
    let project_dir = project_dir
        .canonicalize()
        .unwrap_or_else(|_| project_dir.to_path_buf());

    let state: Shared = Arc::new(Live {
        state: Mutex::new(LiveState {
            svg: Arc::new(String::new()),
            svg3d: Arc::new(String::new()),
            error: None,
            version: 0,
            project_name: project.project.name.clone(),
            layers: Vec::new(),
            planos: Vec::new(),
        }),
        changed: Condvar::new(),
        project_dir: project_dir.clone(),
    });

    rebuild(&project_dir, &state);

    let listener = TcpListener::bind(("127.0.0.1", port))
        .with_context(|| format!("Cannot bind 127.0.0.1:{} (port in use?)", port))?;
    let url = format!("http://127.0.0.1:{}", port);

    println!("◉ cadspec serve — {}", project.project.name);
    println!("  Preview: {}", url);
    println!("  Watching: {}", project_dir.display());
    println!();
    println!("  Edit .cf files — the browser updates automatically.");
    println!("  Click an entity in the viewer to inspect/copy its TOML.");
    println!("  Press Ctrl+C to stop.");

    spawn_watcher(project_dir.clone(), Arc::clone(&state))?;

    if open {
        open_browser(&url);
    }

    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let state = Arc::clone(&state);
        std::thread::spawn(move || {
            let _ = handle_connection(stream, &state);
        });
    }
    Ok(())
}

// ── Background daemon ──────────────────────────────────────────────────────
//
// `serve` runs detached by default: a parent process validates the project and
// the port, spawns the real (foreground) server in its own process group with
// its output redirected to a log file, waits until the port actually accepts a
// connection, then prints the URL and exits. Waiting for real readiness means
// we never claim "running" for a server that failed to come up.

fn runtime_dir(project_dir: &Path) -> PathBuf {
    project_dir.join(".cadspec")
}

fn pid_path(project_dir: &Path) -> PathBuf {
    runtime_dir(project_dir).join("serve.pid")
}

fn log_path(project_dir: &Path) -> PathBuf {
    runtime_dir(project_dir).join("serve.log")
}

/// True if `pid` refers to a live process (`kill -0`).
fn process_alive(pid: u32) -> bool {
    Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// The recorded daemon pid for this project, but only if it is still alive.
fn running_pid(project_dir: &Path) -> Option<u32> {
    let pid: u32 = fs::read_to_string(pid_path(project_dir))
        .ok()?
        .trim()
        .parse()
        .ok()?;
    process_alive(pid).then_some(pid)
}

/// Block until the server accepts a connection on `port`, or `timeout` elapses.
fn wait_until_ready(port: u16, timeout: Duration) -> bool {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

/// Start the live preview server detached in the background (the default).
pub fn serve_daemon(project_dir: &Path, port: u16, open: bool) -> Result<()> {
    // Validate the project up front so config errors surface here, not in a log.
    parse_project(&project_dir.join("project.toml"))?;
    let project_dir = project_dir
        .canonicalize()
        .unwrap_or_else(|_| project_dir.to_path_buf());
    let url = format!("http://127.0.0.1:{}", port);

    if let Some(pid) = running_pid(&project_dir) {
        println!("◉ cadspec serve already running (pid {pid})");
        println!("  Preview: {url}");
        println!("  Stop with: cadspec serve --stop");
        if open {
            open_browser(&url);
        }
        return Ok(());
    }

    // Fail fast on a busy port instead of letting the detached child die quietly.
    match TcpListener::bind(("127.0.0.1", port)) {
        Ok(listener) => drop(listener),
        Err(e) => bail!("Cannot bind 127.0.0.1:{port} (port in use?): {e}"),
    }

    fs::create_dir_all(runtime_dir(&project_dir))?;
    let log = log_path(&project_dir);
    let log_file = File::create(&log)?;

    let exe = std::env::current_exe().context("cannot locate cadspec executable")?;
    let mut cmd = Command::new(exe);
    cmd.arg("serve")
        .arg("--foreground")
        .arg("--path")
        .arg(&project_dir)
        .arg("--port")
        .arg(port.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::from(log_file.try_clone()?))
        .stderr(Stdio::from(log_file));
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Own process group: survives the parent shell / agent command exiting.
        cmd.process_group(0);
    }
    let child = cmd.spawn().context("failed to spawn background server")?;
    let pid = child.id();
    fs::write(pid_path(&project_dir), pid.to_string())?;

    if wait_until_ready(port, Duration::from_secs(5)) {
        println!("◉ cadspec serve — running in background (pid {pid})");
        println!("  Preview: {url}");
        println!("  Logs:    {}", log.display());
        println!("  Stop with: cadspec serve --stop");
        if open {
            open_browser(&url);
        }
        Ok(())
    } else {
        let _ = fs::remove_file(pid_path(&project_dir));
        let tail = fs::read_to_string(&log).unwrap_or_default();
        bail!(
            "server did not come up within 5s. Log:\n{}",
            tail.trim_end()
        );
    }
}

/// Stop the background server running for this project.
pub fn serve_stop(project_dir: &Path, _port: u16) -> Result<()> {
    let project_dir = project_dir
        .canonicalize()
        .unwrap_or_else(|_| project_dir.to_path_buf());
    let pid_file = pid_path(&project_dir);

    let Some(pid) = running_pid(&project_dir) else {
        let _ = fs::remove_file(&pid_file); // clean up any stale pidfile
        println!("No cadspec serve daemon running for this project.");
        return Ok(());
    };

    let stopped = Command::new("kill")
        .arg(pid.to_string())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    let _ = fs::remove_file(&pid_file);

    if stopped {
        println!("✓ Stopped cadspec serve (pid {pid}).");
        Ok(())
    } else {
        bail!("failed to stop process {pid}")
    }
}

fn rebuild(project_dir: &Path, state: &Shared) {
    type PlanoInfo = Vec<(String, String, String)>;
    type Built = (String, String, Vec<(String, String)>, PlanoInfo);
    let result = (|| -> Result<Built> {
        let (project, layers) = load_project_layers(project_dir, None)?;
        let scene = render_scene_from(
            &project.project.name,
            &project.project.units,
            &layers,
            SVG_WIDTH,
            &[],
        );
        let scene3d = render_scene_3d(&layers, SVG_WIDTH);
        let layer_info = layers
            .iter()
            .enumerate()
            .map(|(i, (name, cf))| (name.clone(), layer_display_color(cf, i)))
            .collect();
        let plano_info = project
            .planos
            .iter()
            .map(|p| {
                (
                    p.name.clone(),
                    p.view.clone(),
                    p.title.clone().unwrap_or_else(|| p.name.clone()),
                )
            })
            .collect();
        Ok((scene.svg, scene3d.svg, layer_info, plano_info))
    })();

    let mut st = state.state.lock().unwrap();
    match result {
        Ok((svg, svg3d, layers, planos)) => {
            st.svg = Arc::new(svg);
            st.svg3d = Arc::new(svg3d);
            st.layers = layers;
            st.planos = planos;
            st.error = None;
        }
        Err(e) => {
            st.error = Some(format!("{:#}", e));
        }
    }
    st.version += 1;
    drop(st);
    state.changed.notify_all();
}

/// Build the scene's 3D solids as a glTF document (for the WebGL viewer).
fn scene_gltf(project_dir: &Path) -> Result<String> {
    let (_project, layers) = load_project_layers(project_dir, None)?;
    let meshes = crate::render3d::scene_meshes(&layers);
    Ok(crate::gltf::scene_to_gltf(&meshes))
}

/// Render a plano by name to SVG (on demand, for the `/plano.svg` endpoint).
fn render_named_plano(project_dir: &Path, name: &str) -> Result<String> {
    let project = parse_project(&project_dir.join("project.toml"))?;
    let plano = project
        .planos
        .iter()
        .find(|p| p.name == name)
        .with_context(|| format!("no plano named '{name}'"))?;
    Ok(crate::planos::render_plano(project_dir, plano, SVG_WIDTH)?.svg)
}

fn spawn_watcher(project_dir: PathBuf, state: Shared) -> Result<()> {
    let (tx, rx) = mpsc::channel();
    let mut watcher = RecommendedWatcher::new(
        move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                let _ = tx.send(event);
            }
        },
        notify::Config::default(),
    )?;
    watcher.watch(&project_dir, RecursiveMode::NonRecursive)?;

    std::thread::spawn(move || {
        // Keep the watcher alive inside the thread.
        let _watcher = watcher;
        while let Ok(event) = rx.recv() {
            if !is_relevant(&event) {
                continue;
            }
            // Debounce: absorb the burst of events an editor save produces.
            std::thread::sleep(DEBOUNCE);
            while rx.try_recv().is_ok() {}

            rebuild(&project_dir, &state);
            let st = state.state.lock().unwrap();
            match &st.error {
                None => println!("⟳ rebuilt (v{})", st.version),
                Some(e) => println!("✗ build error (v{}): {}", st.version, e),
            }
        }
    });
    Ok(())
}

fn is_relevant(event: &Event) -> bool {
    match event.kind {
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => {}
        _ => return false,
    }
    event.paths.iter().any(|p| {
        p.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e == "cf" || e == "toml")
    })
}

// ── HTTP ────────────────────────────────────────────────────────────────

fn handle_connection(stream: TcpStream, state: &Shared) -> std::io::Result<()> {
    // Small localhost responses: Nagle's algorithm only adds latency here.
    let _ = stream.set_nodelay(true);
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;

    let target = request_line.split_whitespace().nth(1).unwrap_or("/");
    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p, q),
        None => (target, ""),
    };

    match path {
        "/" => {
            let html = index_html(&state.state.lock().unwrap().project_name);
            respond(
                stream,
                "200 OK",
                "text/html; charset=utf-8",
                html.as_bytes(),
            )
        }
        "/preview.svg" => {
            let svg = Arc::clone(&state.state.lock().unwrap().svg);
            respond(stream, "200 OK", "image/svg+xml", svg.as_bytes())
        }
        "/preview3d.svg" => {
            let svg = Arc::clone(&state.state.lock().unwrap().svg3d);
            respond(stream, "200 OK", "image/svg+xml", svg.as_bytes())
        }
        "/scene.gltf" => {
            let body =
                scene_gltf(&state.project_dir).unwrap_or_else(|_| crate::gltf::scene_to_gltf(&[]));
            respond(stream, "200 OK", "model/gltf+json", body.as_bytes())
        }
        "/plano.svg" => {
            // Rendered on demand (sections run CSG, so we don't precompute all).
            let name = query_param(query, "name").unwrap_or_default();
            let dir = &state.project_dir;
            let svg = render_named_plano(dir, &name).unwrap_or_else(|e| {
                format!(
                    r##"<svg xmlns="http://www.w3.org/2000/svg" width="800" height="200"><rect width="100%" height="100%" fill="#0d0d0d"/><text x="20" y="40" fill="#ff9f9a" font-family="monospace" font-size="14">plano error: {}</text></svg>"##,
                    html_escape(&format!("{e:#}"))
                )
            });
            respond(stream, "200 OK", "image/svg+xml", svg.as_bytes())
        }
        "/state" => {
            let st = state.state.lock().unwrap();
            let layers: Vec<_> = st
                .layers
                .iter()
                .map(|(name, color)| serde_json::json!({"name": name, "color": color}))
                .collect();
            let planos: Vec<_> = st
                .planos
                .iter()
                .map(|(name, view, title)| {
                    serde_json::json!({"name": name, "view": view, "title": title})
                })
                .collect();
            let body = serde_json::json!({
                "version": st.version,
                "project": st.project_name,
                "error": st.error,
                "layers": layers,
                "planos": planos,
            })
            .to_string();
            drop(st);
            respond(stream, "200 OK", "application/json", body.as_bytes())
        }
        "/entity" => {
            let id = query_param(query, "id").unwrap_or_default();
            let body = entity_block_json(&state.project_dir, &id);
            respond(stream, "200 OK", "application/json", body.as_bytes())
        }
        // ── Built-in .cf editor ────────────────────────────────────────────
        "/files" => {
            let mut names: Vec<String> = std::fs::read_dir(&state.project_dir)
                .map(|rd| {
                    rd.filter_map(|e| e.ok())
                        .map(|e| e.file_name().to_string_lossy().into_owned())
                        .filter(|n| n.ends_with(".cf"))
                        .collect()
                })
                .unwrap_or_default();
            names.sort();
            let body = serde_json::json!({ "files": names }).to_string();
            respond(stream, "200 OK", "application/json", body.as_bytes())
        }
        "/file" => {
            let name = query_param(query, "name").unwrap_or_default();
            match safe_cf_name(&name) {
                Some(n) => match std::fs::read_to_string(state.project_dir.join(&n)) {
                    Ok(s) => respond(stream, "200 OK", "text/plain; charset=utf-8", s.as_bytes()),
                    Err(_) => respond(stream, "404 Not Found", "text/plain", b"not found"),
                },
                None => respond(stream, "400 Bad Request", "text/plain", b"bad name"),
            }
        }
        "/save" => {
            let name = query_param(query, "name").unwrap_or_default();
            let body = read_request_body(&mut reader);
            match safe_cf_name(&name) {
                Some(n) => match std::fs::write(state.project_dir.join(&n), &body) {
                    Ok(()) => {
                        rebuild(&state.project_dir, state);
                        let st = state.state.lock().unwrap();
                        let resp = serde_json::json!({
                            "ok": st.error.is_none(),
                            "version": st.version,
                            "error": st.error,
                        })
                        .to_string();
                        drop(st);
                        respond(stream, "200 OK", "application/json", resp.as_bytes())
                    }
                    Err(e) => respond(
                        stream,
                        "500 Internal Server Error",
                        "text/plain",
                        format!("write error: {e}").as_bytes(),
                    ),
                },
                None => respond(stream, "400 Bad Request", "text/plain", b"bad name"),
            }
        }
        "/events" => serve_events(stream, state),
        _ => respond(stream, "404 Not Found", "text/plain", b"not found"),
    }
}

/// Accept only a bare `*.cf` filename (no path traversal) for the editor.
fn safe_cf_name(name: &str) -> Option<String> {
    if name.is_empty()
        || name.contains('/')
        || name.contains('\\')
        || name.contains("..")
        || !name.ends_with(".cf")
    {
        return None;
    }
    Some(name.to_string())
}

/// Read the remaining request headers, then the body of `Content-Length` bytes.
fn read_request_body(reader: &mut BufReader<TcpStream>) -> Vec<u8> {
    let mut len = 0usize;
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            break;
        }
        let t = line.trim_end();
        if t.is_empty() {
            break;
        }
        if let Some(v) = t.to_ascii_lowercase().strip_prefix("content-length:") {
            len = v.trim().parse().unwrap_or(0);
        }
    }
    let mut body = vec![0u8; len];
    let _ = std::io::Read::read_exact(reader, &mut body);
    body
}

fn query_param(query: &str, key: &str) -> Option<String> {
    query.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        (k == key).then(|| percent_decode(v))
    })
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                if let (Some(h), Some(l)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                    out.push(h * 16 + l);
                    i += 3;
                } else {
                    out.push(b'%');
                    i += 1;
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Find the raw TOML block that defines `id` and return it as JSON, so the
/// viewer can hand an agent the exact source to edit.
fn entity_block_json(project_dir: &Path, id: &str) -> String {
    // Generated copies (array/mirror) carry an @ suffix; their source is the base id.
    let base = id.split('@').next().unwrap_or(id);

    let lookup = || -> Option<(String, String, String)> {
        let project = parse_project(&project_dir.join("project.toml")).ok()?;
        for (layer, entry) in &project.layers {
            let path = project_dir.join(&entry.file);
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            if let Some(block) = find_block(&text, base) {
                return Some((layer.clone(), entry.file.clone(), block));
            }
        }
        None
    };

    match lookup() {
        Some((layer, file, block)) => serde_json::json!({
            "id": id,
            "base_id": base,
            "generated": id != base,
            "layer": layer,
            "file": file,
            "block": block,
        })
        .to_string(),
        None => serde_json::json!({ "id": id, "error": "not found" }).to_string(),
    }
}

/// Extract the `[[...]]` block (with leading comments) that contains `id = "<id>"`.
///
/// Uses toml_edit spans, so multi-line values (e.g. `points = [` …) are kept
/// intact instead of being cut at lines that merely look like TOML headers.
fn find_block(text: &str, id: &str) -> Option<String> {
    let doc = toml_edit::ImDocument::parse(text).ok()?;
    let mut span: Option<std::ops::Range<usize>> = None;
    for (_key, item) in doc.iter() {
        if let toml_edit::Item::ArrayOfTables(tables) = item {
            for table in tables.iter() {
                if table.get("id").and_then(|v| v.as_str()) == Some(id) {
                    span = table.span();
                }
            }
        }
    }
    let span = span?;

    let lines: Vec<&str> = text.lines().collect();
    let span_start_line = text[..span.start.min(text.len())].matches('\n').count();
    let span_end_line = text[..span.end.min(text.len())]
        .matches('\n')
        .count()
        .min(lines.len().saturating_sub(1));

    // The span covers the key/value pairs; step back to the [[header]] line
    // and pull in any comment lines directly above it.
    let header = lines[..=span_start_line.min(lines.len().saturating_sub(1))]
        .iter()
        .rposition(|l| l.trim_start().starts_with("[["))?;
    let mut start = header;
    while start > 0 && lines[start - 1].trim_start().starts_with('#') {
        start -= 1;
    }

    Some(
        lines[start..=span_end_line]
            .join("\n")
            .trim_end()
            .to_string(),
    )
}

fn respond(
    mut stream: TcpStream,
    status: &str,
    content_type: &str,
    body: &[u8],
) -> std::io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        status,
        content_type,
        body.len()
    )?;
    stream.write_all(body)?;
    stream.flush()
}

/// Server-sent events: the condvar wakes us the instant a rebuild lands, so
/// the browser is notified with sub-millisecond latency instead of polling.
fn serve_events(mut stream: TcpStream, state: &Shared) -> std::io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-store\r\nConnection: keep-alive\r\n\r\n"
    )?;
    stream.flush()?;

    let mut last = 0u64;
    loop {
        let current = {
            let mut st = state.state.lock().unwrap();
            while st.version == last {
                let (guard, timeout) = state
                    .changed
                    .wait_timeout(st, SSE_KEEPALIVE)
                    .map_err(|_| std::io::Error::other("state poisoned"))?;
                st = guard;
                if timeout.timed_out() && st.version == last {
                    drop(st);
                    // Keep-alive comment so dead clients are detected.
                    write!(stream, ": ping\n\n")?;
                    stream.flush()?;
                    st = state.state.lock().unwrap();
                }
            }
            st.version
        };
        last = current;
        write!(stream, "data: {}\n\n", current)?;
        stream.flush()?;
    }
}

fn open_browser(url: &str) {
    let result = if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(url).spawn()
    } else if cfg!(target_os = "windows") {
        std::process::Command::new("cmd")
            .args(["/C", "start", url])
            .spawn()
    } else {
        std::process::Command::new("xdg-open").arg(url).spawn()
    };
    if result.is_err() {
        println!("  (could not open browser automatically)");
    }
}

// ── Frontend ────────────────────────────────────────────────────────────

fn index_html(project_name: &str) -> String {
    INDEX_HTML.replace("{{PROJECT_NAME}}", &html_escape(project_name))
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

const INDEX_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>{{PROJECT_NAME}} — cadspec live</title>
<script>
  // Theme: follow the OS unless the user toggled an explicit choice. Pre-paint
  // so there is no flash.
  (function () {
    try {
      var t = localStorage.getItem('cadspec-theme');
      if (t === 'light' || t === 'dark') document.documentElement.dataset.theme = t;
    } catch (e) {}
  })();
</script>
<style>
  @import url('https://fonts.googleapis.com/css2?family=IBM+Plex+Mono:wght@400;500&display=swap');
  /* Dark is the default (the lab). System light, or the toggle, flips to the
     plan-white. `[data-theme]` (set by the toggle) overrides the system. */
  :root {
    --bg: #0d0e11; --panel: #14161a; --panel-2: #181b20;
    --ink: #d6dadf; --ink-strong: #ffffff; --ink-dim: #aab0b8;
    --ink-mute: #7c828b; --ink-faint: #565b63;
    --line: #23262c; --line-2: #2e323a;
    --accent: #6ec6e6; --accent-soft: rgba(110,198,230,0.16);
    --ok: #5dd39e; --err: #e0746e; --err-soft: #2a1212; --err-ink: #ff9f9a;
    --code-bg: #0a0b0d; --code-ink: #cdd6df;
    --t-num: #d9a35f; --t-hdr: #c79be0;
  }
  :root[data-theme="light"], :root[data-theme="light"] :root {
    --bg: #f6f8fa; --panel: #ffffff; --panel-2: #eef2f5;
    --ink: #1b2630; --ink-strong: #0a1218; --ink-dim: #41505b;
    --ink-mute: #6b7882; --ink-faint: #97a2ab;
    --line: #dde5ea; --line-2: #cdd7de;
    --accent: #156c80; --accent-soft: rgba(21,108,128,0.12);
    --ok: #1f8f5a; --err: #b23b34; --err-soft: #fbeae9; --err-ink: #9a3027;
    --code-bg: #f0f3f5; --code-ink: #1b2630;
    --t-num: #9a6516; --t-hdr: #7a3fa0;
  }
  @media (prefers-color-scheme: light) {
    :root:not([data-theme]) {
      --bg: #f6f8fa; --panel: #ffffff; --panel-2: #eef2f5;
      --ink: #1b2630; --ink-strong: #0a1218; --ink-dim: #41505b;
      --ink-mute: #6b7882; --ink-faint: #97a2ab;
      --line: #dde5ea; --line-2: #cdd7de;
      --accent: #156c80; --accent-soft: rgba(21,108,128,0.12);
      --ok: #1f8f5a; --err: #b23b34; --err-soft: #fbeae9; --err-ink: #9a3027;
      --code-bg: #f0f3f5; --code-ink: #1b2630;
      --t-num: #9a6516; --t-hdr: #7a3fa0;
    }
  }
  * { margin: 0; padding: 0; box-sizing: border-box; }
  body { background: var(--bg); color: var(--ink); font-family: 'IBM Plex Mono', ui-monospace, 'Cascadia Code', monospace; height: 100vh; display: flex; flex-direction: column; overflow: hidden; }
  header { display: flex; align-items: center; gap: 12px; padding: 9px 14px; background: var(--panel-2); border-bottom: 1px solid var(--line-2); user-select: none; }
  #dot { width: 10px; height: 10px; border-radius: 50%; background: var(--ok); flex: none; transition: background .2s; }
  #dot.err { background: var(--err); }
  #title { font-weight: 600; color: var(--ink-strong); white-space: nowrap; }
  .tag { font-size: 11px; color: var(--ink-mute); border: 1px solid var(--line-2); border-radius: 4px; padding: 2px 7px; white-space: nowrap; }
  button { background: var(--panel); color: var(--ink-dim); border: 1px solid var(--line-2); border-radius: 4px; padding: 3px 10px; font: inherit; font-size: 11px; cursor: pointer; }
  button:hover { color: var(--ink-strong); border-color: var(--ink-faint); }
  button.active { color: var(--accent); border-color: var(--accent); }
  #hint { margin-left: auto; font-size: 11px; color: var(--ink-faint); }
  main { flex: 1; display: flex; overflow: hidden; }
  /* built-in .cf editor */
  #editor { width: 340px; min-width: 200px; max-width: 720px; flex: none; background: var(--panel); border-right: 1px solid var(--line); display: flex; flex-direction: column; overflow: hidden; }
  #editor.hidden { display: none; }
  .ed-head { display: flex; gap: 6px; padding: 7px 8px; border-bottom: 1px solid var(--line); }
  #ed-file { flex: 1; min-width: 0; background: var(--panel-2); color: var(--ink); border: 1px solid var(--line-2); border-radius: 4px; font: inherit; font-size: 11px; padding: 3px 6px; }
  /* highlighted-overlay editor: a coloured <pre> behind a transparent textarea */
  #ed-wrap { flex: 1; position: relative; overflow: hidden; background: var(--code-bg); }
  #ed-hl, #ed-text { position: absolute; inset: 0; margin: 0; border: 0; padding: 10px 12px; font: inherit; font-size: 12px; line-height: 1.55; tab-size: 2; white-space: pre; overflow: auto; }
  #ed-hl { color: var(--code-ink); pointer-events: none; z-index: 0; }
  #ed-hl code { font: inherit; }
  #ed-text { resize: none; background: transparent; color: transparent; caret-color: var(--ink-strong); outline: none; z-index: 1; }
  #ed-text::selection { background: var(--accent-soft); }
  .t-key { color: var(--accent); }
  .t-str { color: var(--ok); }
  .t-num { color: var(--t-num); }
  .t-hdr { color: var(--t-hdr); font-weight: 500; }
  .t-com { color: var(--ink-faint); font-style: italic; }
  .t-bool { color: var(--err); }
  #ed-status { padding: 5px 10px; font-size: 10px; color: var(--ink-faint); border-top: 1px solid var(--line); white-space: nowrap; overflow: hidden; }
  #ed-status.ok { color: var(--ok); }
  #ed-status.err { color: var(--err); }
  #ed-status.dirty { color: var(--accent); }
  #editor-resizer { width: 6px; flex: none; cursor: col-resize; background: transparent; transition: background .15s; }
  #editor-resizer.hidden { display: none; }
  #editor-resizer:hover, #editor-resizer.dragging { background: var(--accent); }
  /* left sidebar: two stacked panes (Layers over Planos), IntelliJ-style */
  #sidebar { width: 200px; min-width: 130px; max-width: 560px; flex: none; background: var(--panel); border-right: 1px solid var(--line); display: flex; flex-direction: column; overflow: hidden; user-select: none; }
  #layers-pane { flex: 1 1 auto; overflow-y: auto; padding: 8px; min-height: 48px; }
  #planos-pane { flex: none; height: 40%; overflow-y: auto; padding: 8px; min-height: 48px; }
  /* horizontal divider between the two panes */
  #pane-divider { height: 6px; flex: none; cursor: row-resize; background: var(--panel-2); border-top: 1px solid var(--line); border-bottom: 1px solid var(--line); transition: background .15s; }
  #pane-divider:hover, #pane-divider.dragging { background: var(--accent); }
  /* drag handle to resize the whole sidebar width */
  #layers-resizer { width: 6px; flex: none; cursor: col-resize; background: transparent; transition: background .15s; }
  #layers-resizer:hover, #layers-resizer.dragging { background: var(--accent); }
  #sidebar h3 { font-size: 10px; color: var(--ink-faint); text-transform: uppercase; letter-spacing: 1px; margin: 2px 0 8px 4px; }
  .layer-row { display: flex; align-items: center; gap: 7px; padding: 5px 6px; border-radius: 4px; cursor: pointer; font-size: 12px; }
  .layer-row:hover, .plano-row:hover { background: var(--panel-2); }
  .layer-dot { width: 9px; height: 9px; border-radius: 50%; flex: none; }
  .layer-row .st { margin-left: auto; font-size: 10px; color: var(--ink-faint); }
  .layer-row.ghost { color: var(--ink-mute); }
  .layer-row.off { color: var(--ink-faint); }
  .plano-row { display: flex; align-items: baseline; gap: 7px; padding: 5px 6px; border-radius: 4px; cursor: pointer; font-size: 12px; }
  .plano-row .pv { margin-left: auto; font-size: 9px; color: var(--ink-faint); text-transform: uppercase; }
  .plano-row.active { background: var(--accent-soft); color: var(--accent); }
  #planos-pane .empty { font-size: 11px; color: var(--ink-faint); padding: 4px 6px; line-height: 1.5; }
  /* viewport */
  #viewport { flex: 1; overflow: hidden; position: relative; cursor: grab; perspective: 2200px; background: var(--bg); }
  #viewport.panning { cursor: grabbing; }
  #viewport.is3d { cursor: default; }
  /* interactive WebGL (glTF) layer, shown in 3D mode */
  #gl { position: absolute; inset: 0; width: 100%; height: 100%; display: none; }
  #gl.show { display: block; }
  #canvas { position: absolute; transform-origin: 0 0; will-change: transform; transform-style: preserve-3d; }
  #canvas svg { display: block; }
  .plane { position: absolute; left: 0; top: 0; }
  /* entity interaction */
  #canvas [data-id] { cursor: pointer; }
  #canvas [data-id]:hover { filter: brightness(1.8); }
  #canvas .sel { filter: drop-shadow(0 0 5px var(--accent)) brightness(1.6); }
  /* inspector */
  #inspector { width: 300px; flex: none; background: var(--panel); border-left: 1px solid var(--line); padding: 12px; overflow-y: auto; display: none; }
  #inspector.show { display: block; }
  #inspector h2 { font-size: 13px; color: var(--accent); word-break: break-all; }
  #inspector .meta { font-size: 11px; color: var(--ink-mute); margin: 6px 0 10px; line-height: 1.6; }
  #inspector pre { background: var(--code-bg); border: 1px solid var(--line-2); border-radius: 5px; padding: 9px; font-size: 11px; line-height: 1.45; white-space: pre-wrap; word-break: break-all; color: var(--code-ink); }
  #inspector .btns { display: flex; gap: 6px; margin-top: 10px; flex-wrap: wrap; }
  #inspector .note { font-size: 10px; color: var(--ink-faint); margin-top: 8px; }
  #error { display: none; position: absolute; left: 16px; right: 16px; bottom: 16px; background: var(--err-soft); border: 1px solid var(--err); border-radius: 6px; padding: 12px 16px; color: var(--err-ink); font-size: 13px; white-space: pre-wrap; max-height: 40%; overflow: auto; z-index: 10; }
  #error.show { display: block; }
  #toast { position: fixed; bottom: 44px; left: 50%; transform: translateX(-50%); background: var(--panel-2); border: 1px solid var(--accent); color: var(--accent); font-size: 11px; padding: 5px 14px; border-radius: 4px; opacity: 0; transition: opacity .2s; pointer-events: none; z-index: 20; }
  #toast.show { opacity: 1; }
  footer { padding: 5px 14px; background: var(--panel); border-top: 1px solid var(--line); font-size: 11px; color: var(--ink-faint); user-select: none; }
</style>
</head>
<body>
<header>
  <span id="dot"></span>
  <span id="title">{{PROJECT_NAME}}</span>
  <span class="tag">cadspec live</span>
  <span class="tag" id="version">v0</span>
  <button id="btn3d" title="extruded 3D view (key: 3)">3D</button>
  <button id="btnfit" title="fit to view (key: F)">fit</button>
  <button id="btneditor" class="active" title="toggle editor (key: E)">editor</button>
  <button id="btntheme" title="theme — follow system / light / dark">◐</button>
  <span id="hint">edit .cf files — preview updates automatically</span>
</header>
<main>
  <aside id="editor">
    <div class="ed-head">
      <select id="ed-file" title="project .cf files"></select>
      <button id="ed-save" title="save (Ctrl+S)">save</button>
    </div>
    <div id="ed-wrap">
      <pre id="ed-hl" aria-hidden="true"><code></code></pre>
      <textarea id="ed-text" spellcheck="false" autocapitalize="off" autocomplete="off" placeholder="select a .cf file…"></textarea>
    </div>
    <div id="ed-status">ready</div>
  </aside>
  <div id="editor-resizer" title="drag to resize the editor"></div>
  <aside id="sidebar">
    <div id="layers-pane"><h3>Layers</h3><div id="layerlist"></div></div>
    <div id="pane-divider" title="drag to resize panes"></div>
    <div id="planos-pane"><h3>Planos</h3><div id="planoslist"></div></div>
  </aside>
  <div id="layers-resizer" title="drag to resize · double-click to reset"></div>
  <div id="viewport">
    <div id="canvas"></div>
    <canvas id="gl"></canvas>
    <pre id="error"></pre>
  </div>
  <aside id="inspector">
    <h2 id="ins-id"></h2>
    <div class="meta" id="ins-meta"></div>
    <pre id="ins-block"></pre>
    <div class="btns">
      <button id="copy-id">copy id</button>
      <button id="copy-toml">copy TOML</button>
      <button id="copy-agent">copy for agent</button>
      <button id="close-ins">close</button>
    </div>
    <div class="note" id="ins-note"></div>
  </aside>
</main>
<div id="toast"></div>
<footer>click: inspect entity · scroll: zoom · drag: pan · double-click: fit · 1-9: cycle layer (on/ghost/off) · 3: 3D · Esc: deselect</footer>
<script>
const canvas = document.getElementById('canvas');
const viewport = document.getElementById('viewport');
const dot = document.getElementById('dot');
const errBox = document.getElementById('error');
const versionTag = document.getElementById('version');
const layerList = document.getElementById('layerlist');
const planosList = document.getElementById('planoslist');
const inspector = document.getElementById('inspector');
const toast = document.getElementById('toast');

let scale = 1, tx = 0, ty = 0;
let fitted = false;
let mode3d = false;
let svgText = '';
let svg3dText = '';
let layersInfo = [];                 // [{name, color}]
let planosInfo = [];                 // [{name, view, title}]
let currentPlano = null;             // active plano name, or null for the model
let planoSvg = '';
const layerState = {};               // name → 'on' | 'ghost' | 'off'
let selectedId = null;

// ── transform / view ────────────────────────────────────────────────
function applyTransform() {
  // The 3D view is a real axonometric projection baked into the SVG, so the
  // canvas only ever needs pan + zoom (no CSS tilt).
  canvas.style.transform = `translate(${tx}px, ${ty}px) scale(${scale})`;
}
function svgSize() {
  const svg = canvas.querySelector('svg');
  if (!svg) return null;
  return { w: parseFloat(svg.getAttribute('width')), h: parseFloat(svg.getAttribute('height')) };
}
function fitToView() {
  if (mode3d) { if (window.gl3d) window.gl3d.frame(); return; }
  const s = svgSize();
  if (!s) return;
  const vw = viewport.clientWidth, vh = viewport.clientHeight;
  scale = Math.min(vw / s.w, vh / s.h) * 0.96;
  tx = (vw - s.w * scale) / 2;
  ty = (vh - s.h * scale) / 2;
  applyTransform();
  fitted = true;
}

// ── rendering ───────────────────────────────────────────────────────
function renderCanvas() {
  if (mode3d) return;                 // 3D is the WebGL (glTF) layer, not the SVG
  const content = currentPlano ? planoSvg : svgText;
  if (!content) return;
  canvas.innerHTML = content;
  applyLayerStates();
  applySelection();
}

function applyLayerStates() {
  // 2D tags layers on <g>; the 3D view tags each projected face — match both.
  canvas.querySelectorAll('[data-layer]').forEach(g => {
    const st = layerState[g.dataset.layer] || 'on';
    g.style.opacity = st === 'on' ? '' : st === 'ghost' ? '0.16' : '0';
    g.style.pointerEvents = st === 'on' ? '' : 'none';
  });
}

function renderLayerPanel() {
  layerList.innerHTML = '';
  layersInfo.forEach((l, i) => {
    const st = layerState[l.name] || 'on';
    const row = document.createElement('div');
    row.className = 'layer-row' + (st !== 'on' ? ' ' + st : '');
    row.innerHTML = `<span class="layer-dot" style="background:${l.color}"></span>` +
                    `<span>${l.name}</span><span class="st">${i + 1} · ${st}</span>`;
    row.onclick = () => cycleLayer(l.name);
    layerList.appendChild(row);
  });
}
function cycleLayer(name) {
  const next = { on: 'ghost', ghost: 'off', off: 'on' };
  layerState[name] = next[layerState[name] || 'on'];
  renderLayerPanel();
  applyLayerStates();
}

// ── planos panel ────────────────────────────────────────────────────
function renderPlanosPanel() {
  planosList.innerHTML = '';
  if (!planosInfo.length) {
    planosList.innerHTML = '<div class="empty">no planos — add [[plano]] to project.toml</div>';
    return;
  }
  planosInfo.forEach(p => {
    const row = document.createElement('div');
    row.className = 'plano-row' + (currentPlano === p.name ? ' active' : '');
    row.innerHTML = `<span>${p.title || p.name}</span><span class="pv">${p.view}</span>`;
    row.onclick = () => openPlano(p.name);
    planosList.appendChild(row);
  });
}
async function fetchPlano(name) {
  return (await fetch('/plano.svg?name=' + encodeURIComponent(name) + '&t=' + Date.now())).text();
}
async function openPlano(name) {
  if (currentPlano === name) {           // toggle off → back to the model
    currentPlano = null; planoSvg = '';
    renderPlanosPanel(); renderCanvas(); fitToView();
    return;
  }
  currentPlano = name;
  renderPlanosPanel();
  planoSvg = await fetchPlano(name);
  renderCanvas();
  fitToView();
}

// ── selection / inspector ───────────────────────────────────────────
function applySelection() {
  canvas.querySelectorAll('.sel').forEach(n => n.classList.remove('sel'));
  if (!selectedId) return;
  canvas.querySelectorAll(`[data-id="${CSS.escape(selectedId)}"]`)
    .forEach(n => n.classList.add('sel'));
}
async function select(id) {
  selectedId = id;
  applySelection();
  const info = await (await fetch('/entity?id=' + encodeURIComponent(id))).json();
  document.getElementById('ins-id').textContent = id;
  if (info.error) {
    document.getElementById('ins-meta').textContent = 'no source block found (entity has no id?)';
    document.getElementById('ins-block').textContent = '';
    document.getElementById('ins-note').textContent = '';
  } else {
    document.getElementById('ins-meta').innerHTML =
      `layer: <b>${info.layer}</b> · file: <b>${info.file}</b>`;
    document.getElementById('ins-block').textContent = info.block;
    document.getElementById('ins-note').textContent = info.generated
      ? `⚠ generated copy — source is "${info.base_id}" (edit its [[array]]/[[mirror]])` : '';
    inspector.dataset.file = info.file;
    inspector.dataset.block = info.block;
  }
  inspector.classList.add('show');
}
function deselect() {
  selectedId = null;
  applySelection();
  inspector.classList.remove('show');
}
function copyText(text, msg) {
  navigator.clipboard.writeText(text).then(() => {
    toast.textContent = msg;
    toast.classList.add('show');
    setTimeout(() => toast.classList.remove('show'), 1200);
  });
}
document.getElementById('copy-id').onclick = () => copyText(selectedId, 'id copied');
document.getElementById('copy-toml').onclick = () =>
  copyText(`# ${inspector.dataset.file}\n${inspector.dataset.block}`, 'TOML copied');
document.getElementById('copy-agent').onclick = () =>
  copyText(`In ${inspector.dataset.file}, modify the entity "${selectedId}". Current definition:\n\n` +
           '```toml\n' + inspector.dataset.block + '\n```', 'agent prompt copied');
document.getElementById('close-ins').onclick = deselect;

// ── data refresh ────────────────────────────────────────────────────
async function refresh() {
  const [stateRes, svgRes, svg3dRes] = await Promise.all([
    fetch('/state'), fetch('/preview.svg?t=' + Date.now()), fetch('/preview3d.svg?t=' + Date.now())
  ]);
  const state = await stateRes.json();
  versionTag.textContent = 'v' + state.version;
  layersInfo = state.layers || [];
  planosInfo = state.planos || [];
  renderLayerPanel();
  renderPlanosPanel();
  if (state.error) {
    dot.classList.add('err');
    errBox.textContent = state.error;
    errBox.classList.add('show');
  } else {
    dot.classList.remove('err');
    errBox.classList.remove('show');
    svgText = await svgRes.text();
    svg3dText = await svg3dRes.text();
    // The active plano may reference changed geometry — re-render it too.
    if (currentPlano) {
      if (planosInfo.some(p => p.name === currentPlano)) planoSvg = await fetchPlano(currentPlano);
      else { currentPlano = null; planoSvg = ''; }  // plano was removed
    }
    renderCanvas();
    if (!fitted) fitToView();
    if (mode3d && window.gl3d) window.gl3d.reload();
  }
}

// ── input ───────────────────────────────────────────────────────────
viewport.addEventListener('wheel', e => {
  if (mode3d) return;                 // OrbitControls handles zoom in 3D
  e.preventDefault();
  const factor = Math.exp(-e.deltaY * 0.0012);
  const next = Math.min(Math.max(scale * factor, 0.05), 50);
  const r = viewport.getBoundingClientRect();
  const mx = e.clientX - r.left, my = e.clientY - r.top;
  tx = mx - (mx - tx) * (next / scale);
  ty = my - (my - ty) * (next / scale);
  scale = next;
  applyTransform();
}, { passive: false });

let panning = false, moved = 0, px = 0, py = 0;
viewport.addEventListener('mousedown', e => {
  if (mode3d) return;                 // OrbitControls handles rotate/pan in 3D
  panning = true; moved = 0; px = e.clientX; py = e.clientY;
  viewport.classList.add('panning');
});
window.addEventListener('mousemove', e => {
  if (!panning) return;
  moved += Math.abs(e.clientX - px) + Math.abs(e.clientY - py);
  tx += e.clientX - px; ty += e.clientY - py;
  px = e.clientX; py = e.clientY;
  applyTransform();
});
window.addEventListener('mouseup', () => {
  panning = false;
  viewport.classList.remove('panning');
});
viewport.addEventListener('click', e => {
  if (mode3d) return;                          // no entity-picking in 3D
  if (moved > 5) return;                       // it was a pan, not a click
  const el = e.target.closest('[data-id]');
  if (el) select(el.getAttribute('data-id'));
  else deselect();
});
viewport.addEventListener('dblclick', fitToView);

function toggle3d() {
  mode3d = !mode3d;
  document.getElementById('btn3d').classList.toggle('active', mode3d);
  viewport.classList.toggle('is3d', mode3d);
  if (mode3d) {
    canvas.style.display = 'none';
    if (window.gl3d) window.gl3d.mount();
  } else {
    canvas.style.display = '';
    if (window.gl3d) window.gl3d.unmount();
    renderCanvas();
    fitToView();
  }
}
document.getElementById('btn3d').onclick = toggle3d;
document.getElementById('btnfit').onclick = fitToView;

window.addEventListener('keydown', e => {
  if (e.target.tagName === 'INPUT') return;
  if (e.key === 'Escape') deselect();
  else if (e.key === 'f' || e.key === 'F') fitToView();
  else if (e.key === '3') toggle3d();
  else if (e.key >= '1' && e.key <= '9') {
    const l = layersInfo[+e.key - 1];
    if (l) cycleLayer(l.name);
  }
});

// ── resizable sidebar (width) ───────────────────────────────────────
(function () {
  const sidebar = document.getElementById('sidebar');
  const rz = document.getElementById('layers-resizer');
  const KEY = 'cadspec.layersWidth', MIN = 130, MAX = 560, DEF = 200;
  const saved = parseInt(localStorage.getItem(KEY) || '', 10);
  if (saved >= MIN && saved <= MAX) sidebar.style.width = saved + 'px';
  let dragging = false;
  rz.addEventListener('mousedown', e => {
    dragging = true; rz.classList.add('dragging');
    document.body.style.cursor = 'col-resize'; e.preventDefault();
  });
  window.addEventListener('mousemove', e => {
    if (!dragging) return;
    const w = Math.min(MAX, Math.max(MIN, e.clientX - sidebar.getBoundingClientRect().left));
    sidebar.style.width = w + 'px';
  });
  window.addEventListener('mouseup', () => {
    if (!dragging) return;
    dragging = false; rz.classList.remove('dragging'); document.body.style.cursor = '';
    localStorage.setItem(KEY, parseInt(sidebar.style.width, 10));
  });
  rz.addEventListener('dblclick', () => {
    sidebar.style.width = DEF + 'px'; localStorage.removeItem(KEY);
  });
})();

// ── stacked panes: drag the Layers/Planos divider (height) ──────────
(function () {
  const sidebar = document.getElementById('sidebar');
  const pane = document.getElementById('planos-pane');
  const div = document.getElementById('pane-divider');
  const KEY = 'cadspec.planosHeight';
  const saved = parseInt(localStorage.getItem(KEY) || '', 10);
  if (saved >= 48) pane.style.height = saved + 'px';
  let dragging = false;
  div.addEventListener('mousedown', e => {
    dragging = true; div.classList.add('dragging');
    document.body.style.cursor = 'row-resize'; e.preventDefault();
  });
  window.addEventListener('mousemove', e => {
    if (!dragging) return;
    const r = sidebar.getBoundingClientRect();
    const h = Math.min(r.height - 60, Math.max(48, r.bottom - e.clientY));
    pane.style.height = h + 'px';
  });
  window.addEventListener('mouseup', () => {
    if (!dragging) return;
    dragging = false; div.classList.remove('dragging'); document.body.style.cursor = '';
    localStorage.setItem(KEY, parseInt(pane.style.height, 10));
  });
})();

const events = new EventSource('/events');
events.onmessage = refresh;
events.onerror = () => dot.classList.add('err');

refresh();
</script>
<script>
  // ── Theme toggle: auto (follow system) → light → dark → auto ───────────────
  (function () {
    var root = document.documentElement;
    var btn = document.getElementById('btntheme');
    var mq = matchMedia('(prefers-color-scheme: light)');
    function mode() {
      var t = root.dataset.theme;
      return t === 'light' || t === 'dark' ? t : 'auto';
    }
    function apply(m) {
      if (m === 'auto') {
        delete root.dataset.theme;
        try { localStorage.removeItem('cadspec-theme'); } catch (e) {}
      } else {
        root.dataset.theme = m;
        try { localStorage.setItem('cadspec-theme', m); } catch (e) {}
      }
      var eff = m === 'auto' ? (mq.matches ? 'light' : 'dark') : m;
      btn.textContent = m === 'auto' ? '◐' : m === 'light' ? '☀' : '☾';
      btn.title = 'theme: ' + m + (m === 'auto' ? ' (' + eff + ')' : '') + ' — click to change';
    }
    btn.addEventListener('click', function () {
      apply({ auto: 'light', light: 'dark', dark: 'auto' }[mode()]);
    });
    mq.addEventListener('change', function () { if (mode() === 'auto') apply('auto'); });
    apply(mode());
  })();

  // ── Built-in .cf editor: syntax highlight + debounced auto-save ─────────────
  (function () {
    var editor = document.getElementById('editor');
    var resizer = document.getElementById('editor-resizer');
    var sel = document.getElementById('ed-file');
    var text = document.getElementById('ed-text');
    var hl = document.querySelector('#ed-hl code');
    var saveBtn = document.getElementById('ed-save');
    var status = document.getElementById('ed-status');
    var toggle = document.getElementById('btneditor');
    var current = null; // the file currently loaded in the textarea
    var timer = null;
    var SAVE_DELAY = 600;

    function setStatus(msg, cls) { status.textContent = msg; status.className = cls || ''; }
    function escHtml(s) { return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;'); }

    // Lightweight .cf (TOML-ish) tokenizer → coloured spans.
    function highlight(code) {
      return code.split('\n').map(function (line) {
        var out = '', rest = escHtml(line);
        var km = rest.match(/^(\s*)([A-Za-z0-9_.\-]+)(\s*=)/);
        if (km) { out += km[1] + '<span class="t-key">' + km[2] + '</span>' + km[3]; rest = rest.slice(km[0].length); }
        out += rest.replace(/(#.*$)|("(?:[^"\\]|\\.)*")|(\[\[?[^\]]*\]\]?)|(-?\b\d+\.?\d*\b)|(\btrue\b|\bfalse\b)/g,
          function (m, c, s, h, n, b) {
            if (c) return '<span class="t-com">' + c + '</span>';
            if (s) return '<span class="t-str">' + s + '</span>';
            if (h) return '<span class="t-hdr">' + h + '</span>';
            if (n) return '<span class="t-num">' + n + '</span>';
            if (b) return '<span class="t-bool">' + b + '</span>';
            return m;
          });
        return out;
      }).join('\n');
    }
    function paint() { hl.innerHTML = highlight(text.value) + '\n'; }
    function syncScroll() { var p = hl.parentNode; p.scrollTop = text.scrollTop; p.scrollLeft = text.scrollLeft; }

    function save() {
      var name = current; // captured: stays correct even if the file switches
      if (!name) return;
      clearTimeout(timer); timer = null;
      setStatus('saving…');
      fetch('/save?name=' + encodeURIComponent(name), { method: 'POST', body: text.value })
        .then(function (r) { return r.json(); })
        .then(function (j) { setStatus(j.ok ? 'saved · ' + name : 'saved · build error (see viewer)', j.ok ? 'ok' : 'err'); })
        .catch(function () { setStatus('save failed', 'err'); });
    }
    function scheduleSave() {
      clearTimeout(timer);
      setStatus('● ' + current, 'dirty');
      timer = setTimeout(save, SAVE_DELAY);
    }
    function loadFile(name) {
      clearTimeout(timer); timer = null;
      fetch('/file?name=' + encodeURIComponent(name))
        .then(function (r) { return r.text(); })
        .then(function (t) { current = name; text.value = t; paint(); syncScroll(); setStatus(name); })
        .catch(function () { setStatus('cannot load ' + name, 'err'); });
    }
    function loadFiles() {
      fetch('/files').then(function (r) { return r.json(); }).then(function (j) {
        sel.innerHTML = '';
        (j.files || []).forEach(function (f) {
          var o = document.createElement('option');
          o.value = f; o.textContent = f; sel.appendChild(o);
        });
        if (sel.options.length) { loadFile(sel.value); }
        else { current = null; text.value = ''; paint(); setStatus('no .cf files'); }
      }).catch(function () { setStatus('cannot list files', 'err'); });
    }

    sel.addEventListener('change', function () {
      if (timer) save();          // flush the pending edit to the old file first
      loadFile(sel.value);
    });
    text.addEventListener('input', function () { paint(); scheduleSave(); });
    text.addEventListener('scroll', syncScroll);
    saveBtn.addEventListener('click', save);
    // Keep editor keystrokes out of the viewer's shortcuts (3 / f / 1-9 / esc).
    text.addEventListener('keydown', function (e) {
      e.stopPropagation();
      if ((e.ctrlKey || e.metaKey) && (e.key === 's' || e.key === 'S')) { e.preventDefault(); save(); }
    });
    sel.addEventListener('keydown', function (e) { e.stopPropagation(); });

    toggle.addEventListener('click', function () {
      var hidden = editor.classList.toggle('hidden');
      resizer.classList.toggle('hidden', hidden);
      toggle.classList.toggle('active', !hidden);
    });
    window.addEventListener('keydown', function (e) {
      var tag = (e.target && e.target.tagName) || '';
      if ((e.key === 'e' || e.key === 'E') && !/INPUT|TEXTAREA|SELECT/.test(tag)) toggle.click();
    });

    // Drag to resize the editor pane.
    var drag = false;
    resizer.addEventListener('mousedown', function (e) { drag = true; resizer.classList.add('dragging'); e.preventDefault(); });
    window.addEventListener('mousemove', function (e) {
      if (!drag) return;
      editor.style.width = Math.max(200, Math.min(720, e.clientX)) + 'px';
    });
    window.addEventListener('mouseup', function () { drag = false; resizer.classList.remove('dragging'); });

    loadFiles();
  })();
</script>
<script type="importmap">
{
  "imports": {
    "three": "https://cdn.jsdelivr.net/npm/three@0.160.0/build/three.module.js",
    "three/addons/": "https://cdn.jsdelivr.net/npm/three@0.160.0/examples/jsm/"
  }
}
</script>
<script type="module">
  // Interactive 3D: load the scene as glTF and orbit it. The flat axonometric
  // SVG is no longer used for 3D — this is the real model.
  import * as THREE from 'three';
  import { OrbitControls } from 'three/addons/controls/OrbitControls.js';
  import { GLTFLoader } from 'three/addons/loaders/GLTFLoader.js';

  var glCanvas = document.getElementById('gl');
  var renderer, scene, camera, controls, model, raf = null, inited = false;
  var loader = new GLTFLoader();

  function bgColor() {
    var c = getComputedStyle(document.documentElement).getPropertyValue('--bg').trim();
    return new THREE.Color(c || '#0d0e11');
  }
  function resize() {
    if (!renderer) return;
    var w = glCanvas.clientWidth, h = glCanvas.clientHeight;
    if (!w || !h) return;
    renderer.setSize(w, h, false);
    camera.aspect = w / h; camera.updateProjectionMatrix();
  }
  function init() {
    if (inited) return; inited = true;
    renderer = new THREE.WebGLRenderer({ canvas: glCanvas, antialias: true });
    renderer.setPixelRatio(window.devicePixelRatio || 1);
    scene = new THREE.Scene();
    camera = new THREE.PerspectiveCamera(45, 1, 0.01, 1e6);
    controls = new OrbitControls(camera, renderer.domElement);
    controls.enableDamping = true; controls.dampingFactor = 0.08;
    scene.add(new THREE.AmbientLight(0xffffff, 0.6));
    var d1 = new THREE.DirectionalLight(0xffffff, 0.85); d1.position.set(1, 2, 1.5); scene.add(d1);
    var d2 = new THREE.DirectionalLight(0xffffff, 0.3); d2.position.set(-1.2, -0.4, -1); scene.add(d2);
    window.addEventListener('resize', resize);
  }
  function frame() {
    if (!model) return;
    var box = new THREE.Box3().setFromObject(model);
    var size = box.getSize(new THREE.Vector3());
    var center = box.getCenter(new THREE.Vector3());
    var r = Math.max(size.x, size.y, size.z, 1) * 0.5;
    controls.target.copy(center);
    var dist = r / Math.tan((camera.fov * Math.PI / 180) / 2) * 1.7;
    camera.position.set(center.x + dist * 0.8, center.y + dist * 0.7, center.z + dist * 0.9);
    camera.near = Math.max(r / 200, 0.001); camera.far = r * 200; camera.updateProjectionMatrix();
    controls.update();
  }
  function load() {
    loader.load('/scene.gltf?t=' + Date.now(), function (g) {
      if (model) scene.remove(model);
      model = g.scene; scene.add(model); frame();
    }, undefined, function () {});
  }
  function loop() {
    raf = requestAnimationFrame(loop);
    controls.update();
    renderer.setClearColor(bgColor(), 1);
    renderer.render(scene, camera);
  }
  window.gl3d = {
    mount: function () { init(); glCanvas.classList.add('show'); resize(); load(); if (!raf) loop(); },
    unmount: function () { glCanvas.classList.remove('show'); if (raf) { cancelAnimationFrame(raf); raf = null; } },
    reload: function () { if (glCanvas.classList.contains('show')) load(); },
    frame: function () { frame(); }
  };
</script>
</body>
</html>
"##;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_html_injects_project_name() {
        let html = index_html("Casa <Lote 12>");
        assert!(html.contains("Casa &lt;Lote 12&gt;"));
        assert!(!html.contains("{{PROJECT_NAME}}"));
    }

    #[test]
    fn relevant_events_filter_by_extension() {
        use notify::event::{CreateKind, EventAttributes};
        let mut event = Event {
            kind: EventKind::Create(CreateKind::File),
            paths: vec![PathBuf::from("/p/muros.cf")],
            attrs: EventAttributes::new(),
        };
        assert!(is_relevant(&event));
        event.paths = vec![PathBuf::from("/p/output.dxf")];
        assert!(!is_relevant(&event));
        event.paths = vec![PathBuf::from("/p/preview.svg")];
        assert!(!is_relevant(&event));
    }

    #[test]
    fn find_block_extracts_entity_with_comments() {
        let text = r#"[layer]
name = "muros"

# Puerta principal
[[arc]]
id = "ar-puerta"
center = [0.0, 2.5]
radius = 0.9

[[line]]
id = "ln-otro"
from = [0.0, 0.0]
to = [1.0, 0.0]
"#;
        let block = find_block(text, "ar-puerta").unwrap();
        assert!(block.starts_with("# Puerta principal"));
        assert!(block.contains("[[arc]]"));
        assert!(block.contains("radius = 0.9"));
        assert!(!block.contains("ln-otro"));

        let last = find_block(text, "ln-otro").unwrap();
        assert!(last.contains("[[line]]"));
        assert!(last.contains("to = [1.0, 0.0]"));

        assert!(find_block(text, "missing").is_none());
    }

    #[test]
    fn find_block_keeps_multiline_arrays_intact() {
        let text = r#"[[polyline]]
id = "pl-huella"
points = [
    [0.30, 0.0],
    [1.55, 0.0],
]
closed = true

[[circle]]
id = "ci-otro"
center = [0.0, 0.0]
radius = 1.0
"#;
        let block = find_block(text, "pl-huella").unwrap();
        assert!(block.contains("[1.55, 0.0],"), "block: {}", block);
        assert!(block.contains("closed = true"), "block: {}", block);
        assert!(!block.contains("ci-otro"));
    }

    #[test]
    fn find_block_does_not_match_belongs_to() {
        let text = r#"[[rect]]
id = "real"
belongs_to = "fake"
width = 1.0
"#;
        assert!(find_block(text, "fake").is_none());
        assert!(find_block(text, "real").is_some());
    }

    #[test]
    fn query_param_decodes_percent_encoding() {
        assert_eq!(query_param("id=ln%2D001", "id").as_deref(), Some("ln-001"));
        assert_eq!(query_param("a=1&id=tx+1", "id").as_deref(), Some("tx 1"));
        assert_eq!(query_param("a=1", "id"), None);
    }
}
