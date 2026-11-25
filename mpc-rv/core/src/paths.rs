//! The user's folders, found the way GLib finds them on Linux (XDG base directories and
//! `user-dirs.dirs`), so the settings stay where they were when this used GLib.

use std::path::PathBuf;

/// A path from an environment variable, if it's set and absolute.
fn env_dir(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).map(PathBuf::from).filter(|p| p.is_absolute())
}

pub fn home_dir() -> PathBuf {
    env_dir("HOME").or_else(|| env_dir("USERPROFILE")).unwrap_or_else(|| PathBuf::from("/"))
}

/// `$XDG_CONFIG_HOME`, or `~/.config` (`%APPDATA%` on Windows).
pub fn config_dir() -> PathBuf {
    if cfg!(windows) {
        return env_dir("APPDATA").unwrap_or_else(|| home_dir().join("AppData/Roaming"));
    }
    env_dir("XDG_CONFIG_HOME").unwrap_or_else(|| home_dir().join(".config"))
}

/// `$XDG_DATA_DIRS`, or `/usr/local/share` and `/usr/share`.
pub fn system_data_dirs() -> Vec<PathBuf> {
    match std::env::var("XDG_DATA_DIRS") {
        Ok(v) if !v.is_empty() => v.split(':').filter(|s| !s.is_empty()).map(PathBuf::from).collect(),
        _ => vec![PathBuf::from("/usr/local/share"), PathBuf::from("/usr/share")],
    }
}

/// The Pictures folder (`XDG_PICTURES_DIR` in `user-dirs.dirs`), or the home folder.
pub fn pictures_dir() -> PathBuf {
    if cfg!(windows) {
        return home_dir().join("Pictures");
    }
    let text = std::fs::read_to_string(config_dir().join("user-dirs.dirs")).unwrap_or_default();
    user_dir(&text, "PICTURES", &home_dir()).unwrap_or_else(home_dir)
}

/// One entry of a `user-dirs.dirs` file: `XDG_<NAME>_DIR="$HOME/..."` or an absolute path.
fn user_dir(text: &str, name: &str, home: &std::path::Path) -> Option<PathBuf> {
    let key = format!("XDG_{name}_DIR");
    let value = text.lines().find_map(|l| l.trim().strip_prefix(&key)?.trim_start().strip_prefix('='))?;
    let value = value.trim().strip_prefix('"')?.strip_suffix('"')?;
    if let Some(rest) = value.strip_prefix("$HOME") {
        let rest = rest.trim_start_matches('/');
        return Some(if rest.is_empty() { home.to_path_buf() } else { home.join(rest) });
    }
    value.starts_with('/').then(|| PathBuf::from(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_dirs_file() {
        let home = std::path::Path::new("/home/u");
        let text = "# comment\nXDG_DESKTOP_DIR=\"$HOME/Desktop\"\nXDG_PICTURES_DIR=\"$HOME/Bilder\"\nXDG_MUSIC_DIR=\"/data/music\"\nXDG_VIDEOS_DIR=\"$HOME/\"\n";
        assert_eq!(user_dir(text, "PICTURES", home), Some(PathBuf::from("/home/u/Bilder")));
        assert_eq!(user_dir(text, "MUSIC", home), Some(PathBuf::from("/data/music")));
        assert_eq!(user_dir(text, "VIDEOS", home), Some(PathBuf::from("/home/u")));
        assert_eq!(user_dir(text, "DOWNLOAD", home), None);
    }
}
