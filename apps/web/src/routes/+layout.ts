// Every page is written out as a file when the app is built, so any web
// server can serve it and no Node process runs in production.
export const prerender = true;

// Each page is its own folder with an index.html in it, which every static
// file server finds without being told how.
export const trailingSlash = 'always';
