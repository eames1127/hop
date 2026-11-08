# Hop

Hop is a small desktop app that shows your SSH hosts as cards and connects to any of them in one
click. It runs on Windows and Kubuntu (KDE Plasma), has no server, and keeps your normal
`~/.ssh/config` as the single source of truth: plain `ssh <host>` keeps working exactly as before.

## Features

- Every host in `~/.ssh/config` (and its `Include` files) as a card with its description, tags,
  address and last-used time.
- **Connect** opens `ssh <host>` in Windows Terminal (falling back to a PowerShell window) or
  Konsole. Kitty and XTerm can be chosen in settings.
- Add, edit and delete hosts with a form. Hop backs up the config first
  (`config.hop-backup-<timestamp>`, last 10 kept), writes to a temporary file and swaps it in,
  and only ever rewrites the block you changed.
- Online checks, plus stale and missing-key flags, gathered in a **Needs attention** list.
- A details panel showing what OpenSSH really uses (`ssh -G`) and a 30-day connection chart.
- Port forward shortcuts saved per host.
- A tray menu with favourites and recent hosts, and a quick-launch shortcut.
- Light and dark themes.

## How Hop stores its notes

Descriptions, tags, favourites and forwards live as comments inside each host block, so they
travel with your config and stay readable by hand:

```
Host nas
	# desc: Synology NAS for backups and media
	# tags: home, storage
	# fav: true
	# forward: 5001:localhost:5001 Admin UI
	# added: 2025-03-14
	HostName 192.168.1.20
	User dan
```

`Host` blocks with a single plain alias are editable. Wildcard blocks (`Host *`), `Match`
blocks and multi-alias hosts are shown read-only and never rewritten.

Connection history (host name and time only) is kept in a small SQLite file in Hop's data folder
(`%APPDATA%\dev.eames.hop` or `~/.local/share/dev.eames.hop`) and can be cleared in settings.

## Quick launch

- **Windows:** press Ctrl+Shift+H.
- **KDE Plasma:** global shortcuts are unreliable under Wayland, so add a custom shortcut in
  System Settings → Shortcuts that runs `hop --quick`.

Quick launch opens Hop with the search box focused; type part of a name and press Enter to
connect to the first match. Closing the window keeps Hop in the tray; use **Quit Hop** in the
tray menu to exit.

## Installing

Download the installer from the [releases page](https://github.com/eames1127/hop/releases):

- **Windows:** the `.exe` (NSIS) installer. It is unsigned, so SmartScreen warns the first time.
- **Kubuntu:** the `.deb` (`sudo apt install ./hop_*.deb`), or the AppImage as a spare.

## Development

You need Node.js 22+, pnpm 9, stable Rust, and nightly Rust's `rustfmt` for formatting. On Linux,
install Tauri's packages:

```bash
sudo apt install libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev patchelf
```

```bash
pnpm install
pnpm tauri dev
```

Set `HOP_SSH_DIR` to point Hop at a copy of a config folder instead of your real `~/.ssh`.

| Check | Command |
| --- | --- |
| Parser and app tests | `cargo test --workspace` |
| Interface tests | `pnpm test` |
| Lint TypeScript and Vue | `pnpm lint` |
| Check CSS formatting | `pnpm format:check` |
| Format Rust | `cargo +nightly fmt --all` |
| Lint Rust | `cargo clippy --workspace --all-targets -- -D warnings` |
| End-to-end smoke test (Windows) | `pnpm test:e2e` |

The code style (tabs, Stroustrup braces, braces on every `if`, 100-column lines) is enforced by
ESLint, Prettier (CSS only) and rustfmt, and checked in CI. Vue components keep their styles in
`src/styles/` rather than `<style>` blocks, so Prettier never has to format TypeScript.

### Developing on Windows

1. Install the prerequisites:
   - Rust through [rustup](https://rustup.rs/) (stable, plus `rustup toolchain install nightly
     --component rustfmt` for formatting).
   - The Microsoft C++ Build Tools with the "Desktop development with C++" workload.
   - The WebView2 runtime (already present on Windows 11).
   - Node.js 22 or later.
   - pnpm 9.15.9: `npm install -g pnpm@9.15.9`. Avoid `corepack enable`, which often fails
     with `EPERM` because it writes into the Node install folder.
2. Close and reopen your terminals (and editor) after installing Rust, so `cargo` is on `PATH`.
3. From the repository folder:

   ```bash
   pnpm install
   pnpm tauri dev
   ```

   `pnpm tauri build --debug` builds a debug app (and installers) without starting the dev server.

To try Hop against a test config rather than your own, point `HOP_SSH_DIR` at a folder holding a
`config` file before starting it, for example in PowerShell:

```powershell
$env:HOP_SSH_DIR = "$PWD\e2e\fixtures\ssh"
pnpm tauri dev
```

### End-to-end smoke test

`pnpm test:e2e` launches the debug build (`target\debug\hop.exe`) against a copy of
`e2e/fixtures/ssh`, with WebView2 remote debugging on port 9333, and checks that the cards render,
search filters them and an invalid host name is refused. It uses Node's built-in `fetch` and
`WebSocket`, so it needs no extra packages. Build first and quit any running Hop, since a second
instance just hands over to the first:

```bash
pnpm tauri build --debug --no-bundle
pnpm test:e2e
```

The script asks the app for the port through `HOP_E2E_DEBUG_PORT`, which only debug builds read;
release builds have no way to open a debugging port. `HOP_E2E_BINARY` and `HOP_E2E_PORT`
override the app path and port. CI runs it on Windows.

Releases are built by GitHub Actions when a `v*` tag is pushed, as a draft release.

See [docs/plan.md](docs/plan.md) for the full project plan.
