// SPDX-License-Identifier: GPL-3.0-or-later
//! What owns the windows (M1 plan, workflow revision D-090): decides what opens at launch (the
//! last workspace, or the welcome list), opens and creates workspaces, and replaces the window when
//! another workspace is opened. One [`MainWindow`] per screen: the welcome list, or an open
//! workspace; a workspace's engine lives exactly as long as its window.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::{Rc, Weak};
use std::time::Duration;

use auroraw_engine::{Engine, KnownWorkspace, OpenedWorkspace, paths};
use slint::{ComponentHandle, ModelRc, SharedString, Timer, VecModel};

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
            me: me.clone(),
        });
        launcher.first_screen()?;
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

        // The folder proposed for a new workspace follows its name until a person edits the folder.
        let automatic = Rc::new(RefCell::new(String::new()));
        {
            let (launcher, weak, automatic) = (self.me.clone(), ui.as_weak(), automatic.clone());
            ui.on_new_workspace(move || {
                let (Some(launcher), Some(ui)) = (launcher.upgrade(), weak.upgrade()) else {
                    return;
                };
                let name = ui.global::<Texts>().invoke_default_workspace_name();
                let location = launcher.default_location(&name);
                *automatic.borrow_mut() = location.clone();
                ui.set_new_name(name);
                ui.set_new_location(location.into());
                ui.set_dialog_error(SharedString::new());
                ui.set_dialog("new-workspace".into());
            });
        }
        {
            let (launcher, weak, automatic) = (self.me.clone(), ui.as_weak(), automatic.clone());
            ui.on_new_name_edited(move |name| {
                let (Some(launcher), Some(ui)) = (launcher.upgrade(), weak.upgrade()) else {
                    return;
                };
                if ui.get_new_location() == automatic.borrow().as_str() && !name.trim().is_empty() {
                    let location = launcher.default_location(&name);
                    *automatic.borrow_mut() = location.clone();
                    ui.set_new_location(location.into());
                }
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

    /// `<Pictures>/Auroraw/<name>`, numbered when it already exists (design note 001 §5.7).
    fn default_location(&self, name: &str) -> String {
        let parent = self.launch.pictures.join("Auroraw");
        let folder = paths::folder_name(name, "Workspace");
        paths::unique_folder(&parent, &folder)
            .to_string_lossy()
            .into_owned()
    }

    /// Creates the workspace the new-workspace dialog describes and opens it.
    fn create(self: &Rc<Self>, ui: &MainWindow) {
        let texts = ui.global::<Texts>();
        let name = ui.get_new_name().trim().to_string();
        if name.is_empty() {
            ui.set_dialog_error(texts.invoke_name_required());
            return;
        }
        let root = match paths::resolve(ui.get_new_location().as_str()) {
            Ok(root) => root,
            Err(e) => {
                ui.set_dialog_error(texts.invoke_cannot_create(e.to_string().into()));
                return;
            }
        };
        // The dialog shows back what was understood, so an error names the folder that was meant.
        ui.set_new_location(root.to_string_lossy().as_ref().into());
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
            "archive" => ui.get_import_archive(),
            "backup" => ui.get_import_backup(),
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
                    "source" => ui.set_import_source(text),
                    "archive" => ui.set_import_archive(text),
                    "backup" => ui.set_import_backup(text),
                    _ if new_workspace_dialog => ui.set_new_location(text),
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
