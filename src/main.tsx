import React from 'react';
import ReactDOM from 'react-dom/client';

// The app's faces, bundled so it never fetches a font: Inter for the UI, JetBrains Mono for
// timecodes and sizes, and Anton for captions. Each splits into unicode-range subsets, which the
// webview loads only when text needs them.
import '@fontsource-variable/inter/wght.css';
import '@fontsource-variable/jetbrains-mono/wght.css';
import '@fontsource/anton/latin-400.css';
import '@fontsource/anton/latin-ext-400.css';

import App from './App';
import './index.css';

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
