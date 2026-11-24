import React from 'react';
import ReactDOM from 'react-dom/client';

// mgck's faces, bundled so the app never fetches a font: Quicksand for the UI (the weights
// mgck's layout loads) and Anton for captions. Each splits into unicode-range subsets, which
// the webview loads only when text needs them.
import '@fontsource/quicksand/latin-300.css';
import '@fontsource/quicksand/latin-400.css';
import '@fontsource/quicksand/latin-500.css';
import '@fontsource/quicksand/latin-600.css';
import '@fontsource/quicksand/latin-700.css';
import '@fontsource/anton/latin-400.css';
import '@fontsource/anton/latin-ext-400.css';

import App from './App';
import './index.css';

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
