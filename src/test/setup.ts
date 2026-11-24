import '@testing-library/jest-dom/vitest';

// The viewer remembers its mode in local storage, which jsdom keeps across a file's tests.
afterEach(() => localStorage.clear());
