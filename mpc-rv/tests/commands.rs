//! Key bindings: only changed bindings are stored, and they load back as they were.

use mpc_rv::commands::{Binding, load, overrides};
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
