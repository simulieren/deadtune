// Keeps a copy of the bridge page so the game's panel opens it without a connection to
// GitHub. The page itself comes from the network first, so an update lands at once; only
// a good answer replaces the copy, never an error page.
"use strict";
var CACHE = "deadtune-bridge-v2";

self.addEventListener("install", function (e) {
    e.waitUntil(caches.open(CACHE).then(function (c) { return c.addAll(["./"]); }));
    self.skipWaiting();
});

self.addEventListener("activate", function (e) {
    e.waitUntil(caches.keys().then(function (names) {
        return Promise.all(names.filter(function (n) { return n !== CACHE; }).map(function (n) { return caches.delete(n); }));
    }).then(function () { return self.clients.claim(); }));
});

self.addEventListener("fetch", function (e) {
    var url = new URL(e.request.url);
    if (url.origin !== self.location.origin || e.request.method !== "GET") { return; }
    e.respondWith(fetch(e.request).then(function (r) {
        if (r.ok) {
            var copy = r.clone();
            caches.open(CACHE).then(function (c) { c.put(url.pathname, copy); });
        }
        return r;
    }).catch(function () {
        return caches.match(url.pathname);
    }));
});
