use std::{
    env, fs,
    path::{Path, PathBuf},
};

fn state_file() -> Option<PathBuf> {
    env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .map(|cache| cache.join("pkt2ai/last-directory"))
}

pub fn remembered_directory() -> PathBuf {
    state_file()
        .and_then(|path| fs::read_to_string(path).ok())
        .map(|path| PathBuf::from(path.trim()))
        .filter(|path| path.is_dir())
        .or_else(|| env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn remember_directory(directory: &Path) {
    let Some(path) = state_file() else { return };
    let Some(parent) = path.parent() else { return };
    if fs::create_dir_all(parent).is_ok() {
        let _ = fs::write(path, directory.to_string_lossy().as_bytes());
    }
}

#[derive(Debug, Clone)]
pub struct FilePicker {
    pub directory: PathBuf,
    pub entries: Vec<PathBuf>,
    pub selected: usize,
}

impl FilePicker {
    pub fn new(directory: impl AsRef<Path>) -> Self {
        let mut picker = Self {
            directory: directory.as_ref().to_path_buf(),
            entries: Vec::new(),
            selected: 0,
        };
        picker.refresh();
        picker
    }
    pub fn refresh(&mut self) {
        self.entries = fs::read_dir(&self.directory)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.is_dir()
                    || path
                        .extension()
                        .and_then(|value| value.to_str())
                        .is_some_and(|extension| {
                            ["pkt", "xml", "json"]
                                .iter()
                                .any(|allowed| extension.eq_ignore_ascii_case(allowed))
                        })
            })
            .collect();
        self.entries.sort_by_key(|path| {
            (
                !path.is_dir(),
                path.file_name().map(|name| name.to_ascii_lowercase()),
            )
        });
        self.selected = self.selected.min(self.entries.len().saturating_sub(1));
    }
    pub fn move_by(&mut self, delta: isize) {
        if self.entries.is_empty() {
            self.selected = 0;
            return;
        }
        self.selected = self
            .selected
            .saturating_add_signed(delta)
            .min(self.entries.len() - 1);
    }
    pub fn choose(&mut self) -> Option<PathBuf> {
        let path = self.entries.get(self.selected)?.clone();
        if path.is_dir() {
            self.directory = path;
            self.selected = 0;
            self.refresh();
            None
        } else {
            Some(path)
        }
    }
    pub fn parent(&mut self) {
        if let Some(parent) = self.directory.parent() {
            self.directory = parent.to_path_buf();
            self.selected = 0;
            self.refresh();
        }
    }
}
