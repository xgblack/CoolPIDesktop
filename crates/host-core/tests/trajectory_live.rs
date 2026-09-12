use host_core::trajectory_live::update;
use serde_json::json;
#[test]
fn live_projection_survives_deltas_and_never_fabricates_running_duration() {
    let mut records = vec![];
    update(
        &mut records,
        "r",
        1,
        "user_message",
        &json!({"text":"hello"}),
        100.,
    );
    update(
        &mut records,
        "r",
        2,
        "message_start",
        &json!({"message":{"role":"user","timestamp":101,"content":"hello"}}),
        101.,
    );
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].aliases, vec!["message:user:101"]);
    update(
        &mut records,
        "r",
        3,
        "message_start",
        &json!({"message":{"role":"assistant","timestamp":102,"content":[]}}),
        102.,
    );
    let changed = update(
        &mut records,
        "r",
        4,
        "message_update",
        &json!({"message":{"role":"assistant","timestamp":102,"duration":55,"content":[{"type":"text","text":"partial"}]}}),
        103.,
    );
    assert_eq!(records.len(), 2);
    assert_eq!(changed.len(), 1);
    assert_eq!(records[1].duration_ms, None);
    assert_eq!(records[1].completed_at, None);
    update(
        &mut records,
        "r",
        5,
        "message_end",
        &json!({"message":{"role":"assistant","timestamp":102,"duration":50,"ttft":5,"stopReason":"stop","content":"done"}}),
        152.,
    );
    assert_eq!(records[1].duration_ms, Some(50.));
    assert_eq!(records[1].status, "succeeded");
    update(
        &mut records,
        "r",
        6,
        "tool_execution_start",
        &json!({"toolCallId":"tool","toolName":"bash","args":{"command":"pwd"}}),
        160.,
    );
    assert_eq!(records[2].parent_id, Some(records[1].id.clone()));
    update(
        &mut records,
        "r",
        7,
        "tool_execution_update",
        &json!({"toolCallId":"tool","partialResult":{"content":"part"}}),
        170.,
    );
    assert!(records[2].output.is_some());
    assert_eq!(records[2].duration_ms, None);
    update(
        &mut records,
        "r",
        8,
        "status",
        &json!({"status":"interrupted"}),
        180.,
    );
    assert_eq!(records[2].status, "interrupted");
    assert_eq!(records[2].duration_ms, None);
}
#[test]
fn subagent_tools_keep_their_explicit_ownership_and_isolated_ids() {
    let mut r = vec![];
    update(
        &mut r,
        "run",
        1,
        "subagent_lifecycle",
        &json!({"payload":{"id":"child","agent":"reviewer","parentToolCallId":"parent","status":"started"}}),
        10.,
    );
    update(
        &mut r,
        "run",
        2,
        "subagent_event",
        &json!({"payload":{"id":"child","event":{"type":"tool_execution_start","toolCallId":"read","toolName":"read","args":{"path":"test"}}}}),
        20.,
    );
    assert_eq!(r[0].parent_id.as_deref(), Some("tool:parent"));
    assert_eq!(r[1].id, "sub:child:tool:read");
    assert_eq!(r[1].parent_id.as_deref(), Some("subagent:child"));
    update(
        &mut r,
        "run",
        3,
        "subagent_event",
        &json!({"payload":{"id":"child","event":{"type":"tool_execution_end","toolCallId":"read","result":"done","isError":false}}}),
        40.,
    );
    assert_eq!(r[1].status, "succeeded");
    assert_eq!(r[1].duration_ms, Some(20.));
}
