import SwiftUI
import VentiCore

/// The line in the cockpit that says where the flight stands in its task:
/// the countdown to the start, then the clock, and all the while how far
/// it still is to goal. The turnpoint to fly to is in the line below it,
/// like any other place. A tap shows the whole task.
struct TaskStrip: View {
    let status: TaskStatusView
    let open: () -> Void

    var body: some View {
        Button(action: open) {
            HStack(spacing: 12) {
                Image(systemName: status.phase == .inGoal ? "flag.checkered.2.crossed" : "flag.checkered")
                    .font(.title3)
                    .frame(width: 30)
                VStack(alignment: .leading, spacing: 1) {
                    Text(Self.headline(status))
                        .font(.headline)
                        .monospacedDigit()
                        .lineLimit(1)
                    Text(Self.detail(status))
                        .font(.system(.subheadline, design: .rounded, weight: .medium))
                        .monospacedDigit()
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .minimumScaleFactor(0.8)
                }
                Spacer(minLength: 8)
                if status.phase != .inGoal {
                    VStack(alignment: .trailing, spacing: 1) {
                        Text("\(Format.kilometres(metres: status.toGoalM)) km")
                            .font(.system(.headline, design: .rounded))
                            .monospacedDigit()
                        Text(Self.goalDetail(status))
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .monospacedDigit()
                    }
                }
                Image(systemName: "chevron.right")
                    .font(.footnote.weight(.semibold))
                    .foregroundStyle(.tertiary)
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 9)
            .contentShape(.rect)
        }
        .buttonStyle(.plain)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Task")
        .accessibilityValue(Self.spoken(status))
        .accessibilityHint("Shows the turnpoints")
        .accessibilityIdentifier("task-strip")
    }

    /// Where the flight stands: "Start in 4:32", "On task 0:41:12".
    static func headline(_ status: TaskStatusView) -> String {
        switch status.phase {
        case .beforeStart:
            if let opens = status.gate?.opensInS, opens > 0 {
                return "Start in \(countdown(opens))"
            }
            return "Start open"
        case .onTask:
            return status.clockS.map { "On task \(Format.stopwatch(seconds: max($0, 0)))" } ?? "On task"
        case .speedSectionDone:
            return status.clockS.map { "Speed section \(Format.stopwatch(seconds: max($0, 0)))" }
                ?? "Speed section done"
        case .inGoal:
            return "In goal"
        }
    }

    /// What goes with it: the edge of the start and the speed it asks
    /// for, or which turnpoint is next.
    static func detail(_ status: TaskStatusView) -> String {
        switch status.phase {
        case .beforeStart:
            guard let gate = status.gate else { return "Cross the start's edge" }
            var text = "Edge \(Format.kilometres(metres: gate.distanceM)) km"
            if let speed = gate.speedToGateMs {
                // Faster than a glider flies means: too far away to be
                // there when the gate opens.
                text += speed > 25
                    ? " · too far for the gate"
                    : " · needs \(Format.kilometresPerHour(metresPerSecond: speed)) km/h"
            } else {
                text += " · cross it to start"
            }
            return text
        case .onTask, .speedSectionDone:
            var parts: [String] = []
            if let next = status.next {
                parts.append("\(Format.count(next.number)) of \(status.reached.count)")
            }
            // The end of the speed section, where it is not the goal:
            // the distance to goal is on the right already.
            if let left = status.toEndOfSpeedM, abs(left - status.toGoalM) > 50 {
                parts.append("ESS in \(Format.kilometres(metres: left)) km")
            }
            return parts.isEmpty ? "On the way to goal" : parts.joined(separator: " · ")
        case .inGoal:
            if let seconds = status.clockS {
                return "Speed section \(Format.stopwatch(seconds: max(seconds, 0)))"
            }
            return "The task is flown"
        }
    }

    /// Under the distance to goal: the glide the rest of the task needs.
    static func goalDetail(_ status: TaskStatusView) -> String {
        if let glide = status.glideToGoal {
            return "to goal · needs \(Format.glideRatio(glide))"
        }
        return "to goal"
    }

    /// Minutes and seconds, or hours too when it is long.
    static func countdown(_ seconds: Int64) -> String {
        seconds >= 3600 ? Format.stopwatch(seconds: seconds) : Format.minutesSeconds(seconds: seconds)
    }

    private static func spoken(_ status: TaskStatusView) -> String {
        var text = "\(headline(status)), \(detail(status))"
        if status.phase != .inGoal {
            text += ", \(Format.kilometres(metres: status.toGoalM)) kilometres to goal"
        }
        return text
    }
}

/// The task during a flight: every turnpoint with whether it is reached,
/// and the two buttons a pilot needs when the task and the flight have
/// come apart.
struct TaskProgressSheet: View {
    @Environment(FlySession.self) private var fly
    @Environment(TaskModel.self) private var tasks
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            List {
                if let task = tasks.task, let status = fly.task {
                    Section {
                        ForEach(task.turnpoints, id: \.number) { turnpoint in
                            let index = Int(turnpoint.number) - 1
                            TaskTurnpointRow(
                                turnpoint: turnpoint,
                                isReached: status.reached.indices.contains(index) ? status.reached[index] : false,
                                isNext: status.next?.number == turnpoint.number
                            )
                        }
                    } header: {
                        Text(TaskStrip.headline(status))
                    } footer: {
                        Text("A turnpoint counts when you cross the edge of its cylinder, from either side. The app counts it only once a position lies beyond the edge: scoring allows a little more, and you should not need it.")
                    }

                    if let gate = status.gate {
                        Section("Start") {
                            if let time = gate.gateUnixS {
                                let title: String = (gate.opensInS ?? 0) > 0 ? "Gate opens" : "Gate opened"
                                LabeledContent(title, value: Format.time(unixSeconds: time))
                            }
                            if let next = gate.nextGateUnixS {
                                LabeledContent("Next gate", value: Format.time(unixSeconds: next))
                            }
                            LabeledContent("Edge of the start", value: "\(Format.kilometres(metres: gate.distanceM)) km")
                            if let arrives = gate.arrivesInS {
                                LabeledContent("At this speed", value: "there in \(TaskStrip.countdown(Int64(arrives)))")
                            }
                        }
                    }

                    Section {
                        Button("Skip to the next turnpoint", systemImage: "forward.end") {
                            fly.stepTask(forward: true)
                        }
                        .disabled(status.next == nil)
                        Button("Back to the one before", systemImage: "backward.end") {
                            fly.stepTask(forward: false)
                        }
                    } footer: {
                        Text("Stepping back to the start forgets the start you took: for going back to take a later gate.")
                    }
                } else {
                    Text("No task is being flown.")
                        .foregroundStyle(.secondary)
                }
            }
            .navigationTitle("Task")
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
}
