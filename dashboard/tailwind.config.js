/** @type {import('tailwindcss').Config} */
module.exports = {
  content: ["./src/**/*.{js,ts,jsx,tsx,mdx}"],
  theme: {
    extend: {
      colors: {
        ink: {
          950: "#0b1210",
          900: "#101a17",
          800: "#162420",
          700: "#1e322c",
        },
        mist: {
          100: "#e8f2ee",
          300: "#9fbfb3",
          500: "#5f8f7d",
        },
        accent: {
          DEFAULT: "#2dd4a8",
          dim: "#1fa884",
        },
      },
      fontFamily: {
        display: ["var(--font-display)", "Georgia", "serif"],
        sans: ["var(--font-sans)", "ui-sans-serif", "system-ui", "sans-serif"],
      },
    },
  },
  plugins: [],
};
