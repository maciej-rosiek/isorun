import { app, BrowserWindow, net, protocol, shell } from 'electron'
import { join } from 'node:path'
import { ORIGIN, serve, type GameFiles } from './serve'

const gameFiles: GameFiles = app.isPackaged
  ? { web: join(process.resourcesPath, 'web'), engine: join(process.resourcesPath, 'engine') }
  : { web: join(import.meta.dirname, '../../../apps/isorun/dist'), engine: join(import.meta.dirname, '../../../engine/dist') }

const startWindowed = process.argv.includes('--windowed')

let mainWindow: BrowserWindow | null = null

function isGameUrl(url: string): boolean {
  return URL.canParse(url) && new URL(url).origin === ORIGIN
}

function openOutside(url: string) {
  if (/^https?:/.test(url)) void shell.openExternal(url)
}

function serveGameFiles() {
  protocol.handle('https', (request) => {
    const url = new URL(request.url)
    if (url.origin === ORIGIN) return serve(url, gameFiles)
    return net.fetch(request, { bypassCustomProtocolHandlers: true })
  })
}

function openWindow() {
  const window = new BrowserWindow({
    title: 'Isorun',
    width: 1280,
    height: 800,
    fullscreen: !startWindowed,
    backgroundColor: '#000000',
    show: false,
  })
  mainWindow = window
  window.removeMenu()
  window.once('ready-to-show', () => window.show())
  window.on('closed', () => {
    mainWindow = null
  })
  window.webContents.setWindowOpenHandler(({ url }) => {
    if (!isGameUrl(url)) openOutside(url)
    return { action: 'deny' }
  })
  window.webContents.on('will-navigate', (event) => {
    if (isGameUrl(event.url)) return
    event.preventDefault()
    openOutside(event.url)
  })
  window.webContents.on('before-input-event', (event, input) => {
    if (input.type !== 'keyDown' || input.key !== 'F11') return
    event.preventDefault()
    window.setFullScreen(!window.isFullScreen())
  })
  void window.loadURL(`${ORIGIN}/?fullscreen`)
}

if (!app.requestSingleInstanceLock()) {
  app.quit()
} else {
  app.on('second-instance', () => {
    if (!mainWindow) return
    if (mainWindow.isMinimized()) mainWindow.restore()
    mainWindow.focus()
  })
  app.on('window-all-closed', () => app.quit())
  void app.whenReady().then(() => {
    serveGameFiles()
    openWindow()
  })
}
