import Foundation
import Observation

/// The bridge for pilots who fly with a vario or another app: a folder the
/// pilot names once, which Venti looks into again whenever it comes to the
/// front, bringing in the flight files that are new there. The pilot flies
/// as before, and the flight is in Venti's logbook by itself.
///
/// The iPhone lets an app read a folder outside its own only through a
/// bookmark of the folder the pilot picked, which is kept here across
/// launches. A file is known by where it lies in the folder, its size and
/// when it was last changed, so only what is new or changed is read; the
/// core's logbook recognises a flight it has in any case.
///
/// A bookmark goes stale when the folder was moved or renamed, or the
/// system was restored: it still leads to the folder then, and a fresh one
/// is kept in its place without a word. Where it leads nowhere any more
/// (the folder is gone, or the app that held it), or cannot be made anew,
/// the folder is lost: `isLost` is set, nothing is brought in, and the
/// logbook says so and offers to name the folder again.
@MainActor
@Observable
final class WatchedFolder {
    /// How a folder is remembered across launches: the system's
    /// bookmarks, or a test's own.
    struct Bookmarks: Sendable {
        var make: @Sendable (URL) throws -> Data
        /// Where a bookmark leads, and whether it is stale: still good
        /// this once, and to be made anew.
        var resolve: @Sendable (Data) throws -> (folder: URL, isStale: Bool)

        static let system = Bookmarks(
            make: { folder in
                try folder.bookmarkData(options: .minimalBookmark, includingResourceValuesForKeys: nil, relativeTo: nil)
            },
            resolve: { bookmark in
                var isStale = false
                let folder = try URL(resolvingBookmarkData: bookmark, bookmarkDataIsStale: &isStale)
                return (folder, isStale)
            }
        )
    }

    /// The menu's and the notice's words. They are the app's: the core
    /// has none for the watched folder. The first three are Android's,
    /// word for word.
    enum Words {
        static let watch = "Bring in new flights from a folder"

        static func watchAnother(_ name: String) -> String {
            "Bring in from another folder than \(name)"
        }

        static func stopWatching(_ name: String) -> String {
            "Stop bringing in from \(name)"
        }

        /// The title of the notice when a picked folder cannot be kept.
        static let cannotBeKept = "The folder cannot be kept"

        /// Said in the logbook while the folder is lost.
        static func lost(_ name: String) -> String {
            "Venti can no longer find the folder \(name), so no new flights come in from it. Choose it again, or stop bringing in from it."
        }

        static let chooseAgain = "Choose the folder again"
    }

    /// The folder's name as the pilot knows it, while one is watched.
    private(set) var name: String?
    /// Whether the watched folder can no longer be found: its bookmark
    /// leads nowhere, or went stale and could not be made anew.
    private(set) var isLost = false

    @ObservationIgnored private let logbook: LogbookModel
    @ObservationIgnored private let store: UserDefaults
    @ObservationIgnored private let bookmarks: Bookmarks
    /// Set while the folder is being looked into: coming to the front
    /// twice in a row looks once.
    @ObservationIgnored private var isLooking = false

    init(logbook: LogbookModel, store: UserDefaults = .standard, bookmarks: Bookmarks = .system) {
        self.logbook = logbook
        self.store = store
        self.bookmarks = bookmarks
        if store.data(forKey: Preferences.watchedFolder) != nil {
            name = store.string(forKey: Preferences.watchedFolderName)
        }
    }

    /// Watches a folder the pilot chose, in place of any other, and brings
    /// in what is in it. Throws where the system gives no bookmark for the
    /// folder; the folder watched before stays then.
    @discardableResult
    func watch(_ folder: URL) async throws -> ImportReport? {
        // A folder picked in the Files app is readable only inside this
        // pair; one in the app's own container needs none and says false.
        let isScoped = folder.startAccessingSecurityScopedResource()
        defer {
            if isScoped {
                folder.stopAccessingSecurityScopedResource()
            }
        }
        let bookmark = try bookmarks.make(folder)
        stop()
        let shown = (try? folder.resourceValues(forKeys: [.localizedNameKey]))?.localizedName
        store.set(bookmark, forKey: Preferences.watchedFolder)
        store.set(shown ?? folder.lastPathComponent, forKey: Preferences.watchedFolderName)
        name = shown ?? folder.lastPathComponent
        return await bringInNew()
    }

    /// No folder is watched any more; the flights brought in stay.
    func stop() {
        Self.forget(in: store)
        name = nil
        isLost = false
    }

    /// Takes the watched folder out of a store, before a model is made
    /// over it: a UI test starts with none, whatever an earlier one left.
    static func forget(in store: UserDefaults) {
        store.removeObject(forKey: Preferences.watchedFolder)
        store.removeObject(forKey: Preferences.watchedFolderName)
        store.removeObject(forKey: Preferences.watchedSeen)
    }

    /// Brings in the flight files of the watched folder that are new since
    /// it was last looked into. `nil` when nothing was new, no folder is
    /// watched, an import is under way, or the folder cannot be read just
    /// now: then it is tried again next time, and the pilot is told
    /// nothing. A folder that is lost is said by `isLost`.
    @discardableResult
    func bringInNew() async -> ImportReport? {
        guard let bookmark = store.data(forKey: Preferences.watchedFolder) else { return nil }
        guard !isLooking, logbook.importProgress == nil else { return nil }
        isLooking = true
        defer { isLooking = false }

        let folder: URL
        let isStale: Bool
        do {
            (folder, isStale) = try bookmarks.resolve(bookmark)
        } catch {
            isLost = true
            return nil
        }
        let isScoped = folder.startAccessingSecurityScopedResource()
        defer {
            if isScoped {
                folder.stopAccessingSecurityScopedResource()
            }
        }
        if isStale {
            // Still good this once: a fresh one in its place, or the
            // folder is lost the next time.
            guard let fresh = try? bookmarks.make(folder) else {
                isLost = true
                return nil
            }
            store.set(fresh, forKey: Preferences.watchedFolder)
        }
        let listed = await Task.detached(priority: .utility) {
            WatchedFolder.flightFiles(in: folder)
        }.value
        // Found, if not read: a drive that is not connected, a folder
        // whose files another app is still writing.
        isLost = false
        guard let found = listed else { return nil }

        let seen = Set(store.stringArray(forKey: Preferences.watchedSeen) ?? [])
        let new = found.filter { !seen.contains($0.key) }
        guard !new.isEmpty else { return nil }
        let report = await logbook.importFiles(at: new.map(\.file))
        // What was tried is seen, a file that is not a flight included: it
        // is tried again once it has changed.
        let known = Self.known(seen: seen, new: new.map(\.key), keep: found.map(\.key))
        store.set(known.sorted(), forKey: Preferences.watchedSeen)
        return report
    }

    /// The keys to keep as seen: those still in the folder, and the new ones.
    nonisolated static func known(seen: Set<String>, new: [String], keep: [String]) -> Set<String> {
        seen.intersection(keep).union(new)
    }

    /// Every flight file in a folder and in the folders inside it, oldest
    /// name first, each with what tells it apart from what it was: where
    /// it lies in the folder, its size and when it was last changed.
    /// `nil` where the folder cannot be read.
    nonisolated static func flightFiles(in folder: URL) -> [(key: String, file: URL)]? {
        guard (try? FileManager.default.contentsOfDirectory(at: folder, includingPropertiesForKeys: nil)) != nil
        else { return nil }
        let depth = folder.standardizedFileURL.pathComponents.count
        return LogbookModel.flightFiles(in: [folder]).map { file in
            let values = try? file.resourceValues(forKeys: [.fileSizeKey, .contentModificationDateKey])
            let path = file.standardizedFileURL.pathComponents.dropFirst(depth).joined(separator: "/")
            let changed = Int64(values?.contentModificationDate?.timeIntervalSince1970 ?? 0)
            return (key: "\(path)|\(values?.fileSize ?? 0)|\(changed)", file: file)
        }
    }
}
