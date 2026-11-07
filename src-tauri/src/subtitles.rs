//! Subtitle files (SRT, WebVTT, ASS/SSA) split into what stays exactly as it was
//! (numbers, timings, headers, styles, positioning tags) and the text to translate,
//! then put back together with the translations, timings untouched.

use std::path::{Path, PathBuf};

use anyhow::Context;

/// Largest subtitle file we read
pub const MAX_SUBTITLE_BYTES: u64 = 10 * 1024 * 1024;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Format {
    Srt,
    Vtt,
    Ass,
}

impl Format {
    pub fn from_path(path: &Path) -> Option<Self> {
        match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
            "srt" => Some(Self::Srt),
            "vtt" => Some(Self::Vtt),
            "ass" | "ssa" => Some(Self::Ass),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Srt => "SRT",
            Self::Vtt => "WebVTT",
            Self::Ass => "ASS",
        }
    }
}

/// One subtitle's text. Each unit is translated on its own: the whole subtitle, or
/// each line when it's a dialogue ("- Hi" / "- Hello").
#[derive(Debug, Clone)]
pub struct Cue {
    /// Formatting kept in front of the text, like "{\an8}" or "<i>"
    prefix: String,
    /// Closing formatting kept after it, like "</i>"
    suffix: String,
    /// Dialogue dashes in front of each unit
    dashes: Vec<String>,
    pub units: Vec<String>,
    /// How many lines the original had, to wrap translations the same way
    line_count: usize,
}

#[derive(Debug)]
enum Part {
    Raw(String),
    Text(usize),
}

pub struct Document {
    pub format: Format,
    parts: Vec<Part>,
    pub cues: Vec<Cue>,
    eol: &'static str,
    bom: bool,
}

pub fn read(path: &Path) -> anyhow::Result<Document> {
    let format =
        Format::from_path(path).context("not a subtitle file (.srt, .vtt, .ass or .ssa)")?;
    let meta = std::fs::metadata(path)?;
    anyhow::ensure!(
        meta.len() <= MAX_SUBTITLE_BYTES,
        "subtitle file is too large"
    );
    let bytes = std::fs::read(path)?;
    Ok(parse(format, &decode(&bytes)))
}

/// UTF-8 (with or without BOM), or UTF-16 with BOM; anything else as Windows-1252-ish Latin-1
fn decode(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(rest).into_owned();
    }
    if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
        let le = bytes[0] == 0xFF;
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|c| {
                if le {
                    u16::from_le_bytes([c[0], c[1]])
                } else {
                    u16::from_be_bytes([c[0], c[1]])
                }
            })
            .collect();
        return String::from_utf16_lossy(&units);
    }
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_owned(),
        Err(_) => bytes.iter().map(|&b| b as char).collect(),
    }
}

pub fn parse(format: Format, text: &str) -> Document {
    let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let lines: Vec<&str> = text
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l))
        .collect();
    let mut doc = Document {
        format,
        parts: Vec::new(),
        cues: Vec::new(),
        eol,
        bom: true,
    };
    match format {
        Format::Srt | Format::Vtt => parse_blocks(&mut doc, &lines),
        Format::Ass => parse_ass(&mut doc, &lines),
    }
    doc
}

/// SRT and WebVTT: a timing line ("00:00:01,000 --> 00:00:02,000") is followed by
/// the text lines, up to an empty line
fn parse_blocks(doc: &mut Document, lines: &[&str]) {
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        doc.raw_line(line, i + 1 < lines.len());
        i += 1;
        if !line.contains("-->") {
            continue;
        }
        let start = i;
        while i < lines.len() && !lines[i].trim().is_empty() {
            i += 1;
        }
        if i > start {
            let cue = cue_from_lines(&lines[start..i], false);
            doc.text(cue);
            doc.raw(doc.eol.to_owned());
        }
    }
}

/// ASS/SSA: the last field of each "Dialogue:" line in [Events] is the text
fn parse_ass(doc: &mut Document, lines: &[&str]) {
    let mut in_events = false;
    for (n, line) in lines.iter().enumerate() {
        let more = n + 1 < lines.len();
        let trimmed = line.trim_start();
        if trimmed.starts_with('[') {
            in_events = trimmed.eq_ignore_ascii_case("[events]");
        }
        if in_events && trimmed.starts_with("Dialogue:") {
            // 9 commas before the text field
            let mut commas = 0;
            let split = line.char_indices().find(|&(_, c)| {
                if c == ',' {
                    commas += 1;
                }
                commas == 9
            });
            if let Some((idx, _)) = split {
                let (head, text) = line.split_at(idx + 1);
                // Vector drawings ({\p1}) aren't text
                if !text.contains("\\p1") && !strip_tags(text).trim().is_empty() {
                    let ass_lines: Vec<String> = text
                        .replace("\\n", "\\N")
                        .split("\\N")
                        .map(|s| s.replace("\\h", " "))
                        .collect();
                    let refs: Vec<&str> = ass_lines.iter().map(|s| s.as_str()).collect();
                    doc.raw(head.to_owned());
                    doc.text(cue_from_lines(&refs, true));
                    if more {
                        doc.raw(doc.eol.to_owned());
                    }
                    continue;
                }
            }
        }
        doc.raw_line(line, more);
    }
}

impl Document {
    fn raw(&mut self, s: String) {
        if let Some(Part::Raw(last)) = self.parts.last_mut() {
            last.push_str(&s);
        } else {
            self.parts.push(Part::Raw(s));
        }
    }

    fn raw_line(&mut self, line: &str, eol: bool) {
        let mut s = line.to_owned();
        if eol {
            s.push_str(self.eol);
        }
        self.raw(s);
    }

    fn text(&mut self, cue: Cue) {
        self.parts.push(Part::Text(self.cues.len()));
        self.cues.push(cue);
    }

    /// The text of the first subtitles, for a preview
    pub fn preview(&self, count: usize) -> Vec<String> {
        self.cues
            .iter()
            .take(count)
            .map(|c| c.units.join(" "))
            .collect()
    }

    /// Writes the file back with each cue's units replaced by `translated` (same
    /// order as `units`). `rtl` adds direction marks for Arabic, Persian, Urdu, Hebrew;
    /// `bilingual` keeps the original text under the translation.
    pub fn render(&self, translated: &[Vec<String>], rtl: bool, bilingual: bool) -> String {
        let mut out = String::new();
        if self.bom {
            out.push('\u{FEFF}');
        }
        let line_break = if self.format == Format::Ass {
            "\\N"
        } else {
            self.eol
        };
        for part in &self.parts {
            match part {
                Part::Raw(s) => out.push_str(s),
                Part::Text(i) => {
                    let cue = &self.cues[*i];
                    let mut lines = cue.render_lines(&translated[*i]);
                    if rtl {
                        lines = lines
                            .into_iter()
                            .map(|l| format!("\u{202B}{l}\u{202C}"))
                            .collect();
                    }
                    if bilingual {
                        lines.extend(cue.render_lines(&cue.units));
                    }
                    out.push_str(&cue.prefix);
                    out.push_str(&lines.join(line_break));
                    out.push_str(&cue.suffix);
                }
            }
        }
        out
    }
}

impl Cue {
    fn render_lines(&self, units: &[String]) -> Vec<String> {
        if self.dashes.iter().any(|d| !d.is_empty()) {
            // Dialogue: one line per speaker, dashes kept
            return units
                .iter()
                .zip(self.dashes.iter().chain(std::iter::repeat(&String::new())))
                .map(|(u, d)| format!("{d}{}", u.trim()))
                .collect();
        }
        let text = units.join(" ");
        if self.line_count >= 2 && text.chars().count() > 40 {
            wrap_in_two(&text)
        } else {
            vec![text.trim().to_owned()]
        }
    }
}

/// Splits a line in two at the space closest to its middle
fn wrap_in_two(text: &str) -> Vec<String> {
    let text = text.trim();
    let chars: Vec<char> = text.chars().collect();
    let middle = chars.len() / 2;
    let split = chars
        .iter()
        .enumerate()
        .filter(|(_, c)| **c == ' ')
        .min_by_key(|(i, _)| i.abs_diff(middle))
        .map(|(i, _)| i);
    match split {
        Some(i) => vec![
            chars[..i].iter().collect::<String>().trim().to_owned(),
            chars[i + 1..].iter().collect::<String>().trim().to_owned(),
        ],
        None => vec![text.to_owned()],
    }
}

fn cue_from_lines(lines: &[&str], ass: bool) -> Cue {
    let first = lines.first().copied().unwrap_or_default();
    let last = lines.last().copied().unwrap_or_default();
    let prefix = leading_tags(first, ass);
    let suffix = if ass {
        String::new()
    } else {
        trailing_tags(last)
    };

    let clean: Vec<String> = lines
        .iter()
        .map(|l| strip_tags(l).trim().to_owned())
        .filter(|l| !l.is_empty())
        .collect();
    let is_dialogue = clean.len() >= 2
        && clean
            .iter()
            .all(|l| l.starts_with('-') || l.starts_with('–'));
    let (dashes, units) = if is_dialogue {
        clean
            .iter()
            .map(|l| {
                let dash: String = l
                    .chars()
                    .take_while(|c| *c == '-' || *c == '–' || *c == ' ')
                    .collect();
                (dash.clone(), l[dash.len()..].trim().to_owned())
            })
            .unzip()
    } else {
        (vec![String::new()], vec![clean.join(" ")])
    };
    Cue {
        prefix,
        suffix,
        dashes,
        units,
        line_count: clean.len().max(1),
    }
}

/// Formatting at the start of a line: "{\an8}", "{\i1}", "<i>", "<font ...>"
fn leading_tags(line: &str, ass: bool) -> String {
    let mut out = String::new();
    let mut rest = line.trim_start();
    loop {
        let close = match rest.chars().next() {
            Some('{') => '}',
            Some('<') if !ass => '>',
            _ => break,
        };
        match rest.find(close) {
            Some(end) => {
                out.push_str(&rest[..=end]);
                rest = &rest[end + 1..];
            }
            None => break,
        }
    }
    out
}

fn trailing_tags(line: &str) -> String {
    let mut rest = line.trim_end();
    let mut tags = Vec::new();
    while rest.ends_with('>') {
        match rest.rfind("</") {
            Some(start) if rest[start..].find('>') == Some(rest.len() - start - 1) => {
                tags.push(rest[start..].to_owned());
                rest = &rest[..start];
            }
            _ => break,
        }
    }
    tags.reverse();
    tags.concat()
}

/// Text without {...} and <...> formatting
pub fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut depth_curly = false;
    let mut depth_angle = false;
    for c in s.chars() {
        match c {
            '{' => depth_curly = true,
            '}' if depth_curly => depth_curly = false,
            '<' => depth_angle = true,
            '>' if depth_angle => depth_angle = false,
            _ if depth_curly || depth_angle => {}
            _ => out.push(c),
        }
    }
    out
}

/// "Show - 05.en.srt" -> "Show - 05.ar.srt", next to the original
pub fn output_path(source: &Path, target_lang: &str) -> PathBuf {
    let ext = source.extension().and_then(|e| e.to_str()).unwrap_or("srt");
    let mut stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("subtitles")
        .to_owned();
    // Drop a language code already there ("name.en", "name.eng", "name.pt-BR")
    if let Some((base, code)) = stem.rsplit_once('.') {
        let looks_like_lang = (2..=3).contains(&code.len())
            && code.chars().all(|c| c.is_ascii_alphabetic())
            || (code.len() == 5 && code.as_bytes()[2] == b'-');
        if looks_like_lang && !base.is_empty() {
            stem = base.to_owned();
        }
    }
    source.with_file_name(format!("{stem}.{target_lang}.{ext}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srt_round_trip_keeps_timings() {
        let srt = "1\r\n00:00:01,000 --> 00:00:02,500\r\n<i>Hello there,\r\nmy friend.</i>\r\n\r\n2\r\n00:00:03,000 --> 00:00:04,000\r\n- Who?\r\n- Me.\r\n";
        let doc = parse(Format::Srt, srt);
        assert_eq!(doc.cues.len(), 2);
        assert_eq!(doc.cues[0].units, vec!["Hello there, my friend."]);
        assert_eq!(doc.cues[1].units, vec!["Who?", "Me."]);
        let same: Vec<Vec<String>> = doc.cues.iter().map(|c| c.units.clone()).collect();
        let out = doc.render(&same, false, false);
        assert!(out.contains(
            "00:00:01,000 --> 00:00:02,500\r\n<i>Hello there, my friend.</i>\r\n\r\n2\r\n"
        ));
        assert!(out.contains("- Who?\r\n- Me."));
        let t = vec![
            vec!["مرحبا يا صديقي العزيز، كيف حالك اليوم؟ أتمنى أن تكون بخير".to_owned()],
            vec!["من؟".to_owned(), "أنا.".to_owned()],
        ];
        let out = doc.render(&t, true, false);
        assert!(out.contains(
            "00:00:03,000 --> 00:00:04,000\r\n\u{202B}- من؟\u{202C}\r\n\u{202B}- أنا.\u{202C}"
        ));
    }

    #[test]
    fn ass_keeps_fields_and_tags() {
        let ass = "[Script Info]\nTitle: x\n\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,{\\an8}Hello,\\Nworld\nDialogue: 0,0:00:03.00,0:00:04.00,Sign,,0,0,0,,{\\p1}m 0 0 l 10 10{\\p0}\n";
        let doc = parse(Format::Ass, ass);
        assert_eq!(doc.cues.len(), 1);
        assert_eq!(doc.cues[0].units, vec!["Hello, world"]);
        let out = doc.render(&[vec!["مرحبا بالعالم".to_owned()]], false, false);
        assert!(out
            .contains("Dialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,{\\an8}مرحبا بالعالم\n"));
        assert!(out.contains("{\\p1}m 0 0 l 10 10{\\p0}"));
    }

    #[test]
    fn output_names() {
        assert_eq!(
            output_path(Path::new(r"C:\a\Show - 05.en.srt"), "ar"),
            PathBuf::from(r"C:\a\Show - 05.ar.srt")
        );
        assert_eq!(
            output_path(Path::new(r"C:\a\Show - 05.ass"), "ar"),
            PathBuf::from(r"C:\a\Show - 05.ar.ass")
        );
    }
}
