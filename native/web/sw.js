// The TypeScript client registered a service worker at /sw.js that kept the app (and the offline
// engine) on the device and answered every page load from its cache. The Rust build has none: a
// browser that still has the old one installs this, which deletes the old client's kept files,
// unregisters itself and reloads the open pages, so they load the page from the network. The
// engine's cache (isorun-engine) is left alone.
self.addEventListener('install', () => self.skipWaiting())
self.addEventListener('activate', (event) => {
  event.waitUntil(
    (async () => {
      for (const key of await caches.keys()) if (key.startsWith('isorun-app-')) await caches.delete(key)
      await self.registration.unregister()
      for (const client of await self.clients.matchAll({ type: 'window' })) client.navigate(client.url)
    })(),
  )
})
