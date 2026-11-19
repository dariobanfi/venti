/// The names under which the app keeps the pilot's settings on the device.
enum Preferences {
    /// The pilot's name, written into recorded flights.
    static let pilot = "VentiPilotName"
    /// The glider, written into recorded flights.
    static let glider = "VentiGliderName"
    /// Whether the vario is silent.
    static let varioMuted = "VentiVarioMuted"
    /// The model the assistant uses.
    static let assistantModel = "VentiAssistantModel"
    /// The address of the Venti server that live wind comes from.
    static let server = "VentiServerAddress"
}
