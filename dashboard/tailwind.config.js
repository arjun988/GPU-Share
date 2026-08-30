/** @type {import('tailwindcss').Config} */
module.exports = {
  content: ["./src/**/*.{js,ts,jsx,tsx,mdx}"],
  theme: {
    extend: {
      colors: {
        canvas: "#F3F5F8",
        line: "#E2E8F0",
        ink: {
          950: "#0B1220",
          900: "#1E293B",
          800: "#FFFFFF",
          700: "#F8FAFC",
          100: "#EEF2F6",
        },
        mist: {
          100: "#0F172A",
          300: "#334155",
          500: "#64748B",
        },
        accent: {
          DEFAULT: "#0B5CAB",
          dim: "#094A8A",
          soft: "#E8F1F8",
        },
        ok: {
          DEFAULT: "#0F766E",
          soft: "#E6F4F1",
        },
        warn: {
          DEFAULT: "#B45309",
          soft: "#FEF3C7",
        },
        bad: {
          DEFAULT: "#B91C1C",
          soft: "#FEE2E2",
        },
      },
      fontFamily: {
        display: ["var(--font-sans)", "ui-sans-serif", "system-ui", "sans-serif"],
        sans: ["var(--font-sans)", "ui-sans-serif", "system-ui", "sans-serif"],
        mono: ["var(--font-mono)", "ui-monospace", "monospace"],
      },
      boxShadow: {
        panel: "0 1px 2px rgba(15, 23, 42, 0.04)",
      },
    },
  },
  plugins: [],
};
