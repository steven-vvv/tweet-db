use std::path::{Component, Path, PathBuf};

pub(super) fn discover_default_config_path() -> PathBuf {
    default_config_candidates()
        .into_iter()
        .find(|path| path.is_file())
        .unwrap_or_else(|| PathBuf::from("config/default.toml"))
}

fn default_config_candidates() -> Vec<PathBuf> {
    if running_from_server_dir() {
        vec![
            PathBuf::from("config.toml"),
            PathBuf::from("config/default.toml"),
            PathBuf::from("server/config.toml"),
            PathBuf::from("server/config/default.toml"),
        ]
    } else {
        vec![
            PathBuf::from("server/config.toml"),
            PathBuf::from("config.toml"),
            PathBuf::from("server/config/default.toml"),
            PathBuf::from("config/default.toml"),
        ]
    }
}

fn running_from_server_dir() -> bool {
    Path::new("Cargo.toml").is_file() && Path::new("src/main.rs").is_file()
}

pub(super) fn resolve_path(base: &Path, relative: &str) -> PathBuf {
    let path = Path::new(relative);
    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    };
    normalize_path(&resolved)
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();

    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    normalized.push(component.as_os_str());
                }
            }
            _ => normalized.push(component.as_os_str()),
        }
    }

    normalized
}
