use std::fs;
use std::io::{BufRead, BufReader};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::webview::NewWindowResponse;
use tauri::{
    AppHandle, Emitter, Manager, State, Url, WebviewUrl, WebviewWindowBuilder, WindowEvent,
};

#[cfg(windows)]
use std::os::windows::process::CommandExt;
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

const BRIDGE_JS: &str = r#"
(function() {
  if (window.__odBridge) return;
  window.__odBridge = true;
  try {
    if (location.protocol === 'tauri:' || location.hostname === 'tauri.localhost') return;
  } catch (_) { return; }

  function ping(action, payload) {
    try {
      var u = location.origin + '/__od_desktop_bridge__/' + action +
              '?v=' + encodeURIComponent(payload || '') + '&t=' + Date.now();
      var a = document.createElement('a');
      a.href = u;
      a.style.display = 'none';
      (document.body || document.documentElement).appendChild(a);
      a.click();
      a.remove();
    } catch (_) {}
  }

  document.addEventListener('keydown', function (e) {
    if (e.key === 'F6') {
      e.preventDefault();
      e.stopPropagation();
      ping('settings', '');
    }
  }, true);
})();
"#;

const BRIDGE_PATH_PREFIX: &str = "/__od_desktop_bridge__/";

#[derive(Clone, Serialize, Deserialize)]
struct Config {
    #[serde(rename = "odysseusPath", default)]
    odysseus_path: String,
    #[serde(default = "default_host")]
    host: String,
    #[serde(default = "default_port")]
    port: u16,
    #[serde(rename = "autoStart", default = "default_true")]
    auto_start: bool,
}

fn default_host() -> String {
    "127.0.0.1".into()
}
fn default_port() -> u16 {
    7000
}
fn default_true() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Config {
            odysseus_path: String::new(),
            host: default_host(),
            port: default_port(),
            auto_start: true,
        }
    }
}

fn config_file(app: &AppHandle) -> PathBuf {
    let dir = app.path().app_config_dir().expect("resolve app config dir");
    let _ = fs::create_dir_all(&dir);
    dir.join("config.json")
}

fn read_config(app: &AppHandle) -> Config {
    match fs::read_to_string(config_file(app)) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => Config::default(),
    }
}

fn write_config(app: &AppHandle, cfg: &Config) {
    if let Ok(s) = serde_json::to_string_pretty(cfg) {
        let _ = fs::write(config_file(app), s);
    }
}

#[derive(Default)]
struct ServerProc {
    child: Option<Child>,
    status: String,
    server_url: String,
    server_host: String,
    server_port: u16,
    crashed: bool,
    local_url: String,
}

struct AppState(Mutex<ServerProc>);

fn emit_status(app: &AppHandle, state: &AppState, status: &str, detail: &str) {
    state.0.lock().unwrap().status = status.to_string();
    let _ = app.emit(
        "server-status",
        serde_json::json!({ "status": status, "detail": detail }),
    );
}

fn emit_log(app: &AppHandle, line: &str) {
    let _ = app.emit("server-log", line.to_string());
}

fn python_exe(base: &str) -> PathBuf {
    let p = Path::new(base);
    if cfg!(windows) {
        p.join("venv").join("Scripts").join("python.exe")
    } else {
        p.join("venv").join("bin").join("python")
    }
}

fn system_python() -> &'static str {
    if cfg!(windows) {
        "py"
    } else {
        "python3"
    }
}

fn hidden_command(program: &str) -> Command {
    let mut c = Command::new(program);
    #[cfg(windows)]
    c.creation_flags(CREATE_NO_WINDOW);
    c
}

fn pipe_logs(app: &AppHandle, child: &mut Child) {
    if let Some(out) = child.stdout.take() {
        let a = app.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                emit_log(&a, &line);
            }
        });
    }
    if let Some(err) = child.stderr.take() {
        let a = app.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(err).lines().map_while(Result::ok) {
                emit_log(&a, &line);
            }
        });
    }
}

fn run_step(app: &AppHandle, program: &str, args: &[&str], cwd: &str, label: &str) -> Result<(), String> {
    emit_log(app, &format!("\n==> {label}"));
    let mut child = hidden_command(program)
        .args(args)
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("{label}: {e}"))?;
    pipe_logs(app, &mut child);
    let status = child.wait().map_err(|e| format!("{label}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{label} failed (exit {:?})", status.code()))
    }
}

fn bootstrap(app: &AppHandle, base: &str) -> Result<(), String> {
    run_step(app, system_python(), &["-m", "venv", "venv"], base, "Creating virtual environment")?;
    let py = python_exe(base);
    let py = py.to_string_lossy().to_string();
    run_step(app, &py, &["-m", "pip", "install", "--upgrade", "pip"], base, "Upgrading pip")?;
    run_step(app, &py, &["-m", "pip", "install", "-r", "requirements.txt"], base, "Installing dependencies")?;
    run_step(app, &py, &["setup.py"], base, "First-time setup")?;
    Ok(())
}

fn kill_child(mut child: Child) {
    let pid = child.id();
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .creation_flags(CREATE_NO_WINDOW)
            .status();
    }
    #[cfg(not(windows))]
    {
        let _ = pid;
        let _ = child.kill();
    }
    let _ = child.wait();
}

fn is_internal_url(url: &Url, server_host: &str, server_port: u16) -> bool {
    let scheme = url.scheme();
    if let Some(host) = url.host_str() {
        if host == "tauri.localhost" || (host == "localhost" && scheme == "tauri") {
            return true;
        }
        if !server_host.is_empty() && host == server_host {
            let port = url.port().unwrap_or(match scheme {
                "https" => 443,
                _ => 80,
            });
            if port == server_port {
                return true;
            }
        }
    }
    scheme == "about" || scheme == "data" || scheme == "tauri" || scheme == "ipc"
}

fn open_url_external(url: &str) {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return;
    }
    #[cfg(windows)]
    {
        let _ = Command::new("rundll32")
            .args(["url.dll,FileProtocolHandler", url])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = Command::new("open").arg(url).spawn();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = Command::new("xdg-open").arg(url).spawn();
    }
}

fn navigate_main(app: &AppHandle, url: &str) {
    if let (Some(win), Ok(parsed)) = (app.get_webview_window("main"), Url::parse(url)) {
        let _ = win.navigate(parsed);
    }
}

fn open_settings_from_server(app: &AppHandle) {
    let local_url = app.state::<AppState>().0.lock().unwrap().local_url.clone();
    if local_url.is_empty() {
        return;
    }
    let target = format!("{local_url}#settings");
    navigate_main(app, &target);
}

#[tauri::command]
fn get_config(app: AppHandle) -> Config {
    read_config(&app)
}

#[tauri::command]
fn set_config(app: AppHandle, cfg: Config) -> Config {
    write_config(&app, &cfg);
    cfg
}

#[tauri::command]
fn validate_path(path: String) -> bool {
    !path.is_empty() && Path::new(&path).join("app.py").is_file()
}

#[tauri::command]
fn server_status(state: State<AppState>) -> String {
    state.0.lock().unwrap().status.clone()
}

#[tauri::command]
fn register_local_url(state: State<AppState>, url: String) {
    let mut g = state.0.lock().unwrap();
    if g.local_url.is_empty() {
        g.local_url = url;
    }
}

#[tauri::command]
fn should_autostart(state: State<AppState>) -> bool {
    !state.0.lock().unwrap().crashed
}

#[tauri::command]
fn open_external(url: String) {
    open_url_external(&url);
}

#[tauri::command]
fn resume_to_server(app: AppHandle) {
    let url = app.state::<AppState>().0.lock().unwrap().server_url.clone();
    if !url.is_empty() {
        navigate_main(&app, &url);
    }
}

#[tauri::command]
fn start_server(app: AppHandle, state: State<AppState>) -> Result<(), String> {
    if state.0.lock().unwrap().child.is_some() {
        return Ok(());
    }
    state.0.lock().unwrap().crashed = false;

    let cfg = read_config(&app);
    if !validate_path(cfg.odysseus_path.clone()) {
        emit_status(&app, &state, "error", "Odysseus folder not found (no app.py)");
        return Err("invalid odysseus path".into());
    }
    let base = cfg.odysseus_path.clone();
    let py = python_exe(&base);

    if !py.is_file() {
        emit_status(&app, &state, "bootstrapping", "Setting up environment (first run)…");
        if let Err(e) = bootstrap(&app, &base) {
            emit_log(&app, &format!("[bootstrap] {e}"));
            emit_status(&app, &state, "error", &e);
            return Err(e);
        }
    }

    emit_status(&app, &state, "starting", "Starting server…");
    let mut child = hidden_command(&py.to_string_lossy())
        .args([
            "-m",
            "uvicorn",
            "app:app",
            "--host",
            &cfg.host,
            "--port",
            &cfg.port.to_string(),
        ])
        .current_dir(&base)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            let m = format!("Failed to start server: {e}");
            emit_status(&app, &state, "error", &m);
            m
        })?;

    pipe_logs(&app, &mut child);
    {
        let mut g = state.0.lock().unwrap();
        g.child = Some(child);
        g.server_url = format!("http://{}:{}/", cfg.host, cfg.port);
        g.server_host = cfg.host.clone();
        g.server_port = cfg.port;
    }

    let host = cfg.host.clone();
    let port = cfg.port;
    let app2 = app.clone();
    std::thread::spawn(move || {
        let st = app2.state::<AppState>();
        let mut became_ready = false;
        let mut startup_ticks: u32 = 0;
        loop {
            let alive = {
                let mut g = st.0.lock().unwrap();
                match g.child.as_mut() {
                    Some(c) => !matches!(c.try_wait(), Ok(Some(_))),
                    None => return,
                }
            };
            if !alive {
                let local_url = {
                    let mut g = st.0.lock().unwrap();
                    g.child = None;
                    g.server_url.clear();
                    g.server_host.clear();
                    if became_ready {
                        g.crashed = true;
                    }
                    g.local_url.clone()
                };
                let detail = if became_ready {
                    "Server exited unexpectedly"
                } else {
                    "Server failed to start (see log)"
                };
                emit_status(&app2, &st, "stopped", detail);
                if became_ready && !local_url.is_empty() {
                    navigate_main(&app2, &local_url);
                }
                return;
            }

            if !became_ready {
                if TcpStream::connect((host.as_str(), port)).is_ok() {
                    became_ready = true;
                    let url = st.0.lock().unwrap().server_url.clone();
                    navigate_main(&app2, &url);
                    emit_status(&app2, &st, "running", "");
                } else {
                    startup_ticks += 1;
                    if startup_ticks > 600 {
                        if let Some(child) = st.0.lock().unwrap().child.take() {
                            kill_child(child);
                        }
                        st.0.lock().unwrap().server_url.clear();
                        emit_status(&app2, &st, "error", "Server did not respond in time");
                        return;
                    }
                }
            }

            std::thread::sleep(Duration::from_millis(if became_ready { 1500 } else { 700 }));
        }
    });

    Ok(())
}

#[tauri::command]
fn stop_server(app: AppHandle, state: State<AppState>) -> Result<(), String> {
    let (child, local_url) = {
        let mut g = state.0.lock().unwrap();
        let child = g.child.take();
        g.server_url.clear();
        g.server_host.clear();
        (child, g.local_url.clone())
    };
    if let Some(child) = child {
        kill_child(child);
    }
    emit_status(&app, &state, "stopped", "Server stopped");
    if !local_url.is_empty() {
        navigate_main(&app, &local_url);
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState(Mutex::new(ServerProc::default())))
        .setup(|app| {
            let handle = app.handle().clone();

            {
                let new_dir = handle.path().app_config_dir().unwrap_or_default();
                let new_cfg = new_dir.join("config.json");
                if !new_cfg.exists() {
                    if let Ok(appdata) = std::env::var("APPDATA") {
                        let old_cfg = PathBuf::from(appdata)
                            .join("dev.odysseus.desktop")
                            .join("config.json");
                        if old_cfg.exists() {
                            let _ = fs::create_dir_all(&new_dir);
                            let _ = fs::copy(&old_cfg, &new_cfg);
                        }
                    }
                }
            }

            let webview_data_dir = handle
                .path()
                .app_data_dir()
                .unwrap_or_default()
                .join("webview-data");
            let _ = fs::create_dir_all(&webview_data_dir);
            #[cfg(windows)]
            unsafe { std::env::set_var("WEBVIEW2_USER_DATA_FOLDER", &webview_data_dir); }

            let builder = WebviewWindowBuilder::new(
                app,
                "main",
                WebviewUrl::App("index.html".into()),
            )
                .title("Odysseus")
                .inner_size(1280.0, 832.0)
                .min_inner_size(900.0, 600.0)
                .center()
                .resizable(true)
                .transparent(true)
                .disable_drag_drop_handler()
                .data_directory(webview_data_dir)
                .initialization_script(BRIDGE_JS)
                .on_navigation({
                    let handle = handle.clone();
                    move |url| {
                        if url.path().starts_with(BRIDGE_PATH_PREFIX) {
                            let action = url
                                .path()
                                .trim_start_matches(BRIDGE_PATH_PREFIX)
                                .trim_end_matches('/');
                            if action == "settings" {
                                open_settings_from_server(&handle);
                            }
                            return false;
                        }
                        let (host, port) = {
                            let st = handle.state::<AppState>();
                            let g = st.0.lock().unwrap();
                            (g.server_host.clone(), g.server_port)
                        };
                        if is_internal_url(url, &host, port) {
                            true
                        } else {
                            open_url_external(url.as_str());
                            false
                        }
                    }
                })
                .on_new_window({
                    let handle = handle.clone();
                    move |url, _features| {
                        let (host, port) = {
                            let st = handle.state::<AppState>();
                            let g = st.0.lock().unwrap();
                            (g.server_host.clone(), g.server_port)
                        };
                        if is_internal_url(&url, &host, port) {
                            if let Some(win) = handle.get_webview_window("main") {
                                let _ = win.navigate(url);
                            }
                        } else {
                            open_url_external(url.as_str());
                        }
                        NewWindowResponse::Deny
                    }
                });

            let win = builder.build()?;

            #[cfg(windows)]
            {
                use window_vibrancy::{apply_acrylic, apply_mica};
                if apply_mica(&win, None).is_err() {
                    let _ = apply_acrylic(&win, None);
                }
            }
            let _ = win;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_config,
            set_config,
            validate_path,
            server_status,
            register_local_url,
            should_autostart,
            start_server,
            stop_server,
            open_external,
            resume_to_server,
        ])
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { .. } = event {
                let state = window.state::<AppState>();
                let child = state.0.lock().unwrap().child.take();
                if let Some(child) = child {
                    kill_child(child);
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
