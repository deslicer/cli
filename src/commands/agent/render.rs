//! Renders AI SDK UI message stream parts to a terminal.
//!
//! Two streams, on purpose: the agent's answer goes to stdout so
//! `deslicer agent run ... > answer.md` captures exactly the answer, and
//! progress (tool calls, reasoning) goes to stderr so it stays visible when
//! stdout is redirected.
//!
//! Unknown part types are ignored rather than rejected. The protocol gains
//! part types between AI SDK releases, and a CLI that hard-fails on one it
//! has not seen would break on a server deploy.

use std::collections::HashMap;
use std::io::Write;

use serde_json::Value;

use crate::errors::CliError;

/// Frame that marks the end of the stream.
pub const DONE_FRAME: &str = "[DONE]";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderOutcome {
    /// Keep reading.
    Continue,
    /// Server signalled end of stream.
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderMode {
    /// Prose on stdout, progress on stderr.
    Human,
    /// One raw protocol frame per line on stdout.
    Json,
}

pub struct Renderer<O: Write, E: Write> {
    out: O,
    err: E,
    mode: RenderMode,
    verbose: bool,
    /// Tool call id to tool name, so an output part can name its tool. Only
    /// the id is repeated on later parts.
    tool_names: HashMap<String, String>,
    /// Whether any assistant text has been written, so a trailing newline is
    /// only added when there is something to terminate.
    wrote_text: bool,
    /// First fatal error part seen. Later ones are noise from teardown.
    failure: Option<String>,
}

impl<O: Write, E: Write> Renderer<O, E> {
    pub fn new(out: O, err: E, mode: RenderMode, verbose: bool) -> Self {
        Self {
            out,
            err,
            mode,
            verbose,
            tool_names: HashMap::new(),
            wrote_text: false,
            failure: None,
        }
    }

    /// The run's failure, if the stream carried an `error` or `abort` part.
    pub fn failure(&self) -> Option<&str> {
        self.failure.as_deref()
    }

    /// Handles one `data:` payload.
    pub fn handle_frame(&mut self, frame: &str) -> Result<RenderOutcome, CliError> {
        let frame = frame.trim();
        if frame.is_empty() {
            return Ok(RenderOutcome::Continue);
        }
        if frame == DONE_FRAME {
            return Ok(RenderOutcome::Done);
        }

        if self.mode == RenderMode::Json {
            writeln!(self.out, "{frame}").map_err(write_failed)?;
            self.out.flush().map_err(write_failed)?;
            // Still parsed below so `error` parts set the exit code.
        }

        let part: Value = match serde_json::from_str(frame) {
            Ok(part) => part,
            Err(err) => {
                // One unreadable frame is not worth discarding a run whose
                // earlier output may already be on screen.
                self.trace(&format!("skipped unparsable stream frame: {err}"))?;
                return Ok(RenderOutcome::Continue);
            }
        };
        let Some(kind) = part.get("type").and_then(Value::as_str) else {
            return Ok(RenderOutcome::Continue);
        };

        match kind {
            "text-delta" => self.on_text_delta(&part)?,
            "reasoning-delta" => self.on_reasoning_delta(&part)?,
            "tool-input-start" => self.on_tool_start(&part)?,
            "tool-output-available" => self.on_tool_output(&part)?,
            "tool-output-error" => self.on_tool_error(&part)?,
            "error" => self.on_error(&part)?,
            "abort" => self.on_abort()?,
            _ => {}
        }
        Ok(RenderOutcome::Continue)
    }

    /// Terminates the answer line once the stream is over.
    pub fn finish(&mut self) -> Result<(), CliError> {
        if self.mode == RenderMode::Human && self.wrote_text {
            writeln!(self.out).map_err(write_failed)?;
        }
        self.out.flush().map_err(write_failed)?;
        self.err.flush().map_err(write_failed)
    }

    fn on_text_delta(&mut self, part: &Value) -> Result<(), CliError> {
        if self.mode != RenderMode::Human {
            return Ok(());
        }
        let Some(delta) = part.get("delta").and_then(Value::as_str) else {
            return Ok(());
        };
        if delta.is_empty() {
            return Ok(());
        }
        write!(self.out, "{delta}").map_err(write_failed)?;
        // Unflushed deltas defeat the point of streaming: stdout is
        // block-buffered when redirected and line-buffered otherwise, and a
        // token rarely ends a line.
        self.out.flush().map_err(write_failed)?;
        self.wrote_text = true;
        Ok(())
    }

    fn on_reasoning_delta(&mut self, part: &Value) -> Result<(), CliError> {
        let Some(delta) = part.get("delta").and_then(Value::as_str) else {
            return Ok(());
        };
        self.trace(delta)
    }

    fn on_tool_start(&mut self, part: &Value) -> Result<(), CliError> {
        let name = part
            .get("toolName")
            .and_then(Value::as_str)
            .unwrap_or("tool");
        if let Some(id) = part.get("toolCallId").and_then(Value::as_str) {
            self.tool_names.insert(id.to_string(), name.to_string());
        }
        self.progress(&format!("[tool] {name}"))
    }

    fn on_tool_output(&mut self, part: &Value) -> Result<(), CliError> {
        let name = self.tool_name(part);
        self.progress(&format!("[tool] {name} ok"))
    }

    fn on_tool_error(&mut self, part: &Value) -> Result<(), CliError> {
        let name = self.tool_name(part);
        let reason = part
            .get("errorText")
            .and_then(Value::as_str)
            .unwrap_or("unknown error");
        // Not fatal: the agent can retry or route around a failed tool, and
        // only an `error` part ends the run.
        self.progress(&format!("[tool] {name} failed: {reason}"))
    }

    fn on_error(&mut self, part: &Value) -> Result<(), CliError> {
        let text = part
            .get("errorText")
            .and_then(Value::as_str)
            .unwrap_or("the agent reported an error")
            .to_string();
        if self.failure.is_none() {
            self.failure = Some(text.clone());
        }
        self.progress(&format!("[error] {text}"))
    }

    fn on_abort(&mut self) -> Result<(), CliError> {
        if self.failure.is_none() {
            self.failure = Some("the run was aborted by the server".into());
        }
        self.progress("[error] run aborted")
    }

    fn tool_name(&self, part: &Value) -> String {
        part.get("toolCallId")
            .and_then(Value::as_str)
            .and_then(|id| self.tool_names.get(id))
            .cloned()
            .unwrap_or_else(|| "tool".to_string())
    }

    /// Status line on stderr. Suppressed in JSON mode, where the raw frames
    /// on stdout already carry everything.
    fn progress(&mut self, line: &str) -> Result<(), CliError> {
        if self.mode != RenderMode::Human {
            return Ok(());
        }
        writeln!(self.err, "{line}").map_err(write_failed)
    }

    /// Diagnostic on stderr, only under `--verbose`.
    fn trace(&mut self, line: &str) -> Result<(), CliError> {
        if !self.verbose {
            return Ok(());
        }
        write!(self.err, "{line}").map_err(write_failed)?;
        self.err.flush().map_err(write_failed)
    }
}

fn write_failed(err: std::io::Error) -> CliError {
    CliError::Other(format!("write output: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Captured {
        out: String,
        err: String,
        failure: Option<String>,
        outcome: RenderOutcome,
    }

    fn render(frames: &[&str], mode: RenderMode, verbose: bool) -> Captured {
        let mut out: Vec<u8> = Vec::new();
        let mut err: Vec<u8> = Vec::new();
        let mut outcome = RenderOutcome::Continue;
        let mut failure = None;
        {
            let mut renderer = Renderer::new(&mut out, &mut err, mode, verbose);
            for frame in frames {
                outcome = renderer.handle_frame(frame).expect("render frame");
                if outcome == RenderOutcome::Done {
                    break;
                }
            }
            renderer.finish().expect("finish");
            failure = failure.or_else(|| renderer.failure().map(str::to_string));
        }
        Captured {
            out: String::from_utf8(out).expect("utf8 stdout"),
            err: String::from_utf8(err).expect("utf8 stderr"),
            failure,
            outcome,
        }
    }

    #[test]
    fn text_deltas_concatenate_on_stdout() {
        let result = render(
            &[
                r#"{"type":"start"}"#,
                r#"{"type":"text-start","id":"t1"}"#,
                r#"{"type":"text-delta","id":"t1","delta":"Hello, "}"#,
                r#"{"type":"text-delta","id":"t1","delta":"world"}"#,
                r#"{"type":"text-end","id":"t1"}"#,
                r#"{"type":"finish"}"#,
            ],
            RenderMode::Human,
            false,
        );
        assert_eq!(result.out, "Hello, world\n");
        assert!(result.err.is_empty());
        assert!(result.failure.is_none());
    }

    #[test]
    fn done_frame_stops_the_stream() {
        let result = render(&[DONE_FRAME], RenderMode::Human, false);
        assert_eq!(result.outcome, RenderOutcome::Done);
    }

    #[test]
    fn tool_lifecycle_is_reported_on_stderr_by_name() {
        let result = render(
            &[
                r#"{"type":"tool-input-start","toolCallId":"c1","toolName":"search_hosts"}"#,
                r#"{"type":"tool-output-available","toolCallId":"c1","output":{}}"#,
            ],
            RenderMode::Human,
            false,
        );
        assert!(result.out.is_empty());
        assert!(result.err.contains("[tool] search_hosts\n"));
        assert!(result.err.contains("[tool] search_hosts ok\n"));
    }

    #[test]
    fn tool_failure_is_reported_but_does_not_fail_the_run() {
        let result = render(
            &[
                r#"{"type":"tool-input-start","toolCallId":"c1","toolName":"list_plans"}"#,
                r#"{"type":"tool-output-error","toolCallId":"c1","errorText":"upstream 503"}"#,
            ],
            RenderMode::Human,
            false,
        );
        assert!(result
            .err
            .contains("[tool] list_plans failed: upstream 503"));
        assert!(result.failure.is_none());
    }

    #[test]
    fn error_part_records_a_failure() {
        let result = render(
            &[r#"{"type":"error","errorText":"model unavailable"}"#],
            RenderMode::Human,
            false,
        );
        assert_eq!(result.failure.as_deref(), Some("model unavailable"));
        assert!(result.err.contains("[error] model unavailable"));
    }

    #[test]
    fn abort_part_records_a_failure() {
        let result = render(&[r#"{"type":"abort"}"#], RenderMode::Human, false);
        assert!(result.failure.is_some());
    }

    #[test]
    fn first_error_wins_over_later_ones() {
        let result = render(
            &[
                r#"{"type":"error","errorText":"first"}"#,
                r#"{"type":"error","errorText":"second"}"#,
            ],
            RenderMode::Human,
            false,
        );
        assert_eq!(result.failure.as_deref(), Some("first"));
    }

    #[test]
    fn unparsable_frame_is_skipped_without_failing() {
        let result = render(
            &["{not json", r#"{"type":"text-delta","delta":"still here"}"#],
            RenderMode::Human,
            false,
        );
        assert_eq!(result.out, "still here\n");
        assert!(result.failure.is_none());
    }

    #[test]
    fn unknown_part_types_are_ignored() {
        let result = render(
            &[r#"{"type":"data-some-future-thing","data":{"a":1}}"#],
            RenderMode::Human,
            false,
        );
        assert!(result.out.is_empty());
        assert!(result.err.is_empty());
    }

    #[test]
    fn reasoning_is_hidden_unless_verbose() {
        let quiet = render(
            &[r#"{"type":"reasoning-delta","delta":"thinking"}"#],
            RenderMode::Human,
            false,
        );
        assert!(quiet.err.is_empty());

        let loud = render(
            &[r#"{"type":"reasoning-delta","delta":"thinking"}"#],
            RenderMode::Human,
            true,
        );
        assert!(loud.err.contains("thinking"));
    }

    #[test]
    fn json_mode_echoes_raw_frames_and_keeps_stdout_clean_of_prose() {
        let result = render(
            &[r#"{"type":"text-delta","delta":"hi"}"#],
            RenderMode::Json,
            false,
        );
        assert_eq!(result.out, "{\"type\":\"text-delta\",\"delta\":\"hi\"}\n");
        assert!(result.err.is_empty());
    }

    #[test]
    fn json_mode_still_records_errors() {
        let result = render(
            &[r#"{"type":"error","errorText":"boom"}"#],
            RenderMode::Json,
            false,
        );
        assert_eq!(result.failure.as_deref(), Some("boom"));
    }

    #[test]
    fn no_trailing_newline_when_nothing_was_written() {
        let result = render(&[r#"{"type":"finish"}"#], RenderMode::Human, false);
        assert!(result.out.is_empty());
    }
}
