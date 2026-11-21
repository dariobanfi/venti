import SwiftUI
import VentiCore

/// A glider chosen on the map or in the list, for a sheet. Only its
/// identifier is kept: the page shows the glider as the newest answer has
/// it.
struct ChosenGlider: Identifiable {
    let id: String
}

/// An aircraft of another network chosen on the map or in the list, for
/// a sheet: by its identifier, the sheet showing the newest answer.
struct ChosenOther: Identifiable {
    let id: String
}

/// One glider in a list: the pilot, and how high, how it climbs and how
/// old the position is.
struct LiveGliderRow: View {
    @Environment(LiveModel.self) private var live
    let glider: LiveGlider

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: glider.landed ? "figure.stand" : "paperplane.fill")
                .foregroundStyle(glider.landed || glider.quiet ? Color.venti(.textSecondary) : Color.venti(.good))
                .frame(width: 28)
            VStack(alignment: .leading, spacing: 2) {
                Text(glider.name)
                    .font(.venti(.headline))
                Text(Self.detail(of: glider, age: live.ageInWords(of: glider)))
                    .font(.venti(.subheadline))
                    .foregroundStyle(.secondary)
                    .monospacedDigit()
            }
        }
        .accessibilityElement(children: .combine)
    }

    /// "1,596 m · -1.0 m/s · Now"
    static func detail(of glider: LiveGlider, age: String) -> String {
        var parts = ["\(Format.metres(Double(glider.altitudeM))) m"]
        if !glider.landed, let climb = glider.climbMs {
            parts.append("\(Format.climb(metresPerSecond: climb)) m/s")
        }
        parts.append(age)
        return parts.joined(separator: " · ")
    }
}

/// The gliders on the map as a list: for reading them side by side, and
/// for pilots who use VoiceOver.
struct LiveList: View {
    @Environment(LiveModel.self) private var live
    @Environment(\.dismiss) private var dismiss
    /// Called with a track's identifier when the pilot chooses its glider:
    /// the list closes, and the map shows the flight.
    let choose: (String) -> Void
    /// Called with an aircraft's identifier when the pilot chooses one of
    /// another network: the list closes, and its sheet opens.
    var chooseOther: (String) -> Void = { _ in }

    /// The other networks' aircraft the map draws.
    private var others: [OtherAircraftView] {
        live.layer.others.flatMap(\.aircraft)
    }

    var body: some View {
        NavigationStack {
            VentiList {
                Section {
                    ForEach(live.gliders, id: \.id) { glider in
                        Button {
                            choose(glider.id)
                            dismiss()
                        } label: {
                            LiveGliderRow(glider: glider)
                        }
                        .foregroundStyle(.primary)
                        .accessibilityIdentifier("live-row")
                    }
                } header: {
                    // How many the map draws, in the core's words.
                    if !live.layer.countWords.isEmpty {
                        Text(live.layer.countWords)
                            .accessibilityIdentifier("live-count")
                    }
                } footer: {
                    if !live.gliders.isEmpty {
                        Text(
                            live.isTruncated
                                ? "There are more than Venti lists at once. Zoom the map in. " + LiveGliderScreen.caution
                                : LiveGliderScreen.caution
                        )
                    }
                }
                // The other networks' aircraft, apart from Venti's pilots,
                // each network with its credit.
                ForEach(live.layer.others.filter { !$0.aircraft.isEmpty }, id: \.source) { network in
                    Section {
                        ForEach(network.aircraft, id: \.id) { aircraft in
                            Button {
                                chooseOther(aircraft.id)
                                dismiss()
                            } label: {
                                OtherAircraftRow(aircraft: aircraft)
                            }
                            .foregroundStyle(.primary)
                            .accessibilityIdentifier("other-row")
                        }
                    } footer: {
                        if let credit = network.credit {
                            Text(credit.text)
                        }
                    }
                }
            }
            .overlay {
                if live.gliders.isEmpty && others.isEmpty {
                    VentiEmpty(
                        title: "Nobody in the air here",
                        text: "Pilots who show their flight to everyone appear on this part of the map while they fly."
                    )
                }
            }
            .navigationTitle("In the air")
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

/// One glider: who flies it, where it is and how old that is, and the way
/// to its track on the server's page.
struct LiveGliderScreen: View {
    @Environment(LiveModel.self) private var live
    @Environment(WindModel.self) private var wind
    let id: String

    @State private var reporting: Reported?

    static let caution =
        "Positions are as each pilot's app sent them. A glider nothing was heard of for a while has most likely no network: it is not known to have landed."

    private var glider: LiveGlider? {
        live.glider(id)
    }

    /// The flight so far, once the server has handed its track out.
    private var flight: LiveFollowView? {
        live.followed.flatMap { $0.glider.id == id ? $0 : nil }
    }

    var body: some View {
        VentiList {
            if let glider {
                Section {
                    LabeledContent("Pilot", value: glider.name)
                    LabeledContent("Position", value: live.ageInWords(of: glider))
                        .accessibilityIdentifier("live-age")
                    LabeledContent("Altitude", value: "\(Format.metres(Double(glider.altitudeM))) m")
                    if !glider.landed {
                        if let climb = glider.climbMs {
                            LabeledContent("Climb", value: "\(Format.climb(metresPerSecond: climb)) m/s")
                        }
                        if let speed = glider.groundSpeedMs {
                            LabeledContent(
                                "Speed", value: "\(Format.kilometresPerHour(metresPerSecond: speed)) km/h"
                            )
                        }
                        if let course = glider.courseDeg {
                            LabeledContent("Flying", value: Format.compassPoint(degrees: course))
                        }
                    }
                    // The flight so far, whose line is on the map.
                    if let flight {
                        LabeledContent("Shown for", value: Format.stopwatch(seconds: flight.durationS))
                            .accessibilityIdentifier("live-duration")
                        LabeledContent("Highest", value: "\(Format.metres(Double(flight.highestM))) m")
                    }
                } footer: {
                    Text(Self.caution)
                }
                if let server = wind.server, let page = URL(string: server.absoluteString + glider.pagePath) {
                    Section {
                        Link(destination: page) {
                            Label("Follow the flight in the browser", systemImage: "safari")
                        }
                    } footer: {
                        Text("The server's page draws the track, the newest half hour or so of a long one, and keeps itself up to date.")
                    }
                }
                // For a track that should not be here: to whoever runs
                // the server, and to nobody else.
                Section {
                    Button("Report this track", systemImage: "exclamationmark.bubble") {
                        reporting = .liveTrack(glider.id)
                    }
                    .accessibilityIdentifier("report-track")
                }
            } else {
                Section {
                    Text(
                        live.followProblem
                            ?? "This glider is no longer on this part of the map: its pilot stopped showing the flight, or it has left the area."
                    )
                    .foregroundStyle(.secondary)
                }
            }
        }
        .navigationTitle(glider.map { "@\($0.handle)" } ?? "Glider")
        .navigationBarTitleDisplayMode(.inline)
        .sheet(item: $reporting) { reported in
            ReportSheet(reported: reported)
        }
        // The flight's track is asked for while its page is open, and
        // drawn on the map behind it.
        .task(id: id) {
            await live.follow(id)
        }
    }
}

/// One aircraft of another network in a list: what kind, who where the
/// owner lets it be said, and the core's line.
struct OtherAircraftRow: View {
    let aircraft: OtherAircraftView

    var body: some View {
        HStack(spacing: VentiSpace.s3) {
            Image(systemName: "paperplane.fill")
                .foregroundStyle(Color.venti(.textMuted))
                .frame(width: 28)
            VStack(alignment: .leading, spacing: 2) {
                Text(aircraft.name.map { "\(aircraft.kindWords) · \($0)" } ?? aircraft.kindWords)
                    .font(.venti(.headline))
                Text(aircraft.line)
                    .font(.venti(.subheadline))
                    .foregroundStyle(.secondary)
            }
        }
        .accessibilityElement(children: .combine)
    }
}

/// An aircraft another network hears, tapped on the map: what kind, who
/// where the owner lets it be said, the core's line, which ends with how
/// old the position is, and the network's credit. The network gives positions only: there
/// is no track to follow.
struct OtherAircraftSheet: View {
    @Environment(LiveModel.self) private var live
    @Environment(\.dismiss) private var dismiss
    let id: String

    var body: some View {
        NavigationStack {
            VentiList {
                if let found = live.other(id) {
                    Section {
                        if let name = found.aircraft.name {
                            Text(name)
                                .font(.venti(.headline))
                                .accessibilityIdentifier("other-name")
                        }
                        // The line ends with how old the position is.
                        Text(found.aircraft.line)
                            .accessibilityIdentifier("other-line")
                    } footer: {
                        if let notice = found.credit?.notice {
                            Text(notice)
                        }
                    }
                    if let credit = found.credit {
                        Section {
                            if let url = URL(string: credit.url) {
                                Link(destination: url) {
                                    Text(credit.text)
                                }
                                .accessibilityIdentifier("other-credit")
                            } else {
                                Text(credit.text)
                                    .accessibilityIdentifier("other-credit")
                            }
                        }
                    }
                } else {
                    Section {
                        Text("This aircraft is no longer on this part of the map.")
                            .foregroundStyle(.secondary)
                    }
                }
            }
            .navigationTitle(live.other(id)?.aircraft.kindWords ?? "Aircraft")
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
