//! The isometric camera (the port of `camera.ts`): it follows the player on a spring, turns,
//! tilts and zooms with an easing, and can be dragged away from the player until the player moves.

use crate::plan::{ground_basis, IsoView, DEFAULT_VIEW};
use isorun_scene::{yaw_to_dir, Camera, Dir8};

const DEG: f32 = std::f32::consts::PI / 180.0;
const ELEVATION_MIN_DEGREES: f32 = 8.0;
const ELEVATION_MAX_DEGREES: f32 = 88.0;
const ELEVATION_DEGREES: f32 = 34.0;
const ELEVATION_ZOOMED_IN_DEGREES: f32 = 24.0;
const ELEVATION_ZOOMED_OUT_DEGREES: f32 = 50.0;
const EASE_PER_SECOND: f32 = 12.0;
const ZOOM_EASE_PER_SECOND: f32 = 10.0;
const FOLLOW_SECONDS: f32 = 0.1;
const SLOW_FOLLOW_SECONDS: f32 = 0.3;
const FOLLOW_CHANGE_PER_SECOND: f32 = 3.0;
pub const TURN_DEGREES: f32 = 45.0;
pub const ZOOM_MIN: f32 = 24.0;
pub const ZOOM_MAX: f32 = 160.0;
pub const TILT_MIN: f32 = -20.0;
pub const TILT_MAX: f32 = 50.0;

/// Where the camera looks from, in degrees: `rotation` around the map, `tilt` up or down from the
/// angle the zoom gives, and `perspective`, the field of view (0 for the isometric view).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraAngles {
    pub rotation: f32,
    pub tilt: f32,
    pub perspective: f32,
}

pub const DEFAULT_ANGLES: CameraAngles = CameraAngles { rotation: 45.0, tilt: 0.0, perspective: 0.0 };

#[derive(Clone, Debug)]
pub struct IsoCamera {
    yaw_target: f32,
    yaw: f32,
    tilt_target: f32,
    tilt: f32,
    perspective_target: f32,
    perspective: f32,
    zoom_target: f32,
    zoom: f32,
    focus: (f32, f32),
    player: (i32, i32),
    pan: (f32, f32),
    follow: (f32, f32, f32),
    placed: bool,
    /// The camera follows the player more slowly, for a travel of many steps.
    pub slow_follow: bool,
}

impl Default for IsoCamera {
    fn default() -> Self {
        IsoCamera::new(DEFAULT_ANGLES, DEFAULT_VIEW.zoom)
    }
}

impl IsoCamera {
    pub fn new(angles: CameraAngles, zoom: f32) -> Self {
        let zoom = clamp_zoom(zoom);
        IsoCamera {
            yaw_target: angles.rotation * DEG,
            yaw: angles.rotation * DEG,
            tilt_target: angles.tilt,
            tilt: angles.tilt,
            perspective_target: angles.perspective,
            perspective: 0.0,
            zoom_target: zoom,
            zoom,
            focus: (0.0, 0.0),
            player: (0, 0),
            pan: (0.0, 0.0),
            follow: (0.0, 0.0, FOLLOW_SECONDS),
            placed: false,
            slow_follow: false,
        }
    }

    pub fn angles(&self) -> CameraAngles {
        CameraAngles { rotation: self.yaw_target / DEG, tilt: self.tilt_target, perspective: self.perspective_target }
    }

    pub fn zoom_goal(&self) -> f32 {
        self.zoom_target
    }

    /// The compass direction that is up on the screen, once the camera ends its turn.
    pub fn facing(&self) -> Dir8 {
        yaw_to_dir(self.yaw_target as f64)
    }

    /// The rotation it is turning to, in radians, for screen-relative directions.
    pub fn yaw_goal(&self) -> f32 {
        self.yaw_target
    }

    pub fn view(&self) -> IsoView {
        IsoView { zoom: self.zoom, fov: self.perspective, ..DEFAULT_VIEW }
    }

    pub fn camera(&self) -> Camera {
        Camera {
            x: self.player.0,
            y: self.player.1,
            eye_x: self.focus.0 as f64,
            eye_y: self.focus.1 as f64,
            yaw: self.yaw as f64,
            pitch: -self.elevation() as f64,
            facing: yaw_to_dir(self.yaw as f64),
        }
    }

    /// Turn the shorter way to these angles.
    pub fn set_angles(&mut self, angles: CameraAngles) {
        let tau = 2.0 * std::f32::consts::PI;
        let turn_by = (((angles.rotation * DEG - self.yaw_target) % tau) + 3.0 * std::f32::consts::PI).rem_euclid(tau) - std::f32::consts::PI;
        self.yaw_target += turn_by;
        self.tilt_target = angles.tilt;
        self.perspective_target = angles.perspective;
    }

    pub fn turn(&mut self, degrees: f32) {
        self.yaw_target += degrees * DEG;
    }

    /// End a free turn (two fingers) on the nearest whole step of `TURN_DEGREES`.
    pub fn snap_turn(&mut self) {
        let step = TURN_DEGREES * DEG;
        self.yaw_target = (self.yaw_target / step).round() * step;
    }

    pub fn tilt_by(&mut self, degrees: f32) {
        self.tilt_target = (self.tilt_target + degrees).clamp(TILT_MIN, TILT_MAX);
    }

    pub fn zoom_by(&mut self, factor: f32) {
        self.zoom_target = clamp_zoom(self.zoom_target * factor);
    }

    /// Move the view with the map under a pointer that moved this many logical pixels.
    pub fn drag(&mut self, dx: f32, dy: f32) {
        let (right, forward) = ground_basis(self.yaw);
        let across = -dx / self.zoom;
        let ahead = dy / (self.zoom * self.elevation().sin());
        let x = right[0] * across + forward[0] * ahead;
        let y = right[1] * across + forward[1] * ahead;
        self.pan = (self.pan.0 + x, self.pan.1 + y);
        self.focus = (self.focus.0 + x, self.focus.1 + y);
    }

    /// Put the view on the player at once, as on a new level.
    pub fn snap_to(&mut self, x: i32, y: i32) {
        self.player = (x, y);
        self.focus = (x as f32, y as f32);
        self.pan = (0.0, 0.0);
        self.follow = (0.0, 0.0, self.follow.2);
        self.placed = true;
    }

    /// Move the camera on by `dt` seconds toward the player at this cell; true while it still moves.
    pub fn update(&mut self, dt: f32, x: i32, y: i32) -> bool {
        if !self.placed {
            self.snap_to(x, y);
        }
        if (x, y) != self.player {
            self.player = (x, y);
            self.pan = (0.0, 0.0);
        }
        let t = 1.0 - (-EASE_PER_SECOND * dt).exp();
        self.yaw = if (self.yaw_target - self.yaw).abs() < 1e-4 { self.yaw_target } else { self.yaw + (self.yaw_target - self.yaw) * t };
        self.tilt = if (self.tilt_target - self.tilt).abs() < 0.01 { self.tilt_target } else { self.tilt + (self.tilt_target - self.tilt) * t };
        self.perspective = if (self.perspective_target - self.perspective).abs() < 0.01 { self.perspective_target } else { self.perspective + (self.perspective_target - self.perspective) * t };
        let zt = 1.0 - (-ZOOM_EASE_PER_SECOND * dt).exp();
        self.zoom = if (self.zoom_target - self.zoom).abs() < 0.01 { self.zoom_target } else { self.zoom * (self.zoom_target / self.zoom).powf(zt) };
        let follow_target = if self.slow_follow { SLOW_FOLLOW_SECONDS } else { FOLLOW_SECONDS };
        let seconds = self.follow.2 + (follow_target - self.follow.2) * (1.0 - (-FOLLOW_CHANGE_PER_SECOND * dt).exp());
        let (fx, vx) = smooth_damp(self.focus.0, x as f32 + self.pan.0, self.follow.0, seconds, dt);
        let (fy, vy) = smooth_damp(self.focus.1, y as f32 + self.pan.1, self.follow.1, seconds, dt);
        let moving = self.yaw != self.yaw_target
            || self.tilt != self.tilt_target
            || self.perspective != self.perspective_target
            || self.zoom != self.zoom_target
            || ((fx - self.focus.0).powi(2) + (fy - self.focus.1).powi(2)).sqrt() > 1e-4;
        self.focus = (fx, fy);
        self.follow = (vx, vy, seconds);
        moving
    }

    /// The angle the camera looks down at: the zoom's angle plus the tilt.
    fn elevation(&self) -> f32 {
        (elevation_for(self.zoom) + self.tilt * DEG).clamp(ELEVATION_MIN_DEGREES * DEG, ELEVATION_MAX_DEGREES * DEG)
    }
}

fn clamp_zoom(zoom: f32) -> f32 {
    zoom.clamp(ZOOM_MIN, ZOOM_MAX)
}

/// The camera looks lower the closer it is, and higher when it is far.
fn elevation_for(zoom: f32) -> f32 {
    let base = DEFAULT_VIEW.zoom;
    if zoom >= base {
        (ELEVATION_DEGREES + (ELEVATION_ZOOMED_IN_DEGREES - ELEVATION_DEGREES) * ((zoom / base).ln() / (ZOOM_MAX / base).ln())) * DEG
    } else {
        (ELEVATION_DEGREES + (ELEVATION_ZOOMED_OUT_DEGREES - ELEVATION_DEGREES) * ((base / zoom).ln() / (base / ZOOM_MIN).ln())) * DEG
    }
}

/// A critically damped spring toward a moving target.
fn smooth_damp(value: f32, target: f32, velocity: f32, seconds: f32, dt: f32) -> (f32, f32) {
    let omega = 2.0 / seconds;
    let x = omega * dt;
    let decay = 1.0 / (1.0 + x + 0.48 * x * x + 0.235 * x * x * x);
    let change = value - target;
    let temp = (velocity + omega * change) * dt;
    (target + (change + temp) * decay, (velocity - omega * temp) * decay)
}
