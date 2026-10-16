/**
 * What the tools need from DCSS's rltiles: the tileinfo modules of a gamedata
 * version, evaluated in their dependency order, and 8-bit PNGs read and
 * written without dependencies.
 */
import fs from 'node:fs'
import path from 'node:path'
import zlib from 'node:zlib'

// ---------------------------------------------------------------- tileinfo
function evalAmd(src, dep) {
  let result
  const define = (a, b) => {
    const factory = typeof a === 'function' ? a : b
    result = factory(dep)
  }
  new Function('define', 'window', src)(define, {})
  return result
}

/** The tileinfo modules in `order` from `dir`, each with its first id, its end and its atlas name. */
export function loadTileinfo(dir, order) {
  const modules = {}
  let prev
  for (const name of order) {
    const exp = evalAmd(fs.readFileSync(path.join(dir, `tileinfo-${name}.js`), 'utf8'), prev)
    const maxKey = Object.keys(exp).find((k) => /^(FLOOR|WALL|FEAT|MAIN|PLAYER)_MAX$/.test(k))
    modules[name] = { exports: exp, first: prev ? modules[order[order.indexOf(name) - 1]].end : 0, end: exp[maxKey], atlas: name }
    prev = exp
  }
  return modules
}

// ---------------------------------------------------------------- png
function crc32(buf) {
  return zlib.crc32 ? zlib.crc32(buf) : crcSlow(buf)
}
function crcSlow(buf) {
  let c = ~0
  for (const b of buf) {
    c ^= b
    for (let k = 0; k < 8; k++) c = (c >>> 1) ^ (0xedb88320 & -(c & 1))
  }
  return ~c >>> 0
}
export function readPng(file) {
  const b = fs.readFileSync(file)
  if (b.readUInt32BE(0) !== 0x89504e47) throw new Error(`${file}: not a PNG`)
  let p = 8
  let w = 0, h = 0, depth = 0, ctype = 0, interlace = 0
  const idat = []
  while (p < b.length) {
    const len = b.readUInt32BE(p)
    const type = b.toString('latin1', p + 4, p + 8)
    const data = b.subarray(p + 8, p + 8 + len)
    if (type === 'IHDR') {
      w = data.readUInt32BE(0)
      h = data.readUInt32BE(4)
      depth = data[8]
      ctype = data[9]
      interlace = data[12]
    } else if (type === 'IDAT') idat.push(data)
    p += 12 + len
  }
  if (depth !== 8 || (ctype !== 6 && ctype !== 2) || interlace) throw new Error(`${file}: only 8-bit RGB/RGBA non-interlaced PNGs are read (depth ${depth}, colour type ${ctype}, interlace ${interlace})`)
  const bpp = ctype === 6 ? 4 : 3
  const raw = zlib.inflateSync(Buffer.concat(idat))
  const stride = w * bpp
  const out = Buffer.alloc(w * h * 4)
  let prevRow = Buffer.alloc(stride)
  for (let y = 0; y < h; y++) {
    const f = raw[y * (stride + 1)]
    const row = Buffer.from(raw.subarray(y * (stride + 1) + 1, (y + 1) * (stride + 1)))
    for (let i = 0; i < stride; i++) {
      const a = i >= bpp ? row[i - bpp] : 0
      const up = prevRow[i]
      const c = i >= bpp ? prevRow[i - bpp] : 0
      let v = row[i]
      if (f === 1) v += a
      else if (f === 2) v += up
      else if (f === 3) v += (a + up) >> 1
      else if (f === 4) {
        const pp = a + up - c
        const pa = Math.abs(pp - a), pb = Math.abs(pp - up), pc = Math.abs(pp - c)
        v += pa <= pb && pa <= pc ? a : pb <= pc ? up : c
      }
      row[i] = v & 0xff
    }
    for (let x = 0; x < w; x++) {
      const s = x * bpp, d = (y * w + x) * 4
      out[d] = row[s]
      out[d + 1] = row[s + 1]
      out[d + 2] = row[s + 2]
      out[d + 3] = bpp === 4 ? row[s + 3] : 255
    }
    prevRow = row
  }
  return { width: w, height: h, data: out }
}
export function writePng(file, { width, height, data }) {
  const chunk = (type, body) => {
    const len = Buffer.alloc(4)
    len.writeUInt32BE(body.length)
    const td = Buffer.concat([Buffer.from(type, 'latin1'), body])
    const crc = Buffer.alloc(4)
    crc.writeUInt32BE(crc32(td) >>> 0)
    return Buffer.concat([len, td, crc])
  }
  const ihdr = Buffer.alloc(13)
  ihdr.writeUInt32BE(width, 0)
  ihdr.writeUInt32BE(height, 4)
  ihdr[8] = 8
  ihdr[9] = 6
  const raw = Buffer.alloc((width * 4 + 1) * height)
  for (let y = 0; y < height; y++) {
    raw[y * (width * 4 + 1)] = 0
    data.copy(raw, y * (width * 4 + 1) + 1, y * width * 4, (y + 1) * width * 4)
  }
  fs.writeFileSync(file, Buffer.concat([Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]), chunk('IHDR', ihdr), chunk('IDAT', zlib.deflateSync(raw, { level: 9 })), chunk('IEND', Buffer.alloc(0))]))
}


// ---------------------------------------------------------------- tile hash
/**
 * A tile's key for its labels: FNV-1a over its size and its pixels, with every
 * texel under half alpha as nothing, so the art finds its labels on any server
 * that has it. `@isorun/render-iso`'s `tileHash` computes the same.
 */
export function tileHash(data, width, height) {
  let h = 0x811c9dc5
  const mix = (byte) => {
    h ^= byte
    h = Math.imul(h, 0x01000193) >>> 0
  }
  for (const v of [width & 255, width >> 8, height & 255, height >> 8]) mix(v)
  for (let i = 0; i < width * height * 4; i += 4) {
    const opaque = data[i + 3] >= 128
    mix(opaque ? data[i] : 0)
    mix(opaque ? data[i + 1] : 0)
    mix(opaque ? data[i + 2] : 0)
    mix(opaque ? 1 : 0)
  }
  return h.toString(16).padStart(8, '0')
}
