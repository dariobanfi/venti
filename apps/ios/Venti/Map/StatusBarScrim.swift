import SwiftUI

/// A soft band behind the clock and the status icons, so they stay readable
/// over a map that fills the screen.
///
/// The map is light in both appearances. In dark appearance the status bar's
/// text is white and would be lost on it without this.
struct StatusBarScrim: View {
    var body: some View {
        LinearGradient(
            colors: [Color(.systemBackground).opacity(0.5), Color(.systemBackground).opacity(0)],
            startPoint: .top,
            endPoint: .bottom
        )
        .frame(height: 96)
        .frame(maxHeight: .infinity, alignment: .top)
        .ignoresSafeArea()
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }
}
