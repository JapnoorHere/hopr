//! asciicast v2 recordings (§9.18): a JSON header line, then one `[time, code, data]`
//! line per event. Only output (`"o"`) and resizes (`"r"`) are written, never input, so
//! typed passwords can't end up in a recording.

use std::io::{self, Write};
use std::time::Instant;

use serde::{Deserialize, Serialize};

/// First line of a recording.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CastHeader {
    /// Always 2.
    pub version: u32,
    /// Columns.
    pub width: u32,
    /// Rows.
    pub height: u32,
    /// Start, Unix seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<i64>,
    /// What was recorded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

/// Streams a session's output into an asciicast v2 file.
pub struct CastWriter<W: Write> {
    out: W,
    start: Instant,
    /// Trailing bytes of a UTF-8 sequence split across chunks.
    carry: Vec<u8>,
}

impl<W: Write> CastWriter<W> {
    /// Start a recording: writes the header.
    pub fn new(mut out: W, header: &CastHeader) -> io::Result<Self> {
        serde_json::to_writer(&mut out, header)?;
        out.write_all(b"\n")?;
        Ok(Self {
            out,
            start: Instant::now(),
            carry: Vec::new(),
        })
    }

    fn elapsed(&self) -> f64 {
        self.start.elapsed().as_secs_f64()
    }

    /// Output bytes, timestamped now.
    pub fn output(&mut self, bytes: &[u8]) -> io::Result<()> {
        let t = self.elapsed();
        self.output_at(t, bytes)
    }

    /// Output bytes at `t` seconds. A UTF-8 character split across chunks is held back
    /// until the rest arrives; invalid bytes become U+FFFD.
    pub fn output_at(&mut self, t: f64, bytes: &[u8]) -> io::Result<()> {
        let mut buf = std::mem::take(&mut self.carry);
        buf.extend_from_slice(bytes);
        let mut text = String::with_capacity(buf.len());
        let mut rest = buf.as_slice();
        loop {
            match std::str::from_utf8(rest) {
                Ok(s) => {
                    text.push_str(s);
                    break;
                }
                Err(e) => {
                    let (good, bad) = rest.split_at(e.valid_up_to());
                    text.push_str(std::str::from_utf8(good).unwrap_or_default());
                    match e.error_len() {
                        Some(n) => {
                            text.push(char::REPLACEMENT_CHARACTER);
                            rest = &bad[n..];
                        }
                        None => {
                            self.carry = bad.to_vec(); // incomplete at the end
                            break;
                        }
                    }
                }
            }
        }
        if text.is_empty() {
            return Ok(());
        }
        self.event(t, "o", &text)
    }

    /// The terminal was resized.
    pub fn resize(&mut self, cols: u32, rows: u32) -> io::Result<()> {
        let t = self.elapsed();
        self.event(t, "r", &format!("{cols}x{rows}"))
    }

    fn event(&mut self, t: f64, code: &str, data: &str) -> io::Result<()> {
        // Microsecond precision, like asciinema.
        let t = (t * 1_000_000.0).round() / 1_000_000.0;
        serde_json::to_writer(&mut self.out, &(t, code, data))?;
        self.out.write_all(b"\n")
    }

    /// Flush buffered events to the file.
    pub fn flush(&mut self) -> io::Result<()> {
        self.out.flush()
    }
}

/// Header and length of a recording, from its first and last lines.
pub fn summary(first_line: &str, last_line: &str) -> Option<(CastHeader, f64)> {
    let header: CastHeader = serde_json::from_str(first_line.trim()).ok()?;
    if header.version != 2 {
        return None;
    }
    let duration = serde_json::from_str::<(f64, String, String)>(last_line.trim())
        .map(|(t, _, _)| t)
        .unwrap_or(0.0);
    Some((header, duration))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header() -> CastHeader {
        CastHeader {
            version: 2,
            width: 80,
            height: 24,
            timestamp: Some(1_760_000_000),
            title: Some("prod".into()),
        }
    }

    #[test]
    fn writes_header_and_events() {
        let mut w = CastWriter::new(Vec::new(), &header()).unwrap();
        w.output_at(0.5, b"hello \x1b[1mworld\x1b[0m\r\n").unwrap();
        w.output_at(1.25, b"\"quoted\"").unwrap();
        w.resize(100, 30).unwrap();
        let text = String::from_utf8(w.out).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 4);
        assert_eq!(
            lines[0],
            r#"{"version":2,"width":80,"height":24,"timestamp":1760000000,"title":"prod"}"#
        );
        assert_eq!(lines[1], r#"[0.5,"o","hello \u001b[1mworld\u001b[0m\r\n"]"#);
        assert_eq!(lines[2], r#"[1.25,"o","\"quoted\""]"#);
        assert!(lines[3].ends_with(r#""r","100x30"]"#));
        let (h, d) = summary(lines[0], lines[2]).unwrap();
        assert_eq!(h, header());
        assert!((d - 1.25).abs() < 1e-9);
    }

    #[test]
    fn keeps_utf8_split_across_chunks() {
        let euro = "€".as_bytes(); // 3 bytes
        let mut w = CastWriter::new(Vec::new(), &header()).unwrap();
        w.output_at(0.1, &[b'a', euro[0]]).unwrap();
        w.output_at(0.2, &euro[1..]).unwrap();
        w.output_at(0.3, &[b'b', 0xff, b'c']).unwrap();
        let text = String::from_utf8(w.out).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[1], r#"[0.1,"o","a"]"#);
        assert_eq!(lines[2], r#"[0.2,"o","€"]"#);
        assert_eq!(lines[3], "[0.3,\"o\",\"b\u{fffd}c\"]");
    }

    #[test]
    fn summary_rejects_other_versions() {
        assert!(summary(r#"{"version":1,"width":1,"height":1}"#, "").is_none());
        let (_, d) = summary(r#"{"version":2,"width":1,"height":1}"#, "").unwrap();
        assert!(d.abs() < f64::EPSILON);
    }
}
