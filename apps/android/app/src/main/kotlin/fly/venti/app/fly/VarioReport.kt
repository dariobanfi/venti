package fly.venti.app.fly

import java.util.UUID

/** A Bluetooth LE device found while scanning. */
data class VarioDevice(
    /** The device's Bluetooth address. */
    val address: String,
    val name: String,
    /** Whether it advertises one of the services varios use. */
    val looksLikeAVario: Boolean,
)

/** The list of found devices with one more, or one updated: likely varios first, then by name. */
fun List<VarioDevice>.with(device: VarioDevice): List<VarioDevice> =
    (filterNot { it.address == device.address } + device)
        .sortedWith(compareBy({ !it.looksLikeAVario }, { it.name }))

sealed interface VarioState {
    /** Nothing chosen and nothing happening. */
    data object Idle : VarioState

    /** Bluetooth cannot be used; the text says why. */
    data class Unavailable(val reason: String) : VarioState

    data object Scanning : VarioState

    data class Connecting(val name: String) : VarioState

    data class Connected(val name: String) : VarioState
}

/**
 * What the connected device offers and what it has sent lately. This is how
 * a vario nobody has documented is worked out: the pilot shares the report.
 */
data class VarioReport(
    /** Every characteristic the device has, as "service / characteristic". */
    val characteristics: List<String> = emptyList(),
    val bytes: Int = 0,
    /** The last lines of text it sent. */
    val lines: List<String> = emptyList(),
    private val unfinishedLine: String = "",
) {
    /** The report after the vario sent these bytes: keeps the last few lines. */
    fun noting(data: ByteArray): VarioReport {
        val pieces = (unfinishedLine + data.decodeToString()).split('\r', '\n')
        val finished = pieces.dropLast(1).filter { it.isNotEmpty() }
        return copy(
            bytes = bytes + data.size,
            lines = (lines + finished).takeLast(LINES_KEPT),
            unfinishedLine = pieces.last().takeLast(LONGEST_LINE),
        )
    }

    /** The report as text, to be shared. */
    fun text(varioName: String?): String =
        (
            listOf(
                "Vario: ${varioName ?: "not connected"}",
                "Received: $bytes bytes",
                "Characteristics (service / characteristic):",
            ) + characteristics + "Last lines:" + lines
            ).joinToString("\n")

    private companion object {
        const val LINES_KEPT = 8
        const val LONGEST_LINE = 200
    }
}

/** Which of a device's characteristics the app listens to. */
object VarioServices {
    /**
     * Whether to listen to a characteristic that can notify.
     *
     * On a known serial service, the app listens to its sending
     * characteristic. On a device with none of them, it listens to
     * everything the device offers outside the standard Bluetooth services:
     * the core ignores whatever is not a sentence it knows.
     *
     * `serial` maps each known serial service to its sending
     * characteristic; `deviceServices` is every service the device has.
     */
    fun listensTo(
        serial: Map<UUID, UUID>,
        deviceServices: Collection<UUID>,
        service: UUID,
        characteristic: UUID,
    ): Boolean {
        val sending = serial[service]
        if (sending != null) return characteristic == sending
        return deviceServices.none { it in serial } && !isStandard(service)
    }

    /** One line of the report: a characteristic and what the app does with it. */
    fun describe(service: UUID, characteristic: UUID, notifies: Boolean, listening: Boolean): String {
        val role = when {
            !notifies -> "silent"
            listening -> "listening"
            else -> "notifies"
        }
        return "${service.toString().uppercase()} / ${characteristic.toString().uppercase()} ($role)"
    }

    /**
     * Whether a service is one the Bluetooth standard defines: battery,
     * device information and the like. Those have short, 16-bit numbers
     * inside one fixed long identifier.
     */
    fun isStandard(service: UUID): Boolean {
        val text = service.toString()
        return text.startsWith("0000") && text.endsWith(STANDARD_ENDING)
    }

    /** Standard services are `0000xxxx-0000-1000-8000-00805f9b34fb`. */
    private const val STANDARD_ENDING = "-0000-1000-8000-00805f9b34fb"
}
