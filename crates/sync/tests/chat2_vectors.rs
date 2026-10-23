//! Runs the chat2 protocol conformance vectors (`protocol/chat2-vectors.json`).
//! The Swift client runs the same file (`Chat2VectorTests.swift`); a new port
//! adds its own runner against it.

use harness_sync::chat_client::{CatchUpPlan, next_row_cursor, plan_catch_up};
use harness_sync::chat_frames::{self as wire, StateHeader};
use serde_json::Value;

fn vectors() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../protocol/chat2-vectors.json"
    );
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn hex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

fn u64_at(v: &Value, key: &str) -> u64 {
    v[key].as_u64().unwrap()
}

#[test]
fn frame_type_bytes_and_header_cap() {
    let v = vectors();
    let types = &v["frameTypes"];
    for (name, byte) in [
        ("hello", wire::frame_type::HELLO),
        ("state", wire::frame_type::STATE),
        ("rowsReq", wire::frame_type::ROWS_REQ),
        ("row", wire::frame_type::ROW),
        ("rowsDone", wire::frame_type::ROWS_DONE),
        ("push", wire::frame_type::PUSH),
        ("ack", wire::frame_type::ACK),
        ("presence", wire::frame_type::PRESENCE),
        ("probe", wire::frame_type::PROBE),
        ("probeOk", wire::frame_type::PROBE_OK),
        ("error", wire::frame_type::ERROR),
    ] {
        assert_eq!(types[name].as_u64(), Some(u64::from(byte)), "{name}");
    }
    assert_eq!(
        v["maxHeaderBytes"].as_u64(),
        Some(wire::MAX_HEADER_BYTES as u64)
    );
}

#[test]
fn encodes_exact_bytes_and_decodes_them_back() {
    for case in vectors()["encode"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let kind = u8::try_from(u64_at(case, "kind")).unwrap();
        let payload = hex(case["payloadHex"].as_str().unwrap());
        let bytes = wire::encode(kind, &case["header"], &payload);
        assert_eq!(bytes, hex(case["bytesHex"].as_str().unwrap()), "{name}");

        let frame = wire::decode(&bytes).expect(name);
        assert_eq!(frame.kind, kind, "{name}");
        assert_eq!(frame.header, case["header"], "{name}");
        assert_eq!(frame.payload, payload, "{name}");
    }
}

#[test]
fn decode_rejects_and_accepts() {
    let v = vectors();
    for case in v["decodeRejects"].as_array().unwrap() {
        let bytes = hex(case["bytesHex"].as_str().unwrap());
        assert!(wire::decode(&bytes).is_none(), "{}", case["name"]);
    }
    for case in v["decodeAccepts"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let frame = wire::decode(&hex(case["bytesHex"].as_str().unwrap())).expect(name);
        assert_eq!(u64::from(frame.kind), u64_at(case, "kind"), "{name}");
        assert_eq!(frame.header, case["header"], "{name}");
        assert_eq!(
            frame.payload,
            hex(case["payloadHex"].as_str().unwrap()),
            "{name}"
        );
    }
}

#[test]
fn plans_catch_up() {
    for case in vectors()["catchUp"].as_array().unwrap() {
        let state = &case["state"];
        let state = StateHeader {
            head_seq: u64_at(state, "headSeq"),
            seq_floor: 0,
            checkpoint_seq: u64_at(state, "checkpointSeq"),
            checkpoint_size: u64_at(state, "checkpointSize"),
            row_count: 0,
            row_bytes: 0,
        };
        let plan = plan_catch_up(
            u64_at(case, "cursor"),
            &state,
            case["frontierContained"].as_bool().unwrap(),
        );
        let after = u64_at(&case["expect"], "after");
        let expect = match case["expect"]["plan"].as_str().unwrap() {
            "rowsOnly" => CatchUpPlan::RowsOnly { after },
            "checkpointThenRows" => CatchUpPlan::CheckpointThenRows { after },
            other => panic!("unknown plan {other}"),
        };
        assert_eq!(plan, expect, "{}", case["name"]);
    }
}

#[test]
fn walks_the_row_cursor() {
    for case in vectors()["rowCursor"]["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let mut cursor = u64_at(case, "cursor");
        for step in case["steps"].as_array().unwrap() {
            let seq = u64_at(step, "seq");
            let next = next_row_cursor(cursor, seq);
            assert_eq!(
                next.is_none(),
                step["gap"].as_bool().unwrap(),
                "{name}: seq {seq}"
            );
            cursor = next.unwrap_or(cursor);
            assert_eq!(cursor, u64_at(step, "cursor"), "{name}: seq {seq}");
        }
    }
}
