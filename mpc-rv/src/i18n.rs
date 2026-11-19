//! Translations from MPC-HC's PO files (`mpcresources/PO`, kept in `res/po`). They're keyed by MPC's
//! English text, so a label is translated when it matches MPC's wording; anything else stays English.
//! MPC marks mnemonics with `&`, GTK with `_`; both are ignored for matching.

use std::{collections::HashMap, path::PathBuf, sync::OnceLock};

/// Text (mnemonics removed) -> every (msgctxt, translation) MPC has for it, menus first.
static TABLE: OnceLock<HashMap<String, Vec<(String, String)>>> = OnceLock::new();

/// The languages MPC has translations for: (PO code, name).
pub const LANGUAGES: &[(&str, &str)] = &[
    ("ar", "العربية"), ("be", "Беларуская"), ("bn", "বাংলা"), ("bs_BA", "Bosanski"), ("ca", "Català"), ("cs", "Čeština"),
    ("da", "Dansk"), ("de", "Deutsch"), ("el", "Ελληνικά"), ("en_GB", "English (UK)"), ("es", "Español"), ("eu", "Euskara"),
    ("fi", "Suomi"), ("fr", "Français"), ("gl", "Galego"), ("he", "עברית"), ("hr", "Hrvatski"), ("hu", "Magyar"),
    ("hy", "Հայերեն"), ("id", "Bahasa Indonesia"), ("it", "Italiano"), ("ja", "日本語"), ("ko", "한국어"), ("lt", "Lietuvių"),
    ("ms_MY", "Bahasa Melayu"), ("nl", "Nederlands"), ("pa", "ਪੰਜਾਬੀ"), ("pl", "Polski"), ("pt_BR", "Português (Brasil)"),
    ("ro", "Română"), ("ru", "Русский"), ("sk", "Slovenčina"), ("sl", "Slovenščina"), ("sr", "Српски"), ("sv", "Svenska"),
    ("th_TH", "ไทย"), ("tr", "Türkçe"), ("tt", "Татарча"), ("uk", "Українська"), ("vi", "Tiếng Việt"), ("zh_CN", "简体中文"),
    ("zh_TW", "繁體中文"),
];

/// Where the PO files are: installed next to the binary, in the data dirs, or in the source tree.
fn po_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(exe_dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.to_path_buf())) {
        dirs.push(exe_dir.join("../share/mpc-rv/po"));
    }
    dirs.extend(gtk4::glib::system_data_dirs().iter().map(|d| d.join("mpc-rv/po")));
    dirs.push(PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/res/po")));
    dirs
}

/// The PO code for `setting` ("" = from the environment, like gettext; "en" = English).
fn resolve(setting: &str) -> Option<String> {
    let env = || ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"].iter().find_map(|v| std::env::var(v).ok().filter(|s| !s.is_empty() && s != "C" && s != "POSIX"));
    let want = if setting.is_empty() { env()?.split(':').next()?.split('.').next()?.to_string() } else { setting.to_string() };
    if want == "en" || want.starts_with("en_US") {
        return None;
    }
    // "de_DE" -> "de_DE" or "de"; "pt_BR" stays itself.
    let base = want.split('_').next().unwrap_or("").to_string();
    LANGUAGES.iter().map(|(c, _)| *c).find(|c| *c == want).or_else(|| LANGUAGES.iter().map(|(c, _)| *c).find(|c| *c == base)).map(String::from)
}

/// Load the translations for the language setting. Call once, before building any UI.
pub fn init(setting: &str) {
    let mut table = HashMap::new();
    if let Some(code) = resolve(setting) {
        for dir in po_dirs() {
            let mut found = false;
            for part in ["menus", "dialogs", "strings"] {
                if let Ok(text) = std::fs::read_to_string(dir.join(format!("mpc-hc.{code}.{part}.po"))) {
                    found = true;
                    for (ctx, id, s) in parse(&text) {
                        if !s.is_empty() {
                            table.entry(key(&id)).or_insert_with(Vec::new).push((ctx, s));
                        }
                    }
                }
            }
            if found {
                break;
            }
        }
    }
    let _ = TABLE.set(table);
}

/// The text with mnemonic markers removed (`&` and `_`; `&&` is a literal `&`).
fn key(s: &str) -> String {
    s.replace("&&", "\u{1}").replace(['&', '_'], "").replace('\u{1}', "&").trim().to_string()
}

/// Translate a UI string. Menu labels keep a mnemonic (MPC's `&x` becomes GTK's `_x`).
pub fn tr(s: &str) -> String {
    lookup(s, |_| true)
}

/// Translate an Options page name: MPC's page captions (context `IDD_PPAGE...`) win over the same
/// words used elsewhere ("Playback" is also a group box, translated differently).
pub fn tr_page(s: &str) -> String {
    lookup(s, |ctx| ctx.starts_with("IDD_PPAGE") && !ctx.contains("_IDC"))
}

fn lookup(s: &str, prefer: impl Fn(&str) -> bool) -> String {
    let Some(all) = TABLE.get().and_then(|t| t.get(&key(s))) else { return s.to_string() };
    let t = &all.iter().find(|(c, _)| prefer(c)).unwrap_or(&all[0]).1;
    if s.contains('_') {
        t.replace('_', "__").replace("&&", "\u{1}").replace('&', "_").replace('\u{1}', "&")
    } else {
        t.replace("&&", "\u{1}").replace('&', "").replace('\u{1}', "&")
    }
}

/// (msgctxt, msgid, msgstr) of a PO file (multi-line strings joined, C escapes undone).
fn parse(text: &str) -> Vec<(String, String, String)> {
    let unquote = |s: &str| -> String {
        let s = s.trim();
        let s = s.strip_prefix('"').and_then(|s| s.strip_suffix('"')).unwrap_or("");
        let mut out = String::new();
        let mut chars = s.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                match chars.next() {
                    Some('n') => out.push('\n'),
                    Some('t') => out.push('\t'),
                    Some(o) => out.push(o),
                    None => {}
                }
            } else {
                out.push(c);
            }
        }
        out
    };
    let mut out = Vec::new();
    let (mut ctx, mut id, mut s, mut field) = (String::new(), String::new(), String::new(), 0);
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("msgctxt ") {
            if field == 2 && !id.is_empty() {
                out.push((std::mem::take(&mut ctx), std::mem::take(&mut id), std::mem::take(&mut s)));
            }
            ctx = unquote(rest);
            field = 0;
        } else if let Some(rest) = line.strip_prefix("msgid ") {
            if field == 2 && !id.is_empty() {
                out.push((std::mem::take(&mut ctx), std::mem::take(&mut id), std::mem::take(&mut s)));
            }
            id = unquote(rest);
            s.clear();
            field = 1;
        } else if let Some(rest) = line.strip_prefix("msgstr ") {
            s = unquote(rest);
            field = 2;
        } else if line.starts_with('"') {
            match field {
                1 => id += &unquote(line),
                2 => s += &unquote(line),
                _ => {}
            }
        } else if line.is_empty() {
            if field == 2 && !id.is_empty() {
                out.push((std::mem::take(&mut ctx), std::mem::take(&mut id), std::mem::take(&mut s)));
            }
            field = 0;
        }
    }
    if field == 2 && !id.is_empty() {
        out.push((ctx, id, s));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn po_and_mnemonics() {
        let po = "msgctxt \"POPUP\"\nmsgid \"&File\"\nmsgstr \"&Datei\"\n\nmsgctxt \"X\"\nmsgid \"\"\n\"Caption&&Menu\"\nmsgstr \"Titel&&Men\\\"\"\n\"ü\"\n";
        let p = parse(po);
        assert_eq!(p, vec![("POPUP".into(), "&File".into(), "&Datei".into()), ("X".into(), "Caption&&Menu".into(), "Titel&&Men\"ü".into())]);
        assert_eq!(key("_Quick Open File..."), "Quick Open File...");
        assert_eq!(key("Caption&&Menu"), "Caption&Menu");
        assert_eq!(resolve("en"), None);
        assert_eq!(resolve("de_AT"), Some("de".into()));
        assert_eq!(resolve("pt_BR"), Some("pt_BR".into()));
        // The German PO file translates MPC's own menu text.
        let de = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/res/po/mpc-hc.de.menus.po")).unwrap();
        assert!(parse(&de).iter().any(|(_, id, s)| id == "&File" && s == "&Datei"));
    }
}
