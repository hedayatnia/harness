import XCTest
import Loro
@testable import Harness

/// #484: a synced row that is concurrent with one of the phone's own ops (a
/// command the host has not seen yet) makes Loro rebuild the changed text from
/// its whole history. A catch-up of many such rows rebuilt once per row; the
/// chat room now imports a contiguous run in one batch.
final class ConcurrentImportCostTests: XCTestCase {
    /// The host streams a reply into one text while a desktop peer writes now
    /// and then (each of its writes starts a new change on the host). The
    /// phone made one op of its own at the start that the host never sees, and
    /// has imported the first `prefixRows` rows; `rows` more follow.
    private func fixture(prefixRows: Int, rows: Int, concurrent: Bool) throws -> (phone: LoroDoc, rows: [Data]) {
        let host = LoroDoc()
        try host.setPeerId(peer: 1)
        let desktop = LoroDoc()
        try desktop.setPeerId(peer: 3)
        let phone = LoroDoc()
        try phone.setPeerId(peer: 2)
        if concurrent {
            try phone.getList(id: "commands").push(v: "steer")
            phone.commit()
        }
        let text = host.getText(id: "reply")
        var updates: [Data] = []
        for n in 0..<(prefixRows + rows) {
            let before = host.oplogVv()
            if n % 10 == 0 {
                try desktop.getMap(id: "meta").insert(key: "seen", v: Int64(n))
                desktop.commit()
                _ = try host.importWith(bytes: desktop.export(mode: .snapshot), origin: "remote")
            }
            try text.pushStr(s: "token ")
            host.commit()
            updates.append(try host.export(mode: .updates(from: before)))
        }
        for row in updates.prefix(prefixRows) {
            _ = try phone.importWith(bytes: row, origin: "remote")
        }
        return (phone, Array(updates.dropFirst(prefixRows)))
    }

    private func text(_ doc: LoroDoc) throws -> String {
        let reply = doc.getText(id: "reply")
        return try reply.slice(startIndex: 0, endIndex: reply.lenUnicode())
    }

    private func seconds(_ work: () throws -> Void) rethrows -> Double {
        let start = DispatchTime.now().uptimeNanoseconds
        try work()
        return Double(DispatchTime.now().uptimeNanoseconds - start) / 1e9
    }

    func testBatchImportOfConcurrentRowsMatchesRowByRowAndRebuildsOnce() throws {
        let (oneByOne, rows) = try fixture(prefixRows: 2_000, rows: 150, concurrent: true)
        let (batched, _) = try fixture(prefixRows: 2_000, rows: 150, concurrent: true)
        let (linear, linearRows) = try fixture(prefixRows: 2_000, rows: 150, concurrent: false)

        let perRow = try seconds {
            for row in rows { _ = try oneByOne.importWith(bytes: row, origin: "remote") }
        }
        let batch = try seconds { _ = try batched.importBatch(bytes: rows) }
        let linearPerRow = try seconds {
            for row in linearRows { _ = try linear.importWith(bytes: row, origin: "remote") }
        }
        print("150 rows after 2000: concurrent per-row \(perRow)s, batched \(batch)s, linear per-row \(linearPerRow)s")

        XCTAssertEqual(try text(batched), try text(oneByOne))
        XCTAssertTrue(try text(batched).hasSuffix("token token "))
        XCTAssertLessThan(batch * 5, perRow, "a batch should rebuild the text once, not once per row")
    }
}
