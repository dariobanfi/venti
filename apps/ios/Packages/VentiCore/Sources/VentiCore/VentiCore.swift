// The Swift interface of the Rust core is generated into `Generated/` by
// `apps/ios/scripts/build-core.sh`. This file exists so the target has a
// source file under version control; add hand-written Swift here only when
// something cannot be expressed in the Rust interface.

import Foundation

extension CoreError {
    /// What went wrong, in the words the core chose for the pilot.
    ///
    /// The generated `localizedDescription` spells out the case and its
    /// fields, which is for a debugger and not for a screen.
    public var message: String {
        switch self {
        case .InvalidFlight(let message),
             .UnknownFlight(let message),
             .Storage(let message),
             .Server(let message),
             .InvalidFile(let message):
            message
        }
    }
}

extension Error {
    /// The error as text for the pilot: the core's own words for its
    /// errors, the system's for everything else.
    public var words: String {
        (self as? CoreError)?.message ?? localizedDescription
    }
}
