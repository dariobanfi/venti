import CoreLocation
import MapLibre
import SwiftUI
import VentiCore

/// The form for a flying site of Venti's own layer: a new one, or one to
/// correct. What it asks, in which order, under which titles, what is
/// wrong with an entry and what giving a site means are the core's
/// (`siteForm`, `siteDraftCheck`); the form shows them and edits a draft.
struct SiteEditForm: View {
    /// Called with the server's answer once the site is written, before
    /// the form closes.
    let onWritten: (SiteWritten) -> Void

    @Environment(AccountModel.self) private var account
    @Environment(WindModel.self) private var wind
    @Environment(SitesModel.self) private var sites
    @Environment(\.dismiss) private var dismiss

    /// The draft as the form began with it, or as the site stood when it
    /// was last looked at: what the pilot's entries are told from.
    @State private var begun: SiteDraftView
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
    /// The site as it stands now, after someone else changed it.
    @State private var standsNow: SiteView?
    /// A site opened from the form: the one in the way, or this one as it
    /// stands now.
    @State private var opened: SiteView?
    @State private var isMoving = false

    private let form = siteForm()

    init(draft: SiteDraftView, onWritten: @escaping (SiteWritten) -> Void) {
        self.onWritten = onWritten
        _begun = State(initialValue: draft)
        _draft = State(initialValue: draft)
        _height = State(initialValue: draft.heightText)
    }

    private var isNew: Bool {
        draft.siteId == nil
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
                            Text(field.title)
                                .textCase(nil)
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
                        if let standsNow {
                            Text("What you entered is still in the form. What you left as it was is as the site stands now.")
                                .foregroundStyle(.venti(.textSecondary))
                                .accessibilityIdentifier("site-carried")
                            Button("Show how it stands now") {
                                opened = standsNow
                            }
                            .accessibilityIdentifier("site-stands-now")
                        }
                        // What giving a site means, with the licence, over
                        // the button that gives it.
                        Text(form.giving)
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
                            Text(isNew ? form.sendNew : form.sendCorrection)
                                .frame(maxWidth: .infinity)
                        }
                        .buttonStyle(.venti(.primary))
                        .disabled(isSending)
                        .listRowSeparator(.hidden)
                        .accessibilityIdentifier("site-send")
                    } footer: {
                        Text(form.note)
                    }
                }
            }
            .scrollDismissesKeyboard(.interactively)
            .navigationTitle(isNew ? form.titleNew : form.titleCorrect)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") {
                        dismiss()
                    }
                }
            }
            .navigationDestination(isPresented: $isMoving) {
                SitePlaceMover(place: draft.position, words: form.placeWords) { place in
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
                Button("Move") {
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

    /// What stands under a field: its faults, the core's line of help, and
    /// how far a text is over what the field takes.
    private func under(_ field: SiteFormField, faults: [SiteDraftFault]) -> some View {
        VStack(alignment: .leading, spacing: VentiSpace.s1) {
            ForEach(faults.filter { $0.field == field.field }, id: \.words) { fault in
                Text(fault.words)
                    .foregroundStyle(.venti(.danger))
                    .accessibilityIdentifier("site-fault")
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
                inTheWay = (error as? SiteEditing.Refused)?.inTheWay
                await lookAgain(with: editing)
            }
        }
    }

    /// After a refused correction: how the site stands now. Where it is at
    /// another revision than the draft's, someone else changed it; the
    /// form then goes on from that state, with the pilot's entries kept.
    private func lookAgain(with editing: SiteEditing) async {
        guard let id = draft.siteId, let page = try? await editing.page(id),
              let revision = page.site.byPilots?.revision, revision != draft.revision,
              let now = try? siteDraftOf(site: page.site)
        else { return }
        draft = draft.carried(from: begun, onto: now)
        begun = now
        height = draft.heightText
        standsNow = page.site
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
    let onUse: (Coordinate) -> Void

    @Environment(OfflineMapsModel.self) private var maps
    @State private var place: Coordinate
    private let start: Coordinate

    init(place: Coordinate, words: String, onUse: @escaping (Coordinate) -> Void) {
        self.words = words
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
                    Text("Use this place")
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.venti(.primary))
                .accessibilityIdentifier("site-place-use")
            }
            .padding(VentiSpace.s4)
            .ventiFloating(in: .rect(cornerRadius: VentiRadius.m))
            .padding(VentiSpace.s4)
        }
        .navigationTitle("Move")
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

/// What a signed-out pilot is told where a site would be added or
/// corrected, with the way to the account.
struct SiteSignIn: View {
    var body: some View {
        VentiList {
            Section {
                Text("A site is added and corrected from your account: it is shown with your handle, so that everyone sees who added what.")
                    .accessibilityIdentifier("site-sign-in-why")
                NavigationLink("Sign in") {
                    AccountScreen()
                }
                .accessibilityIdentifier("site-sign-in")
            }
        }
        .navigationTitle(siteForm().titleNew)
        .navigationBarTitleDisplayMode(.inline)
    }
}

/// On the page of a site that pilots made: who added it and when it was
/// last changed, with the notice, in the core's words, and what a pilot
/// can do with it: correct it, remove it, read its history, report it.
struct SiteByPilotsSection: View {
    let site: SiteView
    let by: SiteByPilots
    /// Whether the site was removed from the map on this page.
    let isRemoved: Bool
    /// Called when the site was written anew: corrected, or put back.
    let onWritten: (SiteWritten) -> Void
    /// Called with the core's line when the site was removed.
    let onRemoved: (String) -> Void

    @Environment(AccountModel.self) private var account
    @Environment(WindModel.self) private var wind
    @Environment(SitesModel.self) private var sites

    @State private var correcting: DraftToCorrect?
    @State private var isAskingRemoval = false
    @State private var why = ""
    @State private var isWorking = false
    @State private var problem: String?
    @State private var reporting: Reported?

    private let form = siteForm()

    var body: some View {
        Section {
            if !isRemoved {
                if wind.takesSitesFromPilots, account.isSignedIn {
                    Button(form.titleCorrect, systemImage: "pencil") {
                        correct()
                    }
                    .accessibilityIdentifier("site-correct")
                    Button("Remove this site", systemImage: "trash", role: .destructive) {
                        why = ""
                        isAskingRemoval = true
                    }
                    .disabled(isWorking)
                    .accessibilityIdentifier("site-remove")
                } else if wind.takesSitesFromPilots {
                    NavigationLink {
                        SiteSignIn()
                    } label: {
                        Label(form.titleCorrect, systemImage: "pencil")
                    }
                    .accessibilityIdentifier("site-correct")
                }
            }
            NavigationLink {
                SiteHistoryScreen(id: site.id, name: site.name, onWritten: onWritten)
            } label: {
                Label("History", systemImage: "clock.arrow.circlepath")
            }
            .accessibilityIdentifier("site-history")
            // On one row, not on the section: a section hands what hangs
            // on it to every row.
            .sheet(item: $correcting) { correcting in
                SiteEditForm(draft: correcting.draft, onWritten: onWritten)
            }
            .sheet(item: $reporting) { reported in
                ReportSheet(reported: reported)
            }
            .alert("Remove this site?", isPresented: $isAskingRemoval) {
                WhyField(why: $why)
                Button("Remove this site", role: .destructive) {
                    remove()
                }
                Button("Cancel", role: .cancel) {}
            } message: {
                Text(WhyField.help)
            }
            if !isRemoved {
                Button("Report this site", systemImage: "exclamationmark.bubble") {
                    reporting = .site(site.id)
                }
                .accessibilityIdentifier("site-report")
            }
            if let problem {
                Text(problem)
                    .foregroundStyle(.venti(.danger))
                    .accessibilityIdentifier("site-edit-problem")
            }
        } header: {
            Text("Added by pilots")
        } footer: {
            Text([by.addedInWords, by.changedInWords, by.note].filter { !$0.isEmpty }.joined(separator: "\n"))
                .accessibilityIdentifier("site-by-pilots")
        }
    }

    /// The form on the site as the page has it; the core refuses a site
    /// that is not of Venti's own layer, in its words.
    private func correct() {
        do {
            correcting = DraftToCorrect(draft: try siteDraftOf(site: site))
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
                onRemoved(words)
            } catch {
                problem = SiteEditing.words(for: error)
            }
        }
    }
}

/// A draft to open the form on.
private struct DraftToCorrect: Identifiable {
    let draft: SiteDraftView
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
/// with what it changed under it; the way to undo the last change, and to
/// put an earlier revision back, which is also how a removed site comes
/// back.
struct SiteHistoryScreen: View {
    let id: String
    let name: String
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
    @State private var puttingBack: UInt32?
    @State private var why = ""
    @State private var isWorking = false
    @State private var reporting: Reported?
    /// A revision's site as it stood, opened.
    @State private var stood: SiteView?

    private var canWrite: Bool {
        wind.takesSitesFromPilots && account.isSignedIn
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
                if canWrite, let undo = history.undoRevision {
                    Section {
                        Button(history.undoWords, systemImage: "arrow.uturn.backward") {
                            ask(undo)
                        }
                        .disabled(isWorking)
                        .accessibilityIdentifier("site-undo")
                    }
                }
                Section {
                    ForEach(history.revisions, id: \.number) { revision in
                        row(revision)
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
        .navigationTitle("History")
        .navigationBarTitleDisplayMode(.inline)
        .task {
            await load()
        }
        .sheet(item: $reporting) { reported in
            ReportSheet(reported: reported)
        }
        .alert(
            "Put this revision back?",
            isPresented: Binding(
                get: { puttingBack != nil },
                set: { isPresented in
                    if !isPresented {
                        puttingBack = nil
                    }
                }
            ),
            presenting: puttingBack
        ) { revision in
            WhyField(why: $why)
            Button("Put back") {
                putBack(revision)
            }
            Button("Cancel", role: .cancel) {}
        } message: { _ in
            Text(WhyField.help)
        }
    }

    /// One revision: the core's line, what it changed, and what can be
    /// done with it.
    private func row(_ revision: SiteRevisionView) -> some View {
        VStack(alignment: .leading, spacing: VentiSpace.s1) {
            Text(revision.words)
            ForEach(revision.changes, id: \.self) { change in
                Text(change)
                    .font(.venti(.footnote))
                    .foregroundStyle(.venti(.textSecondary))
                    .accessibilityIdentifier("site-revision-change")
            }
            HStack(spacing: VentiSpace.s4) {
                if canWrite, revision.canBePutBack {
                    Button("Put back") {
                        ask(revision.number)
                    }
                    .disabled(isWorking)
                    .accessibilityIdentifier("site-put-back")
                }
                Button("Report") {
                    reporting = .siteRevision(id, revision: revision.number)
                }
                .foregroundStyle(.venti(.textSecondary))
            }
            .font(.venti(.footnote))
            .buttonStyle(.borderless)
            .padding(.top, VentiSpace.s1)
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("site-revision")
    }

    private func ask(_ revision: UInt32) {
        why = ""
        puttingBack = revision
    }

    /// The whole history, newest first. The core's pages run from the
    /// first revision on, and it names the revision that undoes the last
    /// change on the last of them: all pages are read, a site's history
    /// being short, and the list is the pages on end, turned round.
    private func load() async {
        do {
            let editing = SiteEditing(account: account, wind: wind)
            var page = try await editing.history(id)
            var revisions = page.revisions
            var pages = 1
            while let after = page.nextAfter, pages < Self.pagesAtMost {
                page = try await editing.history(id, after: after)
                revisions += page.revisions
                pages += 1
            }
            page.revisions = revisions.reversed()
            history = page
            problem = nil
        } catch {
            problem = SiteEditing.words(for: error)
        }
    }

    /// How many pages of a history are read before the list stops.
    private static let pagesAtMost = 20

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
/// before, and from its bar the way to add a site there, where Venti takes
/// sites from pilots. Once a site is added the sheet shows its page.
struct PressedPlaceSheet: View {
    let place: Coordinate
    /// Whether a site's sign lay under the finger: nothing is added on
    /// top of one.
    let isOnASite: Bool
    @Binding var detent: PresentationDetent
    let done: () -> Void

    @Environment(AccountModel.self) private var account
    @Environment(WindModel.self) private var wind

    @State private var isAdding = false
    @State private var needsSignIn = false
    @State private var added: SiteWritten?

    private let form = siteForm()

    var body: some View {
        NavigationStack {
            Group {
                if let added {
                    SiteScreen(site: added.site, said: added.words)
                } else {
                    ForecastScreen(place: place, title: Format.place(lat: place.lat, lon: place.lon))
                        // Over the forecast, where it is seen at half
                        // height too: a bar has room for a sign alone.
                        .safeAreaInset(edge: .top, spacing: 0) {
                            if wind.takesSitesFromPilots, !isOnASite {
                                Button {
                                    if account.isSignedIn {
                                        isAdding = true
                                    } else {
                                        detent = .large
                                        needsSignIn = true
                                    }
                                } label: {
                                    Label("\(form.titleNew) here", systemImage: "plus")
                                        .frame(maxWidth: .infinity)
                                }
                                .buttonStyle(.venti(.quiet))
                                .padding(.horizontal, VentiSpace.s4)
                                .padding(.bottom, VentiSpace.s2)
                                .background(.venti(.bg))
                                .accessibilityIdentifier("add-site-here")
                            }
                        }
                        .navigationDestination(isPresented: $needsSignIn) {
                            SiteSignIn()
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
            SiteEditForm(draft: siteDraftNew(place: place)) { written in
                detent = .large
                added = written
            }
        }
    }
}
