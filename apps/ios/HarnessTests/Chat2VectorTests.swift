import XCTest
@testable import Harness

/// Runs the chat2 protocol conformance vectors (`protocol/chat2-vectors.json`).
/// The Rust client runs the same file (`crates/sync/tests/chat2_vectors.rs`);
/// a new port adds its own runner against it.
final class Chat2VectorTests: XCTestCase {
    private static let file = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent().deletingLastPathComponent()
        .deletingLastPathComponent().deletingLastPathComponent()
        .appendingPathComponent("protocol/chat2-vectors.json")

    private func vectors() throws -> [String: Any] {
        let data = try Data(contentsOf: Self.file)
        return try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
    }

    private func hex(_ s: String) -> Data {
        var out = Data(capacity: s.count / 2)
        var index = s.startIndex
        while index < s.endIndex {
            let next = s.index(index, offsetBy: 2)
            out.append(UInt8(s[index..<next], radix: 16)!)
            index = next
        }
        return out
    }

    private func u64(_ object: Any?, _ key: String) -> UInt64 {
        ((object as? [String: Any])?[key] as? NSNumber)?.uint64Value ?? 0
    }

    func testFrameTypeBytesAndHeaderCap() throws {
        let v = try vectors()
        let types = try XCTUnwrap(v["frameTypes"] as? [String: NSNumber])
        let expected: [String: UInt8] = [
            "hello": ChatFrameType.hello, "state": ChatFrameType.state, "rowsReq": ChatFrameType.rowsReq,
            "row": ChatFrameType.row, "rowsDone": ChatFrameType.rowsDone, "push": ChatFrameType.push,
            "ack": ChatFrameType.ack, "presence": ChatFrameType.presence, "probe": ChatFrameType.probe,
            "probeOk": ChatFrameType.probeOk, "error": ChatFrameType.error,
        ]
        XCTAssertEqual(Set(types.keys), Set(expected.keys))
        for (name, byte) in expected {
            XCTAssertEqual(types[name]?.uint8Value, byte, name)
        }
        XCTAssertEqual((v["maxHeaderBytes"] as? NSNumber)?.intValue, chatFrameMaxHeaderBytes)
    }

    func testEncodesExactBytesAndDecodesThemBack() throws {
        for case let c as [String: Any] in try XCTUnwrap(try vectors()["encode"] as? [Any]) {
            let name = c["name"] as? String ?? "?"
            let kind = try XCTUnwrap((c["kind"] as? NSNumber)?.uint8Value)
            let header = try XCTUnwrap(c["header"] as? [String: Any])
            let payload = hex(c["payloadHex"] as? String ?? "")
            let bytes = ChatWire.encode(kind, header: header, payload: payload)
            XCTAssertEqual(bytes, hex(c["bytesHex"] as? String ?? ""), name)

            let frame = try XCTUnwrap(ChatWire.decode(bytes), name)
            XCTAssertEqual(frame.kind, kind, name)
            XCTAssertEqual(frame.header as NSDictionary, header as NSDictionary, name)
            XCTAssertEqual(frame.payload, payload, name)
        }
    }

    func testDecodeRejectsAndAccepts() throws {
        let v = try vectors()
        for case let c as [String: Any] in try XCTUnwrap(v["decodeRejects"] as? [Any]) {
            XCTAssertNil(ChatWire.decode(hex(c["bytesHex"] as? String ?? "")), c["name"] as? String ?? "?")
        }
        for case let c as [String: Any] in try XCTUnwrap(v["decodeAccepts"] as? [Any]) {
            let name = c["name"] as? String ?? "?"
            let frame = try XCTUnwrap(ChatWire.decode(hex(c["bytesHex"] as? String ?? "")), name)
            XCTAssertEqual(UInt64(frame.kind), u64(c, "kind"), name)
            XCTAssertEqual(frame.header as NSDictionary, try XCTUnwrap(c["header"] as? NSDictionary), name)
            XCTAssertEqual(frame.payload, hex(c["payloadHex"] as? String ?? ""), name)
        }
    }

    func testPlansCatchUp() throws {
        for case let c as [String: Any] in try XCTUnwrap(try vectors()["catchUp"] as? [Any]) {
            let name = c["name"] as? String ?? "?"
            // The vectors carry only the fields the plan reads.
            var header = try XCTUnwrap(c["state"] as? [String: Any])
            header["seqFloor"] = 0
            let state = try XCTUnwrap(ChatStateHeader(header), name)
            let plan = chatPlanCatchUp(cursor: u64(c, "cursor"), state: state,
                                       frontierContained: c["frontierContained"] as? Bool ?? false)
            let after = u64(c["expect"], "after")
            switch (c["expect"] as? [String: Any])?["plan"] as? String {
            case "rowsOnly": XCTAssertEqual(plan, .rowsOnly(after: after), name)
            case "checkpointThenRows": XCTAssertEqual(plan, .checkpointThenRows(after: after), name)
            case let other: XCTFail("\(name): unknown plan \(String(describing: other))")
            }
        }
    }

    func testWalksTheRowCursor() throws {
        let rows = try XCTUnwrap(try vectors()["rowCursor"] as? [String: Any])
        for case let c as [String: Any] in try XCTUnwrap(rows["cases"] as? [Any]) {
            let name = c["name"] as? String ?? "?"
            var cursor = u64(c, "cursor")
            for case let step as [String: Any] in try XCTUnwrap(c["steps"] as? [Any]) {
                let seq = u64(step, "seq")
                let next = chatNextRowCursor(cursor: cursor, seq: seq)
                XCTAssertEqual(next == nil, step["gap"] as? Bool, "\(name): seq \(seq)")
                cursor = next ?? cursor
                XCTAssertEqual(cursor, u64(step, "cursor"), "\(name): seq \(seq)")
            }
        }
    }

    /// The rule must not overflow at the top of the range (Rust saturates).
    func testRowCursorAtTheTopOfTheRange() {
        XCTAssertEqual(chatNextRowCursor(cursor: .max, seq: .max), .max)
        XCTAssertEqual(chatNextRowCursor(cursor: .max - 1, seq: .max), .max)
        XCTAssertNil(chatNextRowCursor(cursor: .max - 2, seq: .max))
    }
}
