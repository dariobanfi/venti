package fly.venti.app.fly

import fly.venti.core.varioSerialServices
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.UUID

/**
 * What the Bluetooth link decides without Bluetooth: which characteristics
 * to listen to, what goes into the report, how the device list is ordered,
 * and how a satellite receiver's sentence is read.
 */
class VarioReportTest {
    /** The serial services as the core lists them: service to sending characteristic. */
    private val serial: Map<UUID, UUID> =
        varioSerialServices().associate { UUID.fromString(it.service) to UUID.fromString(it.notify) }

    private val nordicUart = UUID.fromString("6E400001-B5A3-F393-E0A9-E50E24DCCA9E")
    private val nordicSending = UUID.fromString("6E400003-B5A3-F393-E0A9-E50E24DCCA9E")
    private val nordicReceiving = UUID.fromString("6E400002-B5A3-F393-E0A9-E50E24DCCA9E")
    private val battery = UUID.fromString("0000180F-0000-1000-8000-00805F9B34FB")
    private val batteryLevel = UUID.fromString("00002A19-0000-1000-8000-00805F9B34FB")
    private val unknown = UUID.fromString("12345678-1234-5678-1234-56789ABCDEF0")
    private val unknownData = UUID.fromString("12345678-1234-5678-1234-56789ABCDEF1")

    @Test
    fun onAKnownSerialServiceOnlyItsSendingCharacteristicIsHeard() {
        val services = listOf(nordicUart, battery, unknown)

        assertTrue(VarioServices.listensTo(serial, services, nordicUart, nordicSending))
        assertFalse(VarioServices.listensTo(serial, services, nordicUart, nordicReceiving))
        // With a serial service on the device, nothing else is listened to.
        assertFalse(VarioServices.listensTo(serial, services, unknown, unknownData))
        assertFalse(VarioServices.listensTo(serial, services, battery, batteryLevel))
    }

    @Test
    fun onADeviceWithoutOneEverythingOutsideTheStandardServicesIsHeard() {
        val services = listOf(battery, unknown)

        assertTrue(VarioServices.listensTo(serial, services, unknown, unknownData))
        assertFalse(VarioServices.listensTo(serial, services, battery, batteryLevel))
    }

    @Test
    fun theHm10ServiceCountsAsSerialThoughItsNumberIsShort() {
        val hm10 = UUID.fromString("0000FFE0-0000-1000-8000-00805F9B34FB")
        val hm10Sending = UUID.fromString("0000FFE1-0000-1000-8000-00805F9B34FB")

        assertTrue(VarioServices.isStandard(hm10))
        assertTrue(VarioServices.listensTo(serial, listOf(hm10, battery), hm10, hm10Sending))
        assertFalse(VarioServices.isStandard(nordicUart))
    }

    @Test
    fun theReportKeepsTheLastLinesAcrossChunks() {
        var report = VarioReport()

        report = report.noting("\$LK8EX1,98".toByteArray())
        assertEquals(10, report.bytes)
        assertTrue(report.lines.isEmpty())

        report = report.noting("000,99999,12,99,3.9,*0A\r\n\$LXWP0,N".toByteArray())
        assertEquals(listOf("\$LK8EX1,98000,99999,12,99,3.9,*0A"), report.lines)

        repeat(12) { report = report.noting("line $it\n".toByteArray()) }
        assertEquals(8, report.lines.size)
        assertEquals("line 11", report.lines.last())
        // The line cut off by the chunk before the twelve was finished by them.
        assertTrue(report.lines.none { it.isEmpty() })
    }

    @Test
    fun theReportSaysWhatTheDeviceOffersAndSent() {
        val report = VarioReport(
            characteristics = listOf(
                VarioServices.describe(nordicUart, nordicSending, notifies = true, listening = true),
                VarioServices.describe(nordicUart, nordicReceiving, notifies = false, listening = false),
                VarioServices.describe(battery, batteryLevel, notifies = true, listening = false),
            ),
        ).noting("\$PTAS1,200,,1000,*00\r\n".toByteArray())

        assertEquals(
            """
            Vario: XC Tracer
            Received: 22 bytes
            Characteristics (service / characteristic):
            6E400001-B5A3-F393-E0A9-E50E24DCCA9E / 6E400003-B5A3-F393-E0A9-E50E24DCCA9E (listening)
            6E400001-B5A3-F393-E0A9-E50E24DCCA9E / 6E400002-B5A3-F393-E0A9-E50E24DCCA9E (silent)
            0000180F-0000-1000-8000-00805F9B34FB / 00002A19-0000-1000-8000-00805F9B34FB (notifies)
            Last lines:
            ${'$'}PTAS1,200,,1000,*00
            """.trimIndent(),
            report.text("XC Tracer"),
        )
        assertTrue(VarioReport().text(null).startsWith("Vario: not connected"))
    }

    @Test
    fun likelyVariosComeFirstInTheListThenByName() {
        var devices = emptyList<VarioDevice>()
        devices = devices.with(VarioDevice("03", "Headphones", looksLikeAVario = false))
        devices = devices.with(VarioDevice("02", "XC Tracer", looksLikeAVario = true))
        devices = devices.with(VarioDevice("01", "Bike sensor", looksLikeAVario = false))
        // A device seen again replaces its earlier entry.
        devices = devices.with(VarioDevice("03", "Headphones 2", looksLikeAVario = false))

        assertEquals(listOf("XC Tracer", "Bike sensor", "Headphones 2"), devices.map { it.name })
    }

    @Test
    fun theGeoidsHeightIsReadFromAReceiversFixSentence() {
        assertEquals(
            46.9,
            Nmea.geoidHeight("\$GPGGA,123519,4807.038,N,01131.000,E,1,08,0.9,545.4,M,46.9,M,,*47") ?: 0.0,
            0.0,
        )
        assertEquals(
            -12.5,
            Nmea.geoidHeight("\$GNGGA,101530.00,4740.1,N,01147.9,E,2,12,0.7,1620.1,M,-12.5,M,,") ?: 0.0,
            0.0,
        )
        // No fix, another sentence, a cut-off one: no answer.
        assertNull(Nmea.geoidHeight("\$GPGGA,123519,,,,,0,00,,,M,,M,,*66"))
        assertNull(Nmea.geoidHeight("\$GPRMC,123519,A,4807.038,N,01131.000,E,022.4,084.4,230394,003.1,W*6A"))
        assertNull(Nmea.geoidHeight("\$GPGGA,123519,4807.038,N"))
        assertNull(Nmea.geoidHeight(""))
    }
}
