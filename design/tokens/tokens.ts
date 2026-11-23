// Venti tokens for TypeScript, React Native and map styles.
export const venti = {
  "brand": {
    "ink": "#221640",
    "indigo": "#302157",
    "purple": "#5A4284",
    "violet": "#6B518F",
    "mauve": "#A276A4",
    "lilac": "#B0A5C7",
    "sky": "#40ADB9",
    "mint": "#99D5CA",
    "sage": "#DAE4C5",
    "cream": "#FDEDC5",
    "sand": "#FBD5B1",
    "peach": "#F3A682",
    "gold": "#FDD283",
    "orange": "#F8A65A",
    "coral": "#E56954",
    "brick": "#D65F56",
    "rose": "#B9466F"
  },
  "theme": {
    "light": {
      "bg": "#FFF8E8",
      "surface": "#FFFDF7",
      "surfaceRaised": "#F8ECD3",
      "border": "#E6D8C0",
      "borderStrong": "#8A7CA6",
      "text": "#221640",
      "textSecondary": "#54457A",
      "textMuted": "#6C5F8C",
      "primary": "#302157",
      "primaryPressed": "#221640",
      "onPrimary": "#FDEDC5",
      "accent": "#E56954",
      "onAccent": "#1A0F33",
      "accentText": "#B23A2A",
      "link": "#0A6874",
      "focus": "#0A7F8E",
      "good": "#17714B",
      "goodSoft": "#DCEFDD",
      "caution": "#8A5600",
      "cautionSoft": "#FDE9B8",
      "danger": "#B5123A",
      "dangerSoft": "#FBDDE0",
      "dangerFill": "#B5123A",
      "onDangerFill": "#FFFFFF"
    },
    "dark": {
      "bg": "#272239",
      "surface": "#332D49",
      "surfaceRaised": "#40395A",
      "border": "#544B70",
      "borderStrong": "#8F80B8",
      "text": "#FFF4DA",
      "textSecondary": "#CFC4E2",
      "textMuted": "#AFA2CC",
      "primary": "#FDEDC5",
      "primaryPressed": "#FBD5B1",
      "onPrimary": "#221640",
      "accent": "#F0806B",
      "onAccent": "#1A0F33",
      "accentText": "#FF9C86",
      "link": "#7CD6D8",
      "focus": "#7CD6D8",
      "good": "#6BD9A4",
      "goodSoft": "#1F4A48",
      "caution": "#FDD283",
      "cautionSoft": "#54402F",
      "danger": "#FF93A4",
      "dangerSoft": "#5A1F45",
      "dangerFill": "#C2183F",
      "onDangerFill": "#FFFFFF"
    },
    "flight": {
      "bg": "#FFFFFF",
      "surface": "#FFFFFF",
      "surfaceRaised": "#EFEAF5",
      "border": "#120A24",
      "borderStrong": "#120A24",
      "text": "#120A24",
      "textSecondary": "#221640",
      "textMuted": "#3B2C63",
      "primary": "#120A24",
      "primaryPressed": "#302157",
      "onPrimary": "#FFFFFF",
      "accent": "#E56954",
      "onAccent": "#120A24",
      "accentText": "#8A2215",
      "link": "#06525C",
      "focus": "#06525C",
      "good": "#084F30",
      "goodSoft": "#D3EFDD",
      "caution": "#603B00",
      "cautionSoft": "#FFE39A",
      "danger": "#880826",
      "dangerSoft": "#FFDCE1",
      "dangerFill": "#A00A2E",
      "onDangerFill": "#FFFFFF"
    }
  },
  "series": {
    "light": [
      "#6A4CB8",
      "#E2593F",
      "#0C93A3",
      "#B9800A",
      "#A63A66"
    ],
    "dark": [
      "#9B7FE0",
      "#E8684E",
      "#19A2AF",
      "#BE8A1E",
      "#C94E86"
    ]
  },
  "scale": {
    "vario": {
      "unit": "m/s",
      "kind": "diverging",
      "breaks": [
        -3,
        -1.5,
        -0.5,
        0.5,
        1.5,
        3
      ],
      "light": [
        "#0A5F6C",
        "#1F93A1",
        "#86C9CC",
        "#CBC6C2",
        "#F3A682",
        "#E56954",
        "#B02A45"
      ],
      "dark": [
        "#9BE8E4",
        "#40ADB9",
        "#35788F",
        "#5F5A72",
        "#B0605C",
        "#F0806B",
        "#FDC98A"
      ]
    },
    "wind": {
      "unit": "km/h",
      "kind": "sequential",
      "breaks": [
        5,
        10,
        15,
        20,
        25,
        35
      ],
      "light": [
        "#E9E3F2",
        "#CFC3E3",
        "#B0A0D0",
        "#8F78BA",
        "#6F56A3",
        "#4F3A82",
        "#302157"
      ],
      "dark": [
        "#43336F",
        "#58458C",
        "#6F59A6",
        "#8C77C0",
        "#AA98D6",
        "#C9BCE8",
        "#EAE3F8"
      ]
    },
    "thermal": {
      "unit": "m/s",
      "kind": "sequential",
      "breaks": [
        0.5,
        1,
        1.5,
        2,
        3
      ],
      "light": [
        "#FDE7CF",
        "#FBCDA6",
        "#F6A97C",
        "#EC8260",
        "#D95A4A",
        "#A93640"
      ],
      "dark": [
        "#54304F",
        "#7E3E54",
        "#AC5157",
        "#D96A58",
        "#F59270",
        "#FDC49C"
      ]
    }
  },
  "airspace": {
    "light": {
      "prohibited": "#B5123A",
      "controlled": "#2B4DB8",
      "mandatoryZone": "#6A4CB8",
      "dangerArea": "#8A5600",
      "wildlife": "#17714B"
    },
    "dark": {
      "prohibited": "#FF93A4",
      "controlled": "#9DB6FF",
      "mandatoryZone": "#C3AEF5",
      "dangerArea": "#FDD283",
      "wildlife": "#6BD9A4"
    }
  },
  "map": {
    "light": {
      "land": "#F7EFDD",
      "forest": "#DAE4C5",
      "water": "#99D5CA",
      "glacier": "#FFFFFF",
      "contour": "#C9B9C9",
      "contourIndex": "#A894B4",
      "hillshadeShadow": "#5A4284",
      "hillshadeLight": "#FFF4DA",
      "road": "#FFFFFF",
      "label": "#221640",
      "labelHalo": "#FFF8E8",
      "trackCasing": "#221640",
      "ownPosition": "#FDEDC5",
      "ownPositionRing": "#302157"
    },
    "dark": {
      "land": "#211640",
      "forest": "#26264C",
      "water": "#1D4560",
      "glacier": "#4A3F78",
      "contour": "#43336F",
      "contourIndex": "#5A4790",
      "hillshadeShadow": "#0E0820",
      "hillshadeLight": "#5A4284",
      "road": "#54457A",
      "label": "#FFF4DA",
      "labelHalo": "#1C1236",
      "trackCasing": "#0E0820",
      "ownPosition": "#FDEDC5",
      "ownPositionRing": "#0E0820"
    }
  },
  "type": {
    "display": {
      "fontSize": 56,
      "lineHeight": 56,
      "fontWeight": "900",
      "italic": true,
      "letterSpacingEm": -0.01,
      "tabular": false
    },
    "hero": {
      "fontSize": 36,
      "lineHeight": 38,
      "fontWeight": "900",
      "italic": true,
      "letterSpacingEm": -0.01,
      "tabular": false
    },
    "title": {
      "fontSize": 26,
      "lineHeight": 32,
      "fontWeight": "600",
      "italic": false,
      "letterSpacingEm": -0.005,
      "tabular": false
    },
    "heading": {
      "fontSize": 19,
      "lineHeight": 25,
      "fontWeight": "600",
      "italic": false,
      "letterSpacingEm": 0,
      "tabular": false
    },
    "body": {
      "fontSize": 16,
      "lineHeight": 24,
      "fontWeight": "400",
      "italic": false,
      "letterSpacingEm": 0,
      "tabular": false
    },
    "label": {
      "fontSize": 14,
      "lineHeight": 20,
      "fontWeight": "500",
      "italic": false,
      "letterSpacingEm": 0,
      "tabular": false
    },
    "caption": {
      "fontSize": 12,
      "lineHeight": 16,
      "fontWeight": "400",
      "italic": false,
      "letterSpacingEm": 0.01,
      "tabular": false
    },
    "instrumentXl": {
      "fontSize": 84,
      "lineHeight": 84,
      "fontWeight": "700",
      "italic": false,
      "letterSpacingEm": -0.02,
      "tabular": true
    },
    "instrumentL": {
      "fontSize": 44,
      "lineHeight": 46,
      "fontWeight": "700",
      "italic": false,
      "letterSpacingEm": -0.015,
      "tabular": true
    },
    "instrumentM": {
      "fontSize": 24,
      "lineHeight": 28,
      "fontWeight": "600",
      "italic": false,
      "letterSpacingEm": 0,
      "tabular": true
    }
  },
  "space": {
    "1": 4,
    "2": 8,
    "3": 12,
    "4": 16,
    "5": 24,
    "6": 32,
    "7": 48,
    "8": 72
  },
  "radius": {
    "s": 8,
    "m": 14,
    "l": 24,
    "pill": 999
  },
  "size": {
    "touch": 48,
    "touchFlight": 64,
    "icon": 24,
    "iconFlight": 32
  },
  "motion": {
    "fast": 120,
    "base": 200,
    "slow": 320
  }
} as const;

/** Colour for a value on a data scale, e.g. scaleColor('vario', 2.1, 'light'). */
export function scaleColor(name: keyof typeof venti.scale, value: number, theme: 'light' | 'dark'): string {
  const s = venti.scale[name];
  const i = s.breaks.findIndex((b) => value < b);
  return s[theme][i === -1 ? s.breaks.length : i];
}
