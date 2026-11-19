//! Playlists: natural sorting, every playlist file format, next/previous with shuffle, edits.

use mpc_rv::playlist::{self, Entry, Playlist, natural_cmp};

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
    let dir = std::env::temp_dir().join(format!("mpc-rv-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let d = dir.to_string_lossy();
    let mut a = Entry::new(format!("{d}/a b.mkv"));
    a.label = Some("First".into());
    a.duration = Some(61.0);
    let b = Entry::new("https://example.com/s.mp3");
    for ext in ["mpcpl", "m3u", "m3u8", "pls", "asx"] {
        let file = dir.join(format!("list.{ext}"));
        playlist::save(&file, &[a.clone(), b.clone()]).unwrap();
        let back = playlist::load(&file).unwrap();
        assert_eq!(paths(&back), [a.path.as_str(), b.path.as_str()], "{ext}");
        assert_eq!(back[0].label.as_deref(), Some("First"), "{ext}");
    }
    // All files local to the playlist folder: stored relative, read back absolute.
    let file = dir.join("local.mpcpl");
    playlist::save(&file, &[Entry::new(format!("{d}/x.mkv"))]).unwrap();
    assert!(std::fs::read_to_string(&file).unwrap().contains("1,filename,x.mkv"));
    assert_eq!(paths(&playlist::load(&file).unwrap()), [format!("{d}/x.mkv").as_str()]);
    std::fs::remove_dir_all(&dir).unwrap();
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
