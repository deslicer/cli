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
fn tool_start_uses_a_human_label() {
    let result = render(
        &[
            r#"{"type":"tool-input-start","toolCallId":"c1","toolName":"search_hosts"}"#,
            r#"{"type":"tool-output-available","toolCallId":"c1","output":{}}"#,
        ],
        RenderMode::Human,
        false,
    );
    assert!(result.out.is_empty());
    assert_eq!(result.err, "Search hosts\n");
}

#[test]
fn tool_success_is_silent_unless_verbose() {
    let quiet = render(
        &[
            r#"{"type":"tool-input-start","toolCallId":"c1","toolName":"search_tool"}"#,
            r#"{"type":"tool-output-available","toolCallId":"c1","output":{}}"#,
        ],
        RenderMode::Human,
        false,
    );
    assert_eq!(quiet.err, "Searching tools\n");

    let loud = render(
        &[
            r#"{"type":"tool-input-start","toolCallId":"c1","toolName":"search_tool"}"#,
            r#"{"type":"tool-output-available","toolCallId":"c1","output":{}}"#,
        ],
        RenderMode::Human,
        true,
    );
    assert!(loud.err.contains("Searching tools\n"));
    assert!(loud.err.contains("Searching tools: done\n"));
}

#[test]
fn orchestrator_bookkeeping_is_hidden_unless_verbose() {
    let quiet = render(
        &[
            r#"{"type":"tool-input-start","toolCallId":"c1","toolName":"declare_intent"}"#,
            r#"{"type":"tool-output-available","toolCallId":"c1","output":{}}"#,
            r#"{"type":"tool-input-start","toolCallId":"c2","toolName":"createTaskList"}"#,
            r#"{"type":"tool-output-available","toolCallId":"c2","output":{}}"#,
            r#"{"type":"tool-input-start","toolCallId":"c3","toolName":"updateTaskProgress"}"#,
            r#"{"type":"tool-output-available","toolCallId":"c3","output":{}}"#,
        ],
        RenderMode::Human,
        false,
    );
    assert!(quiet.err.is_empty());

    let loud = render(
        &[r#"{"type":"tool-input-start","toolCallId":"c1","toolName":"declare_intent"}"#],
        RenderMode::Human,
        true,
    );
    assert!(loud.err.contains("Setting intent\n"));
}

#[test]
fn tool_input_detail_is_indented_under_the_label() {
    let result = render(
        &[
            r#"{"type":"tool-input-start","toolCallId":"c1","toolName":"run_tool"}"#,
            r#"{"type":"tool-input-available","toolCallId":"c1","toolName":"run_tool","input":{"tool":"list_apps"}}"#,
        ],
        RenderMode::Human,
        false,
    );
    assert_eq!(result.err, "Running a tool\n  list_apps\n");
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
    assert!(result.err.contains("List plans failed: upstream 503"));
    assert!(result.failure.is_none());
}

#[test]
fn skipped_frame_explains_the_drop() {
    let mut out: Vec<u8> = Vec::new();
    let mut err: Vec<u8> = Vec::new();
    {
        let mut renderer = Renderer::new(&mut out, &mut err, RenderMode::Human, false);
        renderer
            .handle_frame(r#"{"type":"tool-input-start","toolCallId":"c1","toolName":"run_tool"}"#)
            .expect("start");
        renderer.on_skipped_frame().expect("skip");
        renderer.finish().expect("finish");
    }
    let err = String::from_utf8(err).expect("utf8");
    assert!(err.contains("Running a tool\n"));
    assert!(err.contains("skipped a large result (the agent still has it)"));
}

#[test]
fn progress_puts_a_blank_line_before_the_answer() {
    let result = render(
        &[
            r#"{"type":"tool-input-start","toolCallId":"c1","toolName":"search_tool"}"#,
            r#"{"type":"text-delta","delta":"Here they are"}"#,
        ],
        RenderMode::Human,
        false,
    );
    assert_eq!(result.out, "\nHere they are\n");
}

#[test]
fn error_part_records_a_failure() {
    let result = render(
        &[r#"{"type":"error","errorText":"model unavailable"}"#],
        RenderMode::Human,
        false,
    );
    assert_eq!(result.failure.as_deref(), Some("model unavailable"));
    assert!(result.err.contains("Error: model unavailable"));
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
