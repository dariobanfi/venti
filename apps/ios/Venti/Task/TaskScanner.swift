import SwiftUI
import VentiCore
import Vision
import VisionKit

/// Scans the QR code of a task with the camera: at a briefing the task is
/// on a screen or a sheet of paper, and typing it in is how turnpoints go
/// wrong.
///
/// Not tried on a phone yet: the simulator the tests run on has no camera.
struct TaskScanSheet: View {
    @Environment(TaskModel.self) private var tasks
    @Environment(\.dismiss) private var dismiss

    /// Why the code in view was not taken, in words.
    @State private var problem: String?

    var body: some View {
        NavigationStack {
            Group {
                if DataScannerViewController.isSupported, DataScannerViewController.isAvailable {
                    CodeScanner { text in
                        // The core says whether the code holds a task.
                        if tasks.keep(text) {
                            dismiss()
                        } else {
                            problem = tasks.problem
                        }
                    }
                    .ignoresSafeArea(edges: .bottom)
                    .overlay(alignment: .bottom) {
                        Text(problem ?? "Hold the task's QR code in view.")
                            .font(.venti(.subheadline, .medium))
                            .multilineTextAlignment(.center)
                            .padding(.horizontal, 16)
                            .padding(.vertical, 10)
                            .ventiFloating(in: .capsule)
                            .padding(24)
                    }
                } else {
                    ContentUnavailableView {
                        Label("The camera cannot be used", systemImage: "camera")
                    } description: {
                        Text("Allow Venti to use the camera in the iPhone's Settings, or copy the task from a message and paste it.")
                    } actions: {
                        // The way to the switch, as on Android.
                        if let settings = URL(string: UIApplication.openSettingsURLString) {
                            Link("Open the iPhone's Settings", destination: settings)
                        }
                    }
                }
            }
            .navigationTitle("Scan a task")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button(commonWords().cancel) {
                        dismiss()
                    }
                }
            }
        }
    }
}

/// The camera, looking for QR codes. Each code that comes into view is
/// handed over once, as its text.
private struct CodeScanner: UIViewControllerRepresentable {
    let onCode: (String) -> Void

    func makeCoordinator() -> Coordinator {
        Coordinator(onCode: onCode)
    }

    func makeUIViewController(context: Context) -> DataScannerViewController {
        let scanner = DataScannerViewController(
            recognizedDataTypes: [.barcode(symbologies: [.qr])],
            qualityLevel: .balanced,
            recognizesMultipleItems: false,
            isHighFrameRateTrackingEnabled: false,
            isPinchToZoomEnabled: true,
            isGuidanceEnabled: true,
            isHighlightingEnabled: true
        )
        scanner.delegate = context.coordinator
        // Without the camera there is nothing to start; the sheet says so
        // before this view is made.
        try? scanner.startScanning()
        return scanner
    }

    func updateUIViewController(_ scanner: DataScannerViewController, context: Context) {
        context.coordinator.onCode = onCode
    }

    static func dismantleUIViewController(_ scanner: DataScannerViewController, coordinator: Coordinator) {
        scanner.stopScanning()
    }

    @MainActor
    final class Coordinator: NSObject, DataScannerViewControllerDelegate {
        var onCode: (String) -> Void

        init(onCode: @escaping (String) -> Void) {
            self.onCode = onCode
        }

        func dataScanner(
            _ dataScanner: DataScannerViewController, didAdd addedItems: [RecognizedItem], allItems: [RecognizedItem]
        ) {
            for item in addedItems {
                if case .barcode(let code) = item, let text = code.payloadStringValue {
                    onCode(text)
                    return
                }
            }
        }
    }
}
