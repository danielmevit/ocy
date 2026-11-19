use std::borrow::Cow;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Theme {
    Light,
    Dark,
}

impl Theme {
    pub fn suffix(self) -> &'static str {
        match self {
            Theme::Light => "light",
            Theme::Dark => "dark",
        }
    }
}

pub trait FrameSource {
    fn svg(&self, id: &str, theme: Theme, frame: u32) -> Option<Cow<'_, [u8]>>;
}

#[derive(Debug, Clone)]
pub struct DirSource {
    root: PathBuf,
}

impl DirSource {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            root: path.as_ref().to_owned(),
        }
    }
}

impl FrameSource for DirSource {
    fn svg(&self, id: &str, theme: Theme, frame: u32) -> Option<Cow<'_, [u8]>> {
        let path = self
            .root
            .join(format!("{id}-{}-f{frame}.svg", theme.suffix()));
        std::fs::read(path).ok().map(Cow::Owned)
    }
}

#[derive(Debug, Clone)]
pub struct EmbeddedSource {
    entries: Vec<(String, &'static [u8])>,
}

impl EmbeddedSource {
    pub fn new(entries: &[(&str, &'static [u8])]) -> Self {
        Self {
            entries: entries
                .iter()
                .map(|(name, bytes)| ((*name).to_owned(), *bytes))
                .collect(),
        }
    }
}

impl FrameSource for EmbeddedSource {
    fn svg(&self, id: &str, theme: Theme, frame: u32) -> Option<Cow<'_, [u8]>> {
        let name = format!("{id}-{}-f{frame}.svg", theme.suffix());
        self.entries
            .iter()
            .find(|(entry_name, _)| entry_name == &name)
            .map(|(_, bytes)| Cow::Borrowed(*bytes))
    }
}
