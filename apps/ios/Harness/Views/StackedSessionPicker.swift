// The lower pane of the phone's stacked split before it shows a session: the same session rows as Home's list,
// archived shelf included, searchable, without the session already open above. Picking one opens it in this pane; + starts a new session
// for the project above; × closes the pane.

import SwiftUI

struct StackedSessionPicker: View {
    @Environment(AppModel.self) private var model
    /// The session in the upper pane, left out of the list.
    let excluding: String?
    let pick: (String) -> Void
    let newSession: (() -> Void)?
    let close: () -> Void
    @State private var query = ""

    private var chats: [Chat] {
        let open = sortActive(model.overviewChats).filter { $0.id != excluding }
        let needle = query.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        guard !needle.isEmpty else { return open }
        return open.filter {
            $0.displayTitle.lowercased().contains(needle) || ($0.lastMessagePreview ?? "").lowercased().contains(needle)
        }
    }

    var body: some View {
        List {
            if chats.isEmpty, model.archivedMatches(in: nil, query: query).allSatisfy({ $0.id == excluding }) {
                Text(query.isEmpty ? "No other sessions" : "No matching sessions")
                    .font(Theme.sans(12))
                    .foregroundStyle(Theme.textFaint)
                    .listRowBackground(Color.clear)
                    .listRowSeparator(.hidden)
            }
            ForEach(chats) { chat in
                ChatRow(chat: chat, showLocation: true) { pick(chat.id) }
                    .listRowBackground(Color.clear)
                    .listRowSeparator(.hidden)
                    .listRowInsets(EdgeInsets(top: 1, leading: 12, bottom: 1, trailing: 12))
            }
            // An archived session opens here too, as it does from Home's shelf.
            ArchivedSection(query: query, excluding: excluding, path: Binding(get: { [] }, set: { routes in
                if case .chat(let id)? = routes.last { pick(id) }
            }))
        }
        .searchable(text: $query, placement: .navigationBarDrawer(displayMode: .always), prompt: "Search sessions")
        .listStyle(.plain)
        .environment(\.defaultMinListRowHeight, 10)
        .scrollContentBackground(.hidden)
        .background(Theme.surface.ignoresSafeArea())
        .navigationTitle("Sessions")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar(removing: .title)
        .toolbar {
            ToolbarItem(placement: .topBarLeading) {
                Text("Sessions")
                    .font(Theme.sans(15, weight: .medium))
                    .foregroundStyle(Theme.text)
                    .fixedSize()
            }
            .sharedBackgroundVisibility(.hidden)
            ToolbarItemGroup(placement: .topBarTrailing) {
                if let newSession {
                    Button(action: newSession) {
                        Label("New session", systemImage: "plus")
                    }
                    .accessibilityIdentifier("stacked-picker-new")
                }
                Button(action: close) {
                    Label("Close this pane", systemImage: "xmark")
                }
                .accessibilityIdentifier("stacked-pane-close")
            }
        }
        .accessibilityIdentifier("stacked-session-picker")
    }
}
