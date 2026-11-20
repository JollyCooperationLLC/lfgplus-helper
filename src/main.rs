//! LFG+ Helper: lives in the tray (the notification area on Windows, the menu bar on macOS, the tray on Linux) and
//! uploads what the LFG+ addon holds whenever the game writes it.
//!
//!   lfgplus-helper                     run in the tray
//!   lfgplus-helper --once              one pass, print where things stand, exit (no tray)
//!   lfgplus-helper --list              print the folders searched and the saved data found, exit
//!   --api URL      where lfg.plus answers (default https://lfg.plus, or LFGPLUS_API)
//!   --wow PATH     the World of Warcraft folder to use
//!   --state FILE   the helper's state file (default: in the system's config folder)
//!   --interval S   seconds between looks at the saved data (default 60)

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use lfgplus_helper::engine::{self, Engine, Status};
use lfgplus_helper::icon::{TRAY_ZOOM, decode_tga, zoomed};
use lfgplus_helper::startup;
use lfgplus_helper::state::{self, State};
use lfgplus_helper::upload::HttpUploader;
use tao::event::{Event, StartCause};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

const SITE: &str = "https://lfg.plus";
const APP_NAME: &str = "LFG+ Helper";
const LOG_LIMIT: u64 = 256 * 1024;

struct Options {
    api: String,
    wow: Option<PathBuf>,
    state: PathBuf,
    interval: u64,
    once: bool,
    list: bool,
}

fn options() -> Result<Options, String> {
    let mut args = std::env::args().skip(1);
    let mut api = std::env::var("LFGPLUS_API").unwrap_or_else(|_| SITE.into());
    let (mut wow, mut state, mut interval, mut once, mut list) = (None, None, 60, false, false);
    while let Some(arg) = args.next() {
        let mut value = || args.next().ok_or(format!("{arg} needs a value"));
        match arg.as_str() {
            "--api" => api = value()?,
            "--wow" => wow = Some(PathBuf::from(value()?)),
            "--state" => state = Some(PathBuf::from(value()?)),
            "--interval" => interval = value()?.parse().map_err(|_| "--interval takes seconds".to_string())?,
            "--once" => once = true,
            "--list" => list = true,
            _ => return Err(format!("unknown option {arg}")),
        }
    }
    let state = state.unwrap_or_else(|| state::app_dir().join("state.txt"));
    Ok(Options { api, wow, state, interval: interval.max(5), once, list })
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Appends to the helper's log beside its state file, starting a new one past `LOG_LIMIT`.
fn log(state_file: &Path, lines: &mut Vec<String>) {
    if lines.is_empty() {
        return;
    }
    let path = state_file.with_file_name("helper.log");
    if fs::metadata(&path).is_ok_and(|m| m.len() > LOG_LIMIT) {
        let _ = fs::rename(&path, path.with_file_name("helper.old.log"));
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) {
        for line in lines.drain(..) {
            let _ = writeln!(file, "{} {line}", now());
        }
    }
    lines.clear();
}

fn engine_for(o: &Options) -> Engine<HttpUploader> {
    let mut engine = Engine::new(HttpUploader::new(&o.api), o.state.clone());
    if let Some(wow) = &o.wow
        && engine.state.wow.as_ref() != Some(wow)
    {
        engine.choose_wow(wow.clone());
    }
    engine
}

fn main() {
    let o = match options() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    if o.list {
        let engine = engine_for(&o);
        for root in engine.roots() {
            println!("searching {}", root.display());
        }
        for file in lfgplus_helper::wow::saved_files(&engine.roots()) {
            println!("found {}", file.path.display());
        }
        return;
    }
    if o.once {
        let mut engine = engine_for(&o);
        let status = engine.tick(now(), true);
        for line in &engine.journal {
            println!("{line}");
        }
        println!("{}", engine::status_line(&status, now()));
        log(&o.state, &mut engine.journal);
        std::process::exit(if matches!(status, Status::Ready(_)) { 0 } else { 1 });
    }
    // One helper at a time: a second one exits quietly.
    let Some(_lock) = single_instance(&o.state) else { return };
    run_tray(o);
}

fn single_instance(state_file: &Path) -> Option<File> {
    let path = state_file.with_file_name("helper.lock");
    fs::create_dir_all(path.parent()?).ok()?;
    let file = OpenOptions::new().create(true).truncate(false).write(true).open(path).ok()?;
    file.try_lock().ok()?;
    Some(file)
}

enum Command {
    UploadNow,
    ChooseWow(PathBuf),
    Autostart(bool),
}

enum UserEvent {
    Status(String),
    Menu(MenuEvent),
}

/// The background pass: looks every `interval` seconds, or at once when the tray asks.
fn spawn_worker(o: &Options, proxy: EventLoopProxy<UserEvent>) -> Sender<Command> {
    let (tx, rx) = mpsc::channel();
    let mut engine = engine_for(o);
    let (interval, state_file) = (Duration::from_secs(o.interval), o.state.clone());
    std::thread::spawn(move || {
        let mut force = true;
        loop {
            let status = engine.tick(now(), force);
            log(&state_file, &mut engine.journal);
            if proxy.send_event(UserEvent::Status(engine::status_line(&status, now()))).is_err() {
                return;
            }
            force = match rx.recv_timeout(interval) {
                Ok(Command::UploadNow) => true,
                Ok(Command::ChooseWow(dir)) => {
                    engine.choose_wow(dir);
                    true
                }
                Ok(Command::Autostart(on)) => {
                    engine.set_autostart(on);
                    false
                }
                Err(RecvTimeoutError::Timeout) => false,
                Err(RecvTimeoutError::Disconnected) => return,
            };
        }
    });
    tx
}

fn autostart() -> Option<auto_launch::AutoLaunch> {
    let exe = std::env::current_exe().ok()?;
    auto_launch::AutoLaunchBuilder::new()
        .set_app_name(APP_NAME)
        .set_app_path(exe.to_str()?)
        .set_macos_launch_mode(auto_launch::MacOSLaunchMode::LaunchAgent)
        .build()
        .ok()
}

/// Turns starting with the computer on or off. Returns what it is now.
fn apply_autostart(on: bool) -> bool {
    let Some(launch) = autostart() else { return false };
    let _ = if on { launch.enable() } else { launch.disable() };
    launch.is_enabled().unwrap_or(false)
}

fn open_site() {
    let mut command;
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command = std::process::Command::new("cmd");
        command.args(["/C", "start", "", SITE]).creation_flags(0x0800_0000); // no console window
    }
    #[cfg(target_os = "macos")]
    {
        command = std::process::Command::new("open");
        command.arg(SITE);
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        command = std::process::Command::new("xdg-open");
        command.arg(SITE);
    }
    let _ = command.spawn();
}

struct Tray {
    _icon: TrayIcon,
    status: MenuItem,
    upload: MenuItem,
    choose: MenuItem,
    start: CheckMenuItem,
    site: MenuItem,
    quit: MenuItem,
}

/// Whether the app must be moved before it can start with the computer: on macOS, while it still runs from a temporary
/// copy or a disk image.
fn must_move() -> bool {
    cfg!(target_os = "macos") && std::env::current_exe().is_ok_and(|exe| startup::must_move(&exe))
}

fn build_tray(autostart_on: bool, move_first: bool) -> Option<Tray> {
    let (rgba, w, h) = decode_tga(include_bytes!("../assets/icon.tga"))?;
    let icon = Icon::from_rgba(zoomed(&rgba, w, h, TRAY_ZOOM), w, h).ok()?;
    let status = MenuItem::new("Starting…", false, None);
    let upload = MenuItem::new("Upload now", true, None);
    let choose = MenuItem::new("Choose WoW folder…", true, None);
    let start = if move_first {
        CheckMenuItem::new("Move to Applications to start with your Mac", false, false, None)
    } else {
        CheckMenuItem::new("Start with my computer", true, autostart_on, None)
    };
    let site = MenuItem::new("Open lfg.plus", true, None);
    let quit = MenuItem::new("Quit", true, None);
    let menu = Menu::new();
    menu.append_items(&[
        &status,
        &PredefinedMenuItem::separator(),
        &upload,
        &choose,
        &start,
        &site,
        &PredefinedMenuItem::separator(),
        &quit,
    ])
    .ok()?;
    let builder = TrayIconBuilder::new().with_menu(Box::new(menu)).with_tooltip(APP_NAME);
    // The macOS menu bar draws icons from their shape alone, in its own colour, light or dark.
    #[cfg(target_os = "macos")]
    let builder = builder.with_icon_templated(icon);
    #[cfg(not(target_os = "macos"))]
    let builder = builder.with_icon(icon);
    let icon = builder.build().ok()?;
    Some(Tray { _icon: icon, status, upload, choose, start, site, quit })
}

fn run_tray(o: Options) {
    #[allow(unused_mut)]
    let mut event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    // A menu-bar app on macOS: no Dock icon and no app menu.
    #[cfg(target_os = "macos")]
    {
        use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};
        event_loop.set_activation_policy(ActivationPolicy::Accessory);
    }
    let proxy = event_loop.create_proxy();
    let menu_proxy = event_loop.create_proxy();
    MenuEvent::set_event_handler(Some(move |e| {
        let _ = menu_proxy.send_event(UserEvent::Menu(e));
    }));

    // The first run starts with the computer; after that the player's choice stands. An app that must be moved first
    // registers nothing and decides nothing until it has been.
    let saved = State::load(&o.state);
    let wanted = saved.autostart.unwrap_or(true);
    let move_first = must_move();
    let autostart_on = !move_first && apply_autostart(wanted);
    let worker = spawn_worker(&o, proxy);
    if saved.autostart.is_none() && !move_first {
        let _ = worker.send(Command::Autostart(wanted));
    }

    let mut tray: Option<Tray> = None;
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            // The tray is made once the loop runs, as macOS requires.
            Event::NewEvents(StartCause::Init) => {
                tray = build_tray(autostart_on, move_first);
                if tray.is_none() {
                    *control_flow = ControlFlow::Exit;
                }
            }
            Event::UserEvent(UserEvent::Status(text)) => {
                if let Some(t) = &tray {
                    t.status.set_text(&text);
                    let _ = t._icon.set_tooltip(Some(format!("{APP_NAME}\n{text}")));
                }
            }
            Event::UserEvent(UserEvent::Menu(e)) => {
                let Some(t) = &tray else { return };
                if e.id == t.upload.id() {
                    t.status.set_text("Uploading…");
                    let _ = worker.send(Command::UploadNow);
                } else if e.id == t.choose.id() {
                    if let Some(dir) =
                        rfd::FileDialog::new().set_title("Choose your World of Warcraft folder").pick_folder()
                    {
                        let _ = worker.send(Command::ChooseWow(dir));
                    }
                } else if e.id == t.start.id() {
                    let on = apply_autostart(t.start.is_checked());
                    t.start.set_checked(on);
                    let _ = worker.send(Command::Autostart(on));
                } else if e.id == t.site.id() {
                    open_site();
                } else if e.id == t.quit.id() {
                    tray = None;
                    *control_flow = ControlFlow::Exit;
                }
            }
            _ => {}
        }
    });
}
