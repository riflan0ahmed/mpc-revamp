//! Discs and TV: DVD/Blu-ray folder detection, DVB channel lists.

use mpc_rv::{dialogs::{disc_url, is_disc_url}, dvb::{channel_url, is_dvb, parse_channels}};

#[test]
fn disc_urls() {
    let dir = std::env::temp_dir().join(format!("mpc-rv-disc-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("dvd/VIDEO_TS")).unwrap();
    std::fs::create_dir_all(dir.join("bd/BDMV")).unwrap();
    let abs = dir.canonicalize().unwrap();
    assert_eq!(disc_url(&dir.join("dvd")), Some(format!("dvd:///{}/dvd", abs.display())));
    assert_eq!(disc_url(&dir.join("dvd/VIDEO_TS")), Some(format!("dvd:///{}/dvd", abs.display())));
    assert_eq!(disc_url(&dir.join("bd")), Some(format!("bd:///{}/bd", abs.display())));
    assert_eq!(disc_url(&dir), None);
    assert!(is_disc_url("cdda:////dev/sr0") && !is_disc_url("/x.mkv"));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn channel_names() {
    let zap = "# dvbv5-scan -O ZAP\nDas Erste HD:474000000:INVERSION_AUTO:BANDWIDTH_8_MHZ:FEC_AUTO:FEC_AUTO:QAM_AUTO:TRANSMISSION_MODE_AUTO:GUARD_INTERVAL_AUTO:HIERARCHY_NONE:5121:5122:28721\n\n";
    let vdr = "ZDF;ZDFvision:11953:HC34M2S0:S19.2E:27500:110=2:120=deu:130:0:28006:1:1079:0\n";
    assert_eq!(parse_channels(zap), ["Das Erste HD"]);
    assert_eq!(parse_channels(vdr), ["ZDF"]);
    assert_eq!(parse_channels("garbage\n"), Vec::<String>::new());
    assert_eq!(channel_url("Das Erste HD"), "dvb://Das Erste HD");
    assert!(is_dvb("dvb://X") && !is_dvb("/x.ts"));
}
