//! Interned identifiers and tables for paths, users, and cohorts.

use gource_core::StringHasher;
use std::collections::HashMap;

/// Unique identifier for an interned file path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PathId(pub u32);

/// Unique identifier for an interned user / author.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UserId(pub u32);

/// Unique identifier for a Git-of-Theseus cohort.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CohortId(pub u16);

/// Metadata stored for an interned path.
#[derive(Debug, Clone, PartialEq)]
pub struct PathEntry {
    /// Normalized full path, always with a leading slash (e.g. `"/src/main.rs"`).
    pub path: String,
    /// Filename component (e.g. `"main.rs"`).
    pub name: String,
    /// Parent directory with trailing slash (e.g. `"/src/"`).
    pub dir: String,
    /// File extension without dot (e.g. `"rs"`), or empty if none.
    pub ext: String,
    /// Deterministic RGB colour computed from the extension via [`StringHasher`].
    pub colour: [f32; 3],
}

/// Bidirectional table interning normalized file paths.
#[derive(Debug, Clone, PartialEq)]
pub struct PathTable {
    entries: Vec<PathEntry>,
    index: HashMap<String, PathId>,
    hasher: StringHasher,
}

impl Default for PathTable {
    fn default() -> Self {
        Self::new()
    }
}

impl PathTable {
    /// Creates an empty path table using the default [`StringHasher`].
    pub fn new() -> Self {
        Self::with_hasher(StringHasher::default())
    }

    /// Creates an empty path table with a custom string hasher seed.
    pub fn with_hasher(hasher: StringHasher) -> Self {
        Self {
            entries: Vec::new(),
            index: HashMap::new(),
            hasher,
        }
    }

    /// Normalizes a path string, ensuring a leading slash and uniform directory separators.
    pub fn normalize_path(raw: &str) -> String {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return "/".to_string();
        }
        let with_slash = if trimmed.starts_with('/') {
            trimmed.to_string()
        } else {
            format!("/{trimmed}")
        };
        // Normalize multiple consecutive slashes
        let mut result = String::with_capacity(with_slash.len());
        let mut last_was_slash = false;
        for c in with_slash.chars() {
            if c == '/' {
                if !last_was_slash {
                    result.push('/');
                    last_was_slash = true;
                }
            } else {
                result.push(c);
                last_was_slash = false;
            }
        }
        result
    }

    /// Interns a path, returning its [`PathId`].
    pub fn intern(&mut self, path: &str) -> PathId {
        let norm = Self::normalize_path(path);
        if let Some(&id) = self.index.get(&norm) {
            return id;
        }

        let (dir, name) = match norm.rfind('/') {
            Some(pos) => {
                let dir_part = &norm[..=pos];
                let name_part = &norm[pos + 1..];
                (dir_part.to_string(), name_part.to_string())
            }
            None => ("/".to_string(), norm.clone()),
        };

        let ext = match name.rfind('.') {
            Some(dot) if dot != 0 && dot < name.len() - 1 => name[dot + 1..].to_string(),
            _ => String::new(),
        };

        let colour = self.hasher.colour_rgb(&ext);
        let id = PathId(self.entries.len() as u32);
        self.entries.push(PathEntry {
            path: norm.clone(),
            name,
            dir,
            ext,
            colour,
        });
        self.index.insert(norm, id);
        id
    }

    /// Retrieves the [`PathEntry`] for a given [`PathId`].
    pub fn get(&self, id: PathId) -> Option<&PathEntry> {
        self.entries.get(id.0 as usize)
    }

    /// Resolves the full path string for a [`PathId`].
    pub fn resolve(&self, id: PathId) -> Option<&str> {
        self.get(id).map(|e| e.path.as_str())
    }

    /// Finds the [`PathId`] for a path without interning if already present.
    pub fn find(&self, path: &str) -> Option<PathId> {
        let norm = Self::normalize_path(path);
        self.index.get(&norm).copied()
    }

    /// Number of interned paths.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True if no paths have been interned.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterates over all `(PathId, &PathEntry)`.
    pub fn iter(&self) -> impl Iterator<Item = (PathId, &PathEntry)> {
        self.entries
            .iter()
            .enumerate()
            .map(|(i, e)| (PathId(i as u32), e))
    }
}

/// Bidirectional table interning user names.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UserTable {
    users: Vec<String>,
    index: HashMap<String, UserId>,
}

impl UserTable {
    /// Creates an empty user table.
    pub fn new() -> Self {
        Self {
            users: Vec::new(),
            index: HashMap::new(),
        }
    }

    /// Interns a username, returning its [`UserId`].
    pub fn intern(&mut self, username: &str) -> UserId {
        let key = username.trim();
        if let Some(&id) = self.index.get(key) {
            return id;
        }
        let id = UserId(self.users.len() as u32);
        self.users.push(key.to_string());
        self.index.insert(key.to_string(), id);
        id
    }

    /// Resolves a [`UserId`] to its username string.
    pub fn get(&self, id: UserId) -> Option<&str> {
        self.users.get(id.0 as usize).map(|s| s.as_str())
    }

    /// Finds the [`UserId`] for a username if already interned.
    pub fn find(&self, username: &str) -> Option<UserId> {
        self.index.get(username.trim()).copied()
    }

    /// Total number of interned users.
    pub fn len(&self) -> usize {
        self.users.len()
    }

    /// True if no users have been interned.
    pub fn is_empty(&self) -> bool {
        self.users.is_empty()
    }

    /// Iterates over all `(UserId, &str)`.
    pub fn iter(&self) -> impl Iterator<Item = (UserId, &str)> {
        self.users
            .iter()
            .enumerate()
            .map(|(i, s)| (UserId(i as u32), s.as_str()))
    }
}

/// Bidirectional table interning cohort labels.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CohortTable {
    cohorts: Vec<String>,
    index: HashMap<String, CohortId>,
}

impl CohortTable {
    /// Creates an empty cohort table.
    pub fn new() -> Self {
        Self {
            cohorts: Vec::new(),
            index: HashMap::new(),
        }
    }

    /// Interns a cohort label (e.g. `"2024"` or `"2024-Q1"`), returning its [`CohortId`].
    pub fn intern(&mut self, label: &str) -> CohortId {
        let key = label.trim();
        if let Some(&id) = self.index.get(key) {
            return id;
        }
        let id = CohortId(self.cohorts.len() as u16);
        self.cohorts.push(key.to_string());
        self.index.insert(key.to_string(), id);
        id
    }

    /// Resolves a [`CohortId`] to its label string.
    pub fn get(&self, id: CohortId) -> Option<&str> {
        self.cohorts.get(id.0 as usize).map(|s| s.as_str())
    }

    /// Finds the [`CohortId`] for a label if already interned.
    pub fn find(&self, label: &str) -> Option<CohortId> {
        self.index.get(label.trim()).copied()
    }

    /// Total number of interned cohorts.
    pub fn len(&self) -> usize {
        self.cohorts.len()
    }

    /// True if no cohorts have been interned.
    pub fn is_empty(&self) -> bool {
        self.cohorts.is_empty()
    }

    /// Iterates over all `(CohortId, &str)`.
    pub fn iter(&self) -> impl Iterator<Item = (CohortId, &str)> {
        self.cohorts
            .iter()
            .enumerate()
            .map(|(i, s)| (CohortId(i as u16), s.as_str()))
    }
}
