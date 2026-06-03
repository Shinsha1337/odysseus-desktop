const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const dialog = window.__TAURI__.dialog;

const el = (id) => document.getElementById(id);

let cfg = { odysseusPath: "", host: "127.0.0.1", port: 7000, autoStart: true };
let status = "stopped";

function applyStatus(payload) {
  status = payload.status || "stopped";
  const detail = payload.detail || "";
  const busy = status === "starting" || status === "bootstrapping";

  const toggle = el("toggleBtn");
  if (status === "running" || busy) {
    toggle.textContent = "Stop";
    toggle.classList.add("danger");
  } else {
    toggle.textContent = "Start";
    toggle.classList.remove("danger");
  }

  const titleMap = {
    stopped: "Server stopped",
    starting: "Starting server…",
    bootstrapping: "Preparing environment…",
    error: "Failed to start server",
    running: "Server is running",
  };
  const title = titleMap[status] || "Odysseus";
  el("overlayTitle").textContent = title;
  let sub = detail || "";
  if (sub.trim().toLowerCase() === title.trim().toLowerCase()) sub = "";
  el("overlayDetail").textContent =
    sub ||
    (status === "stopped"
      ? "Press “Start” to launch the local Odysseus server."
      : status === "running"
      ? "Close Settings (Esc) to return to it. F6 anywhere reopens this panel."
      : "");

  const action = el("overlayAction");
  if (status === "running") {
    action.disabled = false;
    action.textContent = "Back to Odysseus";
  } else {
    action.disabled = busy;
    action.textContent = busy ? "Please wait…" : "Start server";
  }
}

function overlayAction() {
  if (status === "running") {
    invoke("resume_to_server").catch(() => {});
  } else {
    startServer();
  }
}

let logsVisible = false;
function showLogs(show) {
  logsVisible = show;
  el("logBox").classList.toggle("hidden", !show);
  el("logsToggle").textContent = show ? "Hide log" : "Show log";
}
function appendLog(line) {
  const box = el("logBox");
  const atBottom = box.scrollTop + box.clientHeight >= box.scrollHeight - 8;
  box.textContent += line + "\n";
  if (box.textContent.length > 200000) {
    box.textContent = box.textContent.slice(-150000);
  }
  if (atBottom) box.scrollTop = box.scrollHeight;
}

function _hexToRgb(hex) {
  let h = String(hex || "").trim().replace("#", "");
  if (h.length === 3) h = h[0] + h[0] + h[1] + h[1] + h[2] + h[2];
  if (!/^[0-9a-fA-F]{6}$/.test(h)) return null;
  return [parseInt(h.slice(0, 2), 16), parseInt(h.slice(2, 4), 16), parseInt(h.slice(4, 6), 16)];
}
function _rgbToHex([r, g, b]) {
  const c = (n) => Math.max(0, Math.min(255, Math.round(n))).toString(16).padStart(2, "0");
  return "#" + c(r) + c(g) + c(b);
}
function _mix(a, b, t) {
  const ca = _hexToRgb(a), cb = _hexToRgb(b);
  if (!ca || !cb) return a;
  return _rgbToHex([0, 1, 2].map((i) => ca[i] + (cb[i] - ca[i]) * t));
}
function _luma(hex) {
  const c = _hexToRgb(hex);
  if (!c) return 0;
  return (0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2]) / 255;
}
function _contrast(hex) {
  return _luma(hex) > 0.6 ? "#1a1a1a" : "#ffffff";
}

function applyTheme(colors) {
  if (!colors) return;
  const bg = colors.panel || colors.bg;
  const text = colors.fg;
  const accent = colors.red || colors.accent;
  const border = colors.border;
  if (!_hexToRgb(bg) || !_hexToRgb(text)) return;

  const dark = _luma(bg) < 0.5;
  const root = document.documentElement.style;
  root.setProperty("--bg", colors.bg || bg);
  root.setProperty("--bg-2", colors.panel || bg);
  root.setProperty("--text", text);
  root.setProperty("--muted", _mix(text, bg, 0.45));
  if (border) root.setProperty("--line", border);
  if (accent && _hexToRgb(accent)) {
    root.setProperty("--accent", accent);
    root.setProperty("--accent-hi", _mix(accent, dark ? "#ffffff" : "#000000", 0.18));
    root.setProperty("--accent-fg", _contrast(accent));
  }
}

function loadSavedTheme() {
  try {
    const raw = localStorage.getItem("odysseus-desktop-theme");
    if (raw) applyTheme(JSON.parse(raw));
  } catch (_) {}
}

async function startServer() {
  if (!(await invoke("validate_path", { path: cfg.odysseusPath }))) {
    openSettings("Set the Odysseus folder first.");
    return;
  }
  try {
    await invoke("start_server");
  } catch (e) {
    appendLog("[error] " + e);
  }
}
async function stopServer() {
  try {
    await invoke("stop_server");
  } catch (e) {
    appendLog("[error] " + e);
  }
}
function toggleServer() {
  if (status === "running" || status === "starting" || status === "bootstrapping") {
    stopServer();
  } else {
    startServer();
  }
}

function settingsOpen() {
  return !el("settings").classList.contains("hidden");
}

function openSettings(note) {
  el("pathInput").value = cfg.odysseusPath || "";
  el("hostInput").value = cfg.host || "127.0.0.1";
  el("portInput").value = cfg.port || 7000;
  el("autoStartInput").checked = !!cfg.autoStart;
  el("saveNote").textContent = note || "";
  el("saveNote").className = "hint";
  validatePathField();
  el("settings").classList.remove("hidden");
}

function closeSettings() {
  if (!settingsOpen()) return;
  el("settings").classList.add("hidden");
  if (status === "running") {
    invoke("resume_to_server").catch(() => {});
  }
}

function toggleSettings() {
  if (settingsOpen()) closeSettings();
  else openSettings();
}

async function validatePathField() {
  const p = el("pathInput").value.trim();
  const hint = el("pathHint");
  if (!p) {
    hint.textContent = "";
    hint.className = "hint";
    return;
  }
  const ok = await invoke("validate_path", { path: p });
  hint.textContent = ok ? "Found (app.py present)" : "No app.py in this folder";
  hint.className = "hint " + (ok ? "ok" : "bad");
}

async function browseFolder() {
  if (!dialog || !dialog.open) {
    el("saveNote").textContent = "Dialog unavailable — enter the path manually.";
    return;
  }
  try {
    const picked = await dialog.open({ directory: true, multiple: false, title: "Select the Odysseus folder" });
    if (picked) {
      el("pathInput").value = picked;
      validatePathField();
    }
  } catch (e) {
    el("saveNote").textContent = "Couldn't open dialog: " + e;
  }
}

async function saveSettings() {
  const prev = { ...cfg };
  const next = {
    odysseusPath: el("pathInput").value.trim(),
    host: el("hostInput").value.trim() || "127.0.0.1",
    port: parseInt(el("portInput").value, 10) || 7000,
    autoStart: el("autoStartInput").checked,
  };
  cfg = await invoke("set_config", { cfg: next });

  const note = el("saveNote");
  note.textContent = "Saved.";
  note.className = "hint ok";

  const connChanged =
    prev.odysseusPath !== cfg.odysseusPath || prev.host !== cfg.host || prev.port !== cfg.port;
  if (connChanged && (status === "running" || status === "starting" || status === "bootstrapping")) {
    note.textContent = "Saved — restarting server…";
    await stopServer();
    setTimeout(startServer, 600);
  }
}

function wire() {
  el("settingsBtn").addEventListener("click", () => openSettings());
  el("settingsClose").addEventListener("click", closeSettings);
  el("toggleBtn").addEventListener("click", toggleServer);
  el("overlayAction").addEventListener("click", overlayAction);
  el("logsToggle").addEventListener("click", () => showLogs(!logsVisible));

  el("browseBtn").addEventListener("click", browseFolder);
  el("pathInput").addEventListener("input", validatePathField);
  el("saveBtn").addEventListener("click", saveSettings);

  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      closeSettings();
    } else if (e.key === "F6") {
      e.preventDefault();
      toggleSettings();
    }
  });
}

async function init() {
  try {
    await invoke("register_local_url", { url: window.location.href });
  } catch (_) {}

  wire();
  loadSavedTheme();

  await listen("server-status", (e) => applyStatus(e.payload));
  await listen("server-log", (e) => appendLog(e.payload));

  cfg = await invoke("get_config");
  applyStatus({ status: await invoke("server_status"), detail: "" });

  const wantSettings = window.location.hash === "#settings";
  if (wantSettings) {
    try {
      history.replaceState(null, "", window.location.pathname);
    } catch (_) {}
    openSettings();
    return;
  }

  const validPath = cfg.odysseusPath && (await invoke("validate_path", { path: cfg.odysseusPath }));
  if (!validPath) {
    openSettings("Welcome. Choose the folder where your Odysseus clone lives.");
    return;
  }

  let autoStartAllowed = true;
  try {
    autoStartAllowed = await invoke("should_autostart");
  } catch (_) {}

  if (cfg.autoStart && autoStartAllowed && status !== "running") {
    startServer();
  } else if (!autoStartAllowed) {
    el("overlayTitle").textContent = "Server stopped";
    el("overlayDetail").textContent =
      "The server crashed last time it was started. Press Start to try again, or open Settings.";
    showLogs(true);
  }
}

window.addEventListener("DOMContentLoaded", init);
