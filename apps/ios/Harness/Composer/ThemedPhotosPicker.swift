import PhotosUI
import SwiftUI

extension View {
    /// Photos supplies its own selection and prominent-confirmation palette.
    /// Keep both on the native semantic tint and the active theme's appearance,
    /// without changing the composer's or other sheets' theme-text tint.
    func themedPhotosPicker(isPresented: Binding<Bool>, selection: Binding<[PhotosPickerItem]>) -> some View {
        background {
            Color.clear
                .photosPicker(isPresented: isPresented, selection: selection,
                              maxSelectionCount: 8, matching: .images)
                .tint(Color(uiColor: .systemBlue))
                // Photos' UIKit bridge still reads the legacy accent value.
                .accentColor(Color(uiColor: .systemBlue))
                .harnessAppearance()
        }
    }
}
