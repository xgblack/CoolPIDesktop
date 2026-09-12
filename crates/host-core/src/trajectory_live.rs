//! Bounded live projection, updated before publishing the existing Host event envelope.
use crate::trajectory_history::{Record, make_record};
use serde_json::{Value, json};

pub fn update(
    records: &mut Vec<Record>,
    run: &str,
    seq: u64,
    typ: &str,
    p: &Value,
    now: f64,
) -> Vec<Record> {
    let mut warnings = Vec::new();
    let turn = records.iter().filter_map(|r| r.turn).max().unwrap_or(0);
    let step = records
        .iter()
        .rev()
        .find(|r| r.kind == "assistant")
        .and_then(|r| r.step)
        .unwrap_or(0);
    let mut changed = Vec::new();
    if typ == "subagent_lifecycle" {
        let p = &p["payload"];
        let Some(sid) = p["id"].as_str() else {
            return changed;
        };
        let id = format!("subagent:{sid}");
        let idx = records.iter().position(|r| r.id == id);
        let mut r = idx.map(|i| records[i].clone()).unwrap_or_else(|| {
            let parent = p["parentToolCallId"]
                .as_str()
                .map(|id| format!("tool:{id}"));
            let mut r = make_record(
                id,
                "tool",
                p["agent"].as_str().unwrap_or("subagent").into(),
                "running",
                json!({"content":p["description"]}),
                parent,
                &mut warnings,
            );
            r.turn = Some(turn.max(1));
            r.step = Some(step.max(1));
            r.started_at = Some(now);
            r.timing_source = Some("host".into());
            r
        });
        r.status = match p["status"].as_str() {
            Some("completed") => "succeeded",
            Some("failed") => "failed",
            Some("aborted") => "cancelled",
            _ => "running",
        }
        .into();
        if r.status != "running" {
            r.completed_at = Some(now);
            r.duration_ms = r.started_at.map(|s| now - s);
        }
        if let Some(i) = idx {
            records[i] = r.clone();
        } else {
            records.push(r.clone());
        }
        changed.push(r);
    } else if typ == "subagent_event" {
        let p = &p["payload"];
        let Some(sid) = p["id"].as_str() else {
            return changed;
        };
        let e = &p["event"];
        let event_type = e["type"].as_str().unwrap_or("");
        if event_type.starts_with("subagent_") {
            return changed;
        }
        let prefix = format!("sub:{sid}:");
        let root = format!("subagent:{sid}");
        let owner = records.iter().find(|r| r.id == root).cloned();
        let mut local: Vec<Record> = records
            .iter()
            .filter(|r| r.id.starts_with(&prefix))
            .cloned()
            .map(|mut r| {
                r.id = r.id[prefix.len()..].into();
                r.aliases = r
                    .aliases
                    .iter()
                    .filter_map(|a| a.strip_prefix(&prefix).map(str::to_owned))
                    .collect();
                r.parent_id = r
                    .parent_id
                    .and_then(|p| p.strip_prefix(&prefix).map(str::to_owned));
                r
            })
            .collect();
        for mut r in update(&mut local, run, seq, event_type, e, now) {
            r.id = format!("{prefix}{}", r.id);
            r.aliases = r.aliases.iter().map(|a| format!("{prefix}{a}")).collect();
            r.parent_id = Some(
                r.parent_id
                    .map(|p| format!("{prefix}{p}"))
                    .unwrap_or_else(|| root.clone()),
            );
            r.turn = owner.as_ref().and_then(|o| o.turn);
            r.step = owner.as_ref().and_then(|o| o.step);
            if let Some(i) = records.iter().position(|old| old.id == r.id) {
                records[i] = r.clone();
            } else {
                records.push(r.clone());
            }
            changed.push(r);
        }
    } else if typ == "user_message" {
        let mut r = make_record(
            format!("live:{run}:{seq}"),
            "user",
            "user".into(),
            "succeeded",
            json!({"content":p["text"]}),
            None,
            &mut warnings,
        );
        r.turn = Some(turn + 1);
        r.started_at = Some(now);
        r.timing_source = Some("host".into());
        changed.push(r.clone());
        records.push(r);
    } else if matches!(typ, "message_start" | "message_update" | "message_end") {
        let m = p.get("message").or_else(|| {
            p.get("assistantMessageEvent")
                .and_then(|v| v.get("partial"))
        });
        if let Some(m) = m {
            let role = m["role"].as_str().unwrap_or("assistant");
            if role == "toolResult" {
                return changed;
            }
            let alias = m["timestamp"]
                .as_f64()
                .map(|t| format!("message:{role}:{t}"));
            let idx = alias
                .as_ref()
                .and_then(|a| {
                    records
                        .iter()
                        .position(|r| r.id == *a || r.aliases.contains(a))
                })
                .or_else(|| {
                    if role == "user" {
                        records
                            .iter()
                            .rposition(|r| r.kind == "user" && r.aliases.is_empty())
                    } else if typ != "message_start" {
                        records
                            .iter()
                            .rposition(|r| r.kind == "assistant" && r.status == "running")
                    } else {
                        None
                    }
                });
            let id = idx
                .map(|i| records[i].id.clone())
                .unwrap_or_else(|| format!("live:{run}:{seq}"));
            let status = if typ == "message_end" {
                match m["stopReason"].as_str() {
                    Some("error") => "failed",
                    Some("aborted") => "cancelled",
                    _ => "succeeded",
                }
            } else if role == "user" {
                "succeeded"
            } else {
                "running"
            };
            let mut r = make_record(
                id,
                if role == "assistant" {
                    "assistant"
                } else if role == "user" {
                    "user"
                } else {
                    "context"
                },
                role.into(),
                status,
                m.clone(),
                None,
                &mut warnings,
            );
            r.aliases = alias.into_iter().collect();
            r.turn = Some(turn.max(1));
            r.step = if role == "assistant" {
                Some(idx.and_then(|i| records[i].step).unwrap_or(step + 1))
            } else {
                None
            };
            r.started_at = r
                .started_at
                .or_else(|| idx.and_then(|i| records[i].started_at))
                .or(Some(now));
            if m["timestamp"].is_null() {
                r.timing_source = Some("host".into());
            }
            if status == "running" {
                r.duration_ms = None;
                r.completed_at = None;
            } else if role == "assistant" && r.completed_at.is_none() {
                r.completed_at = Some(now);
                if r.duration_ms.is_none() {
                    r.duration_ms = r.started_at.map(|s| (now - s).max(0.0));
                    r.timing_source = Some("host".into());
                }
            }
            r.error = m["errorMessage"].as_str().map(str::to_owned);
            if let Some(i) = idx {
                records[i] = r.clone();
            } else {
                records.push(r.clone());
            }
            changed.push(r);
        } else if typ == "message_update" {
            let e = &p["assistantMessageEvent"];
            if let Some(delta) = e["delta"].as_str() {
                if matches!(e["type"].as_str(), Some("text_delta" | "thinking_delta")) {
                    let idx = records
                        .iter()
                        .rposition(|r| r.kind == "assistant" && r.status == "running");
                    let mut r = idx.map(|i| records[i].clone()).unwrap_or_else(|| {
                        let mut r = make_record(
                            format!("live:{run}:{seq}"),
                            "assistant",
                            "assistant".into(),
                            "running",
                            json!({"content":[]}),
                            None,
                            &mut warnings,
                        );
                        r.turn = Some(turn.max(1));
                        r.step = Some(step + 1);
                        r.started_at = Some(now);
                        r.timing_source = Some("host".into());
                        r
                    });
                    let field = if e["type"] == "thinking_delta" {
                        "thinking"
                    } else {
                        "text"
                    };
                    let blocks = r.content.as_array_mut();
                    if let Some(blocks) = blocks {
                        if blocks.last().is_some_and(|b| b.get(field).is_some()) {
                            let b = blocks.last_mut().unwrap();
                            let text = format!("{}{}", b[field].as_str().unwrap_or(""), delta);
                            b[field] = json!(text);
                        } else {
                            let mut b = json!({"type":field});
                            b[field] = json!(delta);
                            blocks.push(b);
                        }
                    }
                    if r.content.to_string().len() > 1024 * 1024 {
                        r.truncated = true;
                        r.content = json!("流式内容超过显示限制，完整记录将在结束后从会话读取");
                    }
                    if let Some(i) = idx {
                        records[i] = r.clone();
                    } else {
                        records.push(r.clone());
                    }
                    changed.push(r);
                }
            }
        }
    } else if matches!(
        typ,
        "tool_execution_start" | "tool_execution_update" | "tool_execution_end"
    ) {
        let Some(cid) = p["toolCallId"].as_str() else {
            return changed;
        };
        let id = format!("tool:{cid}");
        let idx = records.iter().position(|r| r.id == id);
        let mut r = idx.map(|i| records[i].clone()).unwrap_or_else(|| {
            let parent = records
                .iter()
                .rev()
                .find(|r| r.kind == "assistant")
                .map(|r| r.id.clone());
            let mut r = make_record(
                id,
                "tool",
                p["toolName"].as_str().unwrap_or("tool").into(),
                "running",
                p.clone(),
                parent,
                &mut warnings,
            );
            r.turn = Some(turn.max(1));
            r.step = Some(step.max(1));
            r.tool_call_id = Some(cid.into());
            r
        });
        if typ == "tool_execution_start" {
            r.started_at = Some(now);
            r.input = p.get("args").cloned();
            r.timing_source = Some("host".into());
        }
        if typ == "tool_execution_update" {
            r.output = p.get("partialResult").cloned();
        }
        if typ == "tool_execution_end" {
            r.output = p.get("result").cloned();
            r.status = if p["isError"] == true {
                "failed"
            } else {
                "succeeded"
            }
            .into();
            r.completed_at = Some(now);
            r.duration_ms = r.started_at.map(|s| (now - s).max(0.0));
        }
        // Live tool payloads have the same bounded projection as history records.
        for v in [&mut r.input, &mut r.output].into_iter().flatten() {
            if v.to_string().len() > 65536 {
                *v = json!({"truncated":true,"message":"完整工具记录请在结束后读取历史"});
                r.truncated = true;
            }
        }
        if let Some(i) = idx {
            records[i] = r.clone();
        } else {
            records.push(r.clone());
        }
        changed.push(r);
    } else if typ == "auto_compaction_start" || typ == "auto_compaction_end" {
        let idx = records
            .iter()
            .rposition(|r| r.kind == "compaction" && r.status == "running");
        let mut r = idx.map(|i| records[i].clone()).unwrap_or_else(|| {
            make_record(
                format!("live:{run}:{seq}"),
                "compaction",
                "compaction".into(),
                "running",
                p.clone(),
                None,
                &mut warnings,
            )
        });
        r.started_at = r.started_at.or(Some(now));
        r.timing_source = Some("host".into());
        if typ == "auto_compaction_end" {
            r.status = if p["error"].is_null() {
                "succeeded"
            } else {
                "failed"
            }
            .into();
            r.output = Some(p.clone());
            r.completed_at = Some(now);
            r.duration_ms = r.started_at.map(|s| now - s);
        }
        if let Some(i) = idx {
            records[i] = r.clone();
        } else {
            records.push(r.clone());
        }
        changed.push(r);
    } else if typ == "error"
        || typ == "status"
            && matches!(
                p["status"].as_str(),
                Some("idle" | "interrupted" | "stopped" | "failed")
            )
    {
        for r in records.iter_mut().filter(|r| r.status == "running") {
            r.status = if typ == "error" || p["status"] == "failed" {
                "failed"
            } else {
                "interrupted"
            }
            .into();
            r.completed_at = None;
            r.duration_ms = None;
            changed.push(r.clone());
        }
    }
    for r in records.iter_mut().chain(changed.iter_mut()) {
        r.resources.clear();
    }
    if records.len() > 512 {
        records.drain(..records.len() - 512);
    }
    let mut bytes = 0;
    let mut cut = 0;
    for (i, r) in records.iter().enumerate().rev() {
        bytes += r.content.to_string().len()
            + r.input.as_ref().map_or(0, |v| v.to_string().len())
            + r.output.as_ref().map_or(0, |v| v.to_string().len());
        if bytes > 4 * 1024 * 1024 {
            cut = i + 1;
            break;
        }
    }
    if cut > 0 && cut < records.len() {
        records.drain(..cut);
    }
    changed
}
