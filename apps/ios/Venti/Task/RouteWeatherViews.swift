import SwiftUI
import VentiCore

/// The weather along the route, under its legs and score: folded until
/// the pilot opens it, then the day, the start, the speed and the glide,
/// and what the core says of the route, of each place and of each leg.
struct RouteWeatherSection: View {
    @Environment(ThermalMapModel.self) private var thermals
    let weather: RouteWeatherModel
    let places: [Coordinate]

    /// The pilot's glide, as the settings keep it.
    @AppStorage(Preferences.glideRatio) private var pilotsGlide = Preferences.defaultGlideRatio
    @State private var isOpen = false
    @State private var chosenDate: String?
    /// The pilot's day by Venti's clock, for when the thermal map has none.
    @State private var today: String?
    /// The start, as a minute of the day; set when the section first opens.
    @State private var start: Int?
    @State private var speed = RouteWeatherWords.speedKmh
    @State private var glide: Double?

    /// What the forecast is asked for: a new route, day or start asks
    /// again (the model answers from what it holds where it can).
    private struct Asked: Equatable {
        let isOpen: Bool
        let places: [Coordinate]
        let day: String?
        let start: Int?
    }

    var body: some View {
        let days = thermals.map?.days ?? []
        let day = days.first(where: { $0.date == chosenDate })?.date ?? days.first?.date ?? today
        VStack(alignment: .leading, spacing: VentiSpace.s2) {
            toggle
            if isOpen {
                if days.count > 1 {
                    VentiSegmented(options: days.map { (value: $0.date, name: $0.name) }, selection: dayChoice(day))
                        .accessibilityIdentifier("route-weather-days")
                }
                settings
                state(day: day)
                if let day, let start, let startUnixS = RouteWeatherWords.startUnixS(date: day, minuteOfDay: start) {
                    along(
                        weather.along(
                            places, startUnixS: startUnixS, speedKmh: speed,
                            glideRatio: glide ?? shownGlide
                        )
                    )
                }
            }
        }
        // The days are the thermal map's: asked for here if the map tab
        // has not yet.
        .task(id: isOpen) {
            guard isOpen else { return }
            if start == nil {
                // A known clock: Venti's, else this iPhone's.
                let venti = await thermals.clock()
                let now = venti == 0 ? Int64(Date().timeIntervalSince1970) : venti
                today = RouteWeatherWords.date(of: now)
                start = RouteWeatherWords.nextFullHour(after: now)
            }
            if thermals.map == nil, thermals.state != .loading {
                await thermals.load()
            }
        }
        .task(id: Asked(isOpen: isOpen, places: places, day: day, start: start)) {
            guard isOpen, let day, start != nil else { return }
            await weather.show(places, on: day)
        }
    }

    /// The glide the section opens with: the pilot's, within the steps.
    private var shownGlide: Double {
        min(max(pilotsGlide, RouteWeatherWords.glideRatios.lowerBound), RouteWeatherWords.glideRatios.upperBound)
    }

    private func dayChoice(_ day: String?) -> Binding<String> {
        Binding(get: { day ?? "" }, set: { chosenDate = $0 })
    }

    /// The heading, which opens and folds the section.
    private var toggle: some View {
        Button {
            isOpen.toggle()
        } label: {
            HStack(spacing: VentiSpace.s1) {
                Text(RouteWeatherWords.title)
                    .font(.venti(.subheadline, .semibold))
                    .foregroundStyle(.venti(.text))
                Spacer(minLength: 0)
                Image(systemName: isOpen ? "chevron.up" : "chevron.down")
                    .font(.ventiType(.caption))
                    .foregroundStyle(.venti(.link))
                    .accessibilityHidden(true)
            }
            .frame(maxWidth: .infinity, minHeight: VentiSize.touch, alignment: .leading)
            .contentShape(.rect)
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(.isHeader)
        .accessibilityValue(isOpen ? "Shown" : "Hidden")
        .accessibilityIdentifier("route-weather-toggle")
    }

    /// The start, the speed and the glide, each a step at a time.
    private var settings: some View {
        VStack(alignment: .leading, spacing: 0) {
            if start != nil {
                Stepper(value: startChoice, in: RouteWeatherWords.starts, step: RouteWeatherWords.startStepMinutes) {
                    LabeledContent("Start", value: Format.clock(minuteOfDay: start ?? 0))
                }
                .accessibilityIdentifier("route-weather-start")
            }
            Stepper(value: $speed, in: RouteWeatherWords.speedsKmh, step: RouteWeatherWords.speedStepKmh) {
                LabeledContent("Speed over the ground", value: "\(Format.count(speed)) km/h")
            }
            .accessibilityIdentifier("route-weather-speed")
            Stepper(value: glideChoice, in: RouteWeatherWords.glideRatios, step: RouteWeatherWords.glideStep) {
                LabeledContent("Glide", value: Format.glideRatio(glide ?? shownGlide))
            }
            .accessibilityIdentifier("route-weather-glide")
        }
        .font(.venti(.subheadline))
        .monospacedDigit()
    }

    private var startChoice: Binding<Int> {
        Binding(get: { start ?? 0 }, set: { start = $0 })
    }

    private var glideChoice: Binding<Double> {
        Binding(get: { glide ?? shownGlide }, set: { glide = $0 })
    }

    /// While the server is asked, and what went wrong, as the app's other
    /// forecast cards say it.
    @ViewBuilder private func state(day: String?) -> some View {
        if weather.isAsking {
            HStack(spacing: VentiSpace.s2) {
                ProgressView()
                Text(RouteWeatherWords.asking)
                    .font(.ventiType(.label))
                    .fixedSize(horizontal: false, vertical: true)
            }
            .accessibilityElement(children: .combine)
            .accessibilityIdentifier("route-weather-asking")
        } else if let problem = weather.problem {
            if problem == RouteWeatherWords.noServer {
                CardCaution(words: problem)
                    .accessibilityIdentifier("route-weather-problem")
            } else {
                CardProblem(words: problem, identifier: "route-weather-problem") {
                    weather.forgetForecast()
                    guard let day else { return }
                    Task {
                        await weather.show(places, on: day)
                    }
                }
            }
        }
    }

    /// What the core says of the route: the whole, then place by place
    /// with the leg after each, then the model and whose data it is.
    private func along(_ along: RouteAlong) -> some View {
        VStack(alignment: .leading, spacing: VentiSpace.s2) {
            Text(along.words)
                .font(.venti(.subheadline))
                .foregroundStyle(.venti(.text))
                .fixedSize(horizontal: false, vertical: true)
                .accessibilityIdentifier("route-weather-words")
            ForEach(Array(along.points.enumerated()), id: \.offset) { index, point in
                VStack(alignment: .leading, spacing: VentiSpace.s1) {
                    Text(point.words)
                        .font(.venti(.subheadline))
                        .foregroundStyle(.venti(.text))
                        .fixedSize(horizontal: false, vertical: true)
                        .accessibilityIdentifier("route-weather-point")
                    if !point.departureWords.isEmpty {
                        Text(point.departureWords)
                            .font(.venti(.footnote))
                            .foregroundStyle(.venti(.textSecondary))
                            .fixedSize(horizontal: false, vertical: true)
                            .accessibilityIdentifier("route-weather-departure")
                    }
                    if index < along.legs.count {
                        Text(along.legs[index].words)
                            .font(.venti(.footnote))
                            .foregroundStyle(.venti(.textSecondary))
                            .fixedSize(horizontal: false, vertical: true)
                            .accessibilityIdentifier("route-weather-leg")
                    }
                }
            }
            let credits = [along.model].compactMap { $0 } + along.credits.map(\.text)
            if !credits.isEmpty {
                Text(credits.joined(separator: "\n"))
                    .font(.ventiType(.caption))
                    .foregroundStyle(.venti(.textMuted))
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityIdentifier("route-weather-credits")
            }
        }
    }
}
