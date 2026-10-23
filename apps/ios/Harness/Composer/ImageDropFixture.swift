#if DEBUG
import SwiftUI
import UIKit
import UniformTypeIdentifiers

/// Opt-in drag sources for XCTest. They exercise real UIKit drag sessions over
/// the actual chat/composer; no drop callback or attachment state is injected.
/// SessionView exposes this only with BOTH -demo and -image-drop-fixture.
struct ImageDropFixture: View {
    var body: some View {
        HStack(spacing: 6) {
            ForEach(ImageDropSource.Kind.allCases, id: \.rawValue) { kind in
                ImageDropSource(kind: kind)
                    .frame(width: 60, height: 40)
            }
        }
        .padding(8)
        .background(Theme.bg, in: RoundedRectangle(cornerRadius: 12))
    }
}

private struct ImageDropSource: UIViewRepresentable {
    enum Kind: String, CaseIterable {
        case png, multiple, invalid, slow, text

        var label: String {
            switch self {
            case .png: "PNG"
            case .multiple: "2 images"
            case .invalid: "Bad image"
            case .slow: "Slow"
            case .text: "Text"
            }
        }
    }

    let kind: Kind
    func makeCoordinator() -> Coordinator { Coordinator(kind: kind) }

    func makeUIView(context: Context) -> UILabel {
        let label = UILabel()
        label.text = kind.label
        label.textAlignment = .center
        label.font = .systemFont(ofSize: 11, weight: .medium)
        label.textColor = .white
        label.backgroundColor = .darkGray
        label.isUserInteractionEnabled = true
        label.isAccessibilityElement = true
        label.accessibilityIdentifier = "image-drop-source-\(kind.rawValue)"
        let drag = UIDragInteraction(delegate: context.coordinator)
        drag.isEnabled = true
        label.addInteraction(drag)
        return label
    }

    func updateUIView(_ uiView: UILabel, context: Context) {}

    final class Coordinator: NSObject, UIDragInteractionDelegate {
        let kind: Kind
        init(kind: Kind) { self.kind = kind }

        func dragInteraction(_ interaction: UIDragInteraction,
                             itemsForBeginning session: UIDragSession) -> [UIDragItem] {
            if kind == .text {
                return [UIDragItem(itemProvider: NSItemProvider(object: "Dragged text" as NSString))]
            }
            let colors: [UIColor] = kind == .multiple ? [.red, .blue] : [.red]
            return colors.map { color in
                let data = kind == .invalid ? Data("invalid PNG".utf8) : Self.png(color)
                let provider = NSItemProvider()
                let slow = kind == .slow
                provider.registerDataRepresentation(forTypeIdentifier: UTType.png.identifier, visibility: .all) { completion in
                    if slow {
                        DispatchQueue.global().asyncAfter(deadline: .now() + 2) { completion(data, nil) }
                    } else {
                        completion(data, nil)
                    }
                    return nil
                }
                return UIDragItem(itemProvider: provider)
            }
        }

        func dragInteraction(_ interaction: UIDragInteraction,
                             sessionIsRestrictedToDraggingApplication session: UIDragSession) -> Bool { true }

        private static func png(_ color: UIColor) -> Data {
            UIGraphicsImageRenderer(size: CGSize(width: 16, height: 16)).pngData { context in
                color.setFill()
                context.fill(CGRect(x: 0, y: 0, width: 16, height: 16))
            }
        }
    }
}
#endif
