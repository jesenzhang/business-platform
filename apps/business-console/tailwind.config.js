/** @type {import('tailwindcss').Config} */
export default {
  content: ['./index.html', './src/**/*.{ts,tsx}'],
  theme: {
    extend: {
      colors: {
        ink: '#1d1d1f',
        muted: '#6e6e73',
        canvas: '#f5f5f7',
        panel: '#ffffff',
        line: '#e5e5ea',
        accent: '#0071e3',
      },
      boxShadow: { panel: '0 12px 32px rgba(29, 29, 31, .06)' },
    },
  },
  plugins: [],
}
