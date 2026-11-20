# LFG+ Helper

A small app that sits in your tray and uploads the endorsements the [LFG+](https://lfg.plus) addon holds, so you
never have to copy them out of the game and paste them on lfg.plus/upload.

It runs on Windows (notification area), macOS (menu bar) and Linux (tray or AppIndicator).

## What it does

1. **Finds World of Warcraft.** It looks where the Battle.net app installed it, then in the usual folders. You can
   pick the folder yourself from the tray menu.
2. **Watches the addon's saved data.** The game writes it to
   `WTF/Account/<ACCOUNT>/SavedVariables/LFGPlus.lua` when you log out or type `/reload`. The helper checks once a
   minute whether that file changed, for every game version and every account it finds.
3. **Uploads what's waiting.** These are the same lines the addon's upload box would give you: endorsements you gave
   and received, and copies you carry for your groups. Lines already uploaded, or too old for the site, are left out.
4. **Tells the addon what went up.** It writes a tiny add-on of its own, `LFGPlus_Helper`, next to LFG+ in
   `Interface/AddOns`. The game never writes there, so nothing gets overwritten. The next time you log in or
   `/reload`, LFG+ marks those endorsements as uploaded, just as if you had pressed "I uploaded these".

The tray menu shows where things stand ("Sent 12 endorsements 5 minutes ago"). It also has **Upload now**,
**Choose WoW folder…**, **Start with my computer**, **Open lfg.plus** and **Quit**.

## What it never does

- It never sends anything but the upload lines. The saved data also holds your setup code; the helper reads past it
  and never keeps, logs or sends it.
- It never writes to the game's own saved data.
- It doesn't log you in or need an account, and sends no cookies. It uses the same upload endpoint as
  lfg.plus/upload.
- It can't tell one endorsement from another. It sends every waiting line exactly as the addon printed it, and only
  ever counts how many it sent.

## Running it

```
lfgplus-helper                   run in the tray
lfgplus-helper --once            look once, upload what's waiting, print where things stand, exit
lfgplus-helper --list            print the folders it searches and the saved data it finds, exit

--wow PATH       use this World of Warcraft folder
--api URL        where lfg.plus answers (default https://lfg.plus, or the LFGPLUS_API environment variable)
--state FILE     the helper's own settings file
--interval S     seconds between looks at the saved data (default 60)
```

The helper keeps its settings and a short log in `%APPDATA%\LFGPlusHelper` (Windows),
`~/Library/Application Support/LFGPlusHelper` (macOS) or `~/.config/LFGPlusHelper` (Linux). The log holds counts
and file paths only.

The first run turns on **Start with my computer**. Untick it in the menu and that choice sticks.

## Building

You need [Rust](https://rustup.rs) 1.89 or newer.

```
cargo build --release
cargo test
cargo clippy --all-targets -- -D warnings
```

On Linux the tray needs GTK 3 and an AppIndicator library to build and run. On Debian and Ubuntu:

```
sudo apt install libgtk-3-dev libxdo-dev libayatana-appindicator3-dev
```

GNOME shows tray icons only with the AppIndicator extension, which most distributions include.

## How it's put together

| File | What it does |
|---|---|
| `src/lua.rs` | Reads and writes the small part of Lua that SavedVariables files use. No Lua runs. |
| `src/saved.rs` | Takes the upload lines and their numbers from the addon's saved data. |
| `src/wow.rs` | Finds World of Warcraft, its game folders and every account's saved data. |
| `src/upload.rs` | Sends lines to lfg.plus's upload endpoint. |
| `src/acks.rs` | Writes the `LFGPlus_Helper` add-on that tells LFG+ what went up. |
| `src/state.rs` | The helper's own settings file. |
| `src/engine.rs` | One pass: find, send, remember, tell the addon. |
| `src/main.rs` | The tray icon and menu. |

## License

All rights reserved, Jolly Cooperation LLC. See [LICENSE](LICENSE).
