//! Key bindings: only changed bindings are stored, and they load back as they were.

use mpc_core::commands::{Binding, COMMANDS, is_dvd, load, overrides};
use std::collections::BTreeMap;

#[test]
fn overrides_roundtrip() {
    let mut b = load(&BTreeMap::new());
    assert!(overrides(&b).is_empty());
    b[0].key = "F9".into();
    let o = overrides(&b);
    assert_eq!(o.len(), 1);
    let text = toml::to_string(&o).unwrap();
    let back: BTreeMap<String, Binding> = toml::from_str(&text).unwrap();
    assert_eq!(load(&back), b);
}

#[test]
fn dvd_commands_are_hidden() {
    let dvd: Vec<&str> = COMMANDS.iter().map(|c| c.action).filter(|a| is_dvd(a)).collect();
    assert_eq!(dvd.len(), 21, "{dvd:?}");
    assert!(is_dvd("file-opendvdbd") && is_dvd("navigate-titlemenu") && is_dvd("navigate-menu-left") && is_dvd("dvd-angle-next"));
    for other in ["navigate-goto", "navigate-skipforward", "navigate-tunerscan", "file-openmedia"] {
        assert!(!is_dvd(other), "{other}");
    }
}
