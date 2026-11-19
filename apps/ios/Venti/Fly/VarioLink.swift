import CoreBluetooth
import Observation
import VentiCore

/// The Bluetooth connection to an external vario.
///
/// Varios send their data as lines of text over one of a few serial-style
/// Bluetooth services; the core knows which. This class finds devices,
/// connects to the one the pilot picks, and passes on every byte it sends.
/// What the bytes mean is the core's business.
///
/// A vario that was used before is reconnected whenever it comes in range,
/// also in the middle of a flight.
@MainActor
@Observable
final class VarioLink: NSObject, @preconcurrency CBCentralManagerDelegate, @preconcurrency CBPeripheralDelegate {
    struct Device: Identifiable, Equatable {
        let id: UUID
        var name: String
        /// Whether it advertises one of the services varios use.
        var looksLikeAVario: Bool
    }

    enum State: Equatable {
        /// Nothing chosen and nothing happening.
        case idle
        /// Bluetooth cannot be used; the text says why.
        case unavailable(String)
        case scanning
        case connecting(String)
        case connected(String)
    }

    private(set) var state: State = .idle
    /// Devices found while scanning, likely varios first.
    private(set) var devices: [Device] = []

    /// Called with every chunk of bytes the vario sends.
    @ObservationIgnored var onBytes: ((Data) -> Void)?

    @ObservationIgnored private var central: CBCentralManager?
    @ObservationIgnored private var found: [UUID: CBPeripheral] = [:]
    @ObservationIgnored private var chosen: CBPeripheral?
    @ObservationIgnored private var wantsScan = false

    private let serialServices: [CBUUID: CBUUID] = Dictionary(
        uniqueKeysWithValues: varioSerialServices().map {
            (CBUUID(string: $0.service), CBUUID(string: $0.notify))
        }
    )

    private static let rememberedKey = "VentiVarioIdentifier"

    /// The name of the connected vario, if one is connected.
    var connectedName: String? {
        if case .connected(let name) = state { name } else { nil }
    }

    /// Whether the pilot has used a vario before. Asking the system for
    /// Bluetooth shows a permission prompt, so the app only does it then or
    /// when the pilot opens the vario list.
    var hasRememberedVario: Bool {
        UserDefaults.standard.string(forKey: Self.rememberedKey) != nil
    }

    /// Starts listing nearby devices.
    func scan() {
        wantsScan = true
        devices = []
        found = [:]
        startCentralIfNeeded()
        scanIfReady()
    }

    func stopScan() {
        wantsScan = false
        central?.stopScan()
        if state == .scanning {
            state = .idle
        }
    }

    func connect(_ device: Device) {
        guard let central, let peripheral = found[device.id] else { return }
        stopScan()
        UserDefaults.standard.set(device.id.uuidString, forKey: Self.rememberedKey)
        connect(peripheral, named: device.name, with: central)
    }

    /// Connects to the vario used last time, as soon as it is in range.
    func reconnectToRememberedVario() {
        guard chosen == nil, hasRememberedVario else { return }
        startCentralIfNeeded()
        reconnectIfReady()
    }

    /// Disconnects and forgets the vario.
    func forget() {
        UserDefaults.standard.removeObject(forKey: Self.rememberedKey)
        if let chosen {
            central?.cancelPeripheralConnection(chosen)
        }
        chosen = nil
        state = .idle
    }

    private func startCentralIfNeeded() {
        if central == nil {
            // Delegate calls arrive on the main queue.
            central = CBCentralManager(delegate: self, queue: nil)
        }
    }

    private func scanIfReady() {
        guard wantsScan, let central, central.state == .poweredOn else { return }
        state = .scanning
        // Every device is listed, not only those advertising a serial
        // service: some varios advertise nothing and must be picked by name.
        central.scanForPeripherals(withServices: nil)
    }

    private func reconnectIfReady() {
        guard chosen == nil,
              let central, central.state == .poweredOn,
              let text = UserDefaults.standard.string(forKey: Self.rememberedKey),
              let id = UUID(uuidString: text),
              let peripheral = central.retrievePeripherals(withIdentifiers: [id]).first
        else { return }
        connect(peripheral, named: peripheral.name ?? "Vario", with: central)
    }

    private func connect(_ peripheral: CBPeripheral, named name: String, with central: CBCentralManager) {
        chosen = peripheral
        peripheral.delegate = self
        state = .connecting(name)
        // A connection request does not time out: the system completes it
        // whenever the device comes in range.
        central.connect(peripheral)
    }

    // MARK: Central

    func centralManagerDidUpdateState(_ central: CBCentralManager) {
        switch central.state {
        case .poweredOn:
            scanIfReady()
            reconnectIfReady()
        case .unauthorized:
            state = .unavailable("Venti is not allowed to use Bluetooth. You can allow it in Settings, under Privacy & Security, Bluetooth.")
        case .poweredOff:
            state = .unavailable("Bluetooth is switched off.")
        case .unsupported:
            state = .unavailable("This device has no Bluetooth LE.")
        default:
            break
        }
    }

    func centralManager(
        _ central: CBCentralManager, didDiscover peripheral: CBPeripheral,
        advertisementData: [String: Any], rssi RSSI: NSNumber
    ) {
        let advertisedName = advertisementData[CBAdvertisementDataLocalNameKey] as? String
        guard let name = advertisedName ?? peripheral.name, !name.isEmpty else { return }
        let advertised = advertisementData[CBAdvertisementDataServiceUUIDsKey] as? [CBUUID] ?? []
        let device = Device(
            id: peripheral.identifier,
            name: name,
            looksLikeAVario: advertised.contains { serialServices[$0] != nil }
        )
        found[device.id] = peripheral
        if let index = devices.firstIndex(where: { $0.id == device.id }) {
            devices[index] = device
        } else {
            devices.append(device)
        }
        devices.sort { ($0.looksLikeAVario ? 0 : 1, $0.name) < ($1.looksLikeAVario ? 0 : 1, $1.name) }
    }

    func centralManager(_ central: CBCentralManager, didConnect peripheral: CBPeripheral) {
        state = .connected(peripheral.name ?? "Vario")
        peripheral.discoverServices(nil)
    }

    func centralManager(
        _ central: CBCentralManager, didFailToConnect peripheral: CBPeripheral, error: (any Error)?
    ) {
        retry(peripheral, with: central)
    }

    func centralManager(
        _ central: CBCentralManager, didDisconnectPeripheral peripheral: CBPeripheral, error: (any Error)?
    ) {
        retry(peripheral, with: central)
    }

    /// Asks again for a vario that was lost, unless the pilot dropped it.
    private func retry(_ peripheral: CBPeripheral, with central: CBCentralManager) {
        guard peripheral.identifier == chosen?.identifier else { return }
        state = .connecting(peripheral.name ?? "Vario")
        central.connect(peripheral)
    }

    // MARK: Peripheral

    func peripheral(_ peripheral: CBPeripheral, didDiscoverServices error: (any Error)?) {
        for service in peripheral.services ?? [] {
            peripheral.discoverCharacteristics(nil, for: service)
        }
    }

    func peripheral(
        _ peripheral: CBPeripheral, didDiscoverCharacteristicsFor service: CBService, error: (any Error)?
    ) {
        let hasSerialService = (peripheral.services ?? []).contains { serialServices[$0.uuid] != nil }
        for characteristic in service.characteristics ?? [] {
            let notifies = !characteristic.properties.isDisjoint(with: [.notify, .indicate])
            // On a known serial service, listen to its sending characteristic.
            // On a device with none of them, listen to everything it offers
            // outside the standard Bluetooth services: the core ignores
            // whatever is not a sentence it knows.
            let wanted =
                if let notify = serialServices[service.uuid] {
                    characteristic.uuid == notify
                } else {
                    !hasSerialService && service.uuid.data.count > 2
                }
            if notifies && wanted {
                peripheral.setNotifyValue(true, for: characteristic)
            }
        }
    }

    func peripheral(
        _ peripheral: CBPeripheral, didUpdateValueFor characteristic: CBCharacteristic, error: (any Error)?
    ) {
        if let data = characteristic.value, !data.isEmpty {
            onBytes?(data)
        }
    }
}
