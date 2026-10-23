// Home list search. Narrows the recency-ordered list the grouping splits into
// sections, so a search inside "Project" grouping still reads by project.

import Foundation

enum HomeFilter {
    /// Names a session is searchable by that live outside the chat itself.
    struct Names {
        var project: String?
        var device: String
    }

    /// Sessions whose title, last message, branch, folder, project or device
    /// contains every word of `query`.
    static func apply(_ chats: [Chat], query: String,
                      names: (Chat) -> Names) -> [Chat] {
        let terms = query.split(whereSeparator: \.isWhitespace).map(String.init)
        return chats.filter { chat in
            guard !terms.isEmpty else { return true }
            let extra = names(chat)
            let fields = [chat.displayTitle, chat.lastMessagePreview, chat.branch,
                          chat.cwd.map { ($0 as NSString).lastPathComponent },
                          extra.project, extra.device].compactMap { $0 }
            // Every word has to appear somewhere: "harness studio" finds the
            // harness sessions on the studio, not every session on either.
            return terms.allSatisfy { term in
                fields.contains { $0.range(of: term, options: [.caseInsensitive, .diacriticInsensitive]) != nil }
            }
        }
    }
}
