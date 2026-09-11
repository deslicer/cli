//! Drains an agent run's SSE body into a renderer.
//!
//! Cancellation is a parameter rather than a hardcoded `ctrl_c()` so tests can
//! drive the loop without registering a process-wide SIGINT handler.

use std::future::Future;
use std::io::Write;

use crate::errors::CliError;
use crate::sse::{SseEvent, SseParser};

use super::render::{RenderOutcome, Renderer};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamEnd {
    /// Server sent its end-of-stream marker.
    Completed,
    /// Caller interrupted; the run keeps going server-side.
    Cancelled,
    /// Connection closed without an end-of-stream marker.
    Truncated,
}

/// Reads the response body to completion, rendering as it goes.
///
/// Dropping `response` on cancellation is what tells the server the client
/// hung up — there is no cancel endpoint, so the closed connection is the
/// signal.
pub async fn consume_stream<O, E, C>(
    mut response: reqwest::Response,
    renderer: &mut Renderer<O, E>,
    cancel: C,
) -> Result<StreamEnd, CliError>
where
    O: Write,
    E: Write,
    C: Future<Output = ()>,
{
    let mut parser = SseParser::new();
    tokio::pin!(cancel);

    loop {
        let chunk = tokio::select! {
            // Biased so an interrupt wins a tie. Unbiased, a body that is
            // already buffered would keep printing after Ctrl-C, at random.
            biased;
            () = &mut cancel => return Ok(StreamEnd::Cancelled),
            chunk = response.chunk() => chunk,
        };

        let chunk = chunk.map_err(read_failed)?;
        let Some(bytes) = chunk else { break };

        for event in parser.push(&bytes) {
            if dispatch(renderer, event)? == RenderOutcome::Done {
                return Ok(StreamEnd::Completed);
            }
        }
    }

    // A body that ends without a blank line still leaves one whole frame in
    // the parser; a server flushing its last event as the connection closes
    // is enough to produce that.
    if let Some(event) = parser.finish() {
        if dispatch(renderer, event)? == RenderOutcome::Done {
            return Ok(StreamEnd::Completed);
        }
    }

    Ok(StreamEnd::Truncated)
}

fn dispatch<O: Write, E: Write>(
    renderer: &mut Renderer<O, E>,
    event: SseEvent,
) -> Result<RenderOutcome, CliError> {
    match event {
        SseEvent::Data(frame) => renderer.handle_frame(&frame),
        // Keepalives. Their only job is to keep proxies from buffering and
        // to prove the connection is alive, both of which already happened
        // by the time we see one.
        SseEvent::Comment(_) => Ok(RenderOutcome::Continue),
        SseEvent::Skipped => renderer.on_skipped_frame(),
    }
}

fn read_failed(err: reqwest::Error) -> CliError {
    if err.is_timeout() {
        return CliError::Transport(
            "the agent stream went silent (no data for 90s); the run may still be \
             running server-side"
                .into(),
        );
    }
    CliError::Transport(format!("read agent stream: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::agent::render::RenderMode;
    use std::future::pending;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    struct Run {
        end: StreamEnd,
        out: String,
        err: String,
        failure: Option<String>,
    }

    async fn run_against(body: &str) -> Run {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/stream"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(body),
            )
            .mount(&server)
            .await;

        let response = reqwest::get(format!("{}/stream", server.uri()))
            .await
            .expect("request");

        let mut out: Vec<u8> = Vec::new();
        let mut err: Vec<u8> = Vec::new();
        let (end, failure) = {
            let mut renderer = Renderer::new(&mut out, &mut err, RenderMode::Human, false);
            let end = consume_stream(response, &mut renderer, pending())
                .await
                .expect("consume stream");
            renderer.finish().expect("finish");
            (end, renderer.failure().map(str::to_string))
        };

        Run {
            end,
            out: String::from_utf8(out).expect("utf8 stdout"),
            err: String::from_utf8(err).expect("utf8 stderr"),
            failure,
        }
    }

    #[tokio::test]
    async fn renders_a_complete_run() {
        let body = concat!(
            ": run 123\n\n",
            "data: {\"type\":\"start\"}\n\n",
            "data: {\"type\":\"text-delta\",\"id\":\"t1\",\"delta\":\"All \"}\n\n",
            "data: {\"type\":\"text-delta\",\"id\":\"t1\",\"delta\":\"good\"}\n\n",
            "data: {\"type\":\"finish\"}\n\n",
            "data: [DONE]\n\n",
        );
        let run = run_against(body).await;
        assert_eq!(run.end, StreamEnd::Completed);
        assert_eq!(run.out, "All good\n");
        assert!(run.failure.is_none());
    }

    #[tokio::test]
    async fn keepalive_comments_are_not_rendered() {
        let body = concat!(
            ": run 123\n\n",
            ": ping\n\n",
            "data: {\"type\":\"text-delta\",\"delta\":\"hi\"}\n\n",
            ": ping\n\n",
            "data: [DONE]\n\n",
        );
        let run = run_against(body).await;
        assert_eq!(run.out, "hi\n");
        assert!(run.err.is_empty());
    }

    #[tokio::test]
    async fn error_part_is_surfaced_as_a_failure() {
        let body = concat!(
            "data: {\"type\":\"error\",\"errorText\":\"model unavailable\"}\n\n",
            "data: [DONE]\n\n",
        );
        let run = run_against(body).await;
        assert_eq!(run.end, StreamEnd::Completed);
        assert_eq!(run.failure.as_deref(), Some("model unavailable"));
    }

    #[tokio::test]
    async fn stream_without_done_is_truncated() {
        let body = "data: {\"type\":\"text-delta\",\"delta\":\"partial\"}\n\n";
        let run = run_against(body).await;
        assert_eq!(run.end, StreamEnd::Truncated);
        assert_eq!(run.out, "partial\n");
    }

    #[tokio::test]
    async fn final_frame_without_a_trailing_blank_line_is_still_dispatched() {
        let body = "data: {\"type\":\"text-delta\",\"delta\":\"tail\"}";
        let run = run_against(body).await;
        assert_eq!(run.out, "tail\n");
    }

    #[tokio::test]
    async fn malformed_frame_does_not_abort_the_run() {
        let body = concat!(
            "data: {not json\n\n",
            "data: {\"type\":\"text-delta\",\"delta\":\"recovered\"}\n\n",
            "data: [DONE]\n\n",
        );
        let run = run_against(body).await;
        assert_eq!(run.end, StreamEnd::Completed);
        assert_eq!(run.out, "recovered\n");
    }

    #[tokio::test]
    async fn cancellation_stops_before_the_body_is_read() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/stream"))
            .respond_with(ResponseTemplate::new(200).set_body_string("data: [DONE]\n\n"))
            .mount(&server)
            .await;
        let response = reqwest::get(format!("{}/stream", server.uri()))
            .await
            .expect("request");

        let mut out: Vec<u8> = Vec::new();
        let mut err: Vec<u8> = Vec::new();
        let mut renderer = Renderer::new(&mut out, &mut err, RenderMode::Human, false);
        let end = consume_stream(response, &mut renderer, std::future::ready(()))
            .await
            .expect("consume stream");

        assert_eq!(end, StreamEnd::Cancelled);
    }
}
