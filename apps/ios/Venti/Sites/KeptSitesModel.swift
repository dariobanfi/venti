import Foundation
import Observation
import VentiCore

/// The pilot's own sites as the screens see them.
///
/// The core keeps them on the iPhone, says which they are and what to say
/// with them, and chooses Today's take-offs with them. This class asks,
/// off the main thread because the core reads and writes files, and keeps
/// the latest list for the screens to draw. With an account the list is
/// on the account too: `AccountModel.keepsInStep` ties the two.
@MainActor
@Observable
final class KeptSitesModel {
    /// The pilot's own sites as last read. `nil` until they were read.
    private(set) var list: KeptSites?
    /// Why the list could not be read, in the core's words.
    private(set) var problem: String?
    /// Counts what the pilot kept and forgot: Today reads its take-offs
    /// again when it has changed.
    private(set) var changes = 0

    /// Called after the pilot kept or forgot a site: with an account the
    /// change goes to it.
    @ObservationIgnored var onChanged: () -> Void = {}

    @ObservationIgnored private let core: Core
    /// The place the list was last read from, for reading it again.
    @ObservationIgnored private var place: Coordinate?

    init(core: Core) {
        self.core = core
    }

    /// Whether a site is one of the pilot's own.
    func isKept(_ id: String) async -> Bool {
        let core = self.core
        return await Task.detached { (try? core.isSiteKept(id: id)) ?? false }.value
    }

    /// The date the source's data was of when a kept site was last seen,
    /// for keeping it again as it is; `nil` for a site that is not kept.
    func asOf(_ id: String) async -> Int64? {
        let core = self.core
        return await Task.detached {
            (try? core.keptSites(from: nil))?.sites.first { $0.site.id == id }?.asOfUnixS
        }.value
    }

    /// Marks a site as one of the pilot's own, with the credits and the
    /// date of the answer it came from.
    func keep(_ site: SiteView, credits: [Credit], asOfUnixS: Int64?) async throws {
        let core = self.core
        try await Task.detached {
            try core.keepSite(site: site, credits: credits, asOfUnixS: asOfUnixS)
        }.value
        await changed()
    }

    /// Unmarks a site.
    func forget(_ id: String) async throws {
        let core = self.core
        try await Task.detached { try core.forgetSite(id: id) }.value
        await changed()
    }

    /// After a keep or a forget: the list and its note are read again,
    /// the screens are told, and so is whoever sends it to the account.
    private func changed() async {
        await load(from: place)
        changes += 1
        onChanged()
    }

    /// After the account's sync, a sign-in or a sign-out: the note says
    /// anew where the list is kept, and where the sync brought or took
    /// sites on this iPhone the screens read them again.
    func followedTheAccount(changedHere: Bool) async {
        await load(from: place)
        if changedHere {
            changes += 1
        }
    }

    /// Reads the list: nearest first from a place, by name without one.
    func load(from place: Coordinate?) async {
        let core = self.core
        self.place = place
        do {
            list = try await Task.detached { try core.keptSites(from: place) }.value
            problem = nil
        } catch {
            problem = error.words
        }
    }

    /// Today's take-offs: the pilot's own first, then those of the
    /// server's answer for the area around the place; without an answer
    /// the pilot's own alone.
    func todayTakeoffs(place: Coordinate?, near: SitesMap?) async throws -> TodayTakeoffs {
        let core = self.core
        return try await Task.detached { try core.todayTakeoffs(place: place, near: near) }.value
    }
}
