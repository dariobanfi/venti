import SwiftUI

extension View {
    /// Shows an import message as an alert and clears it when dismissed.
    func importNotice(_ notice: Binding<ImportNotice?>) -> some View {
        alert(
            notice.wrappedValue?.title ?? "",
            isPresented: Binding(
                get: { notice.wrappedValue != nil },
                set: { isPresented in
                    if !isPresented {
                        notice.wrappedValue = nil
                    }
                }
            ),
            presenting: notice.wrappedValue
        ) { _ in
            Button("OK", role: .cancel) {}
        } message: { notice in
            Text(notice.text)
        }
    }
}
