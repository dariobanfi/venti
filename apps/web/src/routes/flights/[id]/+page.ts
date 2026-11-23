// One page for every identifier: it cannot be written out at build time,
// so the site's fallback page (`200.html`) opens it in the browser, as it
// does wherever the web server finds no file (`apps/web/AGENTS.md`).
export const prerender = false;
export const ssr = false;
