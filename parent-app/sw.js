// Offline shell for the Guardian parent app.
// NETWORK-FIRST for the app's own files so a new release is always picked up (review B8: the old
// cache-first SW served stale code forever, leaving the phone signing an outdated format). ntfy
// traffic is never touched here. Bump CACHE on every release.
const CACHE = "ocpg-parent-v3";
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

// --- Web Push: wake the parent in the background (install-approval works while the app is closed,
// the only way to receive on iOS 16.4+). We show a GENERIC notification on purpose: the push arrives
// unverified through the broker, so we never render its content or trust it — tapping opens the app,
// which re-connects (SSE) and shows the real, signature-verified card. ---
self.addEventListener("push", (e) => {
  let body = "Tap to review a request.";
  try {
    const d = e.data ? e.data.json() : null;
    const inner = d && d.message ? JSON.parse(d.message) : null;
    if (inner && inner.kind === "alert") body = "Security alert — tap to view.";
  } catch (_) {}
  e.waitUntil(self.registration.showNotification("Guardian", {
    body, icon: "./icon.svg", badge: "./icon.svg", tag: "guardian-review", renotify: true,
    data: { url: "./" }
  }));
});

self.addEventListener("notificationclick", (e) => {
  e.notification.close();
  e.waitUntil(
    self.clients.matchAll({ type: "window", includeUncontrolled: true }).then((cs) => {
      for (const c of cs) { if ("focus" in c) return c.focus(); }
      return self.clients.openWindow ? self.clients.openWindow("./") : undefined;
    })
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
