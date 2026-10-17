import { yawToDir, type Camera, type Dir8 } from '@isorun/scene'
import { DEFAULT_VIEW, groundBasis, type IsoView } from './plan.js'

const DEG = Math.PI / 180
const ELEVATION_MIN_DEGREES = 8
const ELEVATION_MAX_DEGREES = 88
const ELEVATION_DEGREES = 34
const ELEVATION_ZOOMED_IN_DEGREES = 24
const ELEVATION_ZOOMED_OUT_DEGREES = 50
const EASE_PER_SECOND = 12
const ZOOM_EASE_PER_SECOND = 10
const FOLLOW_SECONDS = 0.1
const SLOW_FOLLOW_SECONDS = 0.3
const FOLLOW_CHANGE_PER_SECOND = 3
export const TURN_DEGREES = 45
export const ZOOM_MIN = 24
export const ZOOM_MAX = 160
export const TILT_MIN = -20
export const TILT_MAX = 50

/** Where the camera looks from, in degrees: `rotation` around the map, `tilt` up (more from above) or down from the angle the zoom gives, and `perspective`, the field of view (0 for the isometric view). */
export interface CameraAngles {
  rotation: number
  tilt: number
  perspective: number
}

export const DEFAULT_ANGLES: CameraAngles = { rotation: 45, tilt: 0, perspective: 0 }

/**
 * The isometric camera: it follows the player on a spring, turns, tilts and zooms with an easing, and can be dragged away from the player until the player moves.
 * `update` moves it on; `camera` and `view` are what the renderer takes.
 */
export class IsoCamera {
  private yawTarget: number
  private yaw: number
  private tiltTarget: number
  private tilt: number
  private perspectiveTarget: number
  private perspective = 0
  private zoomTarget = DEFAULT_VIEW.zoom
  private zoom = DEFAULT_VIEW.zoom
  private focus = { x: 0, y: 0 }
  private player = { x: 0, y: 0 }
  private pan = { x: 0, y: 0 }
  private follow = { vx: 0, vy: 0, seconds: FOLLOW_SECONDS }
  private placed = false
  /** The camera follows the player more slowly, for a travel of many steps. */
  slowFollow = false

  constructor(angles: CameraAngles = DEFAULT_ANGLES, zoom: number = DEFAULT_VIEW.zoom) {
    this.zoomTarget = this.zoom = clampZoom(zoom)
    this.yawTarget = this.yaw = angles.rotation * DEG
    this.tiltTarget = this.tilt = angles.tilt
    this.perspectiveTarget = angles.perspective
  }

  /** The angles the camera goes to. */
  get angles(): CameraAngles {
    return { rotation: this.yawTarget / DEG, tilt: this.tiltTarget, perspective: this.perspectiveTarget }
  }

  /** The zoom it eases to, in pixels a cell. */
  get zoomGoal(): number {
    return this.zoomTarget
  }

  /** The compass direction that is up on the screen, once the camera ends its turn. */
  get facing(): Dir8 {
    return yawToDir(this.yawTarget)
  }

  /** The rotation it is turning to, in radians, for screen-relative directions. */
  get yawGoal(): number {
    return this.yawTarget
  }

  get view(): IsoView {
    return { ...DEFAULT_VIEW, zoom: this.zoom, fov: this.perspective }
  }

  get camera(): Camera {
    return { x: this.player.x, y: this.player.y, eyeX: this.focus.x, eyeY: this.focus.y, yaw: this.yaw, pitch: -this.elevation(), facing: yawToDir(this.yaw) }
  }

  /** Turn the shorter way to these angles. */
  setAngles(angles: CameraAngles) {
    const turnBy = ((((angles.rotation * DEG - this.yawTarget) % (2 * Math.PI)) + 3 * Math.PI) % (2 * Math.PI)) - Math.PI
    this.yawTarget += turnBy
    this.tiltTarget = angles.tilt
    this.perspectiveTarget = angles.perspective
  }

  turn(degrees: number) {
    this.yawTarget += degrees * DEG
  }

  tiltBy(degrees: number) {
    this.tiltTarget = Math.min(TILT_MAX, Math.max(TILT_MIN, this.tiltTarget + degrees))
  }

  zoomBy(factor: number) {
    this.zoomTarget = clampZoom(this.zoomTarget * factor)
  }

  /** Move the view with the map under a pointer that moved this many CSS pixels; the view goes back to the player when the player moves. */
  drag(dx: number, dy: number) {
    const { right, forward } = groundBasis(this.yaw)
    const across = -dx / this.zoom
    const ahead = dy / (this.zoom * Math.sin(this.elevation()))
    const x = right.x * across + forward.x * ahead
    const y = right.y * across + forward.y * ahead
    this.pan = { x: this.pan.x + x, y: this.pan.y + y }
    this.focus = { x: this.focus.x + x, y: this.focus.y + y }
  }

  /** Put the view on the player at once, as on a new level. */
  snapTo(x: number, y: number) {
    this.player = { x, y }
    this.focus = { x, y }
    this.pan = { x: 0, y: 0 }
    this.follow = { vx: 0, vy: 0, seconds: this.follow.seconds }
    this.placed = true
  }

  /** Move the camera on by `dt` seconds toward the player at this cell; true while it still moves. */
  update(dt: number, x: number, y: number): boolean {
    if (!this.placed) this.snapTo(x, y)
    if (x !== this.player.x || y !== this.player.y) {
      this.player = { x, y }
      this.pan = { x: 0, y: 0 }
    }
    const t = 1 - Math.exp(-EASE_PER_SECOND * dt)
    this.yaw = Math.abs(this.yawTarget - this.yaw) < 1e-4 ? this.yawTarget : this.yaw + (this.yawTarget - this.yaw) * t
    this.tilt = Math.abs(this.tiltTarget - this.tilt) < 0.01 ? this.tiltTarget : this.tilt + (this.tiltTarget - this.tilt) * t
    this.perspective = Math.abs(this.perspectiveTarget - this.perspective) < 0.01 ? this.perspectiveTarget : this.perspective + (this.perspectiveTarget - this.perspective) * t
    const zoomT = 1 - Math.exp(-ZOOM_EASE_PER_SECOND * dt)
    this.zoom = Math.abs(this.zoomTarget - this.zoom) < 0.01 ? this.zoomTarget : this.zoom * Math.pow(this.zoomTarget / this.zoom, zoomT)
    const followTarget = this.slowFollow ? SLOW_FOLLOW_SECONDS : FOLLOW_SECONDS
    const seconds = this.follow.seconds + (followTarget - this.follow.seconds) * (1 - Math.exp(-FOLLOW_CHANGE_PER_SECOND * dt))
    const fx = smoothDamp(this.focus.x, x + this.pan.x, this.follow.vx, seconds, dt)
    const fy = smoothDamp(this.focus.y, y + this.pan.y, this.follow.vy, seconds, dt)
    const moving = this.yaw !== this.yawTarget || this.tilt !== this.tiltTarget || this.perspective !== this.perspectiveTarget || this.zoom !== this.zoomTarget || Math.hypot(fx.value - this.focus.x, fy.value - this.focus.y) > 1e-4
    this.focus = { x: fx.value, y: fy.value }
    this.follow = { vx: fx.velocity, vy: fy.velocity, seconds }
    return moving
  }

  /** The angle the camera looks down at: the zoom's angle plus the tilt. */
  private elevation() {
    return Math.min(ELEVATION_MAX_DEGREES * DEG, Math.max(ELEVATION_MIN_DEGREES * DEG, elevationFor(this.zoom) + this.tilt * DEG))
  }
}

function clampZoom(zoom: number) {
  return Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, zoom))
}

/** The camera looks lower the closer it is, for a film-like view, and higher when it is far. */
function elevationFor(zoom: number) {
  const base = DEFAULT_VIEW.zoom
  if (zoom >= base) return (ELEVATION_DEGREES + (ELEVATION_ZOOMED_IN_DEGREES - ELEVATION_DEGREES) * (Math.log(zoom / base) / Math.log(ZOOM_MAX / base))) * DEG
  return (ELEVATION_DEGREES + (ELEVATION_ZOOMED_OUT_DEGREES - ELEVATION_DEGREES) * (Math.log(base / zoom) / Math.log(base / ZOOM_MIN))) * DEG
}

/** A critically damped spring toward a target that moves: the speed carries over between steps, so a walk of many steps reads as one glide. */
function smoothDamp(value: number, target: number, velocity: number, seconds: number, dt: number) {
  const omega = 2 / seconds
  const x = omega * dt
  const decay = 1 / (1 + x + 0.48 * x * x + 0.235 * x * x * x)
  const change = value - target
  const temp = (velocity + omega * change) * dt
  return { value: target + (change + temp) * decay, velocity: (velocity - omega * temp) * decay }
}
