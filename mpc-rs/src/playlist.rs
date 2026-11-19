//! The playlist (MPC's `CPlaylist`, `PlayerPlaylistBar.cpp`): items, shuffle/repeat stepping, playlist
//! file formats and "next file in folder". mpv only ever gets one file at a time.

use std::{
    cmp::Ordering,
    fmt::Write as _,
    path::{Path, PathBuf},
};

pub const MEDIA_EXTS: &[&str] = &[
    "3g2", "3gp", "aac", "ac3", "aif", "aiff", "alac", "amr", "ape", "asf", "au", "avi", "dts", "divx", "dv",
    "evo", "f4v", "flac", "flv", "h264", "h265", "hevc", "m1v", "m2t", "m2ts", "m2v", "m4a", "m4b", "m4v",
    "mid", "midi", "mka", "mkv", "mov", "mp2", "mp3", "mp4", "mpa", "mpc", "mpe", "mpeg", "mpg", "mts", "mxf",
    "oga", "ogg", "ogm", "ogv", "opus", "ra", "ram", "rm", "rmvb", "tak", "ts", "tta", "vob", "wav", "webm",
    "wma", "wmv", "wv", "y4m",
];
pub const PLAYLIST_EXTS: &[&str] = &["mpcpl", "m3u", "m3u8", "pls", "asx", "cue"];

fn ext(path: &str) -> String {
    Path::new(path).extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default()
}

pub fn is_media(path: &str) -> bool {
    MEDIA_EXTS.contains(&ext(path).as_str())
}

pub fn is_playlist(path: &str) -> bool {
    PLAYLIST_EXTS.contains(&ext(path).as_str())
}

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub path: String,
    pub label: Option<String>,
    pub duration: Option<f64>,
    pub subs: Vec<String>,
    /// Extra audio files (/dub); not saved in playlist files.
    pub audio: Vec<String>,
    /// Per-file mpv options for loadfile (capture devices); not saved in playlist files.
    pub options: String,
    order: u64, // insertion order, for "Restore"
}

impl Entry {
    pub fn new(path: impl Into<String>) -> Entry {
        Entry { path: path.into(), label: None, duration: None, subs: Vec::new(), audio: Vec::new(), options: String::new(), order: 0 }
    }

    /// What the playlist shows: the label, else the file name (or the whole URL).
    pub fn title(&self) -> String {
        if let Some(l) = &self.label {
            return l.clone();
        }
        if self.path.contains("://") {
            return self.path.clone();
        }
        Path::new(&self.path).file_name().map_or(self.path.clone(), |n| n.to_string_lossy().into_owned())
    }
}

#[derive(Default)]
pub struct Playlist {
    pub items: Vec<Entry>,
    pub current: Option<usize>,
    pub shuffle: bool,
    /// Shuffled play order (indices into `items`); rebuilt whenever it's stale.
    shuffled: Vec<usize>,
    next_order: u64,
}

impl Playlist {
    /// Add entries at the end; returns the index of the first one.
    pub fn add(&mut self, entries: Vec<Entry>) -> usize {
        let first = self.items.len();
        for mut e in entries {
            e.order = self.next_order;
            self.next_order += 1;
            self.items.push(e);
        }
        self.shuffled.clear();
        first
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.current = None;
        self.shuffled.clear();
    }

    /// Remove the given indices (any order).
    pub fn remove(&mut self, indices: &[usize]) {
        let mut sorted = indices.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        for &i in sorted.iter().rev() {
            if i < self.items.len() {
                self.items.remove(i);
                self.current = match self.current {
                    Some(c) if c == i => None,
                    Some(c) if c > i => Some(c - 1),
                    c => c,
                };
            }
        }
        self.shuffled.clear();
    }

    /// Move item `from` so it ends up at index `to`.
    pub fn move_item(&mut self, from: usize, to: usize) {
        if from >= self.items.len() || to >= self.items.len() || from == to {
            return;
        }
        self.reorder(|v| {
            let e = v.remove(from);
            v.insert(to, e);
        });
    }

    /// Reorder with `f`, keeping `current` on the same entry.
    fn reorder(&mut self, f: impl FnOnce(&mut Vec<Entry>)) {
        let cur = self.current.map(|c| self.items[c].order);
        f(&mut self.items);
        self.current = cur.and_then(|o| self.items.iter().position(|e| e.order == o));
        self.shuffled.clear();
    }

    pub fn sort_by_label(&mut self) {
        self.reorder(|v| v.sort_by(|a, b| natural_cmp(&a.title(), &b.title())));
    }

    pub fn sort_by_path(&mut self) {
        self.reorder(|v| v.sort_by(|a, b| natural_cmp(&a.path, &b.path)));
    }

    pub fn restore_order(&mut self) {
        self.reorder(|v| v.sort_by_key(|e| e.order));
    }

    /// MPC's "Randomize": shuffle the list itself.
    pub fn randomize(&mut self, rand: &mut impl FnMut(usize) -> usize) {
        self.reorder(|v| {
            for i in (1..v.len()).rev() {
                v.swap(i, rand(i + 1));
            }
        });
    }

    /// Index after (`dir` = 1) or before (`dir` = -1) the current one, following the shuffle order when
    /// shuffle is on. `wrap` goes around the ends.
    pub fn step(&mut self, dir: i64, wrap: bool, rand: &mut impl FnMut(usize) -> usize) -> Option<usize> {
        let n = self.items.len();
        if n == 0 {
            return None;
        }
        let Some(cur) = self.current else { return Some(0) };
        let order: Vec<usize> = if self.shuffle {
            if self.shuffled.len() != n {
                // New random order that starts at the current item.
                let mut v: Vec<usize> = (0..n).filter(|&i| i != cur).collect();
                for i in (1..v.len()).rev() {
                    v.swap(i, rand(i + 1));
                }
                v.insert(0, cur);
                self.shuffled = v;
            }
            self.shuffled.clone()
        } else {
            (0..n).collect()
        };
        let pos = order.iter().position(|&i| i == cur)? as i64 + dir;
        if (0..n as i64).contains(&pos) {
            Some(order[pos as usize])
        } else if wrap {
            Some(order[pos.rem_euclid(n as i64) as usize])
        } else {
            None
        }
    }
}

/// Compare like Windows Explorer / MPC's `StrCmpLogicalW`: case-insensitive, digit runs by value.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut a, mut b) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, _) => return Ordering::Less,
            (_, None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let num = |it: &mut std::iter::Peekable<std::str::Chars>| {
                    let mut s = String::new();
                    while let Some(c) = it.peek().filter(|c| c.is_ascii_digit()) {
                        s.push(*c);
                        it.next();
                    }
                    s
                };
                let (na, nb) = (num(&mut a), num(&mut b));
                let (ta, tb) = (na.trim_start_matches('0'), nb.trim_start_matches('0'));
                let o = ta.len().cmp(&tb.len()).then(ta.cmp(tb));
                if o != Ordering::Equal {
                    return o;
                }
            }
            (Some(x), Some(y)) => {
                let o = x.to_lowercase().cmp(y.to_lowercase());
                if o != Ordering::Equal {
                    return o;
                }
                a.next();
                b.next();
            }
        }
    }
}

/// Media files in a folder, in natural order.
pub fn folder_files(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .filter(|e| e.file_type().is_ok_and(|t| !t.is_dir()))
                .map(|e| e.path().to_string_lossy().into_owned())
                .filter(|p| is_media(p))
                .collect()
        })
        .unwrap_or_default();
    v.sort_by(|a, b| natural_cmp(a, b));
    v
}

/// The file before/after `path` in its folder (MPC's "search in folder" for a one-item playlist).
pub fn folder_neighbor(path: &str, dir: i64) -> Option<String> {
    let files = folder_files(Path::new(path).parent()?);
    let i = files.iter().position(|f| f == path)? as i64 + dir;
    files.get(usize::try_from(i).ok()?).cloned()
}

// --- playlist files ---

fn resolve(base: &Path, p: &str) -> String {
    let p = p.trim();
    if p.contains("://") || Path::new(p).is_absolute() {
        return p.strip_prefix("file://").map_or(p.to_string(), |s| s.to_string());
    }
    base.join(p.replace('\\', "/")).to_string_lossy().into_owned()
}

/// Read a playlist file. A .cue sheet is kept as one entry: mpv plays it with its tracks as chapters.
pub fn load(path: &Path) -> Option<Vec<Entry>> {
    let e = ext(&path.to_string_lossy());
    if e == "cue" {
        return Some(vec![Entry::new(path.to_string_lossy())]);
    }
    let bytes = std::fs::read(path).ok()?;
    let text = String::from_utf8_lossy(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes)).into_owned();
    let base = path.parent().unwrap_or(Path::new("/"));
    let items = match e.as_str() {
        "mpcpl" => parse_mpcpl(&text, base)?,
        "pls" => parse_pls(&text, base),
        "asx" => parse_asx(&text, base),
        _ => parse_m3u(&text, base),
    };
    Some(items)
}

fn parse_mpcpl(text: &str, base: &Path) -> Option<Vec<Entry>> {
    let mut lines = text.lines();
    if lines.next()?.trim() != "MPCPLAYLIST" {
        return None;
    }
    let mut items: Vec<(i64, Entry)> = Vec::new();
    for line in lines {
        let mut parts = line.trim_end_matches('\r').splitn(3, ',');
        let (Some(i), Some(key), Some(value)) = (parts.next(), parts.next(), parts.next()) else { continue };
        let Ok(i) = i.parse::<i64>() else { continue };
        let idx = match items.iter().position(|(n, _)| *n == i) {
            Some(p) => p,
            None => {
                items.push((i, Entry::new("")));
                items.len() - 1
            }
        };
        let e = &mut items[idx].1;
        match key {
            "label" => e.label = Some(value.to_string()),
            "filename" if e.path.is_empty() => e.path = resolve(base, value),
            "subtitle" => e.subs.push(resolve(base, value)),
            _ => {}
        }
    }
    items.sort_by_key(|(i, _)| *i);
    Some(items.into_iter().map(|(_, e)| e).filter(|e| !e.path.is_empty()).collect())
}

fn parse_m3u(text: &str, base: &Path) -> Vec<Entry> {
    let mut out = Vec::new();
    let (mut label, mut duration) = (None, None);
    for line in text.lines().map(str::trim) {
        if let Some(info) = line.strip_prefix("#EXTINF:") {
            let (d, t) = info.split_once(',').unwrap_or((info, ""));
            duration = d.trim().parse::<f64>().ok().filter(|d| *d > 0.0);
            label = Some(t.trim().to_string()).filter(|t| !t.is_empty());
        } else if !line.is_empty() && !line.starts_with('#') {
            let mut e = Entry::new(resolve(base, line));
            e.label = label.take();
            e.duration = duration.take();
            out.push(e);
        }
    }
    out
}

fn parse_pls(text: &str, base: &Path) -> Vec<Entry> {
    let mut items: Vec<(u32, Entry)> = Vec::new();
    for line in text.lines().map(str::trim) {
        let Some((key, value)) = line.split_once('=') else { continue };
        let key = key.trim().to_ascii_lowercase();
        let (field, n) = ["file", "title", "length"]
            .iter()
            .find_map(|f| key.strip_prefix(f).and_then(|n| n.parse::<u32>().ok()).map(|n| (*f, n)))
            .unzip();
        let (Some(field), Some(n)) = (field, n) else { continue };
        let idx = items.iter().position(|(i, _)| *i == n).unwrap_or_else(|| {
            items.push((n, Entry::new("")));
            items.len() - 1
        });
        let e = &mut items[idx].1;
        match field {
            "file" => e.path = resolve(base, value),
            "title" => e.label = Some(value.trim().to_string()),
            _ => e.duration = value.trim().parse::<f64>().ok().filter(|d| *d > 0.0),
        }
    }
    items.sort_by_key(|(i, _)| *i);
    items.into_iter().map(|(_, e)| e).filter(|e| !e.path.is_empty()).collect()
}

/// Minimal ASX reader: each `<entry>` with a `<ref href>` and optional `<title>`.
fn parse_asx(text: &str, base: &Path) -> Vec<Entry> {
    let lower = text.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(start) = lower[from..].find("<entry").map(|i| i + from) {
        let end = lower[start..].find("</entry>").map_or(lower.len(), |i| i + start);
        let (l, t) = (&lower[start..end], &text[start..end]);
        if let Some(r) = l.find("<ref") {
            let attr = l[r..].find("href").map(|h| r + h);
            let href = attr.and_then(|h| {
                let q = t[h..].find(['"', '\''])? + h;
                let quote = &t[q..q + 1];
                let close = t[q + 1..].find(quote)? + q + 1;
                Some(&t[q + 1..close])
            });
            if let Some(href) = href {
                let mut e = Entry::new(resolve(base, &xml_unescape(href)));
                if let (Some(a), Some(b)) = (l.find("<title>"), l.find("</title>")) {
                    e.label = Some(xml_unescape(t[a + 7..b].trim()));
                }
                out.push(e);
            }
        }
        from = end;
    }
    out
}

fn xml_unescape(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Write a playlist; the format comes from the extension (.mpcpl, .pls, .asx, else M3U).
/// Paths are stored relative when every file is in the playlist's own folder, as MPC does.
pub fn save(path: &Path, items: &[Entry]) -> std::io::Result<()> {
    let base: PathBuf = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let all_local = items.iter().all(|e| Path::new(&e.path).parent() == Some(base.as_path()) && e.subs.iter().all(|s| Path::new(s).parent() == Some(base.as_path())));
    let p = |s: &str| if all_local { Path::new(s).file_name().map_or(s.to_string(), |n| n.to_string_lossy().into_owned()) } else { s.to_string() };
    let mut out = String::new();
    match ext(&path.to_string_lossy()).as_str() {
        "mpcpl" => {
            out += "MPCPLAYLIST\n";
            for (i, e) in items.iter().enumerate() {
                let n = i + 1;
                let _ = writeln!(out, "{n},type,0");
                if let Some(l) = &e.label {
                    let _ = writeln!(out, "{n},label,{l}");
                }
                let _ = writeln!(out, "{n},filename,{}", p(&e.path));
                for s in &e.subs {
                    let _ = writeln!(out, "{n},subtitle,{}", p(s));
                }
            }
        }
        "pls" => {
            out += "[playlist]\n";
            for (i, e) in items.iter().enumerate() {
                let n = i + 1;
                let _ = writeln!(out, "File{n}={}", p(&e.path));
                let _ = writeln!(out, "Title{n}={}", e.title());
                let _ = writeln!(out, "Length{n}={}", e.duration.map_or(-1, |d| d as i64));
            }
            let _ = writeln!(out, "NumberOfEntries={}\nVersion=2", items.len());
        }
        "asx" => {
            out += "<ASX version = \"3.0\">\n";
            for e in items {
                let _ = writeln!(out, "<Entry>\n<Title>{}</Title>\n<Ref href = \"{}\"/>\n</Entry>", xml_escape(&e.title()), xml_escape(&p(&e.path)));
            }
            out += "</ASX>\n";
        }
        _ => {
            out += "#EXTM3U\n";
            for e in items {
                let _ = writeln!(out, "#EXTINF:{},{}", e.duration.map_or(-1, |d| d as i64), e.title());
                let _ = writeln!(out, "{}", p(&e.path));
            }
        }
    }
    std::fs::write(path, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(v: &[Entry]) -> Vec<&str> {
        v.iter().map(|e| e.path.as_str()).collect()
    }

    #[test]
    fn natural_order() {
        let mut v = vec!["Ep10.mkv", "ep2.mkv", "Ep1.mkv", "ep02b.mkv"];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(v, ["Ep1.mkv", "ep2.mkv", "ep02b.mkv", "Ep10.mkv"]);
    }

    #[test]
    fn formats_roundtrip() {
        let dir = std::env::temp_dir().join(format!("mpc-rs-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let d = dir.to_string_lossy();
        let mut a = Entry::new(format!("{d}/a b.mkv"));
        a.label = Some("First".into());
        a.duration = Some(61.0);
        let b = Entry::new("https://example.com/s.mp3");
        for ext in ["mpcpl", "m3u", "m3u8", "pls", "asx"] {
            let file = dir.join(format!("list.{ext}"));
            save(&file, &[a.clone(), b.clone()]).unwrap();
            let back = load(&file).unwrap();
            assert_eq!(paths(&back), [a.path.as_str(), b.path.as_str()], "{ext}");
            assert_eq!(back[0].label.as_deref(), Some("First"), "{ext}");
        }
        // All files local to the playlist folder: stored relative, read back absolute.
        let file = dir.join("local.mpcpl");
        save(&file, &[Entry::new(format!("{d}/x.mkv"))]).unwrap();
        assert!(std::fs::read_to_string(&file).unwrap().contains("1,filename,x.mkv"));
        assert_eq!(paths(&load(&file).unwrap()), [format!("{d}/x.mkv").as_str()]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn parse_foreign() {
        let base = Path::new("/m");
        let m3u = parse_m3u("#EXTM3U\n#EXTINF:12,Song\nsub\\a.mp3\n\nb.mp3\n", base);
        assert_eq!(paths(&m3u), ["/m/sub/a.mp3", "/m/b.mp3"]);
        assert_eq!((m3u[0].label.as_deref(), m3u[0].duration), (Some("Song"), Some(12.0)));
        let pls = parse_pls("[playlist]\nFile2=/x/2.mp3\nfile1=1.mp3\nTitle1=One\n", base);
        assert_eq!(paths(&pls), ["/m/1.mp3", "/x/2.mp3"]);
        let asx = parse_asx("<asx><entry><title>T &amp; U</title><REF HREF='a.wmv'/></entry><Entry><Ref href=\"http://h/b\"/></Entry></asx>", base);
        assert_eq!(paths(&asx), ["/m/a.wmv", "http://h/b"]);
        assert_eq!(asx[0].label.as_deref(), Some("T & U"));
        assert!(parse_mpcpl("nope\n", base).is_none());
    }

    #[test]
    fn stepping() {
        let mut pl = Playlist::default();
        pl.add((0..4).map(|i| Entry::new(format!("{i}"))).collect());
        let mut r = |n: usize| n - 1; // deterministic "random"
        assert_eq!(pl.step(1, false, &mut r), Some(0)); // nothing current: start at the top
        pl.current = Some(3);
        assert_eq!(pl.step(1, false, &mut r), None);
        assert_eq!(pl.step(1, true, &mut r), Some(0));
        assert_eq!(pl.step(-1, false, &mut r), Some(2));
        pl.shuffle = true;
        let mut seen = vec![3];
        for _ in 0..3 {
            let n = pl.step(1, false, &mut r).unwrap();
            seen.push(n);
            pl.current = Some(n);
        }
        seen.sort();
        assert_eq!(seen, [0, 1, 2, 3], "shuffle visits every item once");
        assert_eq!(pl.step(1, false, &mut r), None);
    }

    #[test]
    fn edits_keep_current() {
        let mut pl = Playlist::default();
        pl.add(["c", "a", "b"].iter().map(|s| Entry::new(*s)).collect());
        pl.current = Some(0); // "c"
        pl.sort_by_path();
        assert_eq!(paths(&pl.items), ["a", "b", "c"]);
        assert_eq!(pl.current, Some(2));
        pl.restore_order();
        assert_eq!(pl.current, Some(0));
        pl.move_item(0, 2);
        assert_eq!(paths(&pl.items), ["a", "b", "c"]);
        assert_eq!(pl.current, Some(2));
        pl.remove(&[0]);
        assert_eq!(pl.current, Some(1));
        pl.remove(&[1]);
        assert_eq!(pl.current, None);
    }
}
