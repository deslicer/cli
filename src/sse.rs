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
//!
//! A single tool result can exceed the display buffer. Those frames are
//! skipped so the rest of the run can still render; the agent already has
//! the payload server-side.

/// Ceiling on a single unterminated line.
///
/// A server that never sends a newline would otherwise grow the buffer until
/// the process is killed. Tool-output frames can exceed this; they are
/// discarded rather than treated as a fatal transport error.
const MAX_LINE_BYTES: usize = 8 * 1024 * 1024;

/// Ceiling on the `data:` payload accumulated for one frame.
const MAX_EVENT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SseEvent {
    /// A dispatched frame's joined `data:` payload.
    Data(String),
    /// A `:`-prefixed comment. Used for keepalives, ignored by the protocol.
    Comment(String),
    /// A frame was dropped because a line or payload exceeded the display buffer.
    Skipped,
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
    /// Discard bytes until the next newline; the current line overflowed.
    skipping_line: bool,
    /// Discard remaining fields until the next blank line.
    skipping_frame: bool,
}

impl SseParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds raw bytes and returns every frame that completed.
    pub fn push(&mut self, bytes: &[u8]) -> Vec<SseEvent> {
        let mut events = Vec::new();
        for &byte in bytes {
            self.push_byte(byte, &mut events);
        }
        events
    }

    /// Flushes a frame left unterminated when the body ended.
    ///
    /// Servers are supposed to close on a frame boundary, but a truncated
    /// final frame is more useful surfaced than silently dropped.
    pub fn finish(&mut self) -> Option<SseEvent> {
        if self.skipping_line || self.skipping_frame {
            self.reset_skip_state();
            return Some(SseEvent::Skipped);
        }

        let trailing = std::mem::take(&mut self.line);
        let mut events = Vec::new();
        if !trailing.is_empty() {
            self.consume_line(&trailing, &mut events);
        }
        if self.saw_data {
            self.saw_data = false;
            return Some(SseEvent::Data(std::mem::take(&mut self.data)));
        }
        events.pop()
    }

    fn push_byte(&mut self, byte: u8, events: &mut Vec<SseEvent>) {
        if self.skipping_line {
            if byte == b'\n' {
                self.skipping_line = false;
            }
            return;
        }
        if byte == b'\n' {
            let raw = std::mem::take(&mut self.line);
            self.consume_line(&raw, events);
            return;
        }
        if self.line.len() >= MAX_LINE_BYTES {
            self.begin_line_skip();
            return;
        }
        self.line.push(byte);
    }

    fn begin_line_skip(&mut self) {
        self.line.clear();
        self.skipping_line = true;
        self.mark_frame_oversized();
    }

    fn mark_frame_oversized(&mut self) {
        self.skipping_frame = true;
        self.saw_data = false;
        self.data.clear();
    }

    fn reset_skip_state(&mut self) {
        self.skipping_line = false;
        self.skipping_frame = false;
        self.saw_data = false;
        self.line.clear();
        self.data.clear();
    }

    fn consume_line(&mut self, raw: &[u8], events: &mut Vec<SseEvent>) {
        // CRLF terminators leave the CR on the line.
        let raw = raw.strip_suffix(b"\r").unwrap_or(raw);

        if raw.is_empty() {
            self.finish_frame(events);
            return;
        }
        if self.skipping_frame {
            return;
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
            "data" => self.append_data(value),
            // event / id / retry carry no meaning for the data-stream
            // protocol, and an unknown field must be ignored per the spec.
            _ => {}
        }
    }

    fn append_data(&mut self, value: &str) {
        if self.data.len() + value.len() > MAX_EVENT_BYTES {
            self.mark_frame_oversized();
            return;
        }
        if self.saw_data {
            self.data.push('\n');
        }
        self.data.push_str(value);
        self.saw_data = true;
    }

    fn finish_frame(&mut self, events: &mut Vec<SseEvent>) {
        if self.skipping_frame {
            self.reset_skip_state();
            events.push(SseEvent::Skipped);
            return;
        }
        if self.saw_data {
            self.saw_data = false;
            events.push(SseEvent::Data(std::mem::take(&mut self.data)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push(parser: &mut SseParser, text: &str) -> Vec<SseEvent> {
        parser.push(text.as_bytes())
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
    fn oversized_line_is_skipped_and_later_frames_still_dispatch() {
        let mut parser = SseParser::new();
        let chunk = vec![b'x'; 1024 * 1024];
        for _ in 0..=(MAX_LINE_BYTES / chunk.len()) {
            assert!(parser.push(&chunk).is_empty());
        }
        let events = parser.push(b"\n\ndata: {\"type\":\"text-delta\",\"delta\":\"ok\"}\n\n");
        assert_eq!(
            events,
            vec![
                SseEvent::Skipped,
                SseEvent::Data("{\"type\":\"text-delta\",\"delta\":\"ok\"}".into()),
            ]
        );
    }

    #[test]
    fn unterminated_oversized_line_finishes_as_skipped() {
        let mut parser = SseParser::new();
        let chunk = vec![b'x'; 1024 * 1024];
        for _ in 0..=(MAX_LINE_BYTES / chunk.len()) {
            assert!(parser.push(&chunk).is_empty());
        }
        assert_eq!(parser.finish(), Some(SseEvent::Skipped));
        assert_eq!(parser.finish(), None);
    }

    #[test]
    fn oversized_data_payload_is_skipped() {
        let mut parser = SseParser::new();
        let line = format!("data: {}\n", "y".repeat(1024 * 1024));
        for _ in 0..=(MAX_EVENT_BYTES / (1024 * 1024)) {
            assert!(parser.push(line.as_bytes()).is_empty());
        }
        assert_eq!(parser.push(b"\n"), vec![SseEvent::Skipped]);
        assert_eq!(
            push(&mut parser, "data: later\n\n"),
            vec![SseEvent::Data("later".into())]
        );
    }
}
