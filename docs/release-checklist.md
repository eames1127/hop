# Release test checklist

Run this on both real machines, Windows 11 and Kubuntu (KDE Plasma), using the installers from the
GitHub release. Tick each box on each machine; note the version and date at the top of your copy.

| Machine | Version | Date | Tester |
| --- | --- | --- | --- |
| Windows 11 | | | |
| Kubuntu | | | |

## Before you start

Test against a copy of your config first, so a bad save can't cost you SSH access:

```powershell
Copy-Item -Recurse $HOME\.ssh $HOME\ssh-test
$env:HOP_SSH_DIR = "$HOME\ssh-test"
& "<path to installed hop.exe>"  # start Hop from this shell so it sees HOP_SSH_DIR
```

```bash
cp -r ~/.ssh ~/ssh-test
HOP_SSH_DIR=~/ssh-test hop
```

Make sure the copy holds at least one `Host *` block, one `Match` block, a multi-alias host and an
`Include`. Repeat the sections that write (hosts, keys, import) against the real `~/.ssh` only once
they pass on the copy.

## Install and uninstall

- [ ] Windows: the NSIS `.exe` installs (SmartScreen warns, as it's unsigned) and Hop appears in
      the Start menu.
- [ ] Kubuntu: `sudo apt install ./hop_*.deb` installs and Hop appears in the application menu.
- [ ] Kubuntu: the AppImage starts after `chmod +x`.
- [ ] Hop reports the expected version (e.g. in Apps & features or `apt show hop`).
- [ ] Uninstalling removes the app and leaves `~/.ssh/config` and its backups untouched.
- [ ] Reinstalling keeps history and settings in the data folder.

## First start with an existing config

- [ ] Every host, including ones from `Include` files, shows as a card with description, tags,
      address and last-used time.
- [ ] Hosts with comments, odd spacing or CRLF line endings display correctly.
- [ ] Search filters the cards as you type.

## Connect

- [ ] Windows Terminal: **Connect** opens `ssh <host>` in a new tab or window.
- [ ] PowerShell fallback: with Windows Terminal unavailable (or PowerShell chosen), Connect
      opens a PowerShell window.
- [ ] Konsole: Connect opens `ssh <host>`.
- [ ] Kitty and XTerm, if installed: Connect works after picking each in settings.
- [ ] Plain `ssh <host>` in a terminal still works the same as before.

## Add, edit and delete a host

For each action, compare the config with the newest backup (`git diff --no-index` or `diff`).

- [ ] Add a host: it appears as a card, and a `config.hop-backup-<timestamp>` was made first.
- [ ] Edit that host (address, user, description, tags): only its block changed.
- [ ] Edit an existing host from your original config: only its block changed, comments and line
      endings are kept.
- [ ] Delete a host: only its block is gone.
- [ ] After more than 10 saves, only the last 10 backups remain.
- [ ] An invalid host name or value is refused with a clear message, and nothing is written.

## Favourites and tray

- [ ] Marking a host as a favourite moves it up and survives a restart (`# fav: true` in the block).
- [ ] The tray menu lists favourites and recent hosts, and connecting from it works.
- [ ] Closing the window keeps Hop in the tray; **Quit Hop** exits.

## Quick launch

- [ ] Windows: Ctrl+Shift+H opens Hop with search focused; typing and Enter connects to the first
      match.
- [ ] KDE: a custom shortcut in System Settings → Shortcuts running `hop --quick` does the same,
      on both Wayland and X11 if you use both.
- [ ] Quick launch while Hop is already open brings up the existing window, not a second copy.

## Online checks and Needs attention

- [ ] Reachable hosts show as online, unreachable ones as offline.
- [ ] A host whose key file is missing is flagged and listed under **Needs attention**.
- [ ] A host not used for a long time is flagged as stale.
- [ ] Fixing the problem clears the flag.

## Details panel and history

- [ ] The details panel shows the `ssh -G` values, including inherited ones from `Host *`.
- [ ] The 30-day chart reflects the connections made during this test.

## Port forwards

- [ ] Add a forward to a host: it is saved as `# forward:` in that block only.
- [ ] Starting the tunnel makes the remote port reachable locally; stopping it frees the port.
- [ ] Deleting the forward removes only its line.

## SSH keys

- [ ] Settings lists your public keys with their fingerprints.
- [ ] Creating a new key runs `ssh-keygen` and the key appears in the list.
- [ ] Authorising another computer's public key on chosen hosts adds it to their
      `authorized_keys` once, and you can then log in with that key.
- [ ] No private key is read, shown or copied.

## Notes export and import

- [ ] Export on Windows, once with and once without addresses; check the file holds no private
      keys.
- [ ] Import on Kubuntu: existing hosts are updated, new ones can be added, and a backup is made.
- [ ] Hosts whose key file isn't on this computer are flagged.
- [ ] Repeat the other way, Kubuntu to Windows.

## Settings

- [ ] Terminal choice is saved and used on the next Connect.
- [ ] Clearing history empties the recents and the chart.
- [ ] Start with computer (off by default): turning it on starts Hop at login; turning it off
      stops that.
- [ ] Light and dark themes both look right and the choice survives a restart.

## Read-only blocks

- [ ] `Host *`, `Match` and multi-alias blocks show as read-only with no edit or delete.
- [ ] After all the saves above, those blocks are byte-for-byte unchanged.

## Sign-off

- [ ] All boxes ticked on both machines, or problems filed as issues.
- [ ] Test config copies (`ssh-test`) deleted.
