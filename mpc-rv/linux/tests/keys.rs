//! The default key bindings (in `mpc-core`) use GDK key names; each must be one GDK knows.

use gtk4::gdk::Key;
use mpc_rv::commands::COMMANDS;

#[test]
fn default_keys_are_gdk_names() {
    for c in COMMANDS {
        let key = c.default_binding().key;
        if !key.is_empty() {
            let name = key.rsplit('>').next().unwrap();
            assert!(Key::from_name(name).is_some(), "bad key {key} for {}", c.action);
        }
    }
}
