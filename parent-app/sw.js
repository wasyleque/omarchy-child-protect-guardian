// Offline shell for the Guardian parent app.
// NETWORK-FIRST for the app's own files so a new release is always picked up (review B8: the old
// cache-first SW served stale code forever, leaving the phone signing an outdated format). ntfy
// traffic is never touched here. Bump CACHE on every release.
const CACHE = "ocpg-parent-v2";
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
  if (e.request.method !== "GET" || url.origin !== self.location.origin) return; // ntfy etc. → network
  // Network-first: always try the latest, fall back to cache only when offline.
  e.respondWith(
    fetch(e.request)
      .then((res) => {
        const copy = res.clone();
        caches.open(CACHE).then((c) => c.put(e.request, copy)).catch(() => {});
        return res;
      })
      .catch(() => caches.match(e.request).then((hit) => hit || caches.match("./index.html")))
  );
});
