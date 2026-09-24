// SPDX-License-Identifier: GPL-3.0-or-later
//! The launcher: which workspaces this machine knows, creating and opening one. The Qt-side
//! twin of `crates/ui/src/app.rs` (the Slint launcher), on the same engine calls.

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
        #[qproperty(f64, scan_progress, cxx_name = "scanProgress")]
        #[qproperty(QString, scan_status, cxx_name = "scanStatus")]
        type Launcher = super::LauncherRust;

        /// Emitted when a scan has finished.
        #[qsignal]
        #[cxx_name = "scanFinished"]
        fn scan_finished(self: Pin<&mut Launcher>);

        /// Adds `folder` as a source of the open workspace; the scan runs in the background and
        /// reports through `scanProgress`, `scanStatus` and `scanFinished`. Empty, or why not.
        #[qinvokable]
        #[cxx_name = "addSource"]
        fn add_source(self: Pin<&mut Launcher>, folder: &QString) -> QString;

        /// Opens the last workspace, or leaves the welcome screen.
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

        /// Opens the known workspace at `index` in the list.
        #[qinvokable]
        #[cxx_name = "openKnown"]
        fn open_known(self: Pin<&mut Launcher>, index: i32) -> QString;
    }

    // Lets background threads queue work onto the GUI thread (the engine's events).
    impl cxx_qt::Threading for Launcher {}
}

use core::pin::Pin;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use auroraw_engine::{AddSourceRequest, Engine, Event, LocalDirs, OpenedWorkspace, paths};
use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QString, QStringList};

use crate::session::{self, Collector, Session};

/// The Rust side of the launcher.
#[derive(Default)]
pub struct LauncherRust {
    screen: QString,
    workspace_name: QString,
    note: QString,
    known_names: QStringList,
    known_paths: QStringList,
    scan_progress: f64,
    scan_status: QString,
    dirs: Option<LocalDirs>,
    pictures: PathBuf,
}

/// This machine's data and cache folders and Pictures folder; `SPIKE_HOME` moves all three (tests).
fn machine_folders() -> (LocalDirs, PathBuf) {
    if let Some(home) = std::env::var_os("SPIKE_HOME") {
        let home = PathBuf::from(home);
        return (
            LocalDirs {
                data: home.join("data"),
                cache: home.join("cache"),
            },
            home.join("Pictures"),
        );
    }
    let project = directories::ProjectDirs::from("org", "auroraw", "AurorawQtSpike")
        .expect("a home folder");
    let pictures = directories::UserDirs::new()
        .and_then(|user| user.picture_dir().map(Path::to_path_buf))
        .unwrap_or_default();
    (
        LocalDirs {
            data: project.data_local_dir().to_path_buf(),
            cache: project.cache_dir().to_path_buf(),
        },
        pictures,
    )
}

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

fn is_free(folder: &Path) -> bool {
    match std::fs::read_dir(folder) {
        Ok(mut entries) => entries.next().is_none(),
        Err(_) => !folder.exists(),
    }
}

impl qobject::Launcher {
    fn dirs(&self) -> LocalDirs {
        self.dirs.clone().expect("start() was called")
    }

    fn refresh_known(mut self: Pin<&mut Self>) {
        let known = Engine::known_workspaces(&self.dirs());
        let names: QStringList = known.iter().map(|k| QString::from(k.name.as_str())).collect();
        let paths: QStringList = known
            .iter()
            .map(|k| QString::from(k.path.to_string_lossy().as_ref()))
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
        session::set_current(Some(Arc::new(Session {
            engine,
            thumbs: Collector::new(service),
            events: Mutex::new(events),
        })));
        self.as_mut().set_workspace_name(QString::from(name.as_str()));
        self.as_mut().set_screen(QString::from("workspace"));
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
        let (dirs, pictures) = machine_folders();
        {
            let mut rust = self.as_mut().rust_mut();
            rust.dirs = Some(dirs.clone());
            rust.pictures = pictures;
        }
        self.as_mut().set_screen(QString::from("welcome"));
        self.as_mut().refresh_known();
        if let Some(last) = Engine::last_opened_workspace(&dirs) {
            let shown = last.found && self.as_mut().open_root(&last.path).is_none();
            if !shown {
                let note = format!("The last workspace could not be found: {}", last.path.display());
                self.as_mut().set_note(QString::from(note.as_str()));
            }
        }
    }

    pub fn env(&self, name: &QString) -> QString {
        let value = std::env::var(name.to_string()).unwrap_or_default();
        QString::from(value.as_str())
    }

    pub fn default_parent(&self) -> QString {
        QString::from(self.pictures.join("Auroraw").to_string_lossy().as_ref())
    }

    pub fn free_name(&self, parent: &QString, base: &QString) -> QString {
        let parent = paths::resolve(&parent.to_string()).unwrap_or_default();
        QString::from(free_workspace_name(&parent, &base.to_string()).as_str())
    }

    pub fn preview(&self, name: &QString, parent: &QString) -> QString {
        let name = name.to_string();
        match (name.trim(), paths::resolve(&parent.to_string())) {
            ("", _) | (_, Err(_)) => QString::default(),
            (name, Ok(parent)) => QString::from(
                parent
                    .join(paths::folder_name(name, "Workspace"))
                    .to_string_lossy()
                    .as_ref(),
            ),
        }
    }

    pub fn create(mut self: Pin<&mut Self>, name: &QString, parent: &QString) -> QString {
        let name = name.to_string();
        let name = name.trim();
        let parent = match paths::resolve(&parent.to_string()) {
            Ok(parent) => parent,
            Err(e) => return QString::from(e.to_string().as_str()),
        };
        let root = parent.join(paths::folder_name(name, "Workspace"));
        if !is_free(&root) {
            return QString::from(format!("taken:{}", root.display()).as_str());
        }
        session::set_current(None);
        match Engine::create_workspace(&root, name, &self.dirs()) {
            Ok(opened) => {
                self.as_mut().show(opened);
                QString::default()
            }
            Err(e) => QString::from(e.to_string().as_str()),
        }
    }

    pub fn open_known(mut self: Pin<&mut Self>, index: i32) -> QString {
        let path = self
            .known_paths
            .get(index as isize)
            .map(|p| PathBuf::from(p.to_string()));
        match path {
            Some(path) => match self.as_mut().open_root(&path) {
                Some(reason) => QString::from(reason.as_str()),
                None => QString::default(),
            },
            None => QString::default(),
        }
    }

    pub fn add_source(mut self: Pin<&mut Self>, folder: &QString) -> QString {
        let Some(session) = session::current() else {
            return QString::from("no workspace is open");
        };
        let root = match paths::resolve(&folder.to_string()) {
            Ok(root) => root,
            Err(e) => return QString::from(e.to_string().as_str()),
        };
        let started = match session.engine.add_source(AddSourceRequest {
            root,
            name: None,
            merge: false,
        }) {
            Ok(started) => started,
            Err(e) => return QString::from(e.to_string().as_str()),
        };
        self.as_mut().set_scan_progress(0.0);
        self.as_mut().set_scan_status(QString::from("Reading photos…"));
        // The engine's events arrive on its own threads: this one waits for them and queues what the
        // interface shows onto the GUI thread.
        let gui = self.qt_thread();
        std::thread::spawn(move || {
            loop {
                let event = session
                    .events
                    .lock()
                    .unwrap()
                    .recv_timeout(std::time::Duration::from_millis(200));
                match event {
                    Some(Event::JobProgress { job, done, total })
                        if job == started.job && total > 0 =>
                    {
                        let progress = done as f64 / total as f64;
                        let _ = gui.queue(move |mut this| this.as_mut().set_scan_progress(progress));
                    }
                    Some(Event::IndexFinished { job, added, .. }) if job == started.job => {
                        let text = format!("Done: {added} added.");
                        let _ = gui.queue(move |mut this| {
                            this.as_mut().set_scan_progress(1.0);
                            this.as_mut().set_scan_status(QString::from(text.as_str()));
                            this.as_mut().scan_finished();
                        });
                        break;
                    }
                    _ => {}
                }
                // The workspace was closed or another opened: nobody is listening any more.
                if session::current().is_none_or(|now| !Arc::ptr_eq(&now, &session)) {
                    break;
                }
            }
        });
        QString::default()
    }
}
