//! LFG+ Helper: lives in the tray (the notification area on Windows, the menu bar on macOS, the tray on Linux) and
//! uploads what the LFG+ addon holds whenever the game writes it. The command line is in `cli.rs`.

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use std::ffi::OsStr;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use lfgplus_helper::cli::{self, Options, SITE};
use lfgplus_helper::engine::{self, Engine, Status};
use lfgplus_helper::icon::{TRAY_ZOOM, decode_tga, zoomed};
use lfgplus_helper::state::State;
use lfgplus_helper::upload::HttpUploader;
use lfgplus_helper::{VERSION, install, instance, notice, startup, update, user_agent};
use tao::event::{Event, StartCause};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

const APP_NAME: &str = "LFG+ Helper";

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
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

/// A window with one message and an OK button.
fn tell(text: &str) {
    rfd::MessageDialog::new().set_title(APP_NAME).set_description(text).set_buttons(rfd::MessageButtons::Ok).show();
}

fn main() {
    let o = match cli::parse(std::env::args().skip(1), std::env::var("LFGPLUS_API").ok()) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    if o.version {
        println!("{VERSION}");
        return;
    }
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
        instance::log(&o.state, &mut engine.journal, now());
        std::process::exit(if matches!(status, Status::Ready(_)) { 0 } else { 1 });
    }
    // One helper at a time. A second one says where the first is, and whether quitting it would update it.
    let Some(mut lock) = instance::lock(&o.state) else {
        tell(&notice::already_running(instance::running_version(&o.state).as_deref(), VERSION));
        return;
    };
    if let Some(target) = install_target(&o) {
        match move_to(&target, &o, lock) {
            Some(still_here) => lock = still_here,
            None => return,
        }
    }
    instance::note_running(&o.state, VERSION);
    run_tray(o, lock);
}

/// Where a player's own start should copy itself, when it isn't running from there already.
fn install_target(o: &Options) -> Option<PathBuf> {
    if !startup::players_start(o.plain, VERSION) {
        return None;
    }
    let target = install::install_path()?;
    let exe = std::env::current_exe().ok()?;
    install::needs_install(&exe, &target).then_some(target)
}

/// Copies this program to `target` and starts it from there. None when that copy is running now and this one should
/// stop; otherwise the lock to carry on here with (the copy couldn't be made or started).
fn move_to(target: &Path, o: &Options, lock: File) -> Option<File> {
    let exe = std::env::current_exe().ok();
    let copied = exe.as_deref().map(|exe| install::install(exe, target));
    if let Some(Err(e)) = &copied {
        instance::log(&o.state, &mut vec![format!("couldn't install to {}: {e}", target.display())], now());
    }
    if !matches!(copied, Some(Ok(()))) {
        return Some(lock);
    }
    // The copy takes the lock as it starts, so let go of it first.
    drop(lock);
    if std::process::Command::new(target).spawn().is_ok() {
        return None;
    }
    instance::lock(&o.state)
}

enum Command {
    UploadNow,
    ChooseWow(PathBuf),
    ForgetWow,
    Autostart(bool),
    Welcomed,
}

enum UserEvent {
    /// The status line, and whether the player chose the WoW folder.
    Status(String, bool),
    /// A newer release.
    Update(String),
    Menu(MenuEvent),
}

/// The background pass: looks every `interval` seconds, or at once when the tray asks; asks about a newer release once
/// a day.
fn spawn_worker(o: &Options, proxy: EventLoopProxy<UserEvent>) -> Sender<Command> {
    let (tx, rx) = mpsc::channel();
    let mut engine = engine_for(o);
    let (interval, state_file) = (Duration::from_secs(o.interval), o.state.clone());
    std::thread::spawn(move || {
        let mut force = true;
        let mut next_check = 0;
        loop {
            let status = engine.tick(now(), force);
            instance::log(&state_file, &mut engine.journal, now());
            let chosen = engine.state.wow.is_some();
            if proxy.send_event(UserEvent::Status(engine::status_line(&status, now()), chosen)).is_err() {
                return;
            }
            if update::parse(VERSION).is_some() && now() >= next_check {
                next_check = now() + update::CHECK_EVERY;
                if let Some(latest) = update::latest(update::RELEASES, &user_agent())
                    && update::is_newer(&latest, VERSION)
                {
                    let _ = proxy.send_event(UserEvent::Update(latest));
                }
            }
            force = match rx.recv_timeout(interval) {
                Ok(Command::UploadNow) => true,
                Ok(Command::ChooseWow(dir)) => {
                    engine.choose_wow(dir);
                    true
                }
                Ok(Command::ForgetWow) => {
                    engine.forget_wow();
                    true
                }
                Ok(Command::Autostart(on)) => {
                    engine.set_autostart(on);
                    false
                }
                Ok(Command::Welcomed) => {
                    engine.set_welcomed();
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
        .set_app_path(&startup::entry(exe.to_str()?))
        .set_macos_launch_mode(auto_launch::MacOSLaunchMode::LaunchAgent)
        .build()
        .ok()
}

/// Turns starting with the computer on or off, for this copy of the program. Returns what it is now.
fn apply_autostart(on: bool) -> bool {
    let Some(launch) = autostart() else { return false };
    let _ = if on { launch.enable() } else { launch.disable() };
    launch.is_enabled().unwrap_or(false)
}

/// Whether the helper starts with the computer, whichever copy of it that is.
fn autostart_is_on() -> bool {
    autostart().and_then(|launch| launch.is_enabled().ok()).unwrap_or(false)
}

/// Opens an address in the browser, or a file in the program the system uses for it.
fn open(target: &OsStr) {
    let mut command;
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command = std::process::Command::new("cmd");
        command.arg("/C").arg("start").arg("").arg(target).creation_flags(0x0800_0000); // no console window
    }
    #[cfg(target_os = "macos")]
    {
        command = std::process::Command::new("open");
        command.arg(target);
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        command = std::process::Command::new("xdg-open");
        command.arg(target);
    }
    let _ = command.spawn();
}

/// Opens the log, made empty first if nothing was logged yet.
fn open_log(state_file: &Path) {
    let path = instance::log_path(state_file);
    let _ = OpenOptions::new().create(true).append(true).open(&path);
    open(path.as_os_str());
}

struct Tray {
    icon: TrayIcon,
    status: MenuItem,
    upload: MenuItem,
    choose: MenuItem,
    forget: MenuItem,
    start: CheckMenuItem,
    site: MenuItem,
    log: MenuItem,
    version: MenuItem,
    quit: MenuItem,
}

/// Whether the app must be moved before it can start with the computer: on macOS, while it still runs from a temporary
/// copy or a disk image.
fn must_move() -> bool {
    cfg!(target_os = "macos") && std::env::current_exe().is_ok_and(|exe| startup::must_move(&exe))
}

fn build_tray(autostart_on: bool, may_register: bool, move_first: bool, chosen: bool) -> Option<Tray> {
    let (rgba, w, h) = decode_tga(include_bytes!("../assets/icon.tga"))?;
    let icon = Icon::from_rgba(zoomed(&rgba, w, h, TRAY_ZOOM), w, h).ok()?;
    let status = MenuItem::new("Starting…", false, None);
    let upload = MenuItem::new("Upload now", true, None);
    let choose = MenuItem::new("Choose WoW folder…", true, None);
    let forget = MenuItem::new("Find WoW on its own", chosen, None);
    let start = if move_first {
        CheckMenuItem::new("Move to Applications to start with your Mac", false, false, None)
    } else {
        CheckMenuItem::new("Start with my computer", may_register, autostart_on, None)
    };
    let site = MenuItem::new("Open lfg.plus", true, None);
    let log = MenuItem::new("Open log", true, None);
    let version_text = match update::parse(VERSION) {
        Some(_) => format!("Version {VERSION}"),
        None => "Development build".to_string(),
    };
    let version = MenuItem::new(version_text, false, None);
    let quit = MenuItem::new("Quit", true, None);
    let menu = Menu::new();
    menu.append_items(&[
        &status,
        &PredefinedMenuItem::separator(),
        &upload,
        &choose,
        &forget,
        &start,
        &site,
        &log,
        &PredefinedMenuItem::separator(),
        &version,
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
    Some(Tray { icon, status, upload, choose, forget, start, site, log, version, quit })
}

fn run_tray(o: Options, _lock: File) {
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

    // The first run starts with the computer; after that the player's choice stands. Starting it again points it at
    // this copy. An app that must be moved first, a build someone is working on and a run with options register
    // nothing and decide nothing.
    let saved = State::load(&o.state);
    let wanted = saved.autostart.unwrap_or(true);
    let move_first = must_move();
    let may_register = !move_first && startup::players_start(o.plain, VERSION);
    let autostart_on = if may_register { apply_autostart(wanted) } else { !move_first && autostart_is_on() };
    let worker = spawn_worker(&o, proxy);
    if saved.autostart.is_none() && may_register {
        let _ = worker.send(Command::Autostart(wanted));
    }
    let state_file = o.state.clone();

    let mut tray: Option<Tray> = None;
    let mut newer: Option<String> = None;
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            // The tray is made once the loop runs, as macOS requires.
            Event::NewEvents(StartCause::Init) => {
                tray = build_tray(autostart_on, may_register, move_first, saved.wow.is_some());
                if tray.is_none() {
                    *control_flow = ControlFlow::Exit;
                    return;
                }
                // Say hello once, so a player who sees no window knows it's running and where. A Mac app still in the
                // disk image is told to move it every time, and welcomed properly once it has been.
                if !saved.welcomed {
                    tell(&notice::welcome(move_first));
                    if !move_first {
                        let _ = worker.send(Command::Welcomed);
                    }
                }
            }
            // A Mac app opened again while it runs: macOS doesn't start a second one, it tells this one.
            Event::Reopen { .. } => tell(&notice::already_running(None, VERSION)),
            Event::UserEvent(UserEvent::Status(text, chosen)) => {
                if let Some(t) = &tray {
                    t.status.set_text(&text);
                    t.forget.set_enabled(chosen);
                    let _ = t.icon.set_tooltip(Some(format!("{APP_NAME}\n{text}")));
                }
            }
            Event::UserEvent(UserEvent::Update(latest)) => {
                if let Some(t) = &tray {
                    t.version.set_text(format!("Get the new version ({latest})…"));
                    t.version.set_enabled(true);
                }
                newer = Some(latest);
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
                } else if e.id == t.forget.id() {
                    t.forget.set_enabled(false);
                    let _ = worker.send(Command::ForgetWow);
                } else if e.id == t.start.id() {
                    let on = apply_autostart(t.start.is_checked());
                    t.start.set_checked(on);
                    let _ = worker.send(Command::Autostart(on));
                } else if e.id == t.site.id() {
                    open(OsStr::new(SITE));
                } else if e.id == t.log.id() {
                    open_log(&state_file);
                } else if e.id == t.version.id() && newer.is_some() {
                    open(OsStr::new(&format!("{}/latest", update::RELEASES)));
                } else if e.id == t.quit.id() {
                    tray = None;
                    *control_flow = ControlFlow::Exit;
                }
            }
            _ => {}
        }
    });
}
