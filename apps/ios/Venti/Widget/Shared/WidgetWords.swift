import AppIntents
import WidgetKit

// Every word of the home-screen widget that the core does not have, in this
// one file, so that they can move into the core together. The status words
// ("Flyable"), the day names ("Today", "Tomorrow", "Fri"), the day's line
// ("Good from noon") and the take-off's name are the core's and are not
// here: the app writes them into the snapshot as the core said them.
//
// The words the system shows while a widget is chosen and set up have to
// be written where they are used, as literals: the system reads them out
// of the build, not out of the running widget. That is why the widget's
// question and the thing it asks for are declared here and not beside the
// widget.

enum WidgetWords {
    /// The widget's name where the pilot chooses a widget.
    static let name = "Next days at a take-off"

    /// What it is, under the name there.
    static let description = "Whether today and the next days are flyable at one of your take-offs, as Today says it."

    /// Under the days: when they were last brought up to date, with the
    /// time as the iPhone writes times, "Updated 14:05".
    static func updated(_ time: String) -> String {
        "Updated \(time)"
    }

    /// In place of everything while the pilot keeps no take-off.
    static let keepATakeoff = "Keep a take-off in Venti, and its next days are shown here."

    /// In place of the days once what was read is more than a day old, or
    /// none of its days is left.
    static let outOfDate = "This forecast is too old to show. Open Venti for a new one."

    /// In place of the days of a take-off that no forecast has been read
    /// for yet.
    static let noForecastYet = "No forecast yet. Open Venti to read it."
}

/// What the widget asks when the pilot sets it up: which of their own
/// take-offs. Without an answer it is the first of them.
struct ChooseTakeoff: WidgetConfigurationIntent {
    static let title: LocalizedStringResource = "Take-off"
    static let description = IntentDescription("Which of your take-offs the widget shows.")

    @Parameter(title: "Take-off")
    var takeoff: KeptTakeoff?
}

/// One of the pilot's own take-offs, as the widget's setup lists it: by the
/// name Today has for it.
struct KeptTakeoff: AppEntity {
    static let typeDisplayRepresentation: TypeDisplayRepresentation = "Take-off"
    static let defaultQuery = KeptTakeoffs()

    let id: String
    let name: String

    var displayRepresentation: DisplayRepresentation {
        DisplayRepresentation(title: "\(name)")
    }
}
