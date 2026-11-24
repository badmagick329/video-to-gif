/** @type {import('tailwindcss').Config} */

/** A colour token from src/index.css, written as RGB channels so opacity modifiers work. */
const token = (name) => `rgb(var(--${name}) / <alpha-value>)`;

export default {
  content: ['./index.html', './src/**/*.{ts,tsx}'],
  theme: {
    extend: {
      // The editor's palette, from the canvas board `Main.dc.html` (VTG-9). Surfaces are named
      // for where they sit in the window, darkest (status bar) to lightest (controls).
      colors: {
        chrome: token('chrome'),
        canvas: token('canvas'),
        stage: token('stage'),
        viewer: token('viewer'),
        dock: token('dock'),
        panel: token('panel'),
        raised: token('raised'),
        well: token('well'),
        control: {
          DEFAULT: token('control'),
          hover: token('control-hover'),
        },
        hover: token('hover'),
        line: {
          DEFAULT: token('line'),
          subtle: token('line-subtle'),
          control: token('line-control'),
          strong: token('line-strong'),
        },
        fg: {
          DEFAULT: token('fg'),
          label: token('fg-label'),
          soft: token('fg-soft'),
        },
        muted: token('muted'),
        dim: token('dim'),
        faint: token('faint'),
        accent: {
          DEFAULT: token('accent'),
          hover: token('accent-hover'),
          fg: token('accent-fg'),
          soft: token('accent-soft'),
        },
        ok: token('ok'),
        warn: token('warn'),
        danger: token('danger'),
      },
      borderColor: {
        DEFAULT: token('line'),
      },
      fontFamily: {
        // Bundled from @fontsource-variable (src/main.tsx), so nothing is fetched at runtime.
        sans: ['"Inter Variable"', 'system-ui', 'sans-serif'],
        mono: ['"JetBrains Mono Variable"', 'ui-monospace', 'monospace'],
      },
      fontSize: {
        // The board's type scale: 13 px body, 12.5 px controls, 11 px section heads.
        '2xs': ['10.5px', '16px'],
        xs: ['11px', '15px'],
        sm: ['12px', '16px'],
        control: ['12.5px', '17px'],
        base: ['13px', '18px'],
      },
    },
  },
  plugins: [],
};
