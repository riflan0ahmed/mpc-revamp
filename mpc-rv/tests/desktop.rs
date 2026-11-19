//! Desktop integration: MIME types per kind and the .desktop entry.

use mpc_rv::integration::{APP_ID, Kind, desktop_entry, mime_types};

#[test]
fn mime_lists() {
    let v = mime_types(Kind::Video);
    assert!(v.contains(&"video/x-matroska".to_string()) && v.contains(&"video/mp4".to_string()), "{v:?}");
    assert!(mime_types(Kind::Audio).iter().all(|m| m.starts_with("audio/")));
    assert!(!mime_types(Kind::Playlist).is_empty());
    assert!(desktop_entry().contains("MimeType=") && desktop_entry().contains(&format!("Icon={APP_ID}")));
}
