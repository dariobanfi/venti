package fly.venti.app.fly

import android.Manifest
import android.annotation.SuppressLint
import android.bluetooth.BluetoothAdapter
import android.bluetooth.BluetoothDevice
import android.bluetooth.BluetoothGatt
import android.bluetooth.BluetoothGattCallback
import android.bluetooth.BluetoothGattCharacteristic
import android.bluetooth.BluetoothGattDescriptor
import android.bluetooth.BluetoothManager
import android.bluetooth.BluetoothProfile
import android.bluetooth.BluetoothStatusCodes
import android.bluetooth.le.ScanCallback
import android.bluetooth.le.ScanResult
import android.bluetooth.le.ScanSettings
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.PackageManager
import android.os.Build
import android.os.Handler
import android.os.Looper
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.core.content.ContextCompat
import fly.venti.app.settings.Preferences
import fly.venti.core.varioSerialServices
import java.util.UUID

/**
 * The Bluetooth connection to an external vario.
 *
 * Varios send their data as lines of text over one of a few serial-style
 * Bluetooth services; the core knows which. This class finds devices,
 * connects to the one the pilot picks, and passes on every byte it sends.
 * What the bytes mean is the core's business.
 *
 * A vario that was used before is reconnected whenever it comes in range,
 * also in the middle of a flight.
 *
 * To verify with a real vario, all of it: nobody has connected one yet, and
 * an emulator has no Bluetooth to try it with.
 */
// Every way in checks `problem()` first, which covers the permissions, and
// Android's own refusal is caught where a call is made.
@SuppressLint("MissingPermission")
class VarioLink(private val context: Context, private val preferences: Preferences) {
    var state: VarioState by mutableStateOf(VarioState.Idle)
        private set

    /** Devices found while scanning, likely varios first. */
    var devices: List<VarioDevice> by mutableStateOf(emptyList())
        private set

    var report: VarioReport by mutableStateOf(VarioReport())
        private set

    /**
     * Whether the pilot has used a vario before. Asking Android for
     * Bluetooth shows a permission prompt, so the app only does it then or
     * when the pilot opens the vario list.
     */
    var hasRememberedVario: Boolean by mutableStateOf(preferences.rememberedVario != null)
        private set

    /** Called with every chunk of bytes the vario sends, on a Bluetooth thread. */
    @Volatile
    var onBytes: ((ByteArray) -> Unit)? = null

    private val main = Handler(Looper.getMainLooper())
    private val found = mutableMapOf<String, BluetoothDevice>()
    private var gatt: BluetoothGatt? = null
    private var wantsScan = false
    private var isWatchingBluetooth = false

    /** The notifying characteristics still to be switched on, one at a time. */
    private val toSubscribe = ArrayDeque<BluetoothGattCharacteristic>()

    private val serialServices: Map<UUID, UUID> by lazy {
        varioSerialServices().associate { UUID.fromString(it.service) to UUID.fromString(it.notify) }
    }

    /** The name of the connected vario, if one is connected. */
    val connectedName: String? get() = (state as? VarioState.Connected)?.name

    /** The report as text, to be shared. */
    val reportText: String get() = report.text(connectedName)

    /** Starts listing nearby devices. */
    fun scan() {
        wantsScan = true
        devices = emptyList()
        found.clear()
        watchBluetooth()
        scanIfReady()
    }

    fun stopScan() {
        wantsScan = false
        try {
            adapter()?.bluetoothLeScanner?.stopScan(scanCallback)
        } catch (_: RuntimeException) {
            // Bluetooth was switched off or taken away meanwhile.
        }
        if (state == VarioState.Scanning) {
            state = VarioState.Idle
        }
    }

    fun connect(device: VarioDevice) {
        val peripheral = found[device.address] ?: return
        stopScan()
        preferences.rememberedVario = device.address
        hasRememberedVario = true
        // The pilot has just seen it in the list, so it is in range: a
        // direct connection is the quick one.
        connect(peripheral, device.name, whenInRange = false)
    }

    /** Connects to the vario used last time, as soon as it is in range. */
    fun reconnectToRememberedVario() {
        if (gatt != null || !hasRememberedVario) return
        watchBluetooth()
        reconnectIfReady()
    }

    /** Disconnects and forgets the vario. */
    fun forget() {
        preferences.rememberedVario = null
        hasRememberedVario = false
        close()
        state = VarioState.Idle
    }

    /** Why Bluetooth cannot be used right now, in words for the pilot, or `null` if it can. */
    private fun problem(): String? {
        val adapter = adapter()
        return when {
            adapter == null || !context.packageManager.hasSystemFeature(PackageManager.FEATURE_BLUETOOTH_LE) ->
                "This device has no Bluetooth LE."
            !isAllowed(context) ->
                "Venti is not allowed to use Bluetooth. You can allow it in the phone's Settings, " +
                    "under Apps, Venti, Permissions."
            !adapter.isEnabled -> "Bluetooth is switched off."
            else -> null
        }
    }

    private fun adapter(): BluetoothAdapter? = context.getSystemService(BluetoothManager::class.java)?.adapter

    /** Follows Bluetooth being switched on and off, from the first use on. */
    private fun watchBluetooth() {
        if (isWatchingBluetooth) return
        isWatchingBluetooth = true
        val receiver = object : BroadcastReceiver() {
            override fun onReceive(context: Context, intent: Intent) {
                when (intent.getIntExtra(BluetoothAdapter.EXTRA_STATE, BluetoothAdapter.ERROR)) {
                    BluetoothAdapter.STATE_ON -> {
                        scanIfReady()
                        reconnectIfReady()
                    }
                    BluetoothAdapter.STATE_OFF -> {
                        close()
                        state = VarioState.Unavailable("Bluetooth is switched off.")
                    }
                }
            }
        }
        ContextCompat.registerReceiver(
            context.applicationContext,
            receiver,
            IntentFilter(BluetoothAdapter.ACTION_STATE_CHANGED),
            ContextCompat.RECEIVER_NOT_EXPORTED,
        )
    }

    private fun scanIfReady() {
        if (!wantsScan) return
        problem()?.let {
            state = VarioState.Unavailable(it)
            return
        }
        val scanner = adapter()?.bluetoothLeScanner ?: return
        try {
            // Every device is listed, not only those advertising a serial
            // service: some varios advertise nothing and must be picked by
            // name.
            val settings = ScanSettings.Builder().setScanMode(ScanSettings.SCAN_MODE_LOW_LATENCY).build()
            scanner.startScan(null, settings, scanCallback)
            if (gatt == null) {
                state = VarioState.Scanning
            }
        } catch (_: RuntimeException) {
            state = VarioState.Unavailable("Bluetooth could not be searched. Try switching it off and on.")
        }
    }

    private fun reconnectIfReady() {
        val address = preferences.rememberedVario ?: return
        if (gatt != null) return
        problem()?.let {
            state = VarioState.Unavailable(it)
            return
        }
        val peripheral = try {
            adapter()?.getRemoteDevice(address)
        } catch (_: IllegalArgumentException) {
            null
        } ?: return
        connect(peripheral, name(peripheral), whenInRange = true)
    }

    // The call Android 17 offers in place of this one does not exist before it.
    @Suppress("DEPRECATION")
    private fun connect(peripheral: BluetoothDevice, name: String, whenInRange: Boolean) {
        close()
        state = VarioState.Connecting(name)
        gatt = try {
            // Asked to connect "when in range", Android completes the
            // request whenever the device turns up, without a time limit.
            peripheral.connectGatt(context, whenInRange, gattCallback, BluetoothDevice.TRANSPORT_LE)
        } catch (_: RuntimeException) {
            null
        }
    }

    private fun close() {
        toSubscribe.clear()
        try {
            gatt?.disconnect()
            gatt?.close()
        } catch (_: RuntimeException) {
            // Already gone.
        }
        gatt = null
    }

    private fun name(peripheral: BluetoothDevice): String =
        try {
            peripheral.name
        } catch (_: SecurityException) {
            null
        }.orEmpty().ifEmpty { "Vario" }

    private val scanCallback = object : ScanCallback() {
        override fun onScanResult(callbackType: Int, result: ScanResult) {
            val record = result.scanRecord
            val name = record?.deviceName ?: try {
                result.device.name
            } catch (_: SecurityException) {
                null
            }
            if (name.isNullOrEmpty()) return
            val advertised = record?.serviceUuids.orEmpty().map { it.uuid }
            val device = VarioDevice(
                address = result.device.address,
                name = name,
                looksLikeAVario = advertised.any { it in serialServices },
            )
            main.post {
                found[device.address] = result.device
                devices = devices.with(device)
            }
        }

        override fun onScanFailed(errorCode: Int) {
            main.post {
                if (state == VarioState.Scanning) {
                    state = VarioState.Unavailable("Bluetooth could not be searched. Try switching it off and on.")
                }
            }
        }
    }

    // Android calls these on a Bluetooth thread. What changes the state the
    // screens draw is passed to the main thread; the vario's bytes go
    // straight to the core, which takes them from any thread.
    private val gattCallback = object : BluetoothGattCallback() {
        override fun onConnectionStateChange(gatt: BluetoothGatt, status: Int, newState: Int) {
            main.post {
                if (gatt !== this@VarioLink.gatt) return@post
                if (newState == BluetoothProfile.STATE_CONNECTED && status == BluetoothGatt.GATT_SUCCESS) {
                    state = VarioState.Connected(name(gatt.device))
                    report = VarioReport()
                    gatt.discoverServices()
                } else if (newState == BluetoothProfile.STATE_DISCONNECTED) {
                    // Ask again for a vario that was lost or could not be
                    // reached; the pilot has not dropped it.
                    connect(gatt.device, name(gatt.device), whenInRange = true)
                }
            }
        }

        override fun onServicesDiscovered(gatt: BluetoothGatt, status: Int) {
            main.post {
                if (gatt !== this@VarioLink.gatt) return@post
                val services = gatt.services.orEmpty()
                val offered = services.map { it.uuid }
                val lines = mutableListOf<String>()
                toSubscribe.clear()
                for (service in services) {
                    for (characteristic in service.characteristics) {
                        val notifies = characteristic.properties and NOTIFIES != 0
                        val wanted = VarioServices.listensTo(serialServices, offered, service.uuid, characteristic.uuid)
                        if (notifies && wanted) {
                            toSubscribe += characteristic
                        }
                        lines += VarioServices.describe(service.uuid, characteristic.uuid, notifies, wanted)
                    }
                }
                report = report.copy(characteristics = lines)
                subscribeToNext(gatt)
            }
        }

        override fun onDescriptorWrite(gatt: BluetoothGatt, descriptor: BluetoothGattDescriptor, status: Int) {
            main.post {
                if (gatt === this@VarioLink.gatt) subscribeToNext(gatt)
            }
        }

        // Android 13 and later.
        override fun onCharacteristicChanged(
            gatt: BluetoothGatt,
            characteristic: BluetoothGattCharacteristic,
            value: ByteArray,
        ) {
            received(value)
        }

        // Before Android 13 the bytes are read from the characteristic.
        @Deprecated("Called by Android 12 and earlier.")
        @Suppress("DEPRECATION")
        override fun onCharacteristicChanged(gatt: BluetoothGatt, characteristic: BluetoothGattCharacteristic) {
            if (Build.VERSION.SDK_INT < 33) {
                characteristic.value?.let { received(it.copyOf()) }
            }
        }
    }

    /**
     * Switches on notifications for the next characteristic. Android takes
     * one such request at a time and answers each in `onDescriptorWrite`.
     */
    @Suppress("DEPRECATION")
    private fun subscribeToNext(gatt: BluetoothGatt) {
        while (true) {
            val characteristic = toSubscribe.removeFirstOrNull() ?: return
            val descriptor = characteristic.getDescriptor(CLIENT_CONFIGURATION)
            val value = if (characteristic.properties and BluetoothGattCharacteristic.PROPERTY_NOTIFY != 0) {
                BluetoothGattDescriptor.ENABLE_NOTIFICATION_VALUE
            } else {
                BluetoothGattDescriptor.ENABLE_INDICATION_VALUE
            }
            val asked = try {
                gatt.setCharacteristicNotification(characteristic, true)
                when {
                    descriptor == null -> false
                    Build.VERSION.SDK_INT >= 33 ->
                        gatt.writeDescriptor(descriptor, value) == BluetoothStatusCodes.SUCCESS
                    else -> {
                        descriptor.value = value
                        gatt.writeDescriptor(descriptor)
                    }
                }
            } catch (_: RuntimeException) {
                false
            }
            // An answer will come and bring the next one; without one, go on.
            if (asked) return
        }
    }

    private fun received(data: ByteArray) {
        if (data.isEmpty()) return
        onBytes?.invoke(data)
        main.post { report = report.noting(data) }
    }

    companion object {
        private const val NOTIFIES =
            BluetoothGattCharacteristic.PROPERTY_NOTIFY or BluetoothGattCharacteristic.PROPERTY_INDICATE

        /** The standard descriptor that switches a characteristic's notifications on. */
        private val CLIENT_CONFIGURATION: UUID = UUID.fromString("00002902-0000-1000-8000-00805f9b34fb")

        /**
         * What Android asks the pilot before the app may use Bluetooth. Up
         * to Android 11, searching for Bluetooth devices counts as finding
         * the phone's position and needs that permission instead.
         */
        val permissions: Array<String> =
            if (Build.VERSION.SDK_INT >= 31) {
                arrayOf(Manifest.permission.BLUETOOTH_SCAN, Manifest.permission.BLUETOOTH_CONNECT)
            } else {
                arrayOf(Manifest.permission.ACCESS_FINE_LOCATION)
            }

        fun isAllowed(context: Context): Boolean =
            permissions.all {
                ContextCompat.checkSelfPermission(context, it) == PackageManager.PERMISSION_GRANTED
            }
    }
}
