import AppIntents
import SwiftUI
import WidgetKit

/// The app's home-screen widgets: one, the next days at a take-off.
@main
struct VentiWidgets: WidgetBundle {
    var body: some Widget {
        TakeoffDaysWidget()
    }
}

/// "The next days at my take-off": the first of the pilot's own take-offs,
/// or the one chosen in the widget's setup, with today and the next days
/// as Today says them.
///
/// It draws what the app wrote into the folder the two share
/// (`TakeoffSnapshot`), links no Rust and asks no server. A tap opens the
/// app on Today.
struct TakeoffDaysWidget: Widget {
    var body: some WidgetConfiguration {
        AppIntentConfiguration(
            kind: TakeoffWidget.kind, intent: ChooseTakeoff.self, provider: TakeoffDaysProvider()
        ) { entry in
            TakeoffDaysWidgetView(entry: entry)
        }
        .configurationDisplayName(WidgetWords.name)
        .description(WidgetWords.description)
        .supportedFamilies([.systemSmall, .systemMedium])
        // The face has the design language's margin of its own.
        .contentMarginsDisabled()
    }
}

/// What the widget shows at a moment.
struct TakeoffDaysEntry: TimelineEntry {
    let date: Date
    let shown: TakeoffSnapshot.Shown
}

/// Reads the snapshot and says what is shown now and at each moment that
/// changes it: the next day's beginning, when the day's names move on, and
/// the moment what was read is a day old. The app asks for a new timeline
/// whenever it writes.
struct TakeoffDaysProvider: AppIntentTimelineProvider {
    func placeholder(in context: Context) -> TakeoffDaysEntry {
        entry(for: nil, at: Date())
    }

    func snapshot(for configuration: ChooseTakeoff, in context: Context) async -> TakeoffDaysEntry {
        entry(for: configuration.takeoff?.id, at: Date())
    }

    func timeline(for configuration: ChooseTakeoff, in context: Context) async -> Timeline<TakeoffDaysEntry> {
        let now = Date()
        let chosen = configuration.takeoff?.id
        let snapshot = TakeoffSnapshotStore.shared?.read() ?? TakeoffSnapshot()
        let moments = [Int64(now.timeIntervalSince1970)]
            + snapshot.changes(takeoff: chosen, after: Int64(now.timeIntervalSince1970))
        let entries = moments.map { moment in
            TakeoffDaysEntry(
                date: Date(timeIntervalSince1970: TimeInterval(moment)),
                shown: snapshot.shown(takeoff: chosen, nowUnixS: moment)
            )
        }
        return Timeline(entries: entries, policy: .never)
    }

    private func entry(for takeoff: String?, at date: Date) -> TakeoffDaysEntry {
        let snapshot = TakeoffSnapshotStore.shared?.read() ?? TakeoffSnapshot()
        return TakeoffDaysEntry(
            date: date, shown: snapshot.shown(takeoff: takeoff, nowUnixS: Int64(date.timeIntervalSince1970))
        )
    }
}

/// The widget's face in the size the pilot chose, in the design language,
/// on a card's surface.
private struct TakeoffDaysWidgetView: View {
    @Environment(\.widgetFamily) private var family
    let entry: TakeoffDaysEntry

    var body: some View {
        TakeoffDaysView(shown: entry.shown, size: family == .systemMedium ? .medium : .small)
            .ventiThemed()
            .containerBackground(for: .widget) {
                Color.venti(.surface)
            }
            .widgetURL(TakeoffWidget.opensToday)
    }
}
