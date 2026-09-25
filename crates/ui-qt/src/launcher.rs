// SPDX-License-Identifier: GPL-3.0-or-later
//! The launcher: which workspaces this machine knows, creating and opening one, and the interface's
//! own settings (the language). The Qt-side twin of `crates/ui/src/app.rs` (the Slint launcher), on
//! the same engine calls.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qstringlist.h");
        type QStringList = cxx_qt_lib::QStringList;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(QString, screen)]
        #[qproperty(QString, workspace_name, cxx_name = "workspaceName")]
        #[qproperty(QString, note)]
        #[qproperty(QStringList, known_names, cxx_name = "knownNames")]
        #[qproperty(QStringList, known_paths, cxx_name = "knownPaths")]
        #[qproperty(QString, language)]
        #[qproperty(QString, effective_language, cxx_name = "effectiveLanguage")]
        type Launcher = super::LauncherRust;

        /// Opens the workspace named at launch, else the last one, or leaves the welcome screen.
        #[qinvokable]
        fn start(self: Pin<&mut Launcher>);

        /// An environment variable (the tests are given their folders that way).
        #[qinvokable]
        fn env(self: &Launcher, name: &QString) -> QString;

        /// The folder proposed to hold new workspaces.
        #[qinvokable]
        #[cxx_name = "defaultParent"]
        fn default_parent(self: &Launcher) -> QString;

        /// `base`, or `base 2`... when a folder of that name is already in `parent`.
        #[qinvokable]
        #[cxx_name = "freeName"]
        fn free_name(self: &Launcher, parent: &QString, base: &QString) -> QString;

        /// Where the workspace would be created, or an empty text.
        #[qinvokable]
        fn preview(self: &Launcher, name: &QString, parent: &QString) -> QString;

        /// Creates and opens a workspace; an empty text, or why not.
        #[qinvokable]
        fn create(self: Pin<&mut Launcher>, name: &QString, parent: &QString) -> QString;

        /// Opens the known workspace at `index` in the list; an empty text, or why not.
        #[qinvokable]
        #[cxx_name = "openKnown"]
        fn open_known(self: Pin<&mut Launcher>, index: i32) -> QString;

        /// Opens the workspace in the folder `path`; an empty text, or why not.
        #[qinvokable]
        #[cxx_name = "openPath"]
        fn open_path(self: Pin<&mut Launcher>, path: &QString) -> QString;

        /// The languages the interface can be shown in (`en`, `fr`...).
        #[qinvokable]
        fn languages(self: &Launcher) -> QStringList;

        /// Chooses the interface's language (`system`, or one of `languages()`), remembers it and
        /// retranslates what is on screen.
        #[qinvokable]
        #[cxx_name = "chooseLanguage"]
        fn choose_language(self: Pin<&mut Launcher>, code: &QString);

        /// Adds `folder` as a source of the open workspace and starts its scan; the job's identifier,
        /// or `error:` and why not. The scan reports through the `Bus`.
        #[qinvokable]
        #[cxx_name = "addSource"]
        fn add_source(self: Pin<&mut Launcher>, folder: &QString) -> QString;
    }
}

use core::pin::Pin;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use auroraw_engine::{AddSourceRequest, Engine, LocalDirs, OpenedWorkspace, paths};
use cxx_qt::CxxQtType;
use cxx_qt_lib::{QString, QStringList};

use crate::app_settings::{AppSettings, LANGUAGES, resolve_language};
use crate::session::{self, Collector, Session};
use crate::{bus, translation_for};

/// The Rust side of the launcher.
#[derive(Default)]
pub struct LauncherRust {
    screen: QString,
    workspace_name: QString,
    note: QString,
    known_names: QStringList,
    known_paths: QStringList,
    language: QString,
    effective_language: QString,
    dirs: Option<LocalDirs>,
    pictures: PathBuf,
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

fn text(value: &str) -> QString {
    QString::from(value)
}

impl qobject::Launcher {
    fn dirs(&self) -> LocalDirs {
        self.dirs.clone().expect("start() was called")
    }

    fn settings_path(&self) -> PathBuf {
        self.dirs().data.join("app-settings.json")
    }

    fn refresh_known(mut self: Pin<&mut Self>) {
        let known = Engine::known_workspaces(&self.dirs());
        let names: QStringList = known.iter().map(|k| text(&k.name)).collect();
        let paths: QStringList = known
            .iter()
            .map(|k| text(&k.path.to_string_lossy()))
            .collect();
        self.as_mut().set_known_names(names);
        self.as_mut().set_known_paths(paths);
    }

    fn show(mut self: Pin<&mut Self>, opened: OpenedWorkspace) {
        let OpenedWorkspace {
            engine,
            events,
            name,
            previews_path,
            ..
        } = opened;
        let service = engine
            .start_thumbnails(&previews_path, 4)
            .expect("the previews database opens");
        let session = Arc::new(Session {
            engine,
            thumbs: Collector::new(service),
        });
        bus::start_pump(events, &session);
        session::set_current(Some(session));
        self.as_mut().set_workspace_name(text(&name));
        self.as_mut().set_screen(text("workspace"));
    }

    /// Opens the workspace in `root`; the reason when it cannot be.
    fn open_root(mut self: Pin<&mut Self>, root: &Path) -> Option<String> {
        // One workspace at a time: the open one lets go of its folder first.
        session::set_current(None);
        match Engine::open_workspace(root, &self.dirs()) {
            Ok(opened) => {
                self.as_mut().show(opened);
                None
            }
            Err(e) => Some(e.to_string()),
        }
    }

    pub fn start(mut self: Pin<&mut Self>) {
        let launch = crate::launch();
        {
            let mut rust = self.as_mut().rust_mut();
            rust.dirs = Some(launch.dirs.clone());
            rust.pictures = launch.pictures.clone();
        }
        let settings = AppSettings::load(&self.settings_path());
        self.as_mut().set_language(text(&settings.language));
        self.as_mut()
            .set_effective_language(text(resolve_language(&settings.language)));
        self.as_mut().set_screen(text("welcome"));
        self.as_mut().refresh_known();

        // A workspace named at launch first, else the last one opened.
        if let Some(path) = &launch.open {
            if let Some(reason) = self.as_mut().open_root(path) {
                let note = format!("Cannot open {}: {reason}", path.display());
                self.as_mut().set_note(text(&note));
            }
            return;
        }
        if let Some(last) = Engine::last_opened_workspace(&launch.dirs) {
            let shown = last.found && self.as_mut().open_root(&last.path).is_none();
            if !shown {
                let note = format!(
                    "The last workspace could not be found: {}",
                    last.path.display()
                );
                self.as_mut().set_note(text(&note));
            }
        }
    }

    pub fn env(&self, name: &QString) -> QString {
        let value = std::env::var(name.to_string()).unwrap_or_default();
        text(&value)
    }

    pub fn default_parent(&self) -> QString {
        text(&self.pictures.join("Auroraw").to_string_lossy())
    }

    pub fn free_name(&self, parent: &QString, base: &QString) -> QString {
        let parent = paths::resolve(&parent.to_string()).unwrap_or_default();
        text(&free_workspace_name(&parent, &base.to_string()))
    }

    pub fn preview(&self, name: &QString, parent: &QString) -> QString {
        let name = name.to_string();
        match (name.trim(), paths::resolve(&parent.to_string())) {
            ("", _) | (_, Err(_)) => QString::default(),
            (name, Ok(parent)) => text(
                &parent
                    .join(paths::folder_name(name, "Workspace"))
                    .to_string_lossy(),
            ),
        }
    }

    pub fn create(mut self: Pin<&mut Self>, name: &QString, parent: &QString) -> QString {
        let name = name.to_string();
        let name = name.trim();
        let parent = match paths::resolve(&parent.to_string()) {
            Ok(parent) => parent,
            Err(e) => return text(&e.to_string()),
        };
        let root = parent.join(paths::folder_name(name, "Workspace"));
        if !is_free(&root) {
            return text(&format!("taken:{}", root.display()));
        }
        session::set_current(None);
        match Engine::create_workspace(&root, name, &self.dirs()) {
            Ok(opened) => {
                self.as_mut().show(opened);
                QString::default()
            }
            Err(e) => text(&e.to_string()),
        }
    }

    pub fn open_known(mut self: Pin<&mut Self>, index: i32) -> QString {
        let path = self
            .known_paths
            .get(index as isize)
            .map(|p| PathBuf::from(p.to_string()));
        match path {
            Some(path) => match self.as_mut().open_root(&path) {
                Some(reason) => text(&reason),
                None => QString::default(),
            },
            None => QString::default(),
        }
    }

    pub fn open_path(mut self: Pin<&mut Self>, path: &QString) -> QString {
        let root = match paths::resolve(&path.to_string()) {
            Ok(root) => root,
            Err(e) => return text(&e.to_string()),
        };
        match self.as_mut().open_root(&root) {
            Some(reason) => text(&reason),
            None => QString::default(),
        }
    }

    pub fn languages(&self) -> QStringList {
        LANGUAGES.iter().map(|code| text(code)).collect()
    }

    pub fn choose_language(mut self: Pin<&mut Self>, code: &QString) {
        let code = code.to_string();
        let settings = AppSettings {
            language: code.clone(),
        };
        settings.save(&self.settings_path());
        let effective = resolve_language(&code);
        // SAFETY: `self` is a live QObject made by QML; its engine retranslates.
        unsafe {
            let object = self.as_mut().get_unchecked_mut() as *mut Self as *mut std::ffi::c_void;
            crate::glue::set_translation(translation_for(effective), object);
        }
        self.as_mut().set_language(text(&code));
        self.as_mut().set_effective_language(text(effective));
    }

    pub fn add_source(self: Pin<&mut Self>, folder: &QString) -> QString {
        let Some(session) = session::current() else {
            return text("error:no workspace is open");
        };
        let root = match paths::resolve(&folder.to_string()) {
            Ok(root) => root,
            Err(e) => return text(&format!("error:{e}")),
        };
        match session.engine.add_source(AddSourceRequest {
            root,
            name: None,
            merge: false,
        }) {
            Ok(started) => text(&started.job.to_string()),
            Err(e) => text(&format!("error:{e}")),
        }
    }
}
