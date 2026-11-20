import CoreImage
import CoreImage.CIFilterBuiltins
import SwiftUI
import UIKit
import VentiCore

/// The pilot's task: what it is, where it came from and how to hand it on.
///
/// A task comes in as a file or as text copied from a message, in the
/// format XCTrack and other instruments exchange, and goes out the same
/// way or as a QR code another pilot scans.
struct TaskScreen: View {
    @Environment(TaskModel.self) private var tasks

    @State private var isChoosingFile = false
    @State private var isShowingCode = false
    @State private var isConfirmingRemoval = false
    @State private var isPlanningRoute = false
    @State private var isDrawingRoute = false
    @State private var isScanning = false
    @State private var exported: URL?

    var body: some View {
        // The paste button's closure may run off the main actor; it takes
        // the model with it, not this view.
        let model = tasks
        VentiList {
            if let task = tasks.task {
                Section {
                    TaskSketch(task: task)
                        .frame(height: 200)
                        .listRowInsets(EdgeInsets(top: 12, leading: 12, bottom: 12, trailing: 12))
                } footer: {
                    Text(Self.summary(of: task))
                }

                let heading: String = task.isRoute ? "Waypoints" : "Turnpoints"
                Section(heading) {
                    ForEach(task.turnpoints, id: \.number) { turnpoint in
                        TaskTurnpointRow(turnpoint: turnpoint)
                    }
                }

                let times = Self.times(of: task)
                if !times.isEmpty {
                    Section("Times") {
                        ForEach(times, id: \.name) { time in
                            LabeledContent(time.name, value: time.value)
                        }
                    }
                }
            }

            Section {
                PasteButton(payloadType: String.self) { texts in
                    guard let text = texts.first else { return }
                    Task { @MainActor in
                        model.keep(text)
                    }
                }
                .accessibilityIdentifier("task-paste")
                Button("Scan a QR code", systemImage: "qrcode.viewfinder") {
                    isScanning = true
                }
                Button("Open a task file", systemImage: "folder") {
                    isChoosingFile = true
                }
                Button(
                    "Plan a route from your waypoints",
                    systemImage: "point.topleft.down.curvedto.point.bottomright.up"
                ) {
                    isPlanningRoute = true
                }
                // Without waypoints: tap the places on the map, and see
                // what the route would score.
                Button("Draw a route on the map", systemImage: "hand.tap") {
                    isDrawingRoute = true
                }
                .accessibilityIdentifier("task-draw-route")
            } header: {
                Text(tasks.task == nil ? "Bring a task in" : "Replace it with another")
            } footer: {
                VStack(alignment: .leading, spacing: 8) {
                    if let problem = tasks.problem {
                        Label(problem, systemImage: "exclamationmark.triangle")
                            .foregroundStyle(.venti(.danger))
                            .accessibilityIdentifier("task-problem")
                    }
                    Text("Copy a task from a message, or open a file ending in .xctsk: the format XCTrack, Flyskyhy and other instruments exchange. The cockpit then leads from turnpoint to turnpoint, counts down to the start and measures the way to goal. A route of your own waypoints is flown from one to the next.")
                }
            }
        }
        .navigationTitle("Task")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            if tasks.task != nil {
                ToolbarItem(placement: .primaryAction) {
                    // Handing the task on: another pilot scans the code
                    // or opens the file.
                    Menu("More", systemImage: "ellipsis") {
                        Button("Show as a QR code", systemImage: "qrcode") {
                            isShowingCode = true
                        }
                        if let exported {
                            ShareLink("Share as a file", item: exported)
                        }
                        Button("Remove this task", systemImage: "trash", role: .destructive) {
                            isConfirmingRemoval = true
                        }
                    }
                }
            }
        }
        .onAppear {
            // The times of a task are those of today.
            tasks.reload()
        }
        // The file to share is written whenever the task changes.
        .task(id: tasks.task) {
            exported = try? tasks.exportFile()
        }
        // Task files have no type the system knows, so any file can be
        // picked; the core says whether it holds a task.
        .fileImporter(isPresented: $isChoosingFile, allowedContentTypes: [.data]) { result in
            guard case .success(let url) = result else { return }
            tasks.keep(fileAt: url)
        }
        .sheet(isPresented: $isShowingCode) {
            TaskCodeSheet()
                .presentationDetents([.large])
        }
        .sheet(isPresented: $isPlanningRoute) {
            RoutePlanner()
        }
        .fullScreenCover(isPresented: $isDrawingRoute) {
            RouteMapPlanner()
        }
        .sheet(isPresented: $isScanning) {
            TaskScanSheet()
        }
        .confirmationDialog(
            "Remove this task?",
            isPresented: $isConfirmingRemoval,
            titleVisibility: .visible
        ) {
            Button("Remove", role: .destructive) {
                tasks.forget()
            }
        } message: {
            Text("The cockpit goes back to navigating to the take-off or to a place you choose.")
        }
    }

    /// The task's lengths in a sentence.
    static func summary(of task: TaskView) -> String {
        var text = "\(Format.kilometres(metres: task.lengthM)) km along the shortest way through "
        text += task.isRoute ? "its waypoints" : "the edges of its cylinders"
        if let speed = task.speedSectionM {
            text += ", \(Format.kilometres(metres: speed)) km of it in the speed section"
        }
        text += ". Measured on the \(task.measuredOn)."
        if let height = task.finishAltitudeM {
            text += " Goal is to be crossed \(Format.metres(height)) m above its ground."
        }
        return text
    }

    /// A time of the task with its name.
    struct NamedTime: Equatable {
        let name: String
        let value: String
    }

    /// The task's times on this iPhone's clock, in the order of the day.
    static func times(of task: TaskView) -> [NamedTime] {
        var times: [NamedTime] = []
        if let open = task.takeoffOpenUnixS {
            let close = task.takeoffCloseUnixS.map { " to \(Format.time(unixSeconds: $0))" } ?? ""
            times.append(NamedTime(name: "Launch open", value: Format.time(unixSeconds: open) + close))
        }
        if let start = task.start {
            let gates = start.gatesUnixS.map { Format.time(unixSeconds: $0) }.joined(separator: ", ")
            times.append(NamedTime(name: start.inWords, value: gates.isEmpty ? "Start always open" : "Start \(gates)"))
            if let close = start.closeUnixS {
                times.append(NamedTime(name: "Start closes", value: Format.time(unixSeconds: close)))
            }
        }
        if let deadline = task.deadlineUnixS {
            times.append(NamedTime(name: "Task ends", value: Format.time(unixSeconds: deadline)))
        }
        return times
    }
}

/// One turnpoint in a line: its number, name, what it is, and how far the
/// shortest way has come when it touches it.
struct TaskTurnpointRow: View {
    let turnpoint: TaskTurnpointView
    /// Whether the flight has reached it; `nil` outside a flight.
    var isReached: Bool?
    var isNext = false

    var body: some View {
        HStack(spacing: 12) {
            ZStack {
                Circle()
                    .fill(isNext ? Palette.accent : Color.venti(.surfaceRaised))
                if isReached == true {
                    Image(systemName: "checkmark")
                        .font(.venti(.footnote, .bold))
                } else {
                    Text(Format.count(turnpoint.number))
                        .font(.venti(.footnote, .semibold))
                        .monospacedDigit()
                        .foregroundStyle(isNext ? Color.venti(.onPrimary) : Color.venti(.text))
                }
            }
            .frame(width: 28, height: 28)
            VStack(alignment: .leading, spacing: 2) {
                Text(turnpoint.name.isEmpty ? turnpoint.roleInWords : turnpoint.name)
                    .fontWeight(isNext ? .semibold : .regular)
                    .lineLimit(1)
                Text(turnpoint.detail)
                    .font(.venti(.footnote))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer(minLength: 8)
            if turnpoint.number > 1 {
                VStack(alignment: .trailing, spacing: 2) {
                    Text("\(Format.kilometres(metres: turnpoint.alongM)) km")
                        .monospacedDigit()
                    Text("+\(Format.kilometres(metres: turnpoint.legM))")
                        .font(.venti(.footnote))
                        .foregroundStyle(.secondary)
                        .monospacedDigit()
                }
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(Format.count(turnpoint.number)), \(turnpoint.name), \(turnpoint.detail)")
        .accessibilityValue(spoken)
        .accessibilityIdentifier("task-turnpoint")
    }

    private var spoken: String {
        var parts: [String] = []
        if isNext {
            parts.append("next")
        }
        if isReached == true {
            parts.append("reached")
        }
        if turnpoint.number > 1 {
            parts.append("\(Format.kilometres(metres: turnpoint.alongM)) kilometres along the task")
        }
        return parts.joined(separator: ", ")
    }
}

/// The task drawn to scale without a map: its cylinders, its goal and the
/// shortest way through them, north at the top.
struct TaskSketch: View {
    let task: TaskView

    var body: some View {
        Canvas { context, size in
            let frame = Frame(task: task, size: size)
            // The shortest way, under the cylinders it touches.
            var way = Path()
            for (index, place) in task.way.enumerated() {
                if index == 0 {
                    way.move(to: frame.point(place))
                } else {
                    way.addLine(to: frame.point(place))
                }
            }
            context.stroke(way, with: .color(Palette.accent), style: StrokeStyle(lineWidth: 2, lineJoin: .round))

            for turnpoint in task.turnpoints {
                if turnpoint.outline.count >= 3 {
                    var ring = Path()
                    for (index, place) in turnpoint.outline.enumerated() {
                        if index == 0 {
                            ring.move(to: frame.point(place))
                        } else {
                            ring.addLine(to: frame.point(place))
                        }
                    }
                    ring.closeSubpath()
                    context.fill(ring, with: .color(.primary.opacity(turnpoint.role == .goal ? 0.16 : 0.05)))
                    context.stroke(
                        ring, with: .color(.primary),
                        // The start is the one cylinder with a clock on it.
                        style: StrokeStyle(lineWidth: 1.5, dash: turnpoint.role == .start ? [5, 3] : [])
                    )
                } else {
                    let centre = frame.point(turnpoint.position)
                    let dot = CGRect(x: centre.x - 3.5, y: centre.y - 3.5, width: 7, height: 7)
                    context.fill(Path(ellipseIn: dot), with: .color(.primary))
                }
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("A sketch of the task")
        .accessibilityIdentifier("task-sketch")
    }

    /// Fits the task's box into the drawing, with the same scale both ways.
    struct Frame {
        private let west: Double
        private let north: Double
        private let squeeze: Double
        private let scale: Double
        private let left: Double
        private let top: Double

        init(task: TaskView, size: CGSize) {
            // A degree of longitude is shorter than one of latitude.
            let narrowing = cos((task.south + task.north) / 2 * Double.pi / 180)
            let width = max((task.east - task.west) * narrowing, 1e-6)
            let height = max(task.north - task.south, 1e-6)
            let wide = Double(size.width)
            let high = Double(size.height)
            let margin = 8.0
            let fitting = min((wide - 2 * margin) / width, (high - 2 * margin) / height)
            west = task.west
            north = task.north
            squeeze = narrowing
            scale = fitting
            left = (wide - width * fitting) / 2
            top = (high - height * fitting) / 2
        }

        func point(_ place: Coordinate) -> CGPoint {
            let x: Double = left + (place.lon - west) * squeeze * scale
            let y: Double = top + (north - place.lat) * scale
            return CGPoint(x: x, y: y)
        }
    }
}

/// The task as a QR code for another pilot's instrument.
struct TaskCodeSheet: View {
    @Environment(TaskModel.self) private var tasks
    @Environment(\.dismiss) private var dismiss

    /// Compressed, the code has fewer dots and scans better in the sun.
    @State private var isCompressed = true

    var body: some View {
        NavigationStack {
            VentiList {
                Section {
                    if let text = tasks.code(compressed: isCompressed), let image = Self.image(for: text) {
                        Image(uiImage: image)
                            .interpolation(.none)
                            .resizable()
                            .scaledToFit()
                            .padding(16)
                            // A code is read as dark on light, whatever the
                            // appearance of the app.
                            .background(Color.white, in: .rect(cornerRadius: 12))
                            .frame(maxWidth: .infinity)
                            .listRowBackground(Color.clear)
                            .accessibilityLabel("The task as a QR code")
                            .accessibilityIdentifier("task-code")
                    } else {
                        Text("The code could not be drawn.")
                            .foregroundStyle(.secondary)
                    }
                }
                Section {
                    Toggle("Compressed", isOn: $isCompressed)
                } footer: {
                    Text("A compressed code has fewer dots and is read more easily in the sun. An instrument with old software may only read the other kind.")
                }
            }
            .navigationTitle("Task as a QR code")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") {
                        dismiss()
                    }
                }
            }
        }
    }

    /// Draws a text as a QR code, one dot of the code to several of the
    /// picture so that it stays sharp when it is shown large.
    static func image(for text: String) -> UIImage? {
        let filter = CIFilter.qrCodeGenerator()
        filter.message = Data(text.utf8)
        filter.correctionLevel = "M"
        guard let code = filter.outputImage else { return nil }
        let large = code.transformed(by: CGAffineTransform(scaleX: 8, y: 8))
        guard let picture = CIContext().createCGImage(large, from: large.extent) else { return nil }
        return UIImage(cgImage: picture)
    }
}
