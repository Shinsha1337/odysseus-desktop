# Odysseus Desktop

<img width="1354" height="947" alt="odysseus-desktop" src="https://github.com/user-attachments/assets/f22dd851-710b-40e7-849b-c91505b1e1cd" />

Desktop app wrapper for the [Odysseus](https://github.com/pewdiepie-archdaemon/odysseus) self-hosted AI workspace by PewDiePie (Felix Kjellberg).

Built with [Tauri](https://tauri.app) — lightweight, no Electron.

---

## Download

Get the latest installer from [Releases](https://github.com/Shinsha1337/odysseus-desktop/releases).

| Platform | File |
|----------|------|
| Windows | `.exe` or `.msi` installer |
| macOS | coming soon |
| Linux | coming soon |

---

## What it does

The app launches and manages a local Odysseus server process. It bootstraps a Python `venv`, starts the server in the background, and opens the Odysseus UI in a native window.

- No console window required.
- No iframes or custom middleware — the webview navigates directly to the server.
- External links open in your default browser.
- Drag-and-drop into the chat works out of the box.
- If the server crashes, the launcher shows up again instead of a dead error page.

Press **F6** anywhere to reopen the launcher.

---

## Requirements

- An existing [Odysseus](https://github.com/pewdiepie-archdaemon/odysseus) folder (clone it first).
- Python 3.11+ on your `PATH`.
- Windows 10 or 11 (macOS and Linux builds are planned).

---

## Setup

1. Run the installer.
2. On first launch, pick the folder where your Odysseus clone lives.
3. Press **Start** — the app creates a `venv`, installs dependencies, and launches the server. This takes a minute on the first run.
4. Once the server is ready, Odysseus appears in the same window.

Your settings are saved to `%APPDATA%\odysseus.desktop\config.json`. You can also press **F6** at any time to change the folder, toggle autostart, or use a different port.

---

## Build from source

Requirements: [Node.js](https://nodejs.org), [Rust](https://rustup.rs), and the [Tauri prerequisites](https://tauri.app/start/prerequisites/) for your platform.

```bash
git clone https://github.com/Shinsha1337/odysseus-desktop.git
cd odysseus-desktop
npm install
npm run icons
npm run build
```

Installers are written to `src-tauri/target/release/bundle/`.

---

## License

MIT — see [LICENSE](./LICENSE).
