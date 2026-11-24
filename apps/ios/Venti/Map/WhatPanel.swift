import SwiftUI
import VentiCore

/// The one small panel at the map's foot for what it shows (owner decision
/// 19, the web's `WhatPanel.svelte`): one line in words that says what is
/// shown and when ("Rain, 14:15, measured"; "Wind at the ground, 13:00,
/// forecast, ICON-D2"), the one line of time from what was measured
/// through "Now" to the forecast's hours and days, the key as a short
/// strip, or with the thermals by region the five levels by name, and an
/// "i" with the whole key and the notes, folded until the pilot asks. The
/// wind's gusts are a small control of the wind's forecast; its heights
/// stand at the map's side (`ForecastHeightList`). What the pilot has to be told about the data as it
/// stands is never folded away: rain that is not the rain of now, a part
/// of the map with nothing forecast, and what could not be had, with "Try
/// again". Every word about the data is the core's.
struct WhatPanel: View {
    let what: MapWhat
    /// What it is called: "Rain".
    let name: String
    let cells: [MapTimeCell]
    /// The cell the line stands on.
    let chosen: Int
    let choose: (Int) -> Void
    /// Whether the line stands on what was measured.
    let measured: Bool
    let radar: RadarModel
    let thermals: ThermalMapModel
    /// The forecast's day the line stands on.
    let day: ThermalMapDay?
    /// That day's hours of the layer shown, once they are there.
    let hours: ThermalMapHoursView?
    /// The hour by its place among them; `nil` for the day as a whole.
    let hour: Int?
    /// The wind's gusts in place of its mean.
    @Binding var gusts: Bool
    /// The key of the forecast by region, the best first, while its
    /// regions are drawn; empty otherwise.
    var regionLevels: [RegionLevelKey] = []
    /// Whether the forecast shown has squares in the part of the map in view.
    let isInView: Bool
    /// Whether the hour's wind colours the take-offs' fans, which then
    /// have their key under the layer's.
    let showsFansKey: Bool
    let tryRadarAgain: () -> Void
    let tryForecastAgain: () -> Void
    let tryHoursAgain: () -> Void

    /// Whether the whole key and the notes are shown.
    @State private var isOpen = false
    @Environment(\.accessibilityReduceMotion) private var reducesMotion

    private var words: MapLayersWords { mapLayersWords() }

    /// The layer of the forecast shown, as the map offers it.
    private var choice: ThermalMapLayerChoice? {
        thermals.map?.layers.first { $0.layer == what.layer(gusts: gusts) }
    }

    /// Whether the forecast shows an hour, not the day as a whole.
    private var showsAnHour: Bool {
        guard let hours, let hour else { return false }
        return hours.hours.indices.contains(hour)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: VentiSpace.s2) {
            head
            if !cells.isEmpty {
                MapTimeStrip(cells: cells, chosen: chosen, choose: choose)
            }
            controls
            if !levels.isEmpty {
                LevelsStrip(levels: levels)
            } else if !key.steps.isEmpty {
                KeyStrip(steps: key.steps, veil: key.veil)
            }
            if showsFansKey {
                FansKey()
            }
            states
            if isOpen {
                // As high as it is where the screen has the room, and else
                // scrolling in what room there is.
                ViewThatFits(in: .vertical) {
                    folded
                    ScrollView {
                        folded
                    }
                }
            }
        }
        .padding(VentiSpace.s3)
        .frame(maxWidth: .infinity, alignment: .leading)
        .ventiFloating(in: .rect(cornerRadius: VentiRadius.m))
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("what-panel")
    }

    // MARK: The line in words

    /// What is shown and when, in one line.
    var line: String {
        if measured, what == .rain {
            return (radar.shownMoment ?? radar.moment)?.line ?? ""
        }
        if measured {
            return "\(name), \(words.now.lowercased()), \(words.measured)"
        }
        if showsAnHour, let hours, let hour, let day {
            let at = hours.hours[hour].label
            let when = thermals.map?.days.first?.date == day.date ? at : "\(day.name) \(at)"
            return "\(hours.title), \(when), \(words.forecastWord), \(hours.model)"
        }
        if let day, what == .thermals || thermals.map?.layers.isEmpty == true {
            return "\(name), \(day.name), \(words.forecastWord), \(day.model)"
        }
        return ""
    }

    /// The line on the left, the round controls on the right: the radar's
    /// play, and the "i".
    private var head: some View {
        HStack(alignment: .center, spacing: VentiSpace.s2) {
            Text(line)
                .font(.ventiType(.label))
                .fontWeight(.semibold)
                .monospacedDigit()
                .foregroundStyle(.venti(.text))
                .fixedSize(horizontal: false, vertical: true)
                .frame(maxWidth: .infinity, alignment: .leading)
                .accessibilityIdentifier("what-line")
            if isAsking {
                ProgressView()
                    .controlSize(.small)
            }
            if measured, what == .rain, (radar.view?.moments.count ?? 0) > 1, !reducesMotion {
                Button {
                    radar.play()
                } label: {
                    Image(systemName: radar.isPlaying ? "stop.fill" : "play.fill")
                        .font(.ventiType(.label))
                        .foregroundStyle(.venti(.textSecondary))
                        .frame(width: VentiSize.touch, height: VentiSize.touch)
                        .contentShape(.rect)
                }
                .buttonStyle(.plain)
                .padding(.vertical, -VentiSpace.s2)
                .accessibilityLabel(radar.isPlaying ? "Stop" : "Play the radar pictures")
                .accessibilityIdentifier("radar-play")
            }
            Button {
                isOpen.toggle()
            } label: {
                Image(systemName: "info.circle")
                    .font(.ventiType(.heading))
                    .foregroundStyle(.venti(isOpen ? .link : .textSecondary))
                    .frame(width: VentiSize.touch, height: VentiSize.touch)
                    .contentShape(.rect)
            }
            .buttonStyle(.plain)
            .padding(.vertical, -VentiSpace.s2)
            .accessibilityLabel("Key and notes")
            .accessibilityValue(isOpen ? "Shown" : "Hidden")
            .accessibilityIdentifier("what-more")
        }
    }

    /// Whether something of what is shown is being asked for or fetched.
    private var isAsking: Bool {
        if measured, what == .rain {
            return radar.isFetching
        }
        return !measured && thermals.hoursState == .loading
    }

    // MARK: The wind's two controls

    /// The gusts in place of the mean: for the wind's forecast only.
    @ViewBuilder private var controls: some View {
        if what == .wind, !measured, let map = thermals.map,
           let gustsName = map.layers.first(where: { $0.layer == .gusts })?.name
        {
            HStack(spacing: VentiSpace.s2) {
                VentiChip(title: gustsName, isOn: $gusts)
                    .accessibilityIdentifier("what-gusts")
                Spacer(minLength: 0)
            }
        }
    }

    // MARK: The key

    /// A line of the key: its colour, its short words, and its whole words.
    struct KeyLine: Equatable {
        let colour: Color
        let short: String
        let whole: String
    }

    /// The key of what is shown, the least first, and the veil apart: the
    /// radar's steps and what it did not see; the hour's layer; the kinds
    /// of day and the day that is not flyable. The wind stations need
    /// none: each says its numbers.
    private var key: (steps: [KeyLine], veil: KeyLine?) {
        if measured, what == .rain, let view = radar.view {
            let steps = view.key.reversed().map { line in
                KeyLine(
                    colour: RadarColours.onMap(line), short: line.name.isEmpty ? MapTimeLine.short(line.words) : line.name,
                    whole: line.words
                )
            }
            let unseen = KeyLine(
                colour: RadarColours.unseenOnMap, short: MapTimeLine.short(view.unseenWords), whole: view.unseenWords
            )
            return (steps, unseen)
        }
        if measured {
            return ([], nil)
        }
        if showsAnHour, let choice {
            let lines = choice.legend.reversed().map { line in
                KeyLine(
                    colour: ThermalKeyColour.onMap(line), short: MapTimeLine.short(line.words),
                    whole: line.amount.isEmpty ? line.words : "\(line.words), \(line.amount)"
                )
            }
            let isVeil = choice.legend.reversed().map { $0.scale == .veil }
            let steps = zip(lines, isVeil).filter { !$0.1 }.map(\.0)
            let veil = zip(lines, isVeil).first { $0.1 }?.0
            return (steps, veil)
        }
        if what == .thermals, let map = thermals.map, day != nil {
            let lines = map.legend.reversed()
            let steps = lines.compactMap { entry -> KeyLine? in
                guard let step = entry.scaleStep else { return nil }
                return KeyLine(
                    colour: ThermalBand(scaleStep: step).colorOnMap, short: MapTimeLine.short(entry.words),
                    whole: entry.words
                )
            }
            let veil = lines.first { $0.scaleStep == nil }.map { entry in
                KeyLine(
                    colour: Color(uiColor: Palette.notFlyableOnMap), short: MapTimeLine.short(entry.words),
                    whole: entry.words
                )
            }
            return (steps, veil)
        }
        return ([], nil)
    }

    /// The regions' five levels, the worst first as the key's steps are,
    /// each with its name, and its fuller words for behind the "i".
    private var levels: [KeyLine] {
        guard !measured, what == .thermals else { return [] }
        return regionLevels.reversed().map { one in
            KeyLine(colour: one.level.colorOnMap, short: one.name, whole: "\(one.name): \(one.words)")
        }
    }

    // MARK: What the pilot has to be told

    /// What the data as it stands asks to be said: never folded away.
    @ViewBuilder private var states: some View {
        if what == .rain {
            switch radar.state {
            case .none(let words):
                // A Venti without a rain radar: said plainly, and nothing
                // to try again.
                Text(words)
                    .font(.ventiType(.label))
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityIdentifier("radar-none")
            case .failed(let words):
                CardProblem(words: words, identifier: "radar-problem", tryAgain: tryRadarAgain)
            case .loading where cells.isEmpty:
                asking("Asking Venti for the rain radar.", identifier: "radar-asking")
            default:
                EmptyView()
            }
            if measured, let caution = radar.caution {
                CardCaution(words: caution)
                    .accessibilityIdentifier("radar-stale")
            }
            if measured, let problem = radar.pictureProblem {
                CardProblem(words: problem, identifier: "radar-picture-problem") {
                    Task {
                        await radar.tryPictureAgain()
                    }
                }
            }
        }
        if !measured {
            if case .failed(let words) = thermals.state, thermals.map == nil {
                CardProblem(words: words, identifier: "forecast-problem", tryAgain: tryForecastAgain)
            } else if case .failed(let words) = thermals.hoursState {
                CardProblem(words: words, identifier: "forecast-hours-problem", tryAgain: tryHoursAgain)
            } else if thermals.map == nil, thermals.state == .loading {
                asking("Asking Venti for the forecast.", identifier: "forecast-asking")
            } else if thermals.map != nil, day == nil {
                CardCaution(words: "The forecast map has no day from today on.")
                    .accessibilityIdentifier("forecast-problem")
            } else if day != nil, !isInView, thermals.hoursState != .loading {
                Text("No forecast for this part of the map.")
                    .font(.ventiType(.label))
                    .foregroundStyle(.venti(.textSecondary))
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityIdentifier("forecast-none-here")
            }
        }
    }

    private func asking(_ words: String, identifier: String) -> some View {
        HStack(spacing: VentiSpace.s2) {
            ProgressView()
            Text(words)
                .font(.ventiType(.label))
                .fixedSize(horizontal: false, vertical: true)
        }
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier(identifier)
    }

    // MARK: Behind the "i"

    /// The notes of what is shown.
    private var notes: [String] {
        if measured, what == .rain, let view = radar.view {
            return [radar.label] + view.layers.reversed().map(\.what) + [view.note]
                + radar.drawnLayers.compactMap(\.credit).map(\.text)
        }
        if measured {
            return []
        }
        if showsAnHour, let hours, let day {
            return [
                "\(hours.model), run of \(Format.dayAndTime(unixSeconds: hours.runUnixS))", choice?.note ?? "", day.note,
                hours.whole ? "" : hours.note,
            ]
        }
        if let day {
            return ["\(day.model), run of \(Format.dayAndTime(unixSeconds: day.runUnixS))", day.note]
        }
        return []
    }

    /// The whole key, a line each, and the notes.
    private var folded: some View {
        let lines = levels + key.steps.reversed() + (key.veil.map { [$0] } ?? [])
        return VStack(alignment: .leading, spacing: VentiSpace.s3) {
            if !lines.isEmpty {
                VStack(alignment: .leading, spacing: VentiSpace.s1) {
                    ForEach(lines, id: \.whole) { line in
                        MapKeyLine(colour: line.colour, words: line.whole)
                    }
                }
                .accessibilityElement(children: .contain)
                .accessibilityLabel("Key")
                .accessibilityIdentifier("what-key-whole")
            }
            let said = notes.filter { !$0.isEmpty }
            if !said.isEmpty {
                VStack(alignment: .leading, spacing: VentiSpace.s1) {
                    ForEach(said, id: \.self) { note in
                        Text(note)
                    }
                }
                .font(.ventiType(.caption))
                .foregroundStyle(.venti(.textSecondary))
                .fixedSize(horizontal: false, vertical: true)
                .accessibilityElement(children: .combine)
                .accessibilityIdentifier("what-notes")
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// The pack's scale legend (`.v-scale`), short: the steps side by side in
/// their colours, the least first, its two ends named under it, and the
/// veil (what the radar did not see, a day that is not flyable) beside it.
private struct KeyStrip: View {
    let steps: [WhatPanel.KeyLine]
    let veil: WhatPanel.KeyLine?

    var body: some View {
        HStack(alignment: .top, spacing: VentiSpace.s3) {
            VStack(alignment: .leading, spacing: VentiSpace.s1) {
                HStack(spacing: 2) {
                    ForEach(steps, id: \.whole) { step in
                        bar(step.colour)
                            .frame(maxWidth: .infinity)
                    }
                }
                HStack(alignment: .top, spacing: VentiSpace.s2) {
                    Text(steps.first?.short ?? "")
                    Spacer(minLength: 0)
                    if steps.count > 1 {
                        Text(steps.last?.short ?? "")
                            .multilineTextAlignment(.trailing)
                    }
                }
            }
            if let veil {
                VStack(alignment: .leading, spacing: VentiSpace.s1) {
                    bar(veil.colour)
                        .frame(width: VentiSpace.s6)
                    Text(veil.short)
                }
            }
        }
        .font(.ventiType(.caption))
        .foregroundStyle(.venti(.textSecondary))
        .fixedSize(horizontal: false, vertical: true)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Key")
        .accessibilityValue(([steps.first, steps.last].compactMap { $0?.whole } + [veil?.whole].compactMap { $0 }).joined(separator: "; "))
        .accessibilityIdentifier("what-key")
    }

    private func bar(_ colour: Color) -> some View {
        Rectangle()
            .fill(colour)
            .overlay {
                Rectangle()
                    .strokeBorder(.venti(.borderStrong), lineWidth: 1)
            }
            .frame(height: VentiSpace.s3)
    }
}

/// The key of the forecast by region: five columns, a bar of each level's
/// colour over its name, the worst first.
private struct LevelsStrip: View {
    let levels: [WhatPanel.KeyLine]

    var body: some View {
        HStack(alignment: .top, spacing: 2) {
            ForEach(levels, id: \.whole) { level in
                VStack(alignment: .leading, spacing: VentiSpace.s1) {
                    Rectangle()
                        .fill(level.colour)
                        .overlay {
                            Rectangle()
                                .strokeBorder(.venti(.borderStrong), lineWidth: 1)
                        }
                        .frame(height: VentiSpace.s3)
                    Text(level.short)
                        .lineLimit(2)
                        .minimumScaleFactor(0.8)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .font(.ventiType(.caption))
        .foregroundStyle(.venti(.textSecondary))
        .fixedSize(horizontal: false, vertical: true)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Key")
        .accessibilityValue(levels.map(\.short).joined(separator: ", "))
        .accessibilityIdentifier("what-levels")
    }
}

/// The key of the take-offs' fans while the hour's wind colours them
/// (`SiteFans`): the fan in the good colour where the wind fits, hollow in
/// the muted ink where it does not, as the map draws them. On one line
/// where it fits, else the two on lines of their own.
private struct FansKey: View {
    var body: some View {
        ViewThatFits(in: .horizontal) {
            HStack(spacing: VentiSpace.s2) {
                Text("Take-off:")
                swatch(fits: true)
                Text("wind fits")
                swatch(fits: false)
                Text("doesn't fit")
            }
            VStack(alignment: .leading, spacing: VentiSpace.s1) {
                Text("Take-off:")
                HStack(spacing: VentiSpace.s2) {
                    swatch(fits: true)
                    Text("wind fits")
                }
                HStack(spacing: VentiSpace.s2) {
                    swatch(fits: false)
                    Text("doesn't fit")
                }
            }
        }
        .font(.ventiType(.caption))
        .foregroundStyle(.venti(.textSecondary))
        .lineLimit(1)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Take-off")
        .accessibilityValue("Filled: wind fits; hollow: doesn't fit")
        .accessibilityIdentifier("what-fans-key")
    }

    /// A fan's wedge, pointing up, in the colours the map gives it.
    private func swatch(fits: Bool) -> some View {
        FanWedge()
            .fill(Color(uiColor: fits ? Palette.goodOnMap : Palette.haloOnMap))
            .overlay {
                FanWedge()
                    .stroke(
                        Color(uiColor: fits ? Palette.haloOnMap : Palette.notFlyableOnMap),
                        style: StrokeStyle(lineWidth: fits ? 1 : 1.5, lineJoin: .round)
                    )
            }
            .frame(width: VentiSpace.s4, height: VentiSpace.s3)
            .accessibilityHidden(true)
    }
}

/// A quarter of a circle from its middle at the foot, pointing up: the
/// shape of a fan's span in its key.
private struct FanWedge: Shape {
    func path(in rect: CGRect) -> Path {
        let centre = CGPoint(x: rect.midX, y: rect.maxY)
        let radius = min(rect.height, rect.width / (2 * sin(.pi / 4)))
        var path = Path()
        path.move(to: centre)
        path.addArc(
            center: centre, radius: radius, startAngle: .degrees(-135), endAngle: .degrees(-45), clockwise: false
        )
        path.closeSubpath()
        return path
    }
}

/// The map's one line of time (`MapTimeLine`), in the manner of the pack's
/// hour strip (`.v-hours`): a cell a stop, the hour in it, the chosen
/// cell in the primary colour. What was measured is narrower; over the
/// line stand the days' names and "Now". Longer than the panel, it scrolls
/// sideways and keeps the chosen cell in view. Each cell is a button with
/// its words as its label ("Today, 13:00"), the chosen one selected.
struct MapTimeStrip: View {
    let cells: [MapTimeCell]
    let chosen: Int
    let choose: (Int) -> Void

    /// How wide a cell is drawn: a finger's half for an hour, narrower for
    /// a radar's picture.
    private static let wide = VentiSpace.s6
    private static let narrow = VentiSpace.s4
    private static let gap: CGFloat = 2
    /// The space before a day that begins after other cells.
    private static let dayGap = VentiSpace.s2

    var body: some View {
        ScrollViewReader { proxy in
            ScrollView(.horizontal, showsIndicators: false) {
                let rooms = MapTimeLine.rooms(
                    cells, wide: Self.wide, narrow: Self.narrow, gap: Self.gap, dayGap: Self.dayGap
                )
                HStack(alignment: .bottom, spacing: Self.gap) {
                    ForEach(cells.indices, id: \.self) { index in
                        cell(index, room: rooms[index])
                            .padding(.leading, cells[index].startsDay ? Self.dayGap : 0)
                            .id(index)
                    }
                }
            }
            .onAppear {
                proxy.scrollTo(chosen, anchor: .center)
            }
            .onChange(of: chosen) { _, index in
                withAnimation(.easeOut(duration: 0.2)) {
                    proxy.scrollTo(index, anchor: .center)
                }
            }
            .onChange(of: cells.count) { _, _ in
                proxy.scrollTo(chosen, anchor: .center)
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Time")
        .accessibilityIdentifier("map-time")
    }

    private func cell(_ index: Int, room: CGFloat?) -> some View {
        let cell = cells[index]
        let isChosen = index == chosen
        let width = cell.isNarrow ? Self.narrow : Self.wide
        return VStack(alignment: .leading, spacing: 2) {
            // The day's name, a full hour or "Now" over the cell, running
            // on over the cells after it up to the next word, never into
            // it (`MapTimeLine.rooms`).
            Text(cell.over ?? " ")
                .font(.ventiType(.caption))
                .fontWeight(cell.isNow ? .semibold : .regular)
                .foregroundStyle(.venti(cell.isNow ? .text : .textSecondary))
                .lineLimit(1)
                .truncationMode(.tail)
                .fixedSize(horizontal: room == nil, vertical: false)
                .frame(width: room, alignment: .leading)
                .frame(width: width, alignment: .leading)
                .accessibilityHidden(true)
            Button {
                choose(index)
            } label: {
                Text(cell.short)
                    .font(.ventiType(.label))
                    .monospacedDigit()
                    .lineLimit(1)
                    .minimumScaleFactor(0.6)
                    .frame(width: width, height: VentiSpace.s6)
                    .foregroundStyle(.venti(isChosen ? .onPrimary : .textSecondary))
                    .background(
                        .venti(isChosen ? .primary : (cell.stop.isMeasured ? .border : .surfaceRaised)),
                        in: .rect(cornerRadius: VentiRadius.s)
                    )
                    .overlay(alignment: .leading) {
                        // A rule where "Now" stands.
                        if cell.isNow {
                            Rectangle()
                                .fill(.venti(.text))
                                .frame(width: 2)
                        }
                    }
                    .frame(minHeight: VentiSize.touch)
                    .contentShape(.rect)
            }
            .buttonStyle(.plain)
            .accessibilityLabel(cell.label)
            .accessibilityAddTraits(isChosen ? .isSelected : [])
            .accessibilityIdentifier("map-time-cell")
        }
    }
}
