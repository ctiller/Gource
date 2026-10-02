//! Search bar display model (`SearchVM`, [`SearchItem`], [`SearchItemKind`]).

use gource_core::Vec4;

/// Category of item found in search.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SearchItemKind {
    File,
    Directory,
    User,
}

impl SearchItemKind {
    pub fn badge_label(&self) -> &'static str {
        match self {
            Self::File => "FILE",
            Self::Directory => "DIR",
            Self::User => "USER",
        }
    }

    pub fn badge_colour(&self) -> Vec4 {
        match self {
            Self::File => Vec4::new(0.25, 0.65, 0.95, 1.0),
            Self::Directory => Vec4::new(0.95, 0.75, 0.25, 1.0),
            Self::User => Vec4::new(0.35, 0.85, 0.45, 1.0),
        }
    }
}

/// A matched search candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchItem {
    pub kind: SearchItemKind,
    pub name: String,
    pub detail: String,
    pub id: u64,
}

impl SearchItem {
    pub fn new_file(id: u64, name: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            kind: SearchItemKind::File,
            name: name.into(),
            detail: path.into(),
            id,
        }
    }

    pub fn new_dir(id: u64, path: impl Into<String>) -> Self {
        let p = path.into();
        Self {
            kind: SearchItemKind::Directory,
            name: p.clone(),
            detail: p,
            id,
        }
    }

    pub fn new_user(id: u64, name: impl Into<String>) -> Self {
        let n = name.into();
        Self {
            kind: SearchItemKind::User,
            name: n.clone(),
            detail: n,
            id,
        }
    }
}

/// Display model for the search overlay.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SearchVM {
    pub visible: bool,
    pub query: String,
    pub selected_index: usize,
    pub results: Vec<SearchItem>,
}
