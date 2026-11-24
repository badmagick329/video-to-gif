/** @type {import('tailwindcss').Config} */
export default {
  content: ['./index.html', './src/**/*.{ts,tsx}'],
  theme: {
    extend: {
      // mgck's Discord GIF ("-dg") theme, from its tailwind.config.ts. The colours are CSS
      // variables set in src/index.css, light and dark.
      boxShadow: {
        'glow-primary-dg':
          '0 0 10px hsla(25, 95%, 53%, 1), 0 0 2px hsla(25, 95%, 53%, 1) inset',
        'glow-secondary-dg':
          '0 0 10px hsla(0, 0%, 10%, 1), 0 0 2px hsla(0, 0%, 10%, 1) inset',
      },
      colors: {
        border: 'hsl(var(--border))',
        'background-dg': 'hsl(var(--background-dg))',
        'foreground-dg': 'hsl(var(--foreground-dg))',
        'primary-dg': {
          DEFAULT: 'hsl(var(--primary-dg))',
          foreground: 'hsl(var(--primary-dg-foreground))',
        },
        'secondary-dg': {
          DEFAULT: 'hsl(var(--secondary-dg))',
          foreground: 'hsl(var(--secondary-dg-foreground))',
        },
      },
      borderRadius: {
        lg: 'var(--radius)',
        md: 'calc(var(--radius) - 2px)',
        sm: 'calc(var(--radius) - 4px)',
      },
      fontFamily: {
        // Bundled from @fontsource (src/main.tsx), so nothing is fetched at runtime.
        sans: ['Quicksand', 'ui-sans-serif', 'system-ui', 'sans-serif'],
      },
    },
  },
  plugins: [],
};
