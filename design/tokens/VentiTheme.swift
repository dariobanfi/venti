// Venti tokens for SwiftUI.
import SwiftUI

extension Color {
    init(hex: UInt32) {
        self.init(red: Double((hex >> 16) & 0xFF) / 255, green: Double((hex >> 8) & 0xFF) / 255, blue: Double(hex & 0xFF) / 255)
    }
}

enum VentiBrand {
    static let ink = Color(hex: 0x221640)
    static let indigo = Color(hex: 0x302157)
    static let purple = Color(hex: 0x5A4284)
    static let violet = Color(hex: 0x6B518F)
    static let mauve = Color(hex: 0xA276A4)
    static let lilac = Color(hex: 0xB0A5C7)
    static let sky = Color(hex: 0x40ADB9)
    static let mint = Color(hex: 0x99D5CA)
    static let sage = Color(hex: 0xDAE4C5)
    static let cream = Color(hex: 0xFDEDC5)
    static let sand = Color(hex: 0xFBD5B1)
    static let peach = Color(hex: 0xF3A682)
    static let gold = Color(hex: 0xFDD283)
    static let orange = Color(hex: 0xF8A65A)
    static let coral = Color(hex: 0xE56954)
    static let brick = Color(hex: 0xD65F56)
    static let rose = Color(hex: 0xB9466F)
}

struct VentiTheme {
    let bg: Color
    let surface: Color
    let surfaceRaised: Color
    let border: Color
    let borderStrong: Color
    let text: Color
    let textSecondary: Color
    let textMuted: Color
    let primary: Color
    let primaryPressed: Color
    let onPrimary: Color
    let accent: Color
    let onAccent: Color
    let accentText: Color
    let link: Color
    let focus: Color
    let good: Color
    let goodSoft: Color
    let caution: Color
    let cautionSoft: Color
    let danger: Color
    let dangerSoft: Color
    let dangerFill: Color
    let onDangerFill: Color
    let series: [Color]
    let vario: [Color]
    let wind: [Color]
    let thermal: [Color]
    // The day's levels, worst to best: not flyable, glides only, weak, good, very good.
    let level: [Color]
    let levelFillOpacity: [Double]

    static let light = VentiTheme(
        bg: Color(hex: 0xFFF8E8),
        surface: Color(hex: 0xFFFDF7),
        surfaceRaised: Color(hex: 0xF8ECD3),
        border: Color(hex: 0xE6D8C0),
        borderStrong: Color(hex: 0x8A7CA6),
        text: Color(hex: 0x221640),
        textSecondary: Color(hex: 0x54457A),
        textMuted: Color(hex: 0x6C5F8C),
        primary: Color(hex: 0x302157),
        primaryPressed: Color(hex: 0x221640),
        onPrimary: Color(hex: 0xFDEDC5),
        accent: Color(hex: 0xE56954),
        onAccent: Color(hex: 0x1A0F33),
        accentText: Color(hex: 0xB23A2A),
        link: Color(hex: 0x0A6874),
        focus: Color(hex: 0x0A7F8E),
        good: Color(hex: 0x17714B),
        goodSoft: Color(hex: 0xDCEFDD),
        caution: Color(hex: 0x8A5600),
        cautionSoft: Color(hex: 0xFDE9B8),
        danger: Color(hex: 0xB5123A),
        dangerSoft: Color(hex: 0xFBDDE0),
        dangerFill: Color(hex: 0xB5123A),
        onDangerFill: Color(hex: 0xFFFFFF),
        series: [0x6A4CB8, 0xE2593F, 0x0C93A3, 0xB9800A, 0xA63A66].map { Color(hex: $0) },
        vario: [0x0A5F6C, 0x1F93A1, 0x86C9CC, 0xCBC6C2, 0xF3A682, 0xE56954, 0xB02A45].map { Color(hex: $0) },
        wind: [0xE9E3F2, 0xCFC3E3, 0xB0A0D0, 0x8F78BA, 0x6F56A3, 0x4F3A82, 0x302157].map { Color(hex: $0) },
        thermal: [0xFDE7CF, 0xFBCDA6, 0xF6A97C, 0xEC8260, 0xD95A4A, 0xA93640].map { Color(hex: $0) },
        level: [0xD65F56, 0x8A7CA6, 0xFDD283, 0x6BD9A4, 0x6A4CB8].map { Color(hex: $0) },
        levelFillOpacity: [0.6, 0.55, 0.85, 0.6, 0.55]
    )
    static let dark = VentiTheme(
        bg: Color(hex: 0x272239),
        surface: Color(hex: 0x332D49),
        surfaceRaised: Color(hex: 0x40395A),
        border: Color(hex: 0x544B70),
        borderStrong: Color(hex: 0x8F80B8),
        text: Color(hex: 0xFFF4DA),
        textSecondary: Color(hex: 0xCFC4E2),
        textMuted: Color(hex: 0xAFA2CC),
        primary: Color(hex: 0xFDEDC5),
        primaryPressed: Color(hex: 0xFBD5B1),
        onPrimary: Color(hex: 0x221640),
        accent: Color(hex: 0xF0806B),
        onAccent: Color(hex: 0x1A0F33),
        accentText: Color(hex: 0xFF9C86),
        link: Color(hex: 0x7CD6D8),
        focus: Color(hex: 0x7CD6D8),
        good: Color(hex: 0x6BD9A4),
        goodSoft: Color(hex: 0x1F4A48),
        caution: Color(hex: 0xFDD283),
        cautionSoft: Color(hex: 0x54402F),
        danger: Color(hex: 0xFF93A4),
        dangerSoft: Color(hex: 0x5A1F45),
        dangerFill: Color(hex: 0xC2183F),
        onDangerFill: Color(hex: 0xFFFFFF),
        series: [0x9B7FE0, 0xE8684E, 0x19A2AF, 0xBE8A1E, 0xC94E86].map { Color(hex: $0) },
        vario: [0x9BE8E4, 0x40ADB9, 0x35788F, 0x5F5A72, 0xB0605C, 0xF0806B, 0xFDC98A].map { Color(hex: $0) },
        wind: [0x43336F, 0x58458C, 0x6F59A6, 0x8C77C0, 0xAA98D6, 0xC9BCE8, 0xEAE3F8].map { Color(hex: $0) },
        thermal: [0x54304F, 0x7E3E54, 0xAC5157, 0xD96A58, 0xF59270, 0xFDC49C].map { Color(hex: $0) },
        level: [0xE56954, 0x5F5A72, 0xFDD283, 0x6BD9A4, 0xC3AEF5].map { Color(hex: $0) },
        levelFillOpacity: [0.65, 0.8, 0.6, 0.55, 0.6]
    )
    static let flight = VentiTheme(
        bg: Color(hex: 0xFFFFFF),
        surface: Color(hex: 0xFFFFFF),
        surfaceRaised: Color(hex: 0xEFEAF5),
        border: Color(hex: 0x120A24),
        borderStrong: Color(hex: 0x120A24),
        text: Color(hex: 0x120A24),
        textSecondary: Color(hex: 0x221640),
        textMuted: Color(hex: 0x3B2C63),
        primary: Color(hex: 0x120A24),
        primaryPressed: Color(hex: 0x302157),
        onPrimary: Color(hex: 0xFFFFFF),
        accent: Color(hex: 0xE56954),
        onAccent: Color(hex: 0x120A24),
        accentText: Color(hex: 0x8A2215),
        link: Color(hex: 0x06525C),
        focus: Color(hex: 0x06525C),
        good: Color(hex: 0x084F30),
        goodSoft: Color(hex: 0xD3EFDD),
        caution: Color(hex: 0x603B00),
        cautionSoft: Color(hex: 0xFFE39A),
        danger: Color(hex: 0x880826),
        dangerSoft: Color(hex: 0xFFDCE1),
        dangerFill: Color(hex: 0xA00A2E),
        onDangerFill: Color(hex: 0xFFFFFF),
        series: [0x6A4CB8, 0xE2593F, 0x0C93A3, 0xB9800A, 0xA63A66].map { Color(hex: $0) },
        vario: [0x0A5F6C, 0x1F93A1, 0x86C9CC, 0xCBC6C2, 0xF3A682, 0xE56954, 0xB02A45].map { Color(hex: $0) },
        wind: [0xE9E3F2, 0xCFC3E3, 0xB0A0D0, 0x8F78BA, 0x6F56A3, 0x4F3A82, 0x302157].map { Color(hex: $0) },
        thermal: [0xFDE7CF, 0xFBCDA6, 0xF6A97C, 0xEC8260, 0xD95A4A, 0xA93640].map { Color(hex: $0) },
        level: [0xD65F56, 0x8A7CA6, 0xFDD283, 0x6BD9A4, 0x6A4CB8].map { Color(hex: $0) },
        levelFillOpacity: [0.6, 0.55, 0.85, 0.6, 0.55]
    )
}

enum VentiScaleBreaks {
    static let vario: [Double] = [-3.0, -1.5, -0.5, 0.5, 1.5, 3.0] // m/s
    static let wind: [Double] = [5.0, 10.0, 15.0, 20.0, 25.0, 35.0] // km/h
    static let thermal: [Double] = [0.5, 1.0, 1.5, 2.0, 3.0] // m/s
}

enum VentiType {
    static let display = Font.custom("FiraSans-BlackItalic", size: 56)
    static let hero = Font.custom("FiraSans-BlackItalic", size: 36)
    static let title = Font.custom("FiraSans-SemiBold", size: 26)
    static let heading = Font.custom("FiraSans-SemiBold", size: 19)
    static let body = Font.custom("FiraSans-Regular", size: 16)
    static let label = Font.custom("FiraSans-Medium", size: 14)
    static let caption = Font.custom("FiraSans-Regular", size: 12)
    static let instrumentXl = Font.custom("FiraSans-Bold", size: 84).monospacedDigit()
    static let instrumentL = Font.custom("FiraSans-Bold", size: 44).monospacedDigit()
    static let instrumentM = Font.custom("FiraSans-SemiBold", size: 24).monospacedDigit()
}

enum VentiSpace {
    static let s1: CGFloat = 4
    static let s2: CGFloat = 8
    static let s3: CGFloat = 12
    static let s4: CGFloat = 16
    static let s5: CGFloat = 24
    static let s6: CGFloat = 32
    static let s7: CGFloat = 48
    static let s8: CGFloat = 72
}

enum VentiRadius {
    static let s: CGFloat = 8
    static let m: CGFloat = 14
    static let l: CGFloat = 24
    static let pill: CGFloat = 999
}
