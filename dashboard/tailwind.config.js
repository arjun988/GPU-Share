/** @type {import('tailwindcss').Config} */
module.exports = {
  darkMode: "class",
  content: ["./src/**/*.{js,ts,jsx,tsx,mdx}"],
  theme: {
    extend: {
      colors: {
        canvas: "var(--canvas)",
        surface: "var(--surface)",
        fill: "var(--fill)",
        line: "var(--line)",
        ink: {
          950: "var(--ink-950)",
          900: "var(--ink-900)",
          800: "var(--surface)",
          700: "var(--fill)",
          100: "var(--fill-strong)",
        },
        mist: {
          100: "var(--text)",
          300: "var(--text-secondary)",
          500: "var(--text-muted)",
        },
        accent: {
          DEFAULT: "var(--accent)",
          dim: "var(--accent-dim)",
          soft: "var(--accent-soft)",
        },
        ok: {
          DEFAULT: "var(--ok)",
          soft: "var(--ok-soft)",
        },
        warn: {
          DEFAULT: "var(--warn)",
          soft: "var(--warn-soft)",
        },
        bad: {
          DEFAULT: "var(--bad)",
          soft: "var(--bad-soft)",
        },
      },
      fontFamily: {
        display: ["var(--font-sans)", "ui-sans-serif", "system-ui", "sans-serif"],
        sans: ["var(--font-sans)", "ui-sans-serif", "system-ui", "sans-serif"],
        mono: ["var(--font-mono)", "ui-monospace", "monospace"],
      },
      boxShadow: {
        panel: "var(--shadow-panel)",
        sidebar: "var(--shadow-sidebar)",
      },
      width: {
        sidebar: "16.5rem",
      },
    },
  },
  plugins: [],
};
