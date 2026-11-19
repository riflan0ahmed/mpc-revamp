//! Text subtitle files (SRT, WebVTT, ASS/SSA) for the subresync bar and File > Save Subtitles.
//! mpv renders subtitles; this only reads the timings, lets them be changed, and writes them back.
//! ASS files are rewritten in place (only the Start/End fields change), so styles survive.

use std::path::Path;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Srt,
    Vtt,
    Ass,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    /// Times as read from the file, in seconds.
    pub orig_start: f64,
    pub orig_end: f64,
    pub text: String,
    /// For ASS: the index of the Dialogue line in `Doc::raw`.
    raw: usize,
}

#[derive(Clone, Debug)]
pub struct Doc {
    pub kind: Kind,
    pub lines: Vec<Line>,
    /// ASS: the whole file, line by line (Dialogue lines are rewritten on save).
    raw: Vec<String>,
    /// ASS: positions of Start and End in the Dialogue fields.
    ass_fields: (usize, usize),
    /// Subresync anchors: (line index, new start). Lines between anchors are stretched, lines outside
    /// are shifted along with the nearest anchor (MPC's subresync behavior).
    pub anchors: Vec<(usize, f64)>,
}

pub fn kind_of(path: &str) -> Option<Kind> {
    match Path::new(path).extension()?.to_string_lossy().to_ascii_lowercase().as_str() {
        "srt" => Some(Kind::Srt),
        "vtt" => Some(Kind::Vtt),
        "ass" | "ssa" => Some(Kind::Ass),
        _ => None,
    }
}

/// "01:02:03,456", "01:02:03.456", "02:03.456" (VTT) or "1:02:03.45" (ASS) to seconds.
fn parse_time(s: &str) -> Option<f64> {
    let s = s.trim().replace(',', ".");
    let parts: Vec<&str> = s.split(':').collect();
    let (h, m, sec) = match parts.as_slice() {
        [h, m, s] => (h.parse::<f64>().ok()?, m.parse::<f64>().ok()?, s.parse::<f64>().ok()?),
        [m, s] => (0.0, m.parse::<f64>().ok()?, s.parse::<f64>().ok()?),
        _ => return None,
    };
    Some(h * 3600.0 + m * 60.0 + sec)
}

/// Seconds to "hh:mm:ss<sep>mmm" (SRT/VTT) or "h:mm:ss.cc" (ASS).
pub fn format_time(t: f64, kind: Kind) -> String {
    let t = t.max(0.0);
    if kind == Kind::Ass {
        let cs = (t * 100.0).round() as u64;
        return format!("{}:{:02}:{:02}.{:02}", cs / 360_000, cs / 6000 % 60, cs / 100 % 60, cs % 100);
    }
    let ms = (t * 1000.0).round() as u64;
    let sep = if kind == Kind::Srt { ',' } else { '.' };
    format!("{:02}:{:02}:{:02}{sep}{:03}", ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60, ms % 1000)
}

pub fn load(path: &str) -> Option<Doc> {
    let kind = kind_of(path)?;
    let bytes = std::fs::read(path).ok()?;
    let text = String::from_utf8_lossy(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes)).replace("\r\n", "\n");
    let doc = match kind {
        Kind::Ass => parse_ass(&text),
        _ => parse_cues(&text, kind),
    };
    Some(doc).filter(|d| !d.lines.is_empty())
}

/// SRT and WebVTT: blocks of "[id]\nstart --> end [settings]\ntext...".
fn parse_cues(text: &str, kind: Kind) -> Doc {
    let mut lines = Vec::new();
    for block in text.split("\n\n") {
        let rows: Vec<&str> = block.lines().collect();
        let Some(t) = rows.iter().position(|r| r.contains("-->")) else { continue };
        let (a, b) = rows[t].split_once("-->").unwrap();
        let b = b.split_whitespace().next().unwrap_or("");
        if let (Some(start), Some(end)) = (parse_time(a), parse_time(b)) {
            lines.push(Line { orig_start: start, orig_end: end, text: rows[t + 1..].join("\n"), raw: 0 });
        }
    }
    Doc { kind, lines, raw: Vec::new(), ass_fields: (0, 0), anchors: Vec::new() }
}

fn parse_ass(text: &str) -> Doc {
    let raw: Vec<String> = text.lines().map(String::from).collect();
    let mut fields = (1, 2); // ASS default: Layer, Start, End, ...
    let mut n_fields = 10;
    let mut in_events = false;
    let mut lines = Vec::new();
    for (i, row) in raw.iter().enumerate() {
        let t = row.trim();
        if t.starts_with('[') {
            in_events = t.eq_ignore_ascii_case("[events]");
        } else if in_events && let Some(f) = t.strip_prefix("Format:") {
            let names: Vec<String> = f.split(',').map(|s| s.trim().to_ascii_lowercase()).collect();
            n_fields = names.len();
            fields = (names.iter().position(|n| n == "start").unwrap_or(1), names.iter().position(|n| n == "end").unwrap_or(2));
        } else if in_events && let Some(d) = t.strip_prefix("Dialogue:") {
            let parts: Vec<&str> = d.splitn(n_fields, ',').collect();
            let (Some(a), Some(b)) = (parts.get(fields.0), parts.get(fields.1)) else { continue };
            if let (Some(start), Some(end)) = (parse_time(a), parse_time(b)) {
                let text = parts.last().unwrap_or(&"").replace("\\N", "\n");
                lines.push(Line { orig_start: start, orig_end: end, text, raw: i });
            }
        }
    }
    Doc { kind: Kind::Ass, lines, raw, ass_fields: fields, anchors: Vec::new() }
}

impl Doc {
    /// Map an original time to the resynced one: piecewise linear through the anchors, shifted outside.
    pub fn map(&self, t: f64) -> f64 {
        let mut pts: Vec<(f64, f64)> = self.anchors.iter().map(|&(i, new)| (self.lines[i].orig_start, new)).collect();
        pts.sort_by(|a, b| a.0.total_cmp(&b.0));
        match pts.as_slice() {
            [] => t,
            [(o, n)] => t + (n - o),
            _ => {
                let seg = pts.windows(2).find(|w| t <= w[1].0).unwrap_or(&pts[pts.len() - 2..]);
                let ((o0, n0), (o1, n1)) = (seg[0], seg[1]);
                if t <= o0 {
                    t + (n0 - o0)
                } else if t >= o1 {
                    t + (n1 - o1)
                } else {
                    n0 + (t - o0) * (n1 - n0) / (o1 - o0)
                }
            }
        }
    }

    pub fn start(&self, i: usize) -> f64 {
        self.map(self.lines[i].orig_start)
    }

    pub fn end(&self, i: usize) -> f64 {
        self.map(self.lines[i].orig_end)
    }

    /// Give line `i` a new start time; it becomes an anchor.
    pub fn set_start(&mut self, i: usize, t: f64) {
        self.anchors.retain(|(a, _)| *a != i);
        // Anchors must stay in order, or the mapping would fold back on itself.
        let o = self.lines[i].orig_start;
        self.anchors.retain(|&(a, n)| {
            let oa = self.lines[a].orig_start;
            (oa < o && n < t) || (oa > o && n > t)
        });
        self.anchors.push((i, t));
    }

    /// The line showing at (resynced) time `t`, or the last one before it.
    pub fn line_at(&self, t: f64) -> Option<usize> {
        (0..self.lines.len()).rev().find(|&i| self.start(i) <= t)
    }

    /// The file text with the resynced times, all shifted by `offset` seconds (the subtitle delay).
    pub fn to_text(&self, offset: f64) -> String {
        let s = |i| self.start(i) + offset;
        let e = |i| self.end(i) + offset;
        match self.kind {
            Kind::Ass => {
                let mut raw = self.raw.clone();
                for (i, l) in self.lines.iter().enumerate() {
                    let (start, end) = (s(i), e(i));
                    raw[l.raw] = rewrite_ass_line(&raw[l.raw], self.ass_fields, &format_time(start, Kind::Ass), &format_time(end, Kind::Ass));
                }
                raw.join("\n") + "\n"
            }
            kind => {
                let mut out = if kind == Kind::Vtt { "WEBVTT\n\n".to_string() } else { String::new() };
                for (i, l) in self.lines.iter().enumerate() {
                    if kind == Kind::Srt {
                        out += &format!("{}\n", i + 1);
                    }
                    out += &format!("{} --> {}\n{}\n\n", format_time(s(i), kind), format_time(e(i), kind), l.text);
                }
                out
            }
        }
    }
}

fn rewrite_ass_line(line: &str, (fs, fe): (usize, usize), start: &str, end: &str) -> String {
    let (head, rest) = line.split_once(':').unwrap_or((line, ""));
    let mut parts: Vec<String> = rest.splitn(fs.max(fe) + 2, ',').map(String::from).collect();
    let lead = |p: &String| p.len() - p.trim_start().len();
    if let Some(p) = parts.get_mut(fs) {
        *p = format!("{}{start}", &p[..lead(p)]);
    }
    if let Some(p) = parts.get_mut(fe) {
        *p = end.to_string();
    }
    format!("{head}:{}", parts.join(","))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRT: &str = "1\n00:00:01,000 --> 00:00:02,500\nHello\nworld\n\n2\n00:00:10,000 --> 00:00:11,000\nTwo\n\n3\n00:00:20,000 --> 00:00:21,000\nThree\n";
    const ASS: &str = "[Script Info]\nTitle: x\n\n[V4+ Styles]\nFormat: Name, Fontname\nStyle: Default,Arial\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:01.00,0:00:02.50,Default,,0,0,0,,Hi, there\\Nline two\n";

    #[test]
    fn times() {
        assert_eq!(parse_time("01:02:03,456"), Some(3723.456));
        assert_eq!(parse_time("02:03.5"), Some(123.5));
        assert_eq!(format_time(3723.456, Kind::Srt), "01:02:03,456");
        assert_eq!(format_time(3723.456, Kind::Vtt), "01:02:03.456");
        assert_eq!(format_time(3723.456, Kind::Ass), "1:02:03.46");
    }

    #[test]
    fn srt_roundtrip_and_offset() {
        let d = parse_cues(SRT, Kind::Srt);
        assert_eq!(d.lines.len(), 3);
        assert_eq!(d.lines[0].text, "Hello\nworld");
        assert_eq!(parse_cues(&d.to_text(0.0), Kind::Srt).lines, d.lines);
        let shifted = parse_cues(&d.to_text(1.5), Kind::Srt);
        assert_eq!(shifted.lines[1].orig_start, 11.5);
        let vtt = parse_cues("WEBVTT\n\n00:01.000 --> 00:02.000 align:start\nA\n", Kind::Vtt);
        assert_eq!((vtt.lines[0].orig_start, vtt.lines[0].orig_end), (1.0, 2.0));
    }

    #[test]
    fn ass_rewrite_keeps_the_rest() {
        let mut d = parse_ass(ASS);
        assert_eq!(d.lines[0].text, "Hi, there\nline two");
        d.set_start(0, 2.0);
        let out = d.to_text(0.0);
        assert!(out.contains("Dialogue: 0,0:00:02.00,0:00:03.50,Default,,0,0,0,,Hi, there\\Nline two"), "{out}");
        assert!(out.contains("Style: Default,Arial"));
    }

    #[test]
    fn resync_anchors() {
        let mut d = parse_cues(SRT, Kind::Srt);
        d.set_start(1, 12.0); // one anchor: everything shifts by +2
        assert_eq!((d.start(0), d.start(2)), (3.0, 22.0));
        d.set_start(2, 30.0); // second anchor: lines between stretch, before shift with the first
        assert_eq!(d.start(1), 12.0);
        assert_eq!(d.start(0), 3.0);
        assert_eq!(d.map(15.0), 21.0); // halfway between 10->12 and 20->30
        assert_eq!(d.end(2), 31.0);
        d.set_start(1, 40.0); // would cross the anchor after it: that anchor is dropped
        assert_eq!(d.anchors, vec![(1, 40.0)]);
        assert_eq!(d.line_at(41.0), Some(1));
        assert_eq!(d.line_at(0.5), None);
    }
}
