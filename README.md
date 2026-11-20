# LFG+ Helper

Uploads your [LFG+](https://lfg.plus) endorsements for you. It's a small app in your tray, so you never copy records
out of the game and paste them on lfg.plus/upload again.

## Overview

The helper sits in the notification area on Windows, the menu bar on macOS, or the tray on Linux. Click its icon and
the first line says where things stand:

```
Sent 12 endorsements 5 minutes ago
```

Whenever you log out of World of Warcraft or type `/reload`, the game saves what LFG+ holds. Within a minute the
helper uploads everything still waiting: the endorsements you gave and received, and the copies you carry for others
in your groups. The next time you log in, LFG+ shows them as uploaded, just as if you had pressed
**I uploaded these** yourself.

A few things to know:

- **Nothing to log in to.** The helper uses the same upload as lfg.plus/upload, which needs no account.
- **It never touches the game's saved data.** It only reads it. To tell LFG+ what went up, it keeps a tiny add-on of
  its own, **LFG+ Helper** (`LFGPlus_Helper`), next to LFG+ in `Interface/AddOns`. The game only ever reads that one,
  so nothing gets overwritten.
- **Your setup code stays put.** It's in the same saved data. The helper reads past it and never keeps, logs or
  sends it.
- **Every endorsement goes up the same way.** The helper sends each waiting line exactly as LFG+ wrote it, and can't
  tell one from another.
- **It catches up on its own.** If lfg.plus can't be reached, it tries again a minute later, then less often, up to
  every half hour.
- **Every game version and account.** It finds each game version installed (Retail, Classic, WoW Forever…) and every
  account in each.

## Getting started

1. **Set up LFG+.** Install the addon and link your account with your setup code (see the addon's
   [Getting started](https://github.com/JollyCooperationLLC/lfgplus-addon#getting-started)).
2. **Get the helper** from the [Releases](https://github.com/JollyCooperationLLC/lfgplus-helper/releases) page:
   - Windows: `lfgplus-helper-windows.exe`.
   - macOS: `lfgplus-helper-macos.zip`. Open it and drag **LFG+ Helper** into your Applications folder.
   - Linux: `lfgplus-helper-linux`. Make it runnable with `chmod +x lfgplus-helper-linux`.

   You can also build it yourself (see Development).
3. **Run it.** It finds World of Warcraft where the Battle.net app installed it, or in the usual folders. If it can't,
   choose **Choose WoW folder…** in its menu. It starts with your computer from then on.
4. **Play as usual.** After you log out or `/reload`, your endorsements go up on their own. LFG+'s Endorsements tab
   then lists only what you've given or received since.

The first time you log in after installing it, the game may list **LFG+ Helper** as a new add-on. Leave it on: it's
how LFG+ learns what the helper uploaded.

### Opening it on a Mac

The Mac app isn't notarized by Apple, so the first time you open it macOS stops it with "Apple could not verify
'LFG+ Helper' is free of malware". To open it anyway:

1. Open **LFG+ Helper** from Applications once, and close the warning.
2. Open **System Settings → Privacy & Security**, scroll down to the line about LFG+ Helper, and click **Open Anyway**.
3. Confirm with your password or Touch ID.

From then on it opens normally and lives in the menu bar, with no Dock icon. Run it from Applications, not from the
zip or your Downloads folder: macOS runs apps it hasn't checked from a temporary copy, so the menu shows **Move to
Applications to start with your Mac** until you do.

## Menu

| Item | What it does |
|---|---|
| *(the first line)* | Where things stand: what was sent and when, or what's wrong ("Can't reach lfg.plus. Trying again soon.") |
| **Upload now** | Look at the saved data right away and upload anything waiting |
| **Choose WoW folder…** | Pick your World of Warcraft folder, if the helper didn't find it |
| **Start with my computer** | On from the first run; untick it and that choice sticks |
| **Open lfg.plus** | Open the site in your browser |
| **Quit** | Close the helper until you start it again |

## Command line

| Option | What it does |
|---|---|
| `--once` | Look once, upload what's waiting, print where things stand, then exit (no tray icon) |
| `--list` | Print the folders it searches and the saved data it finds, then exit |
| `--wow PATH` | Use this World of Warcraft folder |
| `--interval SECONDS` | How often to look at the saved data (default 60) |
| `--state FILE` | Use this settings file instead of the usual one |
| `--api URL` | Where lfg.plus answers (default `https://lfg.plus`, or the `LFGPLUS_API` environment variable) |

## Configuring

There's nothing to set up beyond the menu. The helper keeps its settings and a short log in:

- Windows: `%APPDATA%\LFGPlusHelper`
- macOS: `~/Library/Application Support/LFGPlusHelper`
- Linux: `~/.config/LFGPlusHelper`

The settings file (`state.txt`) is plain text: the folder you chose, whether it starts with your computer, and how far
each account's endorsements were uploaded. The log (`helper.log`) holds counts and file paths only.

## Privacy

The helper reads each account's `WTF/Account/<ACCOUNT>/SavedVariables/LFGPlus.lua` and sends lfg.plus nothing but
the upload lines in it. It sends no cookies and no account details. Endorsed characters are public on lfg.plus until
their owner hides them, as with uploads you paste yourself.

## Support

Questions or bugs? Write to [support@lfg.plus](mailto:support@lfg.plus). Short answers are in the
[FAQ](https://lfg.plus/faq).

## Development

The helper is written in Rust. It reads SavedVariables with its own small parser (no Lua runs), uploads with `ureq`,
and draws the tray with `tray-icon`. This repository is checked out as the `helper/` submodule of the lfg.plus
repository.

### Layout

```
src/
  main.rs           the tray icon and menu, command-line options, one helper at a time
  engine.rs         one pass: find the saved data, upload what waits, remember it, tell LFG+; retries
  wow.rs            finds World of Warcraft, its game folders and every account's saved data
  saved.rs          the upload lines and their numbers in LFG+'s saved data, and which still wait
  lua.rs            reads and writes the part of Lua that SavedVariables files use (pure)
  upload.rs         POST /api/v1/uploads
  acks.rs           the LFGPlus_Helper add-on that tells LFG+ what went up
  state.rs          the helper's settings file
  startup.rs        on macOS, whether the app must be moved to Applications before it starts with the computer
  icon.rs           reads the tray icon (an uncompressed TGA)
assets/icon.tga     the tray icon: the LFG+ icon the addon shows, drawn a quarter larger
assets/app.ico      the .exe's icon: the LFG+ wordmark from 48 px up, the bubble alone below that
assets/app.icns     the Mac app's icon: the same drawing, sized for macOS
build.rs            puts app.ico, the name and the publisher into the Windows .exe
tools/make-icon.py  draws app.ico and app.icns (Pillow and Inter's variable font; both are committed)
tools/package-macos.sh  builds LFG+ Helper.app around a macOS build, signs it ad hoc and zips it
tests/              the whole pass on a fake World of Warcraft folder, and uploads against a local server
.github/workflows/  build.yml: format, lint, tests and builds on Windows, macOS and Linux; a release on every push to main
```

### Tests

You need [Rust](https://rustup.rs) 1.89 or newer.

```bash
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
cargo llvm-cov --summary-only --ignore-filename-regex 'main\.rs'   # line coverage, with cargo-llvm-cov
```

The SavedVariables parser also has property tests (fuzzed input never panics, strings and numbers round-trip).

On Linux the tray needs GTK 3 and an AppIndicator library to build and run. On Debian and Ubuntu:

```bash
sudo apt install libgtk-3-dev libxdo-dev libayatana-appindicator3-dev
```

GNOME shows tray icons only with the AppIndicator extension, which most distributions include.

### Releases

Every push runs `.github/workflows/build.yml` on Windows, macOS and Linux: format, lint, tests and a release build.
When all three pass on `main`, the builds are published on the
[Releases](https://github.com/JollyCooperationLLC/lfgplus-helper/releases) page. The version is the commit's UTC
time, `year.month.day.hourminutesecond` (for example `2026.11.20.083242`). The Mac app is one program for Apple
Silicon and Intel, signed ad hoc (no Apple account), so macOS asks once before opening it. To build one locally:

```bash
cargo build --release   # target/release/lfgplus-helper(.exe)
```

### Development and testing methodology

- `main.rs` stays thin: the tray only shows what the engine reports and passes clicks back. Everything else is plain
  functions over files and values, so tests can call them.
- Never write the game's own saved data. Anything LFG+ needs to know goes in the helper's add-on, written beside its
  place and renamed over it.
- Read the saved data as untrusted: limit nesting, refuse anything that isn't SavedVariables, and fuzz the parser.

## License

All rights reserved - Jolly Cooperation LLC.
