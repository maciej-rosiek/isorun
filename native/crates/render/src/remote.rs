//! Tile enhancement away from the page's thread (the web's `tile-enhancer.ts`): a `TileJob` and
//! its `EnhancedTile` as bytes, for a Web Worker that runs the same wasm (`enhance_job`), and the
//! hook the web build sets to hand jobs to its workers. Natively the enhancer has threads and
//! never uses this.

use crate::enhance::{enhance_tile, EnhancedTile, TileJob};
use crate::labels::{Joints, Motion, TileLabel};
use crate::lights::{PaintSettings, Upscale};

/// Hands a job's bytes to a worker and calls back with the tile's bytes, or with nothing when the
/// worker failed (the page's thread then enhances the tile itself). Set only where there are
/// workers: without a runner the page's thread enhances a few tiles a frame.
pub type JobRunner = dyn Fn(Vec<u8>, Box<dyn FnOnce(Vec<u8>)>);

thread_local! {
    static RUNNER: std::cell::RefCell<Option<std::rc::Rc<JobRunner>>> = const { std::cell::RefCell::new(None) };
}

/// Set where tile jobs go (the web build's workers).
pub fn set_job_runner(runner: Box<JobRunner>) {
    RUNNER.with(|r| *r.borrow_mut() = Some(runner.into()));
}

pub(crate) fn job_runner() -> Option<std::rc::Rc<JobRunner>> {
    RUNNER.with(|r| r.borrow().clone())
}

/// A worker's whole job: the job's bytes in, the tile's bytes out.
pub fn enhance_job(job: &[u8]) -> Vec<u8> {
    match decode_job(job) {
        Some(job) => encode_tile(&enhance_tile(&job)),
        None => Vec::new(),
    }
}

fn upscale_code(u: Upscale) -> u8 {
    match u {
        Upscale::Pixel => 0,
        Upscale::Scale2x => 1,
        Upscale::Xbr => 2,
        Upscale::ScaleFx => 3,
    }
}

fn upscale_of(c: u8) -> Upscale {
    match c {
        1 => Upscale::Scale2x,
        2 => Upscale::Xbr,
        3 => Upscale::ScaleFx,
        _ => Upscale::Pixel,
    }
}

fn label_bytes(l: &TileLabel) -> [u8; 10] {
    let joints = match l.joints {
        Joints::None => 0,
        Joints::Bricks => 1,
        Joints::Blocks => 2,
        Joints::Cobbles => 3,
        Joints::Planks => 4,
        Joints::Tiles => 5,
        Joints::Cracks => 6,
    };
    let motion = match l.motion {
        Motion::None => 0,
        Motion::Breathe => 1,
        Motion::Hover => 2,
        Motion::Flicker => 3,
    };
    [l.material, l.relief, l.bright_is_high as u8, joints, l.gloss, l.wet as u8, l.glow, l.sparkle as u8, l.translucent as u8, motion]
}

/// The header: w, h (u32 each), framed, upscale, has label, the label (10), radius, levels, lift
/// (f32 each); then the pixels.
const JOB_HEADER: usize = 8 + 3 + 10 + 12;

pub fn encode_job(job: &TileJob) -> Vec<u8> {
    let mut out = Vec::with_capacity(JOB_HEADER + job.pixels.len());
    out.extend_from_slice(&job.w.to_le_bytes());
    out.extend_from_slice(&job.h.to_le_bytes());
    out.push(job.framed as u8);
    out.push(upscale_code(job.paint.upscale));
    out.push(job.label.is_some() as u8);
    out.extend_from_slice(&job.label.as_ref().map(label_bytes).unwrap_or([0; 10]));
    for f in [job.paint.radius, job.paint.levels, job.paint.lift] {
        out.extend_from_slice(&f.to_le_bytes());
    }
    out.extend_from_slice(&job.pixels);
    out
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

pub fn decode_job(b: &[u8]) -> Option<TileJob> {
    let (w, h) = (u32_at(b, 0)?, u32_at(b, 4)?);
    let f = |at: usize| u32_at(b, at).map(f32::from_bits);
    let pixels = b.get(JOB_HEADER..)?.to_vec();
    if pixels.len() != (w * h * 4) as usize {
        return None;
    }
    let label = (b[10] == 1).then(|| crate::labels::decode_label(&b[11..21]));
    Some(TileJob { pixels, w, h, framed: b[8] == 1, paint: PaintSettings { radius: f(21)?, levels: f(25)?, lift: f(29)?, upscale: upscale_of(b[9]) }, label })
}

pub fn encode_tile(t: &EnhancedTile) -> Vec<u8> {
    let mut out = Vec::with_capacity(8 + t.colour.len() * 3);
    out.extend_from_slice(&t.w.to_le_bytes());
    out.extend_from_slice(&t.h.to_le_bytes());
    out.extend_from_slice(&t.colour);
    out.extend_from_slice(&t.normal);
    out.extend_from_slice(&t.surface);
    out
}

pub fn decode_tile(b: &[u8]) -> Option<EnhancedTile> {
    let (w, h) = (u32_at(b, 0)?, u32_at(b, 4)?);
    let n = (w * h * 4) as usize;
    if b.len() != 8 + n * 3 {
        return None;
    }
    Some(EnhancedTile { w, h, colour: b[8..8 + n].to_vec(), normal: b[8 + n..8 + 2 * n].to_vec(), surface: b[8 + 2 * n..].to_vec() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_job_and_its_tile_go_through_bytes() {
        let pixels: Vec<u8> = (0..8 * 8 * 4).map(|i| (i * 7 % 256) as u8).collect();
        let job = TileJob { pixels, w: 8, h: 8, framed: true, paint: PaintSettings { radius: 1.5, levels: 6.0, lift: 0.25, upscale: Upscale::Xbr }, label: Some(crate::labels::UNLABELLED) };
        let back = decode_job(&encode_job(&job)).unwrap();
        assert_eq!(back.pixels, job.pixels);
        assert_eq!((back.w, back.h, back.framed, back.label), (8, 8, true, job.label));
        assert_eq!(back.paint, job.paint);
        let tile = decode_tile(&enhance_job(&encode_job(&job))).unwrap();
        let direct = enhance_tile(&job);
        assert_eq!((tile.w, tile.h), (direct.w, direct.h));
        assert_eq!(tile.colour, direct.colour);
        assert_eq!(tile.normal, direct.normal);
        assert_eq!(tile.surface, direct.surface);
    }
}
