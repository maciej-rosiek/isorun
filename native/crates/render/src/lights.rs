//! How the dungeon is lit and drawn: the lights, the shading, the tiles, the walls and the screen
//! effects (the port of `lights.ts`), and how much work a frame does (`quality.ts`).

/// A colour as `0xrrggbb`, in sRGB as the web build gives it to three.
pub type Hex = u32;

/// `0xrrggbb` (sRGB) to linear 0..1, as three's `Color.set` does with colour management on.
pub fn linear(hex: Hex) -> [f32; 3] {
    let c = |v: u32| {
        let s = v as f32 / 255.0;
        if s <= 0.04045 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    [c((hex >> 16) & 255), c((hex >> 8) & 255), c(hex & 255)]
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointLightSpec {
    pub colour: Hex,
    pub intensity: f32,
    /// Cells to where the light fades out.
    pub distance: f32,
    pub decay: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadingSettings {
    pub wrap: f32,
    pub bands: f32,
    pub relief: f32,
    pub depth: f32,
    pub sprites: f32,
    pub gloss: f32,
    pub rim: f32,
    pub cavity: f32,
    pub variation: f32,
    pub wetness: f32,
    pub glow: f32,
    pub sparkle: f32,
    pub dust: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Upscale {
    Pixel,
    Scale2x,
    Xbr,
    ScaleFx,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaintSettings {
    pub radius: f32,
    pub levels: f32,
    pub lift: f32,
    pub upscale: Upscale,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Walls {
    pub bevel: f32,
    pub moss: f32,
    pub moss_colour: Hex,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ao {
    pub enabled: bool,
    pub intensity: f32,
    pub radius: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Outline {
    pub enabled: bool,
    pub thickness: f32,
    pub strength: f32,
    pub colour: Hex,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Painted {
    pub enabled: bool,
    pub radius: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pixelate {
    pub enabled: bool,
    pub size: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bloom {
    pub enabled: bool,
    pub strength: f32,
    pub radius: f32,
    pub threshold: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TiltShift {
    pub enabled: bool,
    pub blur: f32,
    pub focus: f32,
    pub falloff: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grade {
    pub saturation: f32,
    pub shadows: Hex,
    pub highlights: Hex,
    pub vignette: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aim {
    pub colour: Hex,
    pub brightness: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FogSettings {
    pub enabled: bool,
    pub colour: Hex,
    pub density: f32,
    pub height: f32,
    pub reach: f32,
    pub softness: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WaterSettings {
    pub enabled: bool,
    pub deep: Hex,
    pub shallow: Hex,
    pub light: Hex,
    pub strength: f32,
    pub speed: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LavaSettings {
    pub enabled: bool,
    pub crust: Hex,
    pub molten: Hex,
    pub hot: Hex,
    pub glow: f32,
    pub speed: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CombatSettings {
    pub blood: Hex,
    pub gore: f32,
    pub shake: f32,
    pub hit_stop: f32,
    pub flash: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StepSettings {
    pub colour: Hex,
    pub dust: f32,
    pub prints: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct IsoLights {
    pub label: &'static str,
    pub exposure: f32,
    pub background: Hex,
    pub ambient_sky: Hex,
    pub ambient_ground: Hex,
    pub ambient_intensity: f32,
    pub sun_colour: Hex,
    pub sun_intensity: f32,
    pub torch: PointLightSpec,
    pub shadow_softness: f32,
    pub shading: ShadingSettings,
    pub paint: PaintSettings,
    pub walls: Walls,
    pub ao: Ao,
    pub outline: Outline,
    pub painted: Painted,
    pub pixelate: Pixelate,
    pub bloom: Bloom,
    pub tilt_shift: TiltShift,
    pub grade: Grade,
    pub aim: Aim,
    pub fog: FogSettings,
    pub water: WaterSettings,
    pub lava: LavaSettings,
    pub combat: CombatSettings,
    pub steps: StepSettings,
}

pub fn diorama() -> IsoLights {
    IsoLights {
        label: "Diorama",
        exposure: 1.0,
        background: 0x212121,
        ambient_sky: 0xe4ecff,
        ambient_ground: 0x8c7c6c,
        ambient_intensity: 1.0,
        sun_colour: 0xfff1dc,
        sun_intensity: 4.85,
        torch: PointLightSpec { colour: 0xffb26a, intensity: 6.0, distance: 6.5, decay: 0.5 },
        shadow_softness: 2.5,
        shading: ShadingSettings { wrap: 0.25, bands: 5.0, relief: 0.3, depth: 0.06, sprites: 0.2, gloss: 0.25, rim: 0.4, cavity: 0.55, variation: 0.01, wetness: 0.25, glow: 0.9, sparkle: 0.65, dust: 0.5 },
        paint: PaintSettings { radius: 0.0, levels: 0.0, lift: 1.4, upscale: Upscale::Scale2x },
        walls: Walls { bevel: 0.0, moss: 0.25, moss_colour: 0x707070 },
        ao: Ao { enabled: true, intensity: 1.0, radius: 0.85 },
        outline: Outline { enabled: false, thickness: 1.0, strength: 0.6, colour: 0x2a1e16 },
        painted: Painted { enabled: false, radius: 0.5 },
        pixelate: Pixelate { enabled: false, size: 2.5 },
        bloom: Bloom { enabled: false, strength: 0.5, radius: 0.4, threshold: 0.6 },
        tilt_shift: TiltShift { enabled: true, blur: 0.2, focus: 0.45, falloff: 0.5 },
        grade: Grade { saturation: 0.95, shadows: 0xf0f1ff, highlights: 0xfef2e1, vignette: 0.2 },
        aim: Aim { colour: 0xffd75e, brightness: 1.0 },
        fog: FogSettings { enabled: true, colour: 0xa19b9b, density: 0.4, height: 1.0, reach: 2.0, softness: 0.05 },
        water: WaterSettings { enabled: true, deep: 0x0d1e26, shallow: 0x02061d, light: 0x54899c, strength: 0.15, speed: 0.7 },
        lava: LavaSettings { enabled: true, crust: 0x1c0d09, molten: 0xb02a08, hot: 0xffa030, glow: 0.4, speed: 0.6 },
        combat: CombatSettings { blood: 0x8a0f0f, gore: 1.0, shake: 0.4, hit_stop: 0.07, flash: 1.0 },
        steps: StepSettings { colour: 0x8a7a66, dust: 1.0, prints: 6.0 },
    }
}

fn variation(change: impl FnOnce(&mut IsoLights)) -> IsoLights {
    let mut l = diorama();
    change(&mut l);
    l
}

/// The looks, by the name the settings keep (`LIGHTS` in lights.ts), in its order.
pub const LIGHTS_NAMES: [&str; 11] = ["diorama", "sunny", "storybook", "pixel", "dusk", "moonlit", "rain", "ember", "frost", "miniature", "noir"];

pub fn lights(name: &str) -> Option<IsoLights> {
    Some(match name {
        "diorama" => diorama(),
        "sunny" => variation(|l| {
            l.label = "Sunny clay";
            l.exposure = 1.8;
            l.background = 0x3a3530;
            l.ambient_intensity = 2.5;
            l.sun_colour = 0xfff4e0;
            l.sun_intensity = 5.5;
            l.shading = ShadingSettings { wrap: 0.5, bands: 3.0, relief: 0.4, sprites: 0.3, rim: 0.5, cavity: 0.45, ..l.shading };
            l.paint = PaintSettings { radius: 2.0, levels: 0.0, lift: 1.6, upscale: Upscale::Scale2x };
            l.walls = Walls { bevel: 0.1, moss: 0.25, moss_colour: 0x8a8a7a };
            l.outline = Outline { enabled: true, thickness: 1.0, strength: 0.5, colour: 0x2a1e16 };
            l.tilt_shift.enabled = false;
            l.grade = Grade { saturation: 1.15, shadows: 0xf0f1ff, highlights: 0xfff4e4, vignette: 0.1 };
        }),
        "storybook" => variation(|l| {
            l.label = "Storybook";
            l.background = 0x2a2420;
            l.shading.bands = 4.0;
            l.paint = PaintSettings { radius: 2.0, levels: 0.0, lift: 1.4, upscale: Upscale::Scale2x };
            l.outline = Outline { enabled: true, thickness: 1.0, strength: 0.7, colour: 0x2a1e16 };
            l.painted = Painted { enabled: true, radius: 0.75 };
            l.grade = Grade { saturation: 1.1, shadows: 0xe6e0ff, highlights: 0xffecd0, vignette: 0.25 };
        }),
        "pixel" => variation(|l| {
            l.label = "Pixel";
            l.pixelate = Pixelate { enabled: true, size: 2.5 };
            l.tilt_shift.enabled = false;
        }),
        "dusk" => variation(|l| {
            l.label = "Dusk";
            l.exposure = 1.7;
            l.background = 0x1a1418;
            (l.ambient_sky, l.ambient_ground, l.ambient_intensity) = (0xa8b4e0, 0x7a6858, 1.9);
            (l.sun_colour, l.sun_intensity) = (0xffb070, 4.0);
            l.torch.intensity = 8.0;
            l.grade = Grade { saturation: 0.95, shadows: 0xe0dcff, highlights: 0xffe6c8, vignette: 0.3 };
        }),
        "moonlit" => variation(|l| {
            l.label = "Moonlit";
            l.exposure = 2.0;
            l.background = 0x10141c;
            l.shading.sprites = 0.6;
            (l.ambient_sky, l.ambient_ground, l.ambient_intensity) = (0x8090c0, 0x302838, 1.4);
            (l.sun_colour, l.sun_intensity) = (0xa8c0ff, 2.5);
            l.torch = PointLightSpec { colour: 0xffa860, intensity: 9.0, distance: 8.0, decay: 1.0 };
            l.bloom = Bloom { enabled: true, strength: 0.4, radius: 0.4, threshold: 0.7 };
            l.grade = Grade { saturation: 0.85, shadows: 0xc8d4ff, highlights: 0xfff2e0, vignette: 0.35 };
        }),
        "rain" => variation(|l| {
            l.label = "Rain crypt";
            l.exposure = 1.9;
            l.background = 0x0e1216;
            (l.ambient_sky, l.ambient_ground, l.ambient_intensity) = (0x9fb4cc, 0x3c4448, 1.7);
            (l.sun_colour, l.sun_intensity) = (0xc8d8ec, 3.0);
            l.torch = PointLightSpec { colour: 0xffa858, intensity: 8.0, distance: 9.0, decay: 1.0 };
            l.shading = ShadingSettings { gloss: 0.7, rim: 0.5, cavity: 0.7, wetness: 0.9, dust: 0.2, ..l.shading };
            l.walls.moss_colour = 0x4e5a4a;
            l.water = WaterSettings { strength: 0.25, speed: 0.9, ..l.water };
            l.grade = Grade { saturation: 0.75, shadows: 0xd8e4ff, highlights: 0xfff0dc, vignette: 0.3 };
        }),
        "ember" => variation(|l| {
            l.label = "Ember forge";
            l.exposure = 1.6;
            l.background = 0x1c0e0a;
            (l.ambient_sky, l.ambient_ground, l.ambient_intensity) = (0xb4a098, 0x4a2a20, 1.3);
            (l.sun_colour, l.sun_intensity) = (0xffa070, 2.4);
            l.torch = PointLightSpec { colour: 0xff9448, intensity: 7.0, distance: 9.0, decay: 1.0 };
            l.shading = ShadingSettings { gloss: 0.35, rim: 0.7, cavity: 0.7, glow: 1.6, dust: 1.4, ..l.shading };
            l.bloom = Bloom { enabled: true, strength: 0.3, radius: 0.5, threshold: 0.7 };
            l.lava.glow = 0.8;
            l.grade = Grade { saturation: 0.9, shadows: 0xe8dcff, highlights: 0xffe8c8, vignette: 0.4 };
        }),
        "frost" => variation(|l| {
            l.label = "Frost";
            l.exposure = 2.1;
            l.background = 0x161c24;
            (l.ambient_sky, l.ambient_ground, l.ambient_intensity) = (0xd8ecff, 0x6c7888, 2.4);
            (l.sun_colour, l.sun_intensity) = (0xe4f0ff, 4.2);
            l.torch = PointLightSpec { colour: 0xffc890, intensity: 5.0, distance: 9.0, decay: 1.0 };
            l.shading = ShadingSettings { wrap: 0.35, gloss: 0.6, rim: 0.9, sparkle: 1.6, wetness: 0.4, dust: 0.3, ..l.shading };
            l.walls = Walls { bevel: 0.05, moss: 0.35, moss_colour: 0xdce8f4 };
            l.grade = Grade { saturation: 0.7, shadows: 0xd4e4ff, highlights: 0xf4f8ff, vignette: 0.2 };
        }),
        "miniature" => variation(|l| {
            l.label = "Miniature";
            l.exposure = 2.0;
            l.background = 0x2c2620;
            l.ambient_intensity = 2.5;
            l.shading = ShadingSettings { wrap: 0.55, bands: 5.0, relief: 0.8, depth: 0.08, gloss: 0.15, rim: 0.3, cavity: 0.9, variation: 0.04, ..l.shading };
            l.tilt_shift = TiltShift { enabled: true, blur: 0.55, focus: 0.25, falloff: 0.35 };
            l.grade = Grade { saturation: 1.25, shadows: 0xece4ff, highlights: 0xfff0dc, vignette: 0.15 };
        }),
        "noir" => variation(|l| {
            l.label = "Noir";
            l.exposure = 1.6;
            l.background = 0x080808;
            (l.ambient_sky, l.ambient_ground, l.ambient_intensity) = (0xc0c0c0, 0x303030, 0.9);
            (l.sun_colour, l.sun_intensity) = (0xffffff, 4.0);
            l.torch = PointLightSpec { colour: 0xfff0e0, intensity: 7.0, distance: 7.0, decay: 1.4 };
            l.shading = ShadingSettings { bands: 3.0, gloss: 0.4, rim: 1.2, cavity: 0.8, glow: 0.6, dust: 0.7, ..l.shading };
            l.shadow_softness = 1.0;
            l.grade = Grade { saturation: 0.12, shadows: 0xe8ecf4, highlights: 0xfff8ec, vignette: 0.6 };
        }),
        _ => return None,
    })
}

/// How much work the renderer does for a frame: settings that change the speed more than the look.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IsoQuality {
    pub label: &'static str,
    /// The most device pixels the view draws per logical pixel.
    pub pixel_ratio: f32,
    /// Whether the view draws fewer pixels while the GPU cannot keep up, down to 70%.
    pub adaptive: bool,
    /// Ambient occlusion: the size of its image, a part of the screen's size; 0 turns it off.
    pub ao_resolution: f32,
    pub ao_samples: u32,
    /// 4x multisampling on the scene pass.
    pub antialias: bool,
    pub shadows: bool,
}

pub const QUALITY_LOW: IsoQuality = IsoQuality { label: "Low", pixel_ratio: 1.0, adaptive: true, ao_resolution: 0.0, ao_samples: 8, antialias: false, shadows: true };
pub const QUALITY_MEDIUM: IsoQuality = IsoQuality { label: "Medium", pixel_ratio: 1.5, adaptive: true, ao_resolution: 0.5, ao_samples: 8, antialias: false, shadows: true };
pub const QUALITY_HIGH: IsoQuality = IsoQuality { label: "High", pixel_ratio: 2.0, adaptive: true, ao_resolution: 1.0, ao_samples: 16, antialias: true, shadows: true };

pub fn quality(name: &str) -> Option<IsoQuality> {
    match name {
        "low" => Some(QUALITY_LOW),
        "medium" => Some(QUALITY_MEDIUM),
        "high" => Some(QUALITY_HIGH),
        _ => None,
    }
}

/// How much of the surface work is drawn (a graphics setting apart from the look).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SurfaceDetail {
    Off,
    Low,
    Medium,
    High,
}

/// What each level of surface detail draws (`DETAIL` in shading.ts).
#[derive(Clone, Copy, Debug)]
pub struct Detail {
    pub depth_steps: u32,
    pub gloss: bool,
    pub rim: bool,
    pub cavity: bool,
    pub variation: bool,
    pub wetness: bool,
    pub glow: bool,
    pub sparkle: bool,
    pub dust: bool,
    pub statuses: bool,
    pub liquids: bool,
}

impl SurfaceDetail {
    pub fn detail(self) -> Detail {
        let d = |depth_steps, gloss, rim, cavity, variation, wetness, glow, sparkle, dust, statuses, liquids| Detail { depth_steps, gloss, rim, cavity, variation, wetness, glow, sparkle, dust, statuses, liquids };
        match self {
            SurfaceDetail::Off => d(0, false, false, false, false, false, false, false, false, false, false),
            SurfaceDetail::Low => d(0, false, true, true, true, false, false, false, false, true, false),
            SurfaceDetail::Medium => d(8, true, true, true, true, true, true, false, false, true, false),
            SurfaceDetail::High => d(16, true, true, true, true, true, true, true, true, true, true),
        }
    }
}
