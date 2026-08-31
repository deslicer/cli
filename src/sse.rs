//! Incremental Server-Sent Events parser.
//!
//! Chunk boundaries on a streaming HTTP body land wherever the network puts
//! them — mid-field, mid-UTF-8-sequence, between the two newlines that end a
//! frame. This parser holds the partial tail across `push` calls so callers
//! never have to think about that.
//!
//! Scope is the subset the AI SDK data-stream protocol uses: `data:` fields
//! and `:` comments. `event:`, `id:`, and `retry:` are parsed and discarded
//! rather than rejected, so a server that adds them does not break the CLI.

use crate::errors::CliError;

/// Ceiling on a single unterminated line.
///
/// A server that never sends a newline would otherwise grow the buffer until
/// the process is killed. 8 MiB is far above any real agent frame.
const MAX_LINE_BYTES: usize = 8 * 1024 * 1024;

/// Ceiling on the `data:` payload accumulated for one frame.
const MAX_EVENT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SseEvent {
    /// A dispatched frame's joined `data:` payload.
    Data(String),
    /// A `:`-prefixed comment. Used for keepalives, ignored by the protocol.
    Comment(String),
}

#[derive(Debug, Default)]
pub struct SseParser {
    /// Bytes seen since the last newline.
    line: Vec<u8>,
    /// `data:` values for the frame being built, newline-joined on dispatch.
    data: String,
    /// Set when the frame carries at least one `data:` field, so that an
    /// explicitly empty `data:` still dispatches an empty-string event.
    saw_data: bool,
}

impl SseParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds raw bytes and returns every frame that completed.
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<SseEvent>, CliError> {
        let mut events = Vec::new();
        for &byte in bytes {
            if byte == b'\n' {
                let raw = std::mem::take(&mut self.line);
                self.consume_line(&raw, &mut events)?;
                continue;
            }
            if self.line.len() >= MAX_LINE_BYTES {
                return Err(CliError::Transport(format!(
                    "stream line exceeded {MAX_LINE_BYTES} bytes without a newline"
                )));
            }
            self.line.push(byte);
        }
        Ok(events)
    }

    /// Flushes a frame left unterminated when the body ended.
    ///
    /// Servers are supposed to close on a frame boundary, but a truncated
    /// final frame is more useful surfaced than silently dropped.
    pub fn finish(&mut self) -> Option<SseEvent> {
        let trailing = std::mem::take(&mut self.line);
        let mut events = Vec::new();
        if !trailing.is_empty() {
            // A trailing partial line cannot exceed the guard, so this cannot
            // fail; the result is discarded either way.
            let _ = self.consume_line(&trailing, &mut events);
        }
        if self.saw_data {
            self.saw_data = false;
            return Some(SseEvent::Data(std::mem::take(&mut self.data)));
        }
        events.pop()
    }

    fn consume_line(&mut self, raw: &[u8], events: &mut Vec<SseEvent>) -> Result<(), CliError> {
        // CRLF terminators leave the CR on the line.
        let raw = raw.strip_suffix(b"\r").unwrap_or(raw);

        if raw.is_empty() {
            if self.saw_data {
                self.saw_data = false;
                events.push(SseEvent::Data(std::mem::take(&mut self.data)));
            }
            return Ok(());
        }

        // Lossy is correct here: a malformed byte should degrade one frame,
        // not abort a run that may already have produced useful output.
        let line = String::from_utf8_lossy(raw);
        let (field, value) = match line.split_once(':') {
            Some((field, value)) => (field, value.strip_prefix(' ').unwrap_or(value)),
            // A field with no colon has an empty value per the spec.
            None => (line.as_ref(), ""),
        };

        match field {
            "" => events.push(SseEvent::Comment(value.to_string())),
            "data" => {
                if self.data.len() + value.len() > MAX_EVENT_BYTES {
                    return Err(CliError::Transport(format!(
                        "stream frame exceeded {MAX_EVENT_BYTES} bytes"
                    )));
                }
                if self.saw_data {
                    self.data.push('\n');
                }
                self.data.push_str(value);
                self.saw_data = true;
            }
            // event / id / retry carry no meaning for the data-stream
            // protocol, and an unknown field must be ignored per the spec.
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push(parser: &mut SseParser, text: &str) -> Vec<SseEvent> {
        parser.push(text.as_bytes()).expect("push")
    }

    #[test]
    fn dispatches_a_whole_frame() {
        let mut parser = SseParser::new();
        let events = push(&mut parser, "data: {\"type\":\"start\"}\n\n");
        assert_eq!(events, vec![SseEvent::Data("{\"type\":\"start\"}".into())]);
    }

    #[test]
    fn reassembles_a_frame_split_across_chunks() {
        let mut parser = SseParser::new();
        assert!(push(&mut parser, "data: {\"ty").is_empty());
        assert!(push(&mut parser, "pe\":\"x\"}").is_empty());
        assert!(push(&mut parser, "\n").is_empty());
        let events = push(&mut parser, "\n");
        assert_eq!(events, vec![SseEvent::Data("{\"type\":\"x\"}".into())]);
    }

    #[test]
    fn surfaces_comments_separately_from_data() {
        let mut parser = SseParser::new();
        let events = push(&mut parser, ": keepalive\n\ndata: hi\n\n");
        assert_eq!(
            events,
            vec![
                SseEvent::Comment("keepalive".into()),
                SseEvent::Data("hi".into()),
            ]
        );
    }

    #[test]
    fn handles_crlf_terminators() {
        let mut parser = SseParser::new();
        let events = push(&mut parser, "data: value\r\n\r\n");
        assert_eq!(events, vec![SseEvent::Data("value".into())]);
    }

    #[test]
    fn joins_multiple_data_fields_with_newlines() {
        let mut parser = SseParser::new();
        let events = push(&mut parser, "data: one\ndata: two\n\n");
        assert_eq!(events, vec![SseEvent::Data("one\ntwo".into())]);
    }

    #[test]
    fn ignores_event_and_id_fields() {
        let mut parser = SseParser::new();
        let events = push(&mut parser, "event: message\nid: 7\ndata: body\n\n");
        assert_eq!(events, vec![SseEvent::Data("body".into())]);
    }

    #[test]
    fn dispatches_an_explicitly_empty_data_field() {
        let mut parser = SseParser::new();
        let events = push(&mut parser, "data:\n\n");
        assert_eq!(events, vec![SseEvent::Data(String::new())]);
    }

    #[test]
    fn blank_lines_without_data_dispatch_nothing() {
        let mut parser = SseParser::new();
        assert!(push(&mut parser, "\n\n\n").is_empty());
    }

    #[test]
    fn finish_flushes_a_frame_missing_its_blank_line() {
        let mut parser = SseParser::new();
        assert!(push(&mut parser, "data: tail").is_empty());
        assert_eq!(parser.finish(), Some(SseEvent::Data("tail".into())));
        assert_eq!(parser.finish(), None);
    }

    #[test]
    fn rejects_a_line_that_never_terminates() {
        let mut parser = SseParser::new();
        let chunk = vec![b'x'; 1024 * 1024];
        // One chunk past the ceiling: the guard fires on the byte after the
        // limit, so exactly MAX_LINE_BYTES is still accepted.
        for _ in 0..=(MAX_LINE_BYTES / chunk.len()) {
            if parser.push(&chunk).is_err() {
                return;
            }
        }
        panic!("expected the max-line guard to fire");
    }
}
