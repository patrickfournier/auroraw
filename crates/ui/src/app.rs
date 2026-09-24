// SPDX-License-Identifier: GPL-3.0-or-later
//! What owns the windows (M1 plan, workflow revision D-090): decides what opens at launch (the
//! last workspace, or the welcome list), opens and creates workspaces, and replaces the window when
//! another workspace is opened. One [`MainWindow`] per screen: the welcome list, or an open
//! workspace; a workspace's engine lives exactly as long as its window.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};
use std::time::Duration;

use auroraw_engine::{Engine, KnownWorkspace, OpenedWorkspace, paths};
use slint::platform::{Key, WindowEvent};
use slint::{ComponentHandle, ModelRc, SharedString, Timer, VecModel};

use crate::app_settings::{AppSettings, resolve_language};
use crate::generated::{KnownEntry, MainWindow, Texts};
use crate::{Launch, Platform, start_directory, workspace_shell};

/// A window and what keeps it alive (the timers that feed it).
pub(crate) struct Shell {
    pub(crate) ui: MainWindow,
    _timers: Vec<Timer>,
}

impl Shell {
    pub(crate) fn new(ui: MainWindow, timers: Vec<Timer>) -> Self {
        Self {
            ui,
            _timers: timers,
        }
    }
}

impl Drop for Launcher {
    /// A shown window keeps its component (and, through the component, the engine) alive: hide the
    /// one on screen so that everything is let go of when the launcher is.
    fn drop(&mut self) {
        if let Some(shell) = self.screen.get_mut().take() {
            let _ = shell.ui.hide();
        }
    }
}

/// What the welcome list says above the list.
enum Note {
    None,
    LastNotFound(PathBuf),
}

pub(crate) struct Launcher {
    pub(crate) launch: Launch,
    pub(crate) platform: Platform,
    screen: RefCell<Option<Shell>>,
    /// The workspace on screen, so that opening it again is not a second attempt to lock it.
    current: RefCell<Option<PathBuf>>,
    settings: RefCell<AppSettings>,
    me: Weak<Launcher>,
}

impl Launcher {
    /// Opens what a launch should open: the workspace named on the command line, else the last one
    /// opened, else the welcome list (with a note when the last one cannot be found).
    pub(crate) fn start(
        launch: Launch,
        platform: Platform,
    ) -> Result<Rc<Self>, slint::PlatformError> {
        let launcher = Rc::new_cyclic(|me| Self {
            launch,
            platform,
            screen: RefCell::new(None),
            current: RefCell::new(None),
            settings: RefCell::new(AppSettings::default()),
            me: me.clone(),
        });
        *launcher.settings.borrow_mut() = AppSettings::load(&launcher.settings_path());
        launcher.first_screen()?;
        launcher.apply_language();
        Ok(launcher)
    }

    /// The window on screen (a handle to it: it stays the same window until another workspace opens).
    pub(crate) fn window(&self) -> MainWindow {
        self.screen
            .borrow()
            .as_ref()
            .expect("a screen is always open once the launcher has started")
            .ui
            .clone_strong()
    }

    fn first_screen(self: &Rc<Self>) -> Result<(), slint::PlatformError> {
        let dirs = &self.launch.dirs;
        if let Some(path) = self.launch.open.clone() {
            return match self.open_typed(&path.to_string_lossy()) {
                Ok(()) => Ok(()),
                Err(message) => {
                    self.show_welcome(Note::None)?;
                    self.notify(&self.window(), &message);
                    Ok(())
                }
            };
        }
        match Engine::last_opened_workspace(dirs) {
            Some(last) if last.found => match self.open_typed(&last.path.to_string_lossy()) {
                Ok(()) => Ok(()),
                Err(message) => {
                    self.show_welcome(Note::None)?;
                    self.notify(&self.window(), &message);
                    Ok(())
                }
            },
            Some(last) => self.show_welcome(Note::LastNotFound(last.path)),
            None => self.show_welcome(Note::None),
        }
    }

    /// Shows a message over the window, that a person dismisses.
    pub(crate) fn notify(&self, ui: &MainWindow, message: &str) {
        ui.set_notice(message.into());
    }

    fn show_welcome(self: &Rc<Self>, note: Note) -> Result<(), slint::PlatformError> {
        let ui = MainWindow::new()?;
        ui.set_screen("welcome".into());
        let known = Rc::new(Engine::known_workspaces(&self.launch.dirs));
        ui.set_known(ModelRc::new(VecModel::from(
            known.iter().map(known_entry).collect::<Vec<_>>(),
        )));
        if let Note::LastNotFound(path) = note {
            ui.set_welcome_note(
                ui.global::<Texts>()
                    .invoke_last_not_found(path.to_string_lossy().as_ref().into()),
            );
        }
        self.wire(&ui);

        {
            let (launcher, known, weak) = (self.me.clone(), known.clone(), ui.as_weak());
            ui.on_open_known(move |index| {
                let (Some(launcher), Some(ui)) = (launcher.upgrade(), weak.upgrade()) else {
                    return;
                };
                let Some(entry) = usize::try_from(index).ok().and_then(|i| known.get(i)) else {
                    return;
                };
                if let Err(message) = launcher.open_typed(&entry.path.to_string_lossy()) {
                    launcher.notify(&ui, &message);
                }
            });
        }
        {
            let (launcher, known) = (self.me.clone(), known.clone());
            ui.on_forget_known(move |index| {
                let (Some(launcher), Some(entry)) = (
                    launcher.upgrade(),
                    usize::try_from(index).ok().and_then(|i| known.get(i)),
                ) else {
                    return;
                };
                let _ = Engine::forget_workspace(&launcher.launch.dirs, entry.workspace_id);
                // The list is rebuilt from the registry, by showing the welcome screen again.
                let launcher = launcher.clone();
                Timer::single_shot(Duration::ZERO, move || {
                    let _ = launcher.show_welcome(Note::None);
                });
            });
        }

        *self.current.borrow_mut() = None;
        self.replace(Shell::new(ui, Vec::new()));
        Ok(())
    }

    /// Opens the workspace at the folder a person typed or picked. The error is text for a message.
    pub(crate) fn open_typed(self: &Rc<Self>, typed: &str) -> Result<(), String> {
        let root = paths::resolve(typed).map_err(|e| self.cannot_open(e.to_string()))?;
        if self.current.borrow().as_deref() == Some(root.as_path()) {
            return Ok(());
        }
        let opened = Engine::open_workspace(&root, &self.launch.dirs)
            .map_err(|e| self.cannot_open(e.to_string()))?;
        self.show_workspace(opened, false)
            .map_err(|e| self.cannot_open(e.to_string()))
    }

    fn cannot_open(&self, reason: String) -> String {
        // Any window can speak: `Texts` is the same in all of them.
        match self.screen.borrow().as_ref() {
            Some(shell) => shell
                .ui
                .global::<Texts>()
                .invoke_cannot_open(reason.into())
                .to_string(),
            None => reason,
        }
    }

    fn show_workspace(
        self: &Rc<Self>,
        opened: OpenedWorkspace,
        created: bool,
    ) -> Result<(), slint::PlatformError> {
        let (rebuilt, name, root) = (opened.rebuilt, opened.name.clone(), opened.root.clone());
        let shell = workspace_shell::attach(self, opened, created)?;
        *self.current.borrow_mut() = Some(root);
        self.wire(&shell.ui);
        if rebuilt {
            self.notify(
                &shell.ui,
                shell
                    .ui
                    .global::<Texts>()
                    .invoke_rebuilt_note(name.into())
                    .as_str(),
            );
        }
        self.replace(shell);
        Ok(())
    }

    /// Puts `shell` on screen and lets go of the window it replaces, from the next turn of the
    /// event loop: the callback that asked for the change may still be running in the old one.
    fn replace(&self, shell: Shell) {
        shell.ui.window().on_close_requested(|| {
            let _ = slint::quit_event_loop();
            slint::CloseRequestResponse::HideWindow
        });
        let _ = shell.ui.show();
        let old = self.screen.borrow_mut().replace(shell);
        if let Some(old) = old {
            Timer::single_shot(Duration::ZERO, move || {
                let _ = old.ui.hide();
                drop(old);
            });
        }
    }

    /// What every window does the same way: the dialogs for a new or another workspace, the folder
    /// dialogs behind every Browse button, and the message banner.
    pub(crate) fn wire(self: &Rc<Self>, ui: &MainWindow) {
        ui.set_modifier_name(
            if cfg!(target_os = "macos") {
                "⌘"
            } else {
                "Ctrl"
            }
            .into(),
        );
        ui.set_redo_shortcut(
            if cfg!(target_os = "windows") {
                "Ctrl+Y"
            } else if cfg!(target_os = "macos") {
                "⌘+Shift+Z"
            } else {
                "Ctrl+Shift+Z"
            }
            .into(),
        );
        ui.on_dismiss_notice({
            let weak = ui.as_weak();
            move || {
                if let Some(ui) = weak.upgrade() {
                    ui.set_notice(SharedString::new());
                }
            }
        });
        ui.on_close_dialog({
            let weak = ui.as_weak();
            move || {
                if let Some(ui) = weak.upgrade() {
                    ui.set_dialog(SharedString::new());
                }
            }
        });

        {
            let (launcher, weak) = (self.me.clone(), ui.as_weak());
            ui.on_new_workspace(move || {
                let (Some(launcher), Some(ui)) = (launcher.upgrade(), weak.upgrade()) else {
                    return;
                };
                launcher.new_workspace_dialog(&ui);
            });
        }
        {
            let (launcher, weak) = (self.me.clone(), ui.as_weak());
            ui.on_new_name_edited(move |_| {
                let (Some(launcher), Some(ui)) = (launcher.upgrade(), weak.upgrade()) else {
                    return;
                };
                launcher.update_new_preview(&ui);
            });
        }
        {
            let (launcher, weak) = (self.me.clone(), ui.as_weak());
            ui.on_new_location_edited(move |_| {
                let (Some(launcher), Some(ui)) = (launcher.upgrade(), weak.upgrade()) else {
                    return;
                };
                launcher.update_new_preview(&ui);
            });
        }
        {
            let (launcher, weak) = (self.me.clone(), ui.as_weak());
            ui.on_command(move |id| {
                let (Some(launcher), Some(ui)) = (launcher.upgrade(), weak.upgrade()) else {
                    return;
                };
                launcher.run_command(&ui, id.as_str());
            });
        }
        {
            let (launcher, weak) = (self.me.clone(), ui.as_weak());
            ui.on_language_chosen(move |code| {
                let (Some(launcher), Some(ui)) = (launcher.upgrade(), weak.upgrade()) else {
                    return;
                };
                launcher.choose_language(&ui, code.as_str());
            });
        }
        {
            let (launcher, weak) = (self.me.clone(), ui.as_weak());
            ui.on_create_workspace(move || {
                let (Some(launcher), Some(ui)) = (launcher.upgrade(), weak.upgrade()) else {
                    return;
                };
                launcher.create(&ui);
            });
        }
        {
            let (launcher, weak) = (self.me.clone(), ui.as_weak());
            ui.on_open_workspace(move || {
                let (Some(launcher), Some(ui)) = (launcher.upgrade(), weak.upgrade()) else {
                    return;
                };
                launcher.pick("workspace", &ui);
            });
        }
        {
            let (launcher, weak) = (self.me.clone(), ui.as_weak());
            ui.on_browse_folder(move |which| {
                let (Some(launcher), Some(ui)) = (launcher.upgrade(), weak.upgrade()) else {
                    return;
                };
                launcher.pick(which.as_str(), &ui);
            });
        }
    }

    fn settings_path(&self) -> PathBuf {
        self.launch.dirs.data.join("app-settings.json")
    }

    /// Selects the language of the settings (the bundled translations exist once a window has).
    fn apply_language(&self) {
        let language = self.settings.borrow().language.clone();
        if language != "system" {
            let _ = slint::select_bundled_translation(resolve_language(&language));
        }
    }

    fn choose_language(&self, ui: &MainWindow, code: &str) {
        self.settings.borrow_mut().language = code.to_string();
        self.settings.borrow().save(&self.settings_path());
        let _ = slint::select_bundled_translation(resolve_language(code));
        ui.set_language(code.into());
    }

    /// Opens the New workspace dialog: the folder that will hold the workspace's own folder
    /// (`<Pictures>/Auroraw`), and a name whose folder does not exist there yet.
    fn new_workspace_dialog(&self, ui: &MainWindow) {
        let parent = self.launch.pictures.join("Auroraw");
        let name = free_workspace_name(
            &parent,
            &ui.global::<Texts>().invoke_default_workspace_name(),
        );
        ui.set_new_name(name.into());
        ui.set_new_location(parent.to_string_lossy().as_ref().into());
        ui.set_dialog_error(SharedString::new());
        self.update_new_preview(ui);
        ui.set_dialog("new-workspace".into());
    }

    /// Shows where the workspace would be created: the folder field's folder and, in it, the
    /// name's own folder (`/home/patrick/Pictures/Auroraw` and `Main` make `.../Auroraw/Main`).
    fn update_new_preview(&self, ui: &MainWindow) {
        let name = ui.get_new_name();
        let preview = match (name.trim(), paths::resolve(ui.get_new_location().as_str())) {
            ("", _) | (_, Err(_)) => SharedString::new(),
            (name, Ok(parent)) => ui.global::<Texts>().invoke_workspace_will_be_at(
                parent
                    .join(paths::folder_name(name, "Workspace"))
                    .to_string_lossy()
                    .as_ref()
                    .into(),
            ),
        };
        ui.set_new_preview(preview);
    }

    /// Carries out a command of the menus or their shortcuts (`commands.rs`); false for an id it
    /// does not know.
    pub(crate) fn run_command(self: &Rc<Self>, ui: &MainWindow, id: &str) -> bool {
        match id {
            "file.new-workspace" => self.new_workspace_dialog(ui),
            "file.open-workspace" => self.pick("workspace", ui),
            "file.settings" => {
                ui.set_language(self.settings.borrow().language.as_str().into());
                ui.set_dialog("settings".into());
            }
            "file.import" => {
                if ui.get_screen() == "workspace" {
                    ui.invoke_refresh_volumes();
                    ui.invoke_import_fields_changed();
                    ui.set_dialog("import".into());
                }
            }
            "file.quit" => {
                let _ = slint::quit_event_loop();
            }
            "help.about" => {
                ui.set_about_version(Engine::version().into());
                ui.set_dialog("about".into());
            }
            "edit.undo" => shortcut(ui, "z", false),
            // The platform's own redo, as Slint recognises it: Ctrl+Y on Windows, Ctrl+Shift+Z elsewhere.
            "edit.redo" if cfg!(target_os = "windows") => shortcut(ui, "y", false),
            "edit.redo" => shortcut(ui, "z", true),
            "edit.cut" => shortcut(ui, "x", false),
            "edit.copy" => shortcut(ui, "c", false),
            "edit.paste" => shortcut(ui, "v", false),
            "edit.select-all" => shortcut(ui, "a", false),
            "edit.delete" => press(ui, &Key::Delete.into()),
            _ => return false,
        }
        true
    }

    /// Creates the workspace the new-workspace dialog describes and opens it.
    fn create(self: &Rc<Self>, ui: &MainWindow) {
        let texts = ui.global::<Texts>();
        let name = ui.get_new_name().trim().to_string();
        if name.is_empty() {
            ui.set_dialog_error(texts.invoke_name_required());
            return;
        }
        let parent = match paths::resolve(ui.get_new_location().as_str()) {
            Ok(parent) => parent,
            Err(e) => {
                ui.set_dialog_error(texts.invoke_cannot_create(e.to_string().into()));
                return;
            }
        };
        // The dialog shows back what was understood, so an error names the folder that was meant.
        ui.set_new_location(parent.to_string_lossy().as_ref().into());
        let root = parent.join(paths::folder_name(&name, "Workspace"));
        self.update_new_preview(ui);
        if !is_free(&root) {
            ui.set_dialog_error(texts.invoke_folder_taken(root.to_string_lossy().as_ref().into()));
            return;
        }
        let opened = match Engine::create_workspace(&root, &name, &self.launch.dirs) {
            Ok(opened) => opened,
            Err(e) => {
                ui.set_dialog_error(texts.invoke_cannot_create(e.to_string().into()));
                return;
            }
        };
        if let Err(e) = self.show_workspace(opened, true) {
            ui.set_dialog_error(texts.invoke_cannot_create(e.to_string().into()));
        }
    }

    /// Opens the system's folder dialog for the field `which` names, and applies the answer: a
    /// workspace to open (`workspace` from the welcome screen or a menu), the new workspace's folder
    /// (`workspace` while its dialog is open), or one of the import folders.
    pub(crate) fn pick(self: &Rc<Self>, which: &str, ui: &MainWindow) {
        if ui.get_picking() {
            return;
        }
        let new_workspace_dialog = ui.get_dialog() == "new-workspace";
        let current = match which {
            "source" => ui.get_import_source(),
            "archive" => ui.get_import_destination(),
            "backup" => ui.get_import_backup(),
            "add-source" => ui.get_source_folder(),
            _ if new_workspace_dialog => ui.get_new_location(),
            _ => SharedString::new(),
        };
        let title = ui.global::<Texts>().invoke_pick_title(which.into());
        ui.set_picking(true);
        let which = which.to_string();
        let answer = {
            let (launcher, weak) = (self.me.clone(), ui.as_weak());
            Box::new(move |chosen: Option<PathBuf>| {
                let (Some(launcher), Some(ui)) = (launcher.upgrade(), weak.upgrade()) else {
                    return;
                };
                ui.set_picking(false);
                let Some(folder) = chosen else { return };
                let text: SharedString = folder.to_string_lossy().as_ref().into();
                match which.as_str() {
                    "source" => {
                        ui.set_import_source(text);
                        ui.invoke_import_fields_changed();
                    }
                    "archive" => {
                        ui.set_import_destination(text);
                        ui.invoke_import_fields_changed();
                    }
                    "backup" => ui.set_import_backup(text),
                    "add-source" => ui.set_source_folder(text),
                    _ if new_workspace_dialog => {
                        ui.set_new_location(text);
                        launcher.update_new_preview(&ui);
                    }
                    _ => {
                        if let Err(message) = launcher.open_typed(&text) {
                            launcher.notify(&ui, &message);
                        }
                    }
                }
            })
        };
        if !(self.platform.pick_folder)(&title, start_directory(current.as_str()), answer) {
            ui.set_picking(false);
        }
    }
}

/// `base`, or `base 2`, `base 3`... when a folder of that name is already in `parent`: the name to
/// offer for a new workspace.
fn free_workspace_name(parent: &Path, base: &str) -> String {
    let taken = |name: &str| parent.join(paths::folder_name(name, "Workspace")).exists();
    if !taken(base) {
        return base.to_string();
    }
    (2u32..)
        .map(|n| format!("{base} {n}"))
        .find(|name| !taken(name))
        .expect("some number is free")
}

/// Whether a workspace can be made in `folder`: it does not exist, or is an empty folder.
fn is_free(folder: &Path) -> bool {
    match std::fs::read_dir(folder) {
        Ok(mut entries) => entries.next().is_none(),
        Err(_) => !folder.exists(),
    }
}

fn known_entry(known: &KnownWorkspace) -> KnownEntry {
    KnownEntry {
        name: known.name.clone().into(),
        path: known.path.to_string_lossy().as_ref().into(),
        opened: known
            .opened
            .map(|when| when.to_string().chars().take(10).collect::<String>())
            .unwrap_or_default()
            .into(),
        found: known.found,
    }
}

/// A key pressed and released, delivered to whatever has the keyboard in the window.
fn press(ui: &MainWindow, key: &SharedString) {
    let window = ui.window();
    window.dispatch_event(WindowEvent::KeyPressed { text: key.clone() });
    window.dispatch_event(WindowEvent::KeyReleased { text: key.clone() });
}

/// The shortcut modifier plus `letter`: the text field that has the keyboard does what it would
/// for a person pressing it, which is how the Edit menu reaches the field it is for. Slint reports
/// the Command key of a Mac as `control`, so it is the same key on every platform.
fn shortcut(ui: &MainWindow, letter: &str, shift: bool) {
    let modifier: SharedString = Key::Control.into();
    let shift_key: SharedString = Key::Shift.into();
    let window = ui.window();
    window.dispatch_event(WindowEvent::KeyPressed {
        text: modifier.clone(),
    });
    if shift {
        window.dispatch_event(WindowEvent::KeyPressed {
            text: shift_key.clone(),
        });
    }
    press(ui, &letter.into());
    if shift {
        window.dispatch_event(WindowEvent::KeyReleased { text: shift_key });
    }
    window.dispatch_event(WindowEvent::KeyReleased { text: modifier });
}
