# Hop – Project Plan

Nov 6, 2026 · the maintainer

## Summary and goals

Hop is a small desktop app, built with Tauri, that lists your SSH hosts as cards and connects to any of them in one click. It runs on Windows (desktop) and Kubuntu (laptop), has no server, and keeps your normal `~/.ssh/config` as the single source of truth.

Version 1 is done when it can:

1. Show every host in the SSH config as a card with its description, tags, address and last-used time.
2. Connect to a host in the system's own terminal with one click.
3. Add and edit hosts through a form, writing tidy blocks back to the config with a backup first.
4. Show which hosts are online and flag stale or broken ones.
5. Offer a tray menu of favourites and recent hosts for quick launch.
6. Install from a normal installer on both machines, built automatically by GitHub Actions.

Plain `ssh <host>` in any terminal must keep working exactly as before. Hop adds notes and convenience; it never becomes something you depend on.

## Scope

Version 1 is SSH only, for one person on two machines. Everything else waits until v1 is in daily use.

| In v1 | Not in v1 |
| --- | --- |
| Read and display hosts from `~/.ssh/config`, including `Include` files | Remote desktop, VNC, SFTP, web admin pages, databases |
| Connect in Windows Terminal or Konsole | A built-in terminal inside the app |
| Add, edit and delete hosts with a form, backup first | Reading, copying or moving private keys |
| Descriptions, tags and favourites as config comments | Syncing hosts between machines (use git or dotfiles instead) |
| Online checks and stale or broken flags | Running commands across many hosts |
| Local connection history and counts | Cloud backup, accounts or any server |
| Tray menu with favourites and recent hosts | macOS builds, mobile apps |
| Port forward shortcuts saved per host | Auto-update and code signing |

The code is organised around a "connection type" so other kinds can be added later without a rewrite, but only SSH is built.

## How it works

Hop has two halves: a web-style interface that draws the screens, and a Rust backend that does everything touching your computer. The interface can only ask the backend for a short list of named actions.

&#91;embedded content: Hop architecture · 3 layers\]

Each backend part does one job: the `sshconf` crate reads and writes the config, status checks ask `ssh -G` for a host's final settings and test the port, the platform module opens the right terminal for each OS, and the history store records connections.

A Connect click runs like this:

1. The card calls the `connect` command with the host name.
2. Rust checks the name against the config and the allowed pattern.
3. The platform module starts `wt.exe new-tab ssh nas` or `konsole --new-tab -e ssh nas`.
4. `ssh` takes over in the terminal, using your keys and agent as normal.
5. Hop records the time in history and updates "last used" on the card.

## Windows and Kubuntu specifics

The app code is shared; only the terminal launch, a few paths and the installers differ per machine. Each OS gets a small "platform" module so these differences live in one place.

|  | Windows (desktop) | Kubuntu (laptop) |
| --- | --- | --- |
| SSH client | Built-in OpenSSH (`C:\Windows\System32\OpenSSH\ssh.exe`) | `openssh-client` package |
| Config file | `%USERPROFILE%\.ssh\config` | `~/.ssh/config` |
| Terminal for Connect | Windows Terminal: `wt.exe new-tab ssh <host>`, falling back to a PowerShell window | Konsole: `konsole --new-tab -e ssh <host>` |
| Web view Tauri uses | WebView2 (already on Windows 11) | WebKitGTK (`libwebkit2gtk-4.1`) |
| Tray icon | Notification area, left click opens the menu | Plasma system tray via AppIndicator; the menu opens on click, other click events are limited |
| Quick-launch shortcut | Global shortcut plugin (e.g. Ctrl+Shift+H) | Global shortcuts can fail under Wayland, so bind a KDE custom shortcut to `hop --quick` instead |
| History and settings | `%APPDATA%\<app id>` | `~/.local/share/<app id>` |
| Installer | NSIS `.exe` (unsigned, so SmartScreen warns once) | `.deb` for daily use, AppImage as a spare |

Two details matter for sharing one config across both machines:

- Write key paths as `~/.ssh/id_ed25519`; OpenSSH on Windows expands `~` too, so the same block works on both.
- Keep each file's existing line endings (CRLF on Windows, LF on Linux) when Hop writes, so git diffs stay clean.

## Config and data handling

Hop never loses or reshuffles anything in your SSH config: it edits only the lines it owns and leaves every other byte alone.

**Round-trip parser.** Hop uses its own small parser that keeps comments, blank lines, indentation and order exactly as written. Most SSH config libraries only read, and would reformat the file on save. Rules:

- `Host` blocks with one plain alias are editable in Hop.
- Wildcard blocks (`Host *`), `Match` blocks and multi-alias hosts are shown read-only and never rewritten.
- `Include` files are followed and shown, with their source file named on the card.

**What a host "really" is.** For status checks and the details panel, Hop asks OpenSSH itself with `ssh -G <host>`. That returns the final address, user, port and jump host after every wildcard and `Include` has been applied, so Hop never has to copy OpenSSH's matching rules.

**Hop's own notes** live as comments inside each host block, so they travel with the config and stay readable by hand:

```
# desc: Synology NAS for backups and media
# tags: home, storage
# fav: true
# forward: 5001:localhost:5001 Admin UI
# added: 2025-03-14
```

**Backups.** Before every write, Hop copies the file to `config.hop-backup-<timestamp>` and keeps the last 10. It then writes to a temporary file and swaps it in, so a crash mid-save can't leave a half-written config.

**History** is kept outside the config, in a small SQLite file in Hop's app data folder: host name, time and session length only. Nothing else is recorded, and it can be cleared from settings.

## Security

Hop gets only the permissions it needs, granted by name and path, so even a compromised Hop could at worst damage the SSH config, and a backup exists to restore it.

- **No key access.** Hop never opens private keys. It shows key file names from the config and checks a key file exists; `ssh` itself reads the keys when you connect. It reads only `.pub` files, and new keys are made by `ssh-keygen` in a terminal, so Hop never sees a passphrase.
- **One key per computer.** To set up another computer, it makes its own key and this computer adds the public key to each host's `authorized_keys`. A lost computer is revoked by deleting its one line. The key travels to the host on ssh's stdin, never in a command line, and only after `ssh-keygen` has decoded it and the user has seen its fingerprint.
- **File scope.** Read and write the SSH config, its `Include` files and Hop's backups; read and write Hop's own app data folder. Nothing else.
- **Command scope.** The only programs Hop can start are `ssh` (for `-G` lookups, tunnels and adding a public key), `ssh-keygen` (fingerprints, and new keys in a terminal) and the terminal (`wt.exe` or `konsole`). Arguments are fixed by Hop's Rust code, never built from text in the page.
- **Host name check.** A host name must match `^[A-Za-z0-9._-]+$` before it reaches any command, so text like `nas; rm -rf ~` is refused.
- **No network from the page.** A strict content security policy blocks the interface from loading or sending anything outside the app. Online checks are plain TCP connects made in Rust, only to hosts in your config.
- **Untrusted text stays text.** Descriptions and notes are shown as plain text, never as HTML, so odd characters in a note can't run code.
- **Small command list.** The page can call about a dozen named Rust commands (list hosts, save host, connect, check status and so on). Anything else is refused.

These are set in Tauri's capabilities file and the Rust command layer; exact permission names will be checked against the current Tauri docs during setup.

## Build phases

Seven phases, each ending in something installable on both machines, so Hop is useful from phase 2 onward. Read-only comes before editing, so the config is never at risk while the parser is young.

0. **Set-up.** Create the `hop` repo, scaffold Tauri 2, add the capabilities file and a GitHub Actions build for Windows and Kubuntu.
   - Done when: an empty Hop window installs and opens on both machines from a CI build.
1. **Read-only host list.** Round-trip parser, host cards, search, tag sidebar, read-only handling of wildcard and `Match` blocks.
   - Done when: your real configs on both machines show correctly and nothing is written.
2. **Connect.** Launch Windows Terminal or Konsole with `ssh <host>`, record history, show last used on cards.
   - Done when: you use Hop instead of typing `ssh` for a week.
3. **Add and edit.** The add-host form, edit, delete, favourites, backups and atomic saves, plus the round-trip test suite.
   - Done when: every test config saves back byte-for-byte identical apart from the edited block.
4. **Health and details.** `ssh -G` details panel, online checks, stale and broken flags, missing key warnings, connection history chart.
   - Done when: the "Needs attention" list matches what you'd find by hand.
5. **Tray and quick launch.** Tray menu with favourites and recent hosts, keyboard shortcut (KDE custom shortcut on the laptop), start with the computer.
   - Done when: you can connect without opening the main window.
6. **Polish and v1.0.** Port forward shortcuts, settings (terminal choice, clear history), light and dark themes, README, tagged release.
   - Done when: v1.0 installers are on the GitHub release page.

## Repo layout and tooling

One repo, `hop`, holding a Vue 3 + TypeScript interface and a Rust backend, with the SSH config logic in its own crate so it can be tested without the app.

```
hop/
├── src/                    # interface (Vue 3 + TypeScript, built with Vite)
│   ├── lib/components/     # HostCard.vue, HostDetail.vue, AddHostForm.vue, Sidebar.vue
│   ├── lib/api.ts          # typed wrappers around the Rust commands
│   └── views/              # main window and tray views
├── src-tauri/
│   ├── src/
│   │   ├── commands.rs     # the named commands the interface may call
│   │   ├── platform/       # windows.rs, linux.rs: terminal launch, paths
│   │   ├── status.rs       # online checks
│   │   └── history.rs      # SQLite history
│   ├── capabilities/       # Tauri permission files
│   └── tauri.conf.json
├── crates/sshconf/         # round-trip parser, editor, ssh -G reader
│   └── tests/fixtures/     # sample configs for golden tests
├── .github/workflows/      # ci.yml, release.yml
└── docs/                   # this plan, screenshots, decisions
```

| Tool | Used for |
| --- | --- |
| Tauri 2 | App shell, tray, installers |
| Vue 3 + TypeScript + Vite | Interface, with single-file components and the Composition API |
| Rust `std::process` | Starting `ssh` and the terminal from Rust, so the page never gets a shell permission |
| `rusqlite` | Connection history |
| Tauri plugins: global-shortcut, autostart, single-instance | Quick launch, start with the computer, one window only |
| `cargo test`, Vitest | Parser and interface tests |
| `pnpm` | Interface packages |

## Coding style

Tabs for indentation, opening braces on the same line, braces on every `if`, and one CSS declaration per line. Formatters and linters enforce this on save and in CI, so it never has to be remembered.

**Rules**

- Indent with tabs everywhere, displayed 4 wide.
- Opening brace on the same line as its statement; closing brace on its own line.
- Always use braces, even for a one-line `if`.
- `else` and `else if` start their own line after the closing brace, with the brace on the same line, like \`if\`: `else {`.
- CSS: selector and `{` on one line, one declaration per line, `}` on its own line, a blank line between rules. No single-line rules.
- No long one-liners: lines stay under 100 characters, chained calls go one per line, and ternaries are only for short values.

**TypeScript and Vue**

```ts
function statusLabel(host: Host): string {
	if (host.online) {
		return "Online";
	}
	else if (host.lastError) {
		return "Problem";
	}

	return "Offline";
}
```

**Rust**

```rust
fn is_valid_host_name(name: &str) -> bool {
	if name.is_empty() {
		return false;
	}

	name.chars().all(|c| c.is_ascii_alphanumeric() || ".-_".contains(c))
}
```

**CSS**

```css
.host-card {
	display: flex;
	flex-direction: column;
	gap: 8px;
	padding: 16px 18px;
	border: 1px solid var(--line);
	border-radius: 14px;
}

.host-card:hover {
	border-color: var(--accent);
}
```

**How it's enforced**

| Files | Tool | Key settings |
| --- | --- | --- |
| All | `.editorconfig` | `indent_style = tab`, `indent_size = 4`, `end_of_line = lf` |
| All | `.gitattributes` | `* text=auto eol=lf`, so Windows and Kubuntu commit identical files |
| `.ts`, `.vue` | ESLint as the formatter, with `eslint-plugin-vue` and `@stylistic` | `@stylistic/indent: tab`, `@stylistic/brace-style: stroustrup`, `curly: all`, `@stylistic/max-len: 100` |
| `.css` and `<style>` blocks | Prettier | `useTabs: true`, `tabWidth: 4`; always prints one declaration per line |
| `.rs` | `rustfmt.toml` | `hard_tabs = true`, `max_width = 100`, `control_brace_style = "ClosingNextLine"` |
| Editor | VS Code workspace settings | Format on save: ESLint for TS and Vue, Prettier for CSS, rust-analyzer for Rust |

Prettier isn't used for TypeScript or Vue, because it always prints `} else {` and can't be changed. ESLint's stylistic rules format those files instead. The rustfmt brace option is still a nightly-only setting, so formatting runs with `cargo +nightly fmt`; the build itself stays on stable Rust.

`ci.yml` runs `prettier --check`, `eslint` and `cargo fmt --check`, so anything off-style fails the build.

## Testing, CI and releases

The parser carries the risk, so it gets the most tests; everything is built and tested on Windows and Ubuntu runners on every push.

**Tests**

- Golden round-trip tests: each sample config in `tests/fixtures/` is parsed and saved unchanged, and must match byte for byte. Samples include CRLF files, tabs, `Include`, `Match`, wildcards and odd comments.
- Edit tests: add, change and delete a host, then check only that block changed.
- Command tests: host names that try to inject commands are refused.
- Interface tests with Vitest for search, tag filters and the form's validation.
- A manual checklist per release: connect, add, edit, tray and shortcut on both real machines.

**GitHub Actions**

| Workflow | Runs on | Does |
| --- | --- | --- |
| `ci.yml` | Every push and pull request | `cargo fmt`, `clippy`, `cargo test`, Vitest, a debug build on `windows-latest` and `ubuntu-22.04` |
| `release.yml` | A `v*` tag | Builds the NSIS installer, `.deb` and AppImage with `tauri-action`, attaches them to a draft GitHub release |

**Versions.** Semantic versions starting at `v0.1.0` after phase 1, with `v1.0.0` at the end of phase 6. Install updates by downloading the new installer; auto-update can come later.

## Risks and open questions

The biggest risk is a save that damages the config; backups, atomic writes and read-only handling of complex blocks keep that recoverable.

| Risk | Why it matters | Plan |
| --- | --- | --- |
| A save breaks or reformats the config | You lose SSH access until it's fixed | Read-only first (phases 1–2), golden tests, backup before every write, atomic swap |
| Tray and shortcuts behave differently on KDE | Quick launch feels broken on the laptop | KDE custom shortcut bound to `hop --quick`; test on Plasma early in phase 5 |
| Windows Terminal not installed or disabled | Connect does nothing | Detect `wt.exe`, fall back to PowerShell, let settings pick |
| Tauri permission names change between versions | Capabilities file stops building | Pin the Tauri version; check permission names in the docs at set-up |
| Unsigned Windows installer | SmartScreen warns on each new version | Accept for personal use; revisit signing if others use Hop |

**Open questions**

- [ ] Should both machines share one SSH config (through a private dotfiles repo), or keep separate configs?
- [ ] Konsole only, or a setting for other terminals on the laptop (Yakuake, Kitty)?
- [ ] Should a missing or broken host be hidden, or always shown with a warning?
- [ ] Should Hop start with the computer by default, or only when switched on in settings?
