// Tailwind preset. Import tokens.css as well: the colours point at its variables.
module.exports = { theme: { extend: {
  "colors": {
    "bg": "var(--color-bg)",
    "surface": "var(--color-surface)",
    "surface-raised": "var(--color-surface-raised)",
    "border": "var(--color-border)",
    "border-strong": "var(--color-border-strong)",
    "text": "var(--color-text)",
    "text-secondary": "var(--color-text-secondary)",
    "text-muted": "var(--color-text-muted)",
    "primary": "var(--color-primary)",
    "primary-pressed": "var(--color-primary-pressed)",
    "on-primary": "var(--color-on-primary)",
    "accent": "var(--color-accent)",
    "on-accent": "var(--color-on-accent)",
    "accent-text": "var(--color-accent-text)",
    "link": "var(--color-link)",
    "focus": "var(--color-focus)",
    "good": "var(--color-good)",
    "good-soft": "var(--color-good-soft)",
    "caution": "var(--color-caution)",
    "caution-soft": "var(--color-caution-soft)",
    "danger": "var(--color-danger)",
    "danger-soft": "var(--color-danger-soft)",
    "danger-fill": "var(--color-danger-fill)",
    "on-danger-fill": "var(--color-on-danger-fill)",
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
    "series": {
      "1": "var(--series-1)",
      "2": "var(--series-2)",
      "3": "var(--series-3)",
      "4": "var(--series-4)",
      "5": "var(--series-5)"
    },
    "vario": {
      "1": "var(--vario-1)",
      "2": "var(--vario-2)",
      "3": "var(--vario-3)",
      "4": "var(--vario-4)",
      "5": "var(--vario-5)",
      "6": "var(--vario-6)",
      "7": "var(--vario-7)"
    },
    "wind": {
      "1": "var(--wind-1)",
      "2": "var(--wind-2)",
      "3": "var(--wind-3)",
      "4": "var(--wind-4)",
      "5": "var(--wind-5)",
      "6": "var(--wind-6)",
      "7": "var(--wind-7)"
    },
    "thermal": {
      "1": "var(--thermal-1)",
      "2": "var(--thermal-2)",
      "3": "var(--thermal-3)",
      "4": "var(--thermal-4)",
      "5": "var(--thermal-5)",
      "6": "var(--thermal-6)"
    }
  },
  "fontFamily": {
    "sans": [
      "Fira Sans",
      "system-ui",
      "sans-serif"
    ]
  },
  "fontSize": {
    "display": [
      "56px",
      {
        "lineHeight": "56px",
        "fontWeight": "900",
        "letterSpacing": "-0.01em"
      }
    ],
    "hero": [
      "36px",
      {
        "lineHeight": "38px",
        "fontWeight": "900",
        "letterSpacing": "-0.01em"
      }
    ],
    "title": [
      "26px",
      {
        "lineHeight": "32px",
        "fontWeight": "600",
        "letterSpacing": "-0.005em"
      }
    ],
    "heading": [
      "19px",
      {
        "lineHeight": "25px",
        "fontWeight": "600",
        "letterSpacing": "0em"
      }
    ],
    "body": [
      "16px",
      {
        "lineHeight": "24px",
        "fontWeight": "400",
        "letterSpacing": "0em"
      }
    ],
    "label": [
      "14px",
      {
        "lineHeight": "20px",
        "fontWeight": "500",
        "letterSpacing": "0em"
      }
    ],
    "caption": [
      "12px",
      {
        "lineHeight": "16px",
        "fontWeight": "400",
        "letterSpacing": "0.01em"
      }
    ],
    "instrument-xl": [
      "84px",
      {
        "lineHeight": "84px",
        "fontWeight": "700",
        "letterSpacing": "-0.02em"
      }
    ],
    "instrument-l": [
      "44px",
      {
        "lineHeight": "46px",
        "fontWeight": "700",
        "letterSpacing": "-0.015em"
      }
    ],
    "instrument-m": [
      "24px",
      {
        "lineHeight": "28px",
        "fontWeight": "600",
        "letterSpacing": "0em"
      }
    ]
  },
  "borderRadius": {
    "s": "8px",
    "m": "14px",
    "l": "24px",
    "pill": "999px"
  },
  "boxShadow": {
    "float": "0 2px 6px rgba(34,22,64,.14), 0 8px 24px rgba(34,22,64,.16)",
    "sheet": "0 -4px 28px rgba(34,22,64,.20)"
  },
  "backgroundImage": {
    "sky": "var(--gradient-sky)",
    "climb": "var(--gradient-climb)"
  },
  "minHeight": {
    "touch": "48px",
    "touch-flight": "64px"
  },
  "transitionTimingFunction": {
    "venti": "cubic-bezier(.2,.8,.2,1)"
  }
} } };
