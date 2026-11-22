import SwiftUI

/// The home-screen widget's face: the take-off's name, today and the next
/// days with the status Today gives each, and when they were last brought
/// up to date. It draws what the app wrote (`TakeoffSnapshot`) and decides
/// nothing: every word of a day is the core's.
///
/// It is the widget's whole content, with its own margin, so that a test
/// can draw it without the home screen.
struct TakeoffDaysView: View {
    /// The two sizes the widget comes in.
    enum Size {
        case small, medium

        /// As many days as the size has room for at the usual text size;
        /// fewer are shown where a larger text leaves room for fewer.
        var mostDays: Int {
            switch self {
            case .small: 2
            case .medium: 4
            }
        }
    }

    let shown: TakeoffSnapshot.Shown
    let size: Size

    var body: some View {
        VStack(alignment: .leading, spacing: VentiSpace.s1) {
            switch shown {
            case .noTakeoff:
                // The sign Today's rows have for a take-off.
                Image("icon-site")
                    .resizable()
                    .frame(width: VentiSize.icon, height: VentiSize.icon)
                    .foregroundStyle(.venti(.textSecondary))
                    .accessibilityHidden(true)
                sentence(WidgetWords.keepATakeoff)
            case .noForecast(let name):
                title(name)
                sentence(WidgetWords.noForecastYet)
            case .outOfDate(let name):
                title(name)
                sentence(WidgetWords.outOfDate)
            case .days(let name, let days, let updatedUnixS):
                title(name)
                // As many days as fit whole, the most first.
                ViewThatFits(in: .vertical) {
                    ForEach((1...max(1, min(size.mostDays, days.count))).reversed(), id: \.self) { count in
                        list(Array(days.prefix(count)))
                    }
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
                Text(WidgetWords.updated(Format.time(unixSeconds: updatedUnixS)))
                    .font(.ventiType(.caption))
                    .foregroundStyle(.venti(.textSecondary))
                    .lineLimit(1)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .padding(VentiSpace.s3)
    }

    private func title(_ name: String) -> some View {
        Text(name)
            .font(.ventiType(.heading))
            .foregroundStyle(.venti(.text))
            .lineLimit(1)
            .minimumScaleFactor(0.8)
            .accessibilityAddTraits(.isHeader)
    }

    /// A state in a sentence, in place of the days.
    private func sentence(_ words: String) -> some View {
        Text(words)
            .font(.ventiType(.label))
            .foregroundStyle(.venti(.textSecondary))
            .minimumScaleFactor(0.8)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }

    @ViewBuilder private func list(_ days: [TakeoffSnapshot.Day]) -> some View {
        switch size {
        case .small:
            // The day over its status: side by side, "Tomorrow" and
            // "Not flyable" have no room.
            VStack(alignment: .leading, spacing: VentiSpace.s1) {
                ForEach(days) { day in
                    VStack(alignment: .leading, spacing: 2) {
                        Text(day.name)
                            .font(.ventiType(.caption))
                            .foregroundStyle(.venti(.textSecondary))
                            .lineLimit(1)
                        VentiStatus(day)
                            .fixedSize()
                    }
                    .accessibilityElement(children: .combine)
                }
            }
        case .medium:
            // A line a day: its name, its line where that says more than
            // the status (the flying hours), and the status.
            VStack(alignment: .leading, spacing: 2) {
                ForEach(days) { day in
                    HStack(spacing: VentiSpace.s2) {
                        // As wide as the widest of the names, so that the
                        // lines begin under each other.
                        ZStack(alignment: .leading) {
                            ForEach(days) { other in
                                name(other).hidden()
                            }
                            name(day)
                        }
                        .layoutPriority(1)
                        // Whole or not at all: a line cut short would
                        // name half an hour.
                        ViewThatFits(in: .horizontal) {
                            Text(day.line == day.status ? "" : day.line)
                                .font(.ventiType(.label))
                                .foregroundStyle(.venti(.textSecondary))
                                .lineLimit(1)
                                .fixedSize()
                            Color.clear
                                .frame(height: 0)
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                        VentiStatus(day)
                            .fixedSize()
                            .layoutPriority(1)
                    }
                    .accessibilityElement(children: .combine)
                }
            }
        }
    }

    private func name(_ day: TakeoffSnapshot.Day) -> some View {
        Text(day.name)
            .font(.ventiType(.label))
            .foregroundStyle(.venti(.text))
            .lineLimit(1)
            .fixedSize()
    }
}

extension VentiStatus {
    /// The pack's status for a day of the widget: the core's word and the
    /// colour of its verdict, as Today has them, in the lower form.
    init(_ day: TakeoffSnapshot.Day) {
        let kind: Kind =
            switch day.verdict {
            case .flyable: .good
            case .marginal: .caution
            case .notFlyable: .danger
            }
        self.init(kind: kind, word: day.status, isCompact: true)
    }
}
