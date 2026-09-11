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

use super::tool_display::ToolDisplay;

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
    /// Whether any progress line was printed, so the answer can start on a
    /// fresh line after tools.
    wrote_progress: bool,
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
            wrote_progress: false,
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
            "tool-input-available" => self.on_tool_input(&part)?,
            "tool-output-available" => self.on_tool_output(&part)?,
            "tool-output-error" => self.on_tool_error(&part)?,
            "error" => self.on_error(&part)?,
            "abort" => self.on_abort()?,
            _ => {}
        }
        Ok(RenderOutcome::Continue)
    }

    /// A frame was dropped because it exceeded the display buffer.
    pub fn on_skipped_frame(&mut self) -> Result<RenderOutcome, CliError> {
        self.progress("  skipped a large result (the agent still has it)")?;
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
        if !self.wrote_text && self.wrote_progress {
            writeln!(self.out).map_err(write_failed)?;
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
        let display = self.remember_tool(part);
        if display.is_internal() && !self.verbose {
            return Ok(());
        }
        self.progress(&display.label())
    }

    fn on_tool_input(&mut self, part: &Value) -> Result<(), CliError> {
        let display = self.remember_tool(part);
        if display.is_internal() && !self.verbose {
            return Ok(());
        }
        let Some(detail) = ToolDisplay::input_detail(part.get("input")) else {
            return Ok(());
        };
        self.progress(&format!("  {detail}"))
    }

    fn on_tool_output(&mut self, part: &Value) -> Result<(), CliError> {
        if !self.verbose {
            return Ok(());
        }
        let display = self.tool_display(part);
        self.progress(&format!("{}: done", display.label()))
    }

    fn on_tool_error(&mut self, part: &Value) -> Result<(), CliError> {
        let display = self.tool_display(part);
        let reason = part
            .get("errorText")
            .and_then(Value::as_str)
            .unwrap_or("unknown error");
        // Not fatal: the agent can retry or route around a failed tool, and
        // only an `error` part ends the run.
        self.progress(&format!("{} failed: {reason}", display.label()))
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
        self.progress(&format!("Error: {text}"))
    }

    fn on_abort(&mut self) -> Result<(), CliError> {
        if self.failure.is_none() {
            self.failure = Some("the run was aborted by the server".into());
        }
        self.progress("Error: run aborted")
    }

    fn remember_tool(&mut self, part: &Value) -> ToolDisplay {
        let name = part
            .get("toolName")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| self.tool_display(part).raw().to_string());
        if let Some(id) = part.get("toolCallId").and_then(Value::as_str) {
            self.tool_names.insert(id.to_string(), name.clone());
        }
        ToolDisplay::new(name)
    }

    fn tool_display(&self, part: &Value) -> ToolDisplay {
        let name = part
            .get("toolCallId")
            .and_then(Value::as_str)
            .and_then(|id| self.tool_names.get(id))
            .cloned()
            .unwrap_or_else(|| "tool".to_string());
        ToolDisplay::new(name)
    }

    /// Status line on stderr. Suppressed in JSON mode, where the raw frames
    /// on stdout already carry everything.
    fn progress(&mut self, line: &str) -> Result<(), CliError> {
        if self.mode != RenderMode::Human {
            return Ok(());
        }
        writeln!(self.err, "{line}").map_err(write_failed)?;
        self.wrote_progress = true;
        Ok(())
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
#[path = "render_tests.rs"]
mod tests;
