// Minimal offline shell for the Guardian parent app.
// Caches only the static app files; never caches ntfy traffic (always live).
const CACHE = "ocpg-parent-v1";
const SHELL = ["./", "./index.html", "./manifest.webmanifest", "./icon.svg", "./vendor/noble-ed25519.js"];

self.addEventListener("install", (e) => {
  e.waitUntil(caches.open(CACHE).then((c) => c.addAll(SHELL)).then(() => self.skipWaiting()));
});

self.addEventListener("activate", (e) => {
  e.waitUntil(
    caches.keys().then((keys) => Promise.all(keys.filter((k) => k !== CACHE).map((k) => caches.delete(k))))
      .then(() => self.clients.claim())
  );
});

self.addEventListener("fetch", (e) => {
  const url = new URL(e.request.url);
  // Only serve the app's own origin+scope from cache; let everything else (ntfy) hit the network.
  if (e.request.method !== "GET" || url.origin !== self.location.origin) return;
  e.respondWith(
    caches.match(e.request).then((hit) => hit || fetch(e.request).catch(() => caches.match("./index.html")))
  );
});
