//! Location of Gource's data files.
//!
//! Port of `gSDLAppResourceDir` (`SDLAppInit` in `src/core/sdlapp.cpp`).
//! Textures and fonts are embedded in the Rust binaries (see
//! `gource_draw::resources`), so this is only needed for files that other
//! programs read, such as the Mercurial log template `gource.style`.

use std::path::Path;

/// The `data/` directory of the source tree this crate was built from.
const SOURCE_TREE_DATA_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data");

/// Directory of Gource's data files, with a trailing slash.
///
/// The configured directory is `GOURCE_RESOURCE_DIR` set at build time (the
/// equivalent of the C++ `SDLAPP_RESOURCE_DIR`, i.e. `./configure`'s
/// `pkgdatadir`), or else the source tree's `data/` directory. See
/// [`resolve_resource_dir`] for the fallbacks.
pub fn resource_dir() -> String {
    let source_tree = Path::new(SOURCE_TREE_DATA_DIR)
        .canonicalize()
        .ok()
        .map(|p| p.to_string_lossy().into_owned());
    let configured: Vec<&str> = option_env!("GOURCE_RESOURCE_DIR")
        .into_iter()
        .chain(source_tree.as_deref())
        .collect();
    // C++ uses argv[0] (`exepath`), not the resolved executable path.
    let exe_path = std::env::args_os()
        .next()
        .map(|arg| arg.to_string_lossy().into_owned())
        .unwrap_or_default();
    let cwd = std::env::current_dir()
        .ok()
        .map(|p| p.to_string_lossy().into_owned());
    resolve_resource_dir(&configured, &exe_path, cwd.as_deref(), |dir| {
        Path::new(dir).is_dir()
    })
}

/// The selection logic of [`resource_dir`] (C++ `SDLAppInit`, non-Windows):
///
/// 1. the first `configured` directory that exists,
/// 2. otherwise `data/` next to the executable, if `exe_path` has a directory
///    part,
/// 3. otherwise `data/` in the working directory (relative `data/` if the
///    working directory is unknown).
///
/// The result always ends with a slash ([`add_slash`]).
pub fn resolve_resource_dir(
    configured: &[&str],
    exe_path: &str,
    cwd: Option<&str>,
    is_dir: impl Fn(&str) -> bool,
) -> String {
    let dir = if let Some(dir) = configured.iter().find(|dir| is_dir(dir)) {
        (*dir).to_owned()
    } else if let Some(pos) = exe_path.rfind('/') {
        format!("{}data/", &exe_path[..=pos])
    } else if let Some(cwd) = cwd {
        format!("{cwd}/data/")
    } else {
        "data/".to_owned()
    };
    add_slash(&dir)
}

/// Port of `SDLAppAddSlash`: append a `/` unless the path is empty or
/// already ends with one.
pub fn add_slash(path: &str) -> String {
    if path.is_empty() || path.ends_with('/') {
        path.to_owned()
    } else {
        format!("{path}/")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_slash_matches_cpp() {
        assert_eq!(add_slash(""), "");
        assert_eq!(add_slash("data"), "data/");
        assert_eq!(add_slash("/usr/share/gource/"), "/usr/share/gource/");
    }

    #[test]
    fn first_existing_configured_dir_wins() {
        let exists = |dir: &str| dir != "/missing";
        assert_eq!(
            resolve_resource_dir(
                &["/missing", "/usr/share/gource", "/other"],
                "/opt/bin/gource",
                Some("/home/me"),
                exists
            ),
            "/usr/share/gource/"
        );
    }

    #[test]
    fn falls_back_to_data_next_to_the_executable() {
        assert_eq!(
            resolve_resource_dir(&["/missing"], "/opt/bin/gource", Some("/home/me"), |_| {
                false
            }),
            "/opt/bin/data/"
        );
        assert_eq!(
            resolve_resource_dir(&[], "target/debug/gource", None, |_| true),
            "target/debug/data/"
        );
    }

    #[test]
    fn falls_back_to_data_in_the_working_directory() {
        assert_eq!(
            resolve_resource_dir(&[], "gource", Some("/home/me"), |_| true),
            "/home/me/data/"
        );
        assert_eq!(resolve_resource_dir(&[], "", None, |_| true), "data/");
    }

    #[test]
    fn source_tree_data_dir_is_found() {
        let dir = resource_dir();
        assert!(dir.ends_with("/data/"), "{dir}");
        assert!(Path::new(&format!("{dir}gource.style")).is_file(), "{dir}");
    }
}
