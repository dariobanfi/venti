import CoreLocation
import MapLibre
import SwiftUI
import VentiCore

/// The form for a flying site of Venti's own two tiers: a new one, or one
/// to correct. What it asks, in which order, under which titles, what is
/// wrong with an entry and what giving a site means are the core's
/// (`siteForm`, `siteDraftCheck`, and the tier's title, sentence, notice
/// and button: `SiteAdding` for a new site, `SiteCorrecting` for a
/// correction); the form shows them and edits a draft.
struct SiteEditForm: View {
    /// Called with the server's answer once the site is written, before
    /// the form closes.
    let onWritten: (SiteWritten) -> Void

    @Environment(AccountModel.self) private var account
    @Environment(WindModel.self) private var wind
    @Environment(SitesModel.self) private var sites
    @Environment(\.dismiss) private var dismiss

    /// The form's title, what giving means, the tier's notice and the
    /// button, as the core says them for the tier the site goes to or is
    /// of.
    private let says: Says
    /// The site the draft was made from, or as it stood when it was last
    /// looked at: what the core tells the pilot's entries from. Absent for
    /// a new site.
    @State private var base: SiteView?
    @State private var draft: SiteDraftView
    @State private var height: String
    @State private var comment = ""
    /// Faults are shown from the first try to send.
    @State private var hasTried = false
    @State private var isSending = false
    /// The refusal, in the core's words.
    @State private var problem: String?
    /// The site that stands at the place already, to open instead.
    @State private var inTheWay: SiteInTheWay?
    /// What the core made of the draft after someone else changed the
    /// site: its words, what the others changed, and the fields both did.
    @State private var rebased: SiteDraftRebased?
    /// A site opened from the form: the one in the way, or this one as it
    /// stands now.
    @State private var opened: SiteView?
    @State private var isMoving = false

    private let form = siteForm()
    private let words = siteWords()

    /// What the form says for a tier: the core's words, of `SiteAdding`
    /// or of `SiteCorrecting`.
    private struct Says {
        let title: String
        let giving: String
        let note: String
        let send: String
    }

    /// A new site at a place, with the words of the tier it goes to.
    init(adding: SiteAdding, at place: Coordinate, onWritten: @escaping (SiteWritten) -> Void) {
        self.init(adding: adding, draft: siteDraftNew(place: place), onWritten: onWritten)
    }

    /// A new site on a draft the core made: "Add this place" of a place
    /// where pilots take off, at the pilot's own take-off.
    init(adding: SiteAdding, draft: SiteDraftView, onWritten: @escaping (SiteWritten) -> Void) {
        self.init(
            draft: draft, of: nil,
            says: Says(title: adding.title, giving: adding.giving, note: adding.note, send: adding.sendWords),
            onWritten: onWritten
        )
    }

    /// A correction of a site, on the draft the core made of it, with
    /// the words the core has for correcting a site of its tier.
    init(
        draft: SiteDraftView, of site: SiteView, correcting: SiteCorrecting,
        onWritten: @escaping (SiteWritten) -> Void
    ) {
        self.init(
            draft: draft, of: site,
            says: Says(
                title: correcting.title, giving: correcting.giving, note: correcting.note,
                send: correcting.sendWords
            ),
            onWritten: onWritten
        )
    }

    private init(draft: SiteDraftView, of site: SiteView?, says: Says, onWritten: @escaping (SiteWritten) -> Void) {
        self.says = says
        self.onWritten = onWritten
        _base = State(initialValue: site)
        _draft = State(initialValue: draft)
        _height = State(initialValue: draft.heightText)
    }

    var body: some View {
        let faults = hasTried ? siteDraftCheck(draft: draft, comment: comment) : []
        NavigationStack {
            ScrollViewReader { list in
                VentiList {
                    ForEach(form.fields, id: \.field) { field in
                        Section {
                            entry(field)
                                .id(field.field)
                        } header: {
                            // A field both the pilot and someone else
                            // changed is marked as one with a fault is:
                            // sending again overwrites theirs.
                            if rebased?.clashes.contains(field.field) == true {
                                Label {
                                    Text(field.title)
                                } icon: {
                                    Image("icon-caution")
                                        .resizable()
                                        .frame(width: VentiSpace.s4, height: VentiSpace.s4)
                                }
                                .foregroundStyle(.venti(.danger))
                                .textCase(nil)
                                .accessibilityIdentifier("site-clash")
                            } else {
                                Text(field.title)
                                    .textCase(nil)
                            }
                        } footer: {
                            under(field, faults: faults)
                        }
                    }
                    Section {
                        if let problem {
                            Text(problem)
                                .foregroundStyle(.venti(.danger))
                                .accessibilityIdentifier("site-problem")
                        }
                        if let inTheWay {
                            Button(inTheWay.words) {
                                open(inTheWay.id)
                            }
                            .accessibilityIdentifier("site-in-the-way")
                        }
                        // Someone else changed the site meanwhile: the
                        // core's words for what became of the draft, what
                        // the others changed, and the site as it stands.
                        if let rebased {
                            Text(rebased.words)
                                .foregroundStyle(.venti(.textSecondary))
                                .accessibilityIdentifier("site-carried")
                            ForEach(rebased.theirs, id: \.self) { change in
                                Text(change)
                                    .font(.venti(.footnote))
                                    .foregroundStyle(.venti(.textSecondary))
                                    .accessibilityIdentifier("site-theirs")
                            }
                            Button(words.showNow) {
                                opened = base
                            }
                            .accessibilityIdentifier("site-stands-now")
                        }
                        // What giving a site means, with the licence, over
                        // the button that gives it.
                        Text(says.giving)
                            .font(.venti(.footnote))
                            .foregroundStyle(.venti(.textSecondary))
                            .accessibilityIdentifier("site-giving")
                        if let licence = URL(string: form.licenceUrl) {
                            Link(destination: licence) {
                                Label(form.licence, systemImage: "arrow.up.right")
                            }
                            .accessibilityIdentifier("site-licence")
                        }
                        // It stays tappable with a fault in the form: the
                        // tap is how the pilot learns what is missing.
                        Button {
                            send(scrolling: list)
                        } label: {
                            Text(says.send)
                                .frame(maxWidth: .infinity)
                        }
                        .buttonStyle(.venti(.primary))
                        .disabled(isSending)
                        .listRowSeparator(.hidden)
                        .accessibilityIdentifier("site-send")
                    } footer: {
                        Text(says.note)
                            .accessibilityIdentifier("site-form-notice")
                    }
                }
            }
            .scrollDismissesKeyboard(.interactively)
            .navigationTitle(says.title)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button(words.cancel) {
                        dismiss()
                    }
                }
            }
            .navigationDestination(isPresented: $isMoving) {
                SitePlaceMover(place: draft.position, words: form.placeWords, names: words) { place in
                    draft = siteDraftMoved(draft: draft, place: place)
                    isMoving = false
                }
            }
            .navigationDestination(item: $opened) { site in
                SiteScreen(site: site)
            }
        }
    }

    /// What one field of the form is entered with.
    @ViewBuilder
    private func entry(_ field: SiteFormField) -> some View {
        switch field.field {
        case .name:
            TextField(field.title, text: $draft.name)
                .accessibilityIdentifier("site-name")
        case .place:
            LabeledContent {
                Button(words.movePlace) {
                    isMoving = true
                }
                .buttonStyle(.borderless)
                .accessibilityIdentifier("site-move")
            } label: {
                Text(Format.place(lat: draft.position.lat, lon: draft.position.lon))
                    .monospacedDigit()
                    .accessibilityIdentifier("site-place")
            }
        case .height:
            TextField(field.title, text: heightEntered)
                .keyboardType(.numbersAndPunctuation)
                .accessibilityIdentifier("site-height")
        case .uses:
            SiteChoices(
                choices: form.uses.map { ($0.words, draft.uses.contains($0.kind)) },
                identifier: "site-use", widthAtLeast: 2 * VentiSize.touch + VentiSpace.s5
            ) { index in
                draft.toggleUse(form.uses[index].kind)
            }
        case .wind:
            SiteChoices(
                choices: form.winds.map { ($0.words, draft.windFromDeg.contains($0.fromDeg)) },
                identifier: "site-wind"
            ) { index in
                draft.toggleWind(form.winds[index].fromDeg)
            }
            let words = siteDraftWindInWords(windFromDeg: draft.windFromDeg)
            if !words.isEmpty {
                Text(words)
                    .foregroundStyle(.venti(.textSecondary))
                    .accessibilityIdentifier("site-wind-words")
            }
        case .description:
            TextField(field.title, text: $draft.description, axis: .vertical)
                .lineLimit(3...10)
                .accessibilityIdentifier("site-description-field")
        case .fee:
            TextField(field.title, text: $draft.fee)
        case .website:
            TextField(field.title, text: $draft.website)
                .keyboardType(.URL)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
        case .noFlightTime:
            TextField(field.title, text: $draft.noFlightTime)
        case .guests:
            VentiSegmented(options: form.guests.map { ($0.value, $0.words) }, selection: $draft.guestGuidelines)
                .listRowSeparator(.hidden)
        case .comment:
            TextField(field.title, text: $comment)
                .accessibilityIdentifier("site-comment")
        }
    }

    /// What stands under a field: its faults, the core's sentence where
    /// the pilot and someone else both changed it, the core's line of
    /// help, and how far a text is over what the field takes.
    private func under(_ field: SiteFormField, faults: [SiteDraftFault]) -> some View {
        VStack(alignment: .leading, spacing: VentiSpace.s1) {
            ForEach(faults.filter { $0.field == field.field }, id: \.words) { fault in
                Text(fault.words)
                    .foregroundStyle(.venti(.danger))
                    .accessibilityIdentifier("site-fault")
            }
            ForEach((rebased?.clashesInWords ?? []).filter { $0.field == field.field }, id: \.words) { clash in
                Text(clash.words)
                    .foregroundStyle(.venti(.danger))
                    .accessibilityIdentifier("site-clash-words")
            }
            if !field.help.isEmpty {
                Text(field.help)
            }
            if let most = field.atMost, let count = length(of: field.field), count > Int(most) {
                Text(verbatim: "\(count) of \(most) characters")
                    .foregroundStyle(.venti(.danger))
            }
        }
    }

    /// How long the text of a field is, for the fields that hold text.
    private func length(of field: SiteField) -> Int? {
        switch field {
        case .name: draft.name.count
        case .description: draft.description.count
        case .fee: draft.fee.count
        case .website: draft.website.count
        case .noFlightTime: draft.noFlightTime.count
        case .comment: comment.count
        case .place, .height, .uses, .wind, .guests: nil
        }
    }

    /// The height as typed, handed to the draft with every key.
    private var heightEntered: Binding<String> {
        Binding {
            height
        } set: { text in
            height = text
            draft.setHeight(text)
        }
    }

    private func send(scrolling list: ScrollViewProxy) {
        hasTried = true
        guard !isSending else { return }
        // The core's check first: what it finds stands at the fields.
        if let fault = siteDraftCheck(draft: draft, comment: comment).first {
            withAnimation {
                list.scrollTo(fault.field, anchor: .top)
            }
            return
        }
        isSending = true
        problem = nil
        inTheWay = nil
        let editing = SiteEditing(account: account, wind: wind)
        Task {
            defer { isSending = false }
            do {
                let written = try await editing.write(draft, comment: comment)
                await sites.refresh()
                onWritten(written)
                dismiss()
            } catch {
                problem = SiteEditing.words(for: error)
                let refused = error as? SiteEditing.Refused
                inTheWay = refused?.inTheWay
                if refused?.wasChanged == true {
                    await goOnFromHowItStands(with: editing)
                }
            }
        }
    }

    /// After a correction that was refused because someone else changed
    /// the site: the site is read as it stands, and the core carries the
    /// draft onto it (`siteDraftRebased`), which is the one merge. The
    /// form goes on from there, and nothing the pilot entered is lost.
    private func goOnFromHowItStands(with editing: SiteEditing) async {
        guard let base, let page = try? await editing.page(base.id),
              let rebased = try? siteDraftRebased(base: base, edited: draft, now: page.site)
        else { return }
        draft = rebased.draft
        self.base = page.site
        height = draft.heightText
        self.rebased = rebased
        await sites.refresh()
    }

    /// Opens the site in the way, read from Venti as it stands.
    private func open(_ id: String) {
        let editing = SiteEditing(account: account, wind: wind)
        Task {
            do {
                opened = try await editing.page(id).site
            } catch {
                problem = SiteEditing.words(for: error)
            }
        }
    }
}

extension View {
    /// For the words of a button that are a sentence of the core's and
    /// not a verb ("I took off here and it was good", "Add a place pilots
    /// take off from"): the pack's button holds one line, and on a narrow
    /// iPhone with larger text that cut the sentence short. Here it goes
    /// on to a second and third line, with the pack's space above and
    /// below, which changes nothing while one line holds it.
    func sentenceInAButton() -> some View {
        lineLimit(3)
            .multilineTextAlignment(.center)
            .padding(.vertical, VentiSpace.s2)
    }
}

/// Choices to tick, several at once: the uses of a site and the sixteen
/// points of the compass, as pills in the manner of the pack's segmented
/// control, in as many rows as the width asks for.
private struct SiteChoices: View {
    let choices: [(words: String, isOn: Bool)]
    let identifier: String
    /// How wide a pill is at least: room for the longest of the words.
    var widthAtLeast = VentiSize.touch + VentiSpace.s5
    let toggle: (Int) -> Void

    var body: some View {
        LazyVGrid(
            columns: [GridItem(.adaptive(minimum: widthAtLeast), spacing: VentiSpace.s2)],
            alignment: .leading, spacing: VentiSpace.s2
        ) {
            ForEach(choices.indices, id: \.self) { index in
                let choice = choices[index]
                Button {
                    toggle(index)
                } label: {
                    Text(choice.words)
                        .font(.ventiType(.label))
                        .lineLimit(1)
                        .minimumScaleFactor(0.8)
                        .padding(.horizontal, VentiSpace.s1)
                        .frame(maxWidth: .infinity, minHeight: VentiSize.touch - VentiSpace.s2)
                        .foregroundStyle(.venti(choice.isOn ? .onPrimary : .textSecondary))
                        .background(.venti(choice.isOn ? .primary : .surfaceRaised), in: .capsule)
                        .contentShape(.capsule)
                }
                .buttonStyle(.plain)
                .accessibilityAddTraits(choice.isOn ? .isSelected : [])
                .accessibilityIdentifier("\(identifier)-\(choice.words)")
            }
        }
        .padding(.vertical, VentiSpace.s1)
    }
}

/// Moving a site's place: the map under a mark that stays in its middle,
/// the place in numbers, and "Use this place".
private struct SitePlaceMover: View {
    let words: String
    /// The core's names of the buttons.
    let names: SiteWords
    let onUse: (Coordinate) -> Void

    @Environment(OfflineMapsModel.self) private var maps
    @State private var place: Coordinate
    private let start: Coordinate

    init(place: Coordinate, words: String, names: SiteWords, onUse: @escaping (Coordinate) -> Void) {
        self.words = words
        self.names = names
        self.onUse = onUse
        start = place
        _place = State(initialValue: place)
    }

    var body: some View {
        SitePlaceMap(start: start, source: maps.browsingSource(at: nil)) { middle in
            place = middle
        }
        .ignoresSafeArea(edges: .bottom)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Map")
        .accessibilityIdentifier("site-place-map")
        .overlay {
            // The mark's middle is the place.
            SiteGlyph(kind: .takeoff)
                .allowsHitTesting(false)
        }
        .overlay(alignment: .bottom) {
            VStack(alignment: .leading, spacing: VentiSpace.s2) {
                Text(words)
                    .font(.venti(.footnote))
                    .foregroundStyle(.venti(.textSecondary))
                Text(Format.place(lat: place.lat, lon: place.lon))
                    .monospacedDigit()
                    .accessibilityIdentifier("site-place-moved")
                Button {
                    onUse(place)
                } label: {
                    Text(names.usePlace)
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.venti(.primary))
                .accessibilityIdentifier("site-place-use")
            }
            .padding(VentiSpace.s4)
            .ventiFloating(in: .rect(cornerRadius: VentiRadius.m))
            .padding(VentiSpace.s4)
        }
        .navigationTitle(names.movePlace)
        .navigationBarTitleDisplayMode(.inline)
    }
}

/// The map a site's place is moved on: it says where its middle is
/// whenever it comes to rest.
private struct SitePlaceMap: UIViewRepresentable {
    let start: Coordinate
    var source: MapStyle.Source = .standard
    let onMiddle: (Coordinate) -> Void

    func makeCoordinator() -> Coordinator {
        Coordinator()
    }

    func makeUIView(context: Context) -> MLNMapView {
        let map = MapSetup.makeMapView(from: source)
        map.delegate = context.coordinator
        map.setCenter(
            CLLocationCoordinate2D(latitude: start.lat, longitude: start.lon), zoomLevel: 15, animated: false
        )
        return map
    }

    func updateUIView(_ map: MLNMapView, context: Context) {
        context.coordinator.onMiddle = onMiddle
    }

    @MainActor
    final class Coordinator: NSObject, @preconcurrency MLNMapViewDelegate {
        var onMiddle: (Coordinate) -> Void = { _ in }

        func mapView(_ mapView: MLNMapView, regionDidChangeAnimated animated: Bool) {
            let middle = mapView.centerCoordinate
            onMiddle(Coordinate(lat: middle.latitude, lon: middle.longitude))
        }
    }
}

/// On the page of a site of Venti's own: how many pilots say they took
/// off here, which opens their lines, and "I took off here and it was
/// good" where the core offers it; once said, the core's sentence with
/// the ways to change the day and to take it back. The app shows what is
/// offered and holds no rule about who may.
struct SiteConfirmSection: View {
    let site: SiteView
    let by: SiteByPilots
    let offers: SiteOffers
    /// The site's confirmations, once they are read.
    let confirmations: SiteConfirmations?
    /// Called with the site as it stands with the confirmation counted.
    let onConfirmed: (SiteWritten) -> Void
    /// Called with the core's line once the confirmation is taken back.
    let onWithdrawn: (String) -> Void

    @Environment(AccountModel.self) private var account
    @Environment(WindModel.self) private var wind

    @State private var isConfirming = false
    @State private var isAskingWithdrawal = false
    @State private var isWorking = false
    @State private var problem: String?

    private let words = siteWords()

    var body: some View {
        // The count as the list has it, and until it is read as the site
        // itself says it.
        let count = confirmations?.words ?? by.confirmedInWords
        // Empty where nobody takes off, as the core reads the list for
        // this site.
        let nobody = confirmations?.nobodyWords ?? ""
        // No section without a row in it.
        let rows = [count, nobody, offers.confirmedWords, offers.confirm.words, offers.withdraw.words, problem ?? ""]
        if rows.contains(where: { !$0.isEmpty }) {
            Section {
                if !count.isEmpty {
                    NavigationLink {
                        SiteConfirmationsScreen(site: site, read: confirmations)
                    } label: {
                        Text(count)
                    }
                    .accessibilityIdentifier("site-confirmations")
                } else if !nobody.isEmpty {
                    Text(nobody)
                        .foregroundStyle(.venti(.textSecondary))
                        .accessibilityIdentifier("site-confirmations-none")
                }
                if offers.hasConfirmed {
                    Label(offers.confirmedWords, systemImage: "checkmark.circle")
                        .accessibilityIdentifier("site-confirmed")
                }
                if offers.confirm.isOffered {
                    Button {
                        isConfirming = true
                    } label: {
                        Text(offers.confirm.words)
                            .sentenceInAButton()
                            .frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.venti(offers.hasConfirmed ? .quiet : .primary))
                    .disabled(isWorking)
                    .listRowSeparator(.hidden)
                    .accessibilityIdentifier("site-confirm")
                    // On one row, not on the section: a section hands what
                    // hangs on it to every row.
                    .sheet(isPresented: $isConfirming) {
                        SiteConfirmSheet(site: site, onConfirmed: onConfirmed)
                    }
                    .alert(words.withdrawQuestion, isPresented: $isAskingWithdrawal) {
                        Button(words.withdraw, role: .destructive) {
                            withdraw()
                        }
                        Button(words.cancel, role: .cancel) {}
                    }
                }
                if offers.withdraw.isOffered {
                    Button(offers.withdraw.words, role: .destructive) {
                        isAskingWithdrawal = true
                    }
                    .disabled(isWorking)
                    .accessibilityIdentifier("site-withdraw")
                }
                if let problem {
                    Text(problem)
                        .foregroundStyle(.venti(.danger))
                        .accessibilityIdentifier("site-confirm-problem")
                }
            }
        }
    }

    private func withdraw() {
        guard !isWorking else { return }
        isWorking = true
        problem = nil
        let editing = SiteEditing(account: account, wind: wind)
        Task {
            defer { isWorking = false }
            do {
                onWithdrawn(try await editing.withdraw(site.id))
            } catch {
                problem = SiteEditing.words(for: error)
            }
        }
    }
}

/// The sheet a pilot says "I took off here and it was good" in: the
/// day as a menu with today chosen, an optional line, the pilot's own
/// flights of that day from here where the logbook has any, and the
/// button. Every word and the days are the core's (`siteConfirmForm`).
struct SiteConfirmSheet: View {
    let site: SiteView
    let onConfirmed: (SiteWritten) -> Void

    @Environment(AccountModel.self) private var account
    @Environment(WindModel.self) private var wind
    @Environment(\.dismiss) private var dismiss

    @State private var form: SiteConfirmForm?
    /// Today as the form was made for it: by Venti's clock, on this
    /// iPhone's calendar.
    @State private var today = ""
    @State private var day = ""
    @State private var note = ""
    @State private var flights: [SiteConfirmFlight] = []
    @State private var flight: String?
    @State private var isSending = false
    @State private var problem: String?

    private let words = siteWords()

    var body: some View {
        NavigationStack {
            VentiList {
                if let form {
                    Section {
                        Text(form.about)
                            .foregroundStyle(.venti(.textSecondary))
                            .accessibilityIdentifier("site-confirm-about")
                    }
                    Section {
                        Picker(form.dayTitle, selection: $day) {
                            ForEach(form.days, id: \.day) { day in
                                Text(day.words).tag(day.day)
                            }
                        }
                        .pickerStyle(.menu)
                        .accessibilityIdentifier("site-confirm-day")
                    }
                    Section {
                        TextField(form.noteTitle, text: $note, axis: .vertical)
                            .lineLimit(1...4)
                            .accessibilityIdentifier("site-confirm-note")
                    } header: {
                        Text(form.noteTitle)
                            .textCase(nil)
                    } footer: {
                        VStack(alignment: .leading, spacing: VentiSpace.s1) {
                            Text(form.noteHelp)
                            if note.count > Int(form.noteAtMost) {
                                Text(verbatim: "\(note.count) of \(form.noteAtMost) characters")
                                    .foregroundStyle(.venti(.danger))
                            }
                        }
                    }
                    // Only where the logbook has a flight of that day
                    // that began here.
                    if !flights.isEmpty {
                        Section {
                            Picker(form.flightTitle, selection: $flight) {
                                Text(form.noFlight).tag(String?.none)
                                ForEach(flights, id: \.id) { flight in
                                    Text(flight.words).tag(String?.some(flight.id))
                                }
                            }
                            .pickerStyle(.inline)
                            .labelsHidden()
                            .accessibilityIdentifier("site-confirm-flight")
                        } header: {
                            Text(form.flightTitle)
                                .textCase(nil)
                        } footer: {
                            Text(form.flightHelp)
                        }
                    }
                    Section {
                        if let problem {
                            Text(problem)
                                .foregroundStyle(.venti(.danger))
                                .accessibilityIdentifier("site-confirm-refused")
                        }
                        Button {
                            send()
                        } label: {
                            Text(form.send)
                                .frame(maxWidth: .infinity)
                        }
                        .buttonStyle(.venti(.primary))
                        .disabled(isSending)
                        .listRowSeparator(.hidden)
                        .accessibilityIdentifier("site-confirm-send")
                    }
                } else if let problem {
                    Section {
                        Text(problem)
                            .foregroundStyle(.venti(.danger))
                    }
                }
            }
            .scrollDismissesKeyboard(.interactively)
            .navigationTitle(form?.title ?? "")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button(words.cancel) {
                        dismiss()
                    }
                }
            }
        }
        // At full height: the line is typed over a keyboard, which would
        // leave a sheet at half height no room for the day and the button.
        .task {
            begin()
        }
        .onChange(of: day) { _, day in
            // The pilot's own flights of the day chosen, as the core
            // finds them in the logbook.
            flights = account.flights(toConfirm: site, on: day, utcOffsetS: SiteDay.utcOffsetS(on: day))
            if !flights.contains(where: { $0.id == flight }) {
                flight = nil
            }
        }
    }

    /// The form for today. The day is Venti's, by the clock every age in
    /// the app is counted with, on this iPhone's calendar: Venti counts a
    /// confirmation's day from its own date.
    private func begin() {
        guard form == nil else { return }
        do {
            let today = SiteDay.day(unixSeconds: wind.now)
            let form = try siteConfirmForm(today: today)
            self.today = today
            self.form = form
            day = form.days.first?.day ?? today
        } catch {
            problem = error.words
        }
    }

    private func send() {
        guard !isSending else { return }
        isSending = true
        problem = nil
        let editing = SiteEditing(account: account, wind: wind)
        Task {
            defer { isSending = false }
            do {
                let request = try account.confirmRequest(
                    for: site.id, day: day, today: today, note: note, flight: flight
                )
                onConfirmed(try await editing.confirm(request, day: day))
                dismiss()
            } catch {
                problem = SiteEditing.words(for: error)
            }
        }
    }
}

/// "Pilots who took off here": the count, and each pilot's confirmation
/// in the core's line, the newest day first.
struct SiteConfirmationsScreen: View {
    /// The site: the core reads the list for it.
    let site: SiteView

    @Environment(AccountModel.self) private var account
    @Environment(WindModel.self) private var wind

    @State private var list: SiteConfirmations?
    @State private var problem: String?

    private let words = siteWords()

    /// - Parameter read: the list as the site's page has it, shown until
    ///   it is read anew.
    init(site: SiteView, read: SiteConfirmations?) {
        self.site = site
        _list = State(initialValue: read)
    }

    var body: some View {
        VentiList {
            if let list {
                Section {
                    if list.lines.isEmpty {
                        Text(list.nobodyWords)
                            .foregroundStyle(.venti(.textSecondary))
                    }
                    ForEach(list.lines.indices, id: \.self) { index in
                        Text(list.lines[index].words)
                            .accessibilityIdentifier("site-confirmation")
                    }
                } header: {
                    if !list.words.isEmpty {
                        Text(list.words)
                            .textCase(nil)
                            .accessibilityIdentifier("site-confirmations-words")
                    }
                } footer: {
                    // Where more pilots confirmed than the list holds.
                    if !list.notWholeWords.isEmpty {
                        Text(list.notWholeWords)
                            .accessibilityIdentifier("site-confirmations-not-whole")
                    }
                }
            } else if let problem {
                Section {
                    Text(problem)
                        .foregroundStyle(.venti(.danger))
                        .accessibilityIdentifier("site-confirmations-problem")
                    Button("Try again") {
                        Task {
                            await load()
                        }
                    }
                }
            } else {
                Section {
                    ProgressView()
                        .frame(maxWidth: .infinity)
                }
            }
        }
        .navigationTitle(words.confirmationsTitle)
        .navigationBarTitleDisplayMode(.inline)
        .task {
            await load()
        }
    }

    private func load() async {
        do {
            list = try await SiteEditing(account: account, wind: wind).confirmations(of: site)
            problem = nil
        } catch {
            if list == nil {
                problem = SiteEditing.words(for: error)
            }
        }
    }
}

/// On the page of a site of Venti's own: what the core offers a pilot to
/// do with it (correct it, remove it, read its history, report it), each
/// with the core's words, the core's one line about what is not offered,
/// and who added the site and changed it last.
struct SiteByPilotsSection: View {
    let site: SiteView
    let by: SiteByPilots
    let offers: SiteOffers
    /// Called when the site was written anew: corrected, or put back.
    let onWritten: (SiteWritten) -> Void
    /// Called with the core's line when the site was removed; the entries
    /// wait until it is through.
    let onRemoved: (String) async -> Void

    @Environment(AccountModel.self) private var account
    @Environment(WindModel.self) private var wind
    @Environment(SitesModel.self) private var sites

    @State private var correcting: DraftToCorrect?
    @State private var isAskingRemoval = false
    @State private var why = ""
    @State private var isWorking = false
    @State private var problem: String?
    @State private var reporting: Reported?

    private let words = siteWords()

    var body: some View {
        Section {
            if offers.correct.isOffered {
                Button(offers.correct.words, systemImage: "pencil") {
                    correct()
                }
                .disabled(isWorking)
                .accessibilityIdentifier("site-correct")
            }
            if offers.remove.isOffered {
                Button(offers.remove.words, systemImage: "trash", role: .destructive) {
                    why = ""
                    isAskingRemoval = true
                }
                .disabled(isWorking)
                .accessibilityIdentifier("site-remove")
            }
            if offers.history.isOffered {
                NavigationLink {
                    SiteHistoryScreen(id: site.id, onWritten: onWritten)
                } label: {
                    Label(offers.history.words, systemImage: "clock.arrow.circlepath")
                }
                .accessibilityIdentifier("site-history")
                // On one row, not on the section: a section hands what
                // hangs on it to every row.
                .sheet(item: $correcting) { correcting in
                    SiteEditForm(
                        draft: correcting.draft, of: correcting.site, correcting: correcting.says,
                        onWritten: onWritten
                    )
                }
                .sheet(item: $reporting) { reported in
                    ReportSheet(reported: reported)
                }
                .alert(words.removeQuestion, isPresented: $isAskingRemoval) {
                    WhyField(why: $why)
                    Button(words.remove, role: .destructive) {
                        remove()
                    }
                    Button(words.cancel, role: .cancel) {}
                } message: {
                    Text(words.removeAbout)
                }
            }
            if offers.report.isOffered {
                Button(offers.report.words, systemImage: "exclamationmark.bubble") {
                    reporting = .site(site.id)
                }
                .accessibilityIdentifier("site-report")
            }
            // Why something a pilot might look for is not here; of a
            // removed site, that it was removed.
            if !offers.note.isEmpty {
                Text(offers.note)
                    .foregroundStyle(.venti(.textSecondary))
                    .accessibilityIdentifier("site-offers-note")
            }
            if let problem {
                Text(problem)
                    .foregroundStyle(.venti(.danger))
                    .accessibilityIdentifier("site-edit-problem")
            }
        } footer: {
            Text([by.addedInWords, by.changedInWords].filter { !$0.isEmpty }.joined(separator: "\n"))
                .accessibilityIdentifier("site-by-pilots")
        }
    }

    /// The form on the site as the page has it, with the core's words for
    /// correcting a site of its tier; the core refuses a site that is not
    /// of Venti's own, in its words.
    private func correct() {
        do {
            correcting = DraftToCorrect(
                draft: try siteDraftOf(site: site), site: site, says: try siteCorrecting(site: site)
            )
            problem = nil
        } catch {
            problem = error.words
        }
    }

    private func remove() {
        guard !isWorking else { return }
        isWorking = true
        problem = nil
        let editing = SiteEditing(account: account, wind: wind)
        Task {
            defer { isWorking = false }
            do {
                let words = try await editing.remove(site.id, comment: why)
                await sites.refresh()
                await onRemoved(words)
            } catch {
                problem = SiteEditing.words(for: error)
            }
        }
    }
}

/// A draft to open the form on, with the site it was made from and what
/// the core says on the form of a site of its tier.
private struct DraftToCorrect: Identifiable {
    let draft: SiteDraftView
    let site: SiteView
    let says: SiteCorrecting
    var id: String { "\(draft.siteId ?? "") \(draft.revision ?? 0)" }
}

/// The line about a change, where an alert asks for it: the core's title
/// and help of the form's last field.
private struct WhyField: View {
    @Binding var why: String

    private static let field = siteForm().fields.first { $0.field == .comment }
    static let help = field?.help ?? ""

    var body: some View {
        TextField(Self.field?.title ?? "", text: $why)
            .accessibilityIdentifier("site-why")
    }
}

/// A site's history: its revisions, newest first, each in the core's line
/// with what it changed under it; the way to undo the last change, to put
/// an earlier revision back, to bring a removed site back and to report a
/// revision, each where the core offers it (`siteHistoryOffers`) and under
/// the core's words, with the core's line about what is not offered. It
/// needs the site's identifier and nothing else.
struct SiteHistoryScreen: View {
    /// The site's identifier.
    let id: String
    /// Called when a revision was put back.
    let onWritten: (SiteWritten) -> Void

    @Environment(AccountModel.self) private var account
    @Environment(WindModel.self) private var wind
    @Environment(SitesModel.self) private var sites

    @State private var history: SiteHistory?
    @State private var problem: String?
    /// The core's line about what was last done from here.
    @State private var said: String?
    /// The revision to put back, once the pilot has said why.
    @State private var puttingBack: PuttingBack?
    @State private var why = ""
    @State private var isWorking = false
    @State private var reporting: Reported?

    private let words = siteWords()

    /// A revision to put back, and whether that brings a removed site
    /// back: the question and the button say which.
    private struct PuttingBack: Identifiable {
        let revision: UInt32
        let bringsBack: Bool
        var id: UInt32 { revision }
    }

    var body: some View {
        VentiList {
            if let said {
                Section {
                    Label(said, systemImage: "checkmark.circle")
                        .accessibilityIdentifier("site-history-said")
                }
            }
            if let problem {
                Section {
                    Text(problem)
                        .foregroundStyle(.venti(.danger))
                        .accessibilityIdentifier("site-history-problem")
                    Button("Try again") {
                        Task {
                            await load()
                        }
                    }
                }
            }
            if let history {
                // What the core offers whoever is signed in here in this
                // history, asked once for the list.
                let offers = account.historyOffers(history, takesSitesFromPilots: wind.takesSitesFromPilots)
                // Why something a pilot might look for is not here.
                if let note = offers?.note, !note.isEmpty {
                    Section {
                        Text(note)
                            .foregroundStyle(.venti(.textSecondary))
                            .accessibilityIdentifier("site-history-note")
                    }
                }
                // A removed site: the revision that brings it back as it
                // stood before, under the core's words.
                if let offer = offers?.bringBack, offer.isOffered, let back = history.bringBackRevision {
                    Section {
                        Button(offer.words, systemImage: "arrow.uturn.backward") {
                            ask(back)
                        }
                        .disabled(isWorking)
                        .accessibilityIdentifier("site-bring-back")
                    }
                }
                if let offer = offers?.undo, offer.isOffered, let undo = history.undoRevision {
                    Section {
                        Button(offer.words, systemImage: "arrow.uturn.backward") {
                            ask(undo)
                        }
                        .disabled(isWorking)
                        .accessibilityIdentifier("site-undo")
                    }
                }
                Section {
                    ForEach(history.revisions, id: \.number) { revision in
                        row(revision, offers: offers)
                    }
                } footer: {
                    if let credit = history.credit {
                        Text([credit.notice, credit.text].compactMap { $0 }.joined(separator: "\n"))
                    }
                }
            } else if problem == nil {
                Section {
                    ProgressView()
                        .frame(maxWidth: .infinity)
                }
            }
        }
        .navigationTitle(words.history)
        .navigationBarTitleDisplayMode(.inline)
        .task {
            await load()
        }
        .sheet(item: $reporting) { reported in
            ReportSheet(reported: reported)
        }
        .alert(
            puttingBack?.bringsBack == true ? words.bringBackQuestion : words.putBackQuestion,
            isPresented: Binding(
                get: { puttingBack != nil },
                set: { isPresented in
                    if !isPresented {
                        puttingBack = nil
                    }
                }
            ),
            presenting: puttingBack
        ) { asked in
            WhyField(why: $why)
            Button(asked.bringsBack ? words.bringBack : words.putBack) {
                putBack(asked.revision)
            }
            Button(words.cancel, role: .cancel) {}
        } message: { asked in
            Text(asked.bringsBack ? words.bringBackAbout : words.putBackAbout)
        }
    }

    /// One revision: the core's line, what it changed, and what the core
    /// offers to do with it.
    private func row(_ revision: SiteRevisionView, offers: SiteHistoryOffers?) -> some View {
        VStack(alignment: .leading, spacing: VentiSpace.s1) {
            Text(revision.words)
            ForEach(revision.changes, id: \.self) { change in
                Text(change)
                    .font(.venti(.footnote))
                    .foregroundStyle(.venti(.textSecondary))
                    .accessibilityIdentifier("site-revision-change")
            }
            HStack(spacing: VentiSpace.s4) {
                if offers?.putBack.isOffered == true, revision.canBePutBack {
                    Button(revision.putBackWords) {
                        ask(revision.number)
                    }
                    .disabled(isWorking)
                    .accessibilityIdentifier("site-put-back")
                }
                if let report = offers?.reportRevision, report.isOffered {
                    Button(report.words) {
                        reporting = .siteRevision(id, revision: revision.number)
                    }
                    .foregroundStyle(.venti(.textSecondary))
                }
            }
            .font(.venti(.footnote))
            .buttonStyle(.borderless)
            .padding(.top, VentiSpace.s1)
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("site-revision")
    }

    /// Asks before a revision is put back. On a removed site that brings
    /// the site back, and the question says so.
    private func ask(_ revision: UInt32) {
        why = ""
        puttingBack = PuttingBack(revision: revision, bringsBack: history?.bringBackRevision != nil)
    }

    /// The whole history, newest first. The core's pages run from the
    /// first revision on, and it names the revision that undoes the last
    /// change, or brings a removed site back, on the last of them: all
    /// pages are read, a site's history being short, each with the last
    /// revision of the page before so that the core says what its first
    /// one changed, and the list is the pages on end, turned round.
    private func load() async {
        do {
            var whole = try await SiteEditing(account: account, wind: wind).wholeHistory(id)
            whole.revisions.reverse()
            history = whole
            problem = nil
        } catch {
            problem = SiteEditing.words(for: error)
        }
    }

    private func putBack(_ revision: UInt32) {
        guard !isWorking else { return }
        isWorking = true
        let editing = SiteEditing(account: account, wind: wind)
        Task {
            defer { isWorking = false }
            do {
                let written = try await editing.putBack(id, revision: revision, comment: why)
                said = written.words
                problem = nil
                onWritten(written)
                await sites.refresh()
                await load()
            } catch {
                problem = SiteEditing.words(for: error)
            }
        }
    }
}

/// What a long press on the map opens: the forecast for the place, as
/// before, and from its bar the core's entry for adding a place pilots
/// take off from (`SiteAdding`): shown whenever the core has words for
/// it, opening the account where the core says it leads to signing in,
/// and the form otherwise. A partner of Venti adds on the website: the
/// iPhone shows a partner no entry. Once a site is added the sheet shows
/// its page.
struct PressedPlaceSheet: View {
    let place: Coordinate
    /// Whether a site's sign lay under the finger: nothing is added on
    /// top of one.
    let isOnASite: Bool
    @Binding var detent: PresentationDetent
    /// Shows what a glide reaches from the place, on the map; `nil` where
    /// the map offers it not.
    var reach: (() -> Void)?
    let done: () -> Void

    @Environment(AccountModel.self) private var account
    @Environment(WindModel.self) private var wind

    @State private var isAdding = false
    @State private var needsSignIn = false
    @State private var added: SiteWritten?

    var body: some View {
        let adding = account.adding(takesSitesFromPilots: wind.takesSitesFromPilots)
        NavigationStack {
            Group {
                if let added {
                    SiteScreen(site: added.site, said: added.words)
                } else {
                    ForecastScreen(place: place, title: Format.place(lat: place.lat, lon: place.lon))
                        // Over the forecast, where it is seen at half
                        // height too: a bar has room for a sign alone.
                        .safeAreaInset(edge: .top, spacing: 0) {
                            let offersAdding = adding.map { !isOnASite && !$0.entryWords.isEmpty && !$0.asPartner } ?? false
                            if reach != nil || offersAdding {
                                VStack(spacing: VentiSpace.s2) {
                                    if let reach {
                                        Button(action: reach) {
                                            Label {
                                                Text(GlideReachWords.entry)
                                            } icon: {
                                                Image("icon-glide")
                                            }
                                            .sentenceInAButton()
                                            .frame(maxWidth: .infinity)
                                        }
                                        .buttonStyle(.venti(.quiet))
                                        .accessibilityIdentifier("glide-reach-here")
                                    }
                                    if let adding, offersAdding {
                                        addButton(adding)
                                    }
                                }
                                .padding(.horizontal, VentiSpace.s4)
                                .padding(.bottom, VentiSpace.s2)
                                .background(.venti(.bg))
                            }
                        }
                        .navigationDestination(isPresented: $needsSignIn) {
                            AccountScreen()
                        }
                }
            }
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done", action: done)
                }
            }
        }
        .sheet(isPresented: $isAdding) {
            if let adding {
                SiteEditForm(adding: adding, at: place) { written in
                    detent = .large
                    added = written
                }
            }
        }
    }

    /// "Add a site here", or the way to sign in for it.
    private func addButton(_ adding: SiteAdding) -> some View {
        Button {
            if adding.leadsToSignIn {
                detent = .large
                needsSignIn = true
            } else {
                isAdding = true
            }
        } label: {
            Group {
                if adding.leadsToSignIn {
                    Text(adding.entryWords)
                } else {
                    Label(adding.entryWords, systemImage: "plus")
                }
            }
            .sentenceInAButton()
            .frame(maxWidth: .infinity)
        }
        .buttonStyle(.venti(.quiet))
        // On the button itself: named after its
        // ground, the element reaches under the
        // bar, and a tap on its middle hits that.
        .accessibilityIdentifier(adding.leadsToSignIn ? "add-site-sign-in" : "add-site-here")
    }
}
