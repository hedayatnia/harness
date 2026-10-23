// Image attachments on a queued message, edited before it sends (#187).
//
// The draft belongs to one queue row. It starts from the row's committed
// upload paths and only ever changes that list: the row's text, id and slot
// stay as they are. New images are staged here and uploaded to the chat's
// host when the edit is confirmed; the result is committed through the same
// protected edit the desktop uses (`FinishQueuedMessageEdit` with
// `attachments`), so the host swaps the list on that row and nowhere else.

import Foundation

struct QueuedAttachmentDraft: Equatable {
    enum Source: Equatable {
        /// Already uploaded to the host; kept by path.
        case committed(path: String)
        /// Picked on this phone; uploads on confirm.
        case staged(StagedAttachment)
    }

    struct Item: Identifiable, Equatable {
        let id: String
        var source: Source
        /// Appshot captures carry context in the row's text that names their
        /// path, so they stay put rather than leave that context dangling.
        var locked: Bool = false

        var path: String? {
            if case .committed(let path) = source { return path }
            return nil
        }

        var staged: StagedAttachment? {
            if case .staged(let staged) = source { return staged }
            return nil
        }
    }

    /// The queue row these attachments go with.
    let rowId: String
    /// The row's text as the host leased it, unchanged by this editor.
    let text: String
    let originalPaths: [String]
    private(set) var items: [Item]

    init(row: QueuedMessage, leasedText: String? = nil) {
        rowId = row.id
        text = leasedText ?? row.text
        originalPaths = row.attachments
        let appshots = AppshotContext.presentations(text)
        items = row.attachments.enumerated().map { index, path in
            Item(id: "\(index):\(path)", source: .committed(path: path),
                 locked: appshots[path] != nil)
        }
    }

    // MARK: Editing

    mutating func add(_ staged: [StagedAttachment]) {
        items.append(contentsOf: staged.map { Item(id: $0.id, source: .staged($0)) })
    }

    /// Swap one image for another in the same slot.
    @discardableResult
    mutating func replace(id: String, with staged: StagedAttachment) -> Bool {
        guard let index = items.firstIndex(where: { $0.id == id }), !items[index].locked else {
            return false
        }
        items[index] = Item(id: staged.id, source: .staged(staged))
        return true
    }

    @discardableResult
    mutating func remove(id: String) -> Bool {
        guard let index = items.firstIndex(where: { $0.id == id }), !items[index].locked else {
            return false
        }
        items.remove(at: index)
        return true
    }

    // MARK: Committing

    var isChanged: Bool {
        items.map(\.path) != originalPaths.map(Optional.some)
    }

    /// Images that still need to reach the host, in slot order.
    var pendingUploads: [StagedAttachment] { items.compactMap(\.staged) }

    /// The row's text with only its attachments changed, or nil when that
    /// would leave the message with nothing in it. Any Appshot context on the
    /// row is carried over byte for byte.
    var commitText: String? {
        let visible = MessageQueue.visibleText(text, attachments: originalPaths)
        let body = visible == attachmentOnlyText ? "" : visible
        guard let edited = MessageQueue.editedText(body, hasAttachments: !items.isEmpty) else {
            return nil
        }
        return edited + (AppshotContext.suffix(text) ?? "")
    }

    var canConfirm: Bool { isChanged && commitText != nil }

    /// The attachment list to commit, in slot order, once every staged image
    /// has an uploaded path. Nil while any is missing.
    func committedPaths(uploaded: [String: String]) -> [String]? {
        var paths: [String] = []
        for item in items {
            switch item.source {
            case .committed(let path): paths.append(path)
            case .staged(let staged):
                guard let path = uploaded[staged.id] else { return nil }
                paths.append(path)
            }
        }
        return paths
    }

    /// `FinishQueuedMessageEdit` params for this draft: commits to this row
    /// only, under the lease the editor holds. Nil if the lease is for
    /// another row or the message would end up empty.
    func finishParams(chatId: String, lease: QueueEditLease,
                      uploaded: [String: String]) -> [String: Any]? {
        guard lease.rowId == rowId, let text = commitText,
              let paths = committedPaths(uploaded: uploaded) else { return nil }
        return SessionStore.finishQueuedEditParams(chatId: chatId, lease: lease, action: "commit",
                                                   text: text, attachments: paths)
    }
}
