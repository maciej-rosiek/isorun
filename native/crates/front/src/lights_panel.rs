//! Tune the look (the port of `lights-panel.ts`): a side panel to pick a light variant and tune
//! every value of it. The values are kept on the device (`isorun.lights.current`, restored on the
//! next start), can be saved under a name (`isorun.lights.saved`), and show as text to copy back
//! into `lights.rs`. What the panel set last is the Look setting's Custom ([`lights_for`]).

use isorun_client::platform::storage::{get_raw, set_raw};
use isorun_render::lights::Upscale;
use isorun_render::{IsoLights, LIGHTS_NAMES};
use serde_json::{Map, Value};

const CURRENT_KEY: &str = "isorun.lights.current";
const SAVED_KEY: &str = "isorun.lights.saved";
const BASE: &str = "diorama";

pub const UPSCALES: [&str; 4] = ["pixel", "scale2x", "xbr", "scalefx"];

fn upscale_index(u: Upscale) -> f64 {
    match u {
        Upscale::Pixel => 0.0,
        Upscale::Scale2x => 1.0,
        Upscale::Xbr => 2.0,
        Upscale::ScaleFx => 3.0,
    }
}

fn upscale_of(i: f64) -> Upscale {
    match i.round() as i64 {
        0 => Upscale::Pixel,
        2 => Upscale::Xbr,
        3 => Upscale::ScaleFx,
        _ => Upscale::Scale2x,
    }
}

/// What a field holds and how it is set.
#[derive(Clone, Copy)]
pub enum Kind {
    Number { min: f64, max: f64, step: f64, names: Option<&'static [&'static str]> },
    Colour,
    Toggle,
}

/// One value of the look. A toggle's value is 1 or 0.
#[derive(Clone, Copy)]
pub struct Field {
    pub label: &'static str,
    pub kind: Kind,
    pub get: fn(&IsoLights) -> f64,
    pub set: fn(&mut IsoLights, f64),
}

pub struct Section {
    pub title: &'static str,
    pub fields: Vec<Field>,
}

macro_rules! num {
    ($label:expr, $min:expr, $max:expr, $step:expr, |$l:ident| $path:expr) => {
        Field { label: $label, kind: Kind::Number { min: $min, max: $max, step: $step, names: None }, get: |$l| $path as f64, set: |$l, v| $path = v as _ }
    };
}
macro_rules! colour {
    ($label:expr, |$l:ident| $path:expr) => {
        Field { label: $label, kind: Kind::Colour, get: |$l| $path as f64, set: |$l, v| $path = v as u32 }
    };
}
macro_rules! toggle {
    (|$l:ident| $path:expr) => {
        Field { label: "On", kind: Kind::Toggle, get: |$l| if $path { 1.0 } else { 0.0 }, set: |$l, v| $path = v != 0.0 }
    };
}

/// The panel's sections, in order: every value of [`IsoLights`] a player may want to move.
pub fn sections() -> Vec<Section> {
    let s = |title, fields| Section { title, fields };
    vec![
        s("Overall", vec![num!("Exposure", 0.5, 6.0, 0.1, |l| l.exposure), colour!("Background", |l| l.background)]),
        s(
            "Fill light",
            vec![
                num!("Strength", 0.0, 6.0, 0.05, |l| l.ambient_intensity),
                colour!("From above", |l| l.ambient_sky),
                colour!("From below", |l| l.ambient_ground),
            ],
        ),
        s(
            "Wall torches",
            vec![
                colour!("Colour", |l| l.torch.colour),
                num!("Strength", 0.0, 30.0, 0.25, |l| l.torch.intensity),
                num!("Reach", 1.0, 16.0, 0.5, |l| l.torch.distance),
                num!("Falloff", 0.0, 2.0, 0.1, |l| l.torch.decay),
            ],
        ),
        s(
            "Shading",
            vec![
                num!("Wrap (clay)", 0.0, 1.0, 0.05, |l| l.shading.wrap),
                num!("Flat tones", 0.0, 8.0, 1.0, |l| l.shading.bands),
                num!("Relief", 0.0, 2.0, 0.05, |l| l.shading.relief),
                num!("Depth", 0.0, 0.15, 0.005, |l| l.shading.depth),
                num!("Sprites", 0.2, 1.5, 0.05, |l| l.shading.sprites),
            ],
        ),
        s(
            "Surfaces",
            vec![
                num!("Gloss", 0.0, 2.0, 0.05, |l| l.shading.gloss),
                num!("Rim light", 0.0, 2.0, 0.05, |l| l.shading.rim),
                num!("Dirt and wear", 0.0, 1.5, 0.05, |l| l.shading.cavity),
                num!("Cell variation", 0.0, 0.3, 0.01, |l| l.shading.variation),
                num!("Wetness", 0.0, 1.0, 0.05, |l| l.shading.wetness),
                num!("Glow", 0.0, 3.0, 0.05, |l| l.shading.glow),
                num!("Sparkle", 0.0, 3.0, 0.05, |l| l.shading.sparkle),
                num!("Torch dust", 0.0, 2.0, 0.05, |l| l.shading.dust),
            ],
        ),
        s(
            "Painted tiles",
            vec![
                Field {
                    label: "Upscale",
                    kind: Kind::Number { min: 0.0, max: 3.0, step: 1.0, names: Some(&UPSCALES) },
                    get: |l| upscale_index(l.paint.upscale),
                    set: |l, v| l.paint.upscale = upscale_of(v),
                },
                num!("Brush", 0.0, 8.0, 1.0, |l| l.paint.radius),
                num!("Lift", 0.5, 3.0, 0.1, |l| l.paint.lift),
                num!("Tones", 0.0, 16.0, 1.0, |l| l.paint.levels),
            ],
        ),
        s(
            "Walls",
            vec![
                num!("Bevel", 0.0, 0.3, 0.01, |l| l.walls.bevel),
                num!("Moss", 0.0, 1.0, 0.05, |l| l.walls.moss),
                colour!("Moss colour", |l| l.walls.moss_colour),
            ],
        ),
        s(
            "Ambient occlusion (GTAO)",
            vec![toggle!(|l| l.ao.enabled), num!("Strength", 0.0, 1.0, 0.05, |l| l.ao.intensity), num!("Radius", 0.1, 4.0, 0.05, |l| l.ao.radius)],
        ),
        s(
            "Outlines",
            vec![
                toggle!(|l| l.outline.enabled),
                num!("Thickness", 0.5, 4.0, 0.25, |l| l.outline.thickness),
                num!("Strength", 0.0, 1.0, 0.05, |l| l.outline.strength),
                colour!("Colour", |l| l.outline.colour),
            ],
        ),
        s("Painted screen", vec![toggle!(|l| l.painted.enabled), num!("Brush", 0.5, 4.0, 0.25, |l| l.painted.radius)]),
        s("Pixel look", vec![toggle!(|l| l.pixelate.enabled), num!("Size", 2.0, 8.0, 0.5, |l| l.pixelate.size)]),
        s(
            "Bloom",
            vec![
                toggle!(|l| l.bloom.enabled),
                num!("Strength", 0.0, 3.0, 0.05, |l| l.bloom.strength),
                num!("Radius", 0.0, 1.0, 0.05, |l| l.bloom.radius),
                num!("Threshold", 0.0, 2.0, 0.05, |l| l.bloom.threshold),
            ],
        ),
        s(
            "Tilt-shift blur",
            vec![
                toggle!(|l| l.tilt_shift.enabled),
                num!("Blur", 0.0, 6.0, 0.1, |l| l.tilt_shift.blur),
                num!("Sharp band", 0.0, 1.0, 0.05, |l| l.tilt_shift.focus),
                num!("Falloff", 0.05, 1.0, 0.05, |l| l.tilt_shift.falloff),
            ],
        ),
        s(
            "Colour grade",
            vec![
                num!("Saturation", 0.0, 2.0, 0.05, |l| l.grade.saturation),
                colour!("Dark tint", |l| l.grade.shadows),
                colour!("Bright tint", |l| l.grade.highlights),
                num!("Vignette", 0.0, 1.5, 0.05, |l| l.grade.vignette),
            ],
        ),
        s(
            "Fog",
            vec![
                toggle!(|l| l.fog.enabled),
                colour!("Colour", |l| l.fog.colour),
                num!("Density", 0.0, 1.0, 0.05, |l| l.fog.density),
                num!("Height", 0.1, 2.0, 0.05, |l| l.fog.height),
                num!("Reach", 1.0, 10.0, 0.5, |l| l.fog.reach),
                num!("Softness", 0.0, 1.0, 0.05, |l| l.fog.softness),
            ],
        ),
        s(
            "Water",
            vec![
                toggle!(|l| l.water.enabled),
                colour!("Deep", |l| l.water.deep),
                colour!("Shallow", |l| l.water.shallow),
                colour!("Light", |l| l.water.light),
                num!("Strength", 0.0, 1.0, 0.05, |l| l.water.strength),
                num!("Speed", 0.0, 3.0, 0.1, |l| l.water.speed),
            ],
        ),
        s(
            "Lava",
            vec![
                toggle!(|l| l.lava.enabled),
                colour!("Crust", |l| l.lava.crust),
                colour!("Molten", |l| l.lava.molten),
                colour!("Hot", |l| l.lava.hot),
                num!("Glow", 0.0, 3.0, 0.1, |l| l.lava.glow),
                num!("Speed", 0.0, 3.0, 0.1, |l| l.lava.speed),
            ],
        ),
        s(
            "Combat",
            vec![
                colour!("Blood", |l| l.combat.blood),
                num!("Gore", 0.0, 3.0, 0.1, |l| l.combat.gore),
                num!("Shake", 0.0, 3.0, 0.1, |l| l.combat.shake),
                num!("Hit stop", 0.0, 0.3, 0.01, |l| l.combat.hit_stop),
                num!("Flash", 0.0, 3.0, 0.1, |l| l.combat.flash),
            ],
        ),
        s(
            "Steps",
            vec![colour!("Colour", |l| l.steps.colour), num!("Dust", 0.0, 3.0, 0.1, |l| l.steps.dust), num!("Footprints", 0.0, 30.0, 1.0, |l| l.steps.prints)],
        ),
        s("Pointer", vec![colour!("Colour", |l| l.aim.colour), num!("Brightness", 0.2, 4.0, 0.1, |l| l.aim.brightness)]),
    ]
}

fn fields() -> Vec<(String, Field)> {
    sections().into_iter().flat_map(|s| s.fields.into_iter().map(move |f| (format!("{}/{}", s.title, f.label), f))).collect()
}

/// The values a look is kept as: `"Section/Label"` to a number, or true / false for a toggle.
pub type Values = Map<String, Value>;

pub fn values_of(lights: &IsoLights) -> Values {
    fields()
        .into_iter()
        .map(|(key, f)| {
            let v = (f.get)(lights);
            let value = match f.kind {
                Kind::Toggle => Value::Bool(v != 0.0),
                _ => serde_json::Number::from_f64(round6(v)).map(Value::Number).unwrap_or(Value::Null),
            };
            (key, value)
        })
        .collect()
}

fn round6(v: f64) -> f64 {
    (v * 1e6).round() / 1e6
}

/// The base look with `values` over it; a value of the wrong kind is passed over.
pub fn with_values(values: &Values) -> IsoLights {
    let mut lights = isorun_render::lights(BASE).expect("the base look");
    for (key, f) in fields() {
        match (f.kind, values.get(&key)) {
            (Kind::Toggle, Some(Value::Bool(b))) => (f.set)(&mut lights, if *b { 1.0 } else { 0.0 }),
            (Kind::Toggle, _) => {}
            (_, Some(Value::Number(n))) => (f.set)(&mut lights, n.as_f64().unwrap_or(0.0)),
            _ => {}
        }
    }
    lights
}

fn read_json(key: &str) -> Value {
    get_raw(key).and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(Value::Null)
}

/// The lights tuned in the panel last (the Look setting's Custom), or none when none were.
pub fn custom_lights() -> Option<IsoLights> {
    match read_json(CURRENT_KEY) {
        Value::Object(o) => Some(with_values(&o)),
        _ => None,
    }
}

/// The lights a Look setting stands for.
pub fn lights_for(look: &str) -> IsoLights {
    if look == "custom" {
        return custom_lights().unwrap_or_else(|| isorun_render::lights(BASE).expect("the base look"));
    }
    isorun_render::lights(look).unwrap_or_else(|| isorun_render::lights(BASE).expect("the base look"))
}

#[derive(Clone)]
struct Saved {
    name: String,
    values: Values,
}

fn read_saved() -> Vec<Saved> {
    match read_json(SAVED_KEY) {
        Value::Array(a) => a
            .into_iter()
            .filter_map(|e| {
                let name = e.get("name")?.as_str()?.to_string();
                let values = e.get("values").and_then(Value::as_object).cloned().unwrap_or_default();
                Some(Saved { name, values })
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn write_saved(saved: &[Saved]) {
    let v: Vec<Value> = saved.iter().map(|s| serde_json::json!({ "name": s.name, "values": s.values })).collect();
    set_raw(SAVED_KEY, Some(&Value::Array(v).to_string()));
}

/// The lights as text to copy back into `lights.rs`: one line a part.
pub fn values_text(lights: &IsoLights) -> String {
    let mut out = String::new();
    for s in sections() {
        let parts: Vec<String> = s
            .fields
            .iter()
            .map(|f| {
                let v = (f.get)(lights);
                match f.kind {
                    Kind::Colour => format!("{}: #{:06x}", f.label, v as u32),
                    Kind::Toggle => format!("{}: {}", f.label, v != 0.0),
                    Kind::Number { names: Some(n), .. } => format!("{}: {}", f.label, n.get(v as usize).copied().unwrap_or("")),
                    Kind::Number { .. } => format!("{}: {}", f.label, round6(v)),
                }
            })
            .collect();
        out.push_str(&format!("{}: {}\n", s.title, parts.join(", ")));
    }
    out
}

/// The panel: open over the game (Tune the look), or beside the test bed.
pub struct LightsPanel {
    lights: IsoLights,
    active: Option<String>,
    saved: Vec<Saved>,
    naming: Option<String>,
    sections: Vec<Section>,
}

/// What a frame of the panel came to.
#[derive(Clone, Debug, PartialEq)]
pub enum LightsEvent {
    /// The look changed: put it on the renderer (and the Look setting is Custom now).
    Changed(IsoLights),
    Close,
}

impl LightsPanel {
    /// The panel on `requested` (a built-in look's name), or on what was tuned last.
    pub fn new(requested: &str) -> Self {
        let stored = read_json(CURRENT_KEY);
        let builtin = isorun_render::lights(requested).filter(|_| LIGHTS_NAMES.contains(&requested));
        let (lights, active) = match (builtin, &stored) {
            (Some(l), _) => (l, Some(format!("built-in/{requested}"))),
            (None, Value::Object(o)) => (with_values(o), None),
            (None, _) => (isorun_render::lights(BASE).expect("base"), Some(format!("built-in/{BASE}"))),
        };
        LightsPanel { lights, active, saved: read_saved(), naming: None, sections: sections() }
    }

    pub fn lights(&self) -> &IsoLights {
        &self.lights
    }

    fn sync(&mut self) -> LightsEvent {
        set_raw(CURRENT_KEY, Some(&Value::Object(values_of(&self.lights)).to_string()));
        LightsEvent::Changed(self.lights.clone())
    }

    /// Draw the panel down the right of the screen (`.look-panel`: 22rem wide, the full height).
    pub fn ui(&mut self, ctx: &egui::Context) -> Option<LightsEvent> {
        let mut out = None;
        let screen = ctx.content_rect();
        let width = 22.0 * 15.0;
        let frame = egui::Frame::NONE
            .fill(egui::Color32::from_rgba_unmultiplied(17, 17, 17, 235))
            .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(0x33)))
            .inner_margin(10.0);
        egui::Area::new(egui::Id::NULL.with("look-panel")).order(egui::Order::Foreground).fixed_pos(egui::pos2(screen.right() - width, screen.top())).show(
            ctx,
            |ui| {
                frame.show(ui, |ui| {
                    ui.set_width(width - 20.0);
                    ui.set_height(screen.height() - 20.0);
                    let accent = egui::Color32::from_rgb(0x99, 0xcc, 0xff);
                    ui.style_mut().override_font_id = Some(egui::FontId::monospace(13.0));
                    ui.style_mut().visuals.override_text_color = Some(egui::Color32::from_gray(0xdd));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                        if ui.button("Close").clicked() {
                            out = Some(LightsEvent::Close);
                        }
                    });
                    isorun_ui::scroll::area(ui, egui::Id::unique("lights-panel-scroll"), egui::ScrollArea::vertical().auto_shrink([false, false]), |ui| {
                        ui.label(egui::RichText::new("Lights").color(accent));
                        ui.horizontal_wrapped(|ui| {
                            for name in LIGHTS_NAMES {
                                let Some(l) = isorun_render::lights(name) else { continue };
                                let id = format!("built-in/{name}");
                                if preset_button(ui, l.label, self.active.as_deref() == Some(id.as_str()), accent) {
                                    self.lights = l;
                                    self.active = Some(id);
                                    out = Some(self.sync());
                                }
                            }
                            let mut remove = None;
                            let mut pick = None;
                            for (i, entry) in self.saved.iter().enumerate() {
                                let id = format!("saved/{}", entry.name);
                                if preset_button(ui, &entry.name, self.active.as_deref() == Some(id.as_str()), accent) {
                                    pick = Some((with_values(&entry.values), id));
                                }
                                if ui.small_button("x").on_hover_text(format!("Delete {}", entry.name)).clicked() {
                                    remove = Some(i);
                                }
                            }
                            if let Some((l, id)) = pick {
                                self.lights = l;
                                self.active = Some(id);
                                out = Some(self.sync());
                            }
                            if let Some(i) = remove {
                                self.saved.remove(i);
                                write_saved(&self.saved);
                            }
                            if ui.button("Save as...").clicked() {
                                self.naming = Some(self.active.as_deref().and_then(|a| a.strip_prefix("saved/")).unwrap_or("").to_string());
                            }
                        });
                        if let Some(name) = &mut self.naming {
                            let mut done = false;
                            ui.horizontal(|ui| {
                                ui.label("Name for these lights");
                                let r = ui.text_edit_singleline(name);
                                r.request_focus();
                                done = r.lost_focus();
                            });
                            if done {
                                let name = name.trim().to_string();
                                self.naming = None;
                                if !name.is_empty() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                    self.saved.retain(|s| s.name != name);
                                    self.saved.push(Saved { name: name.clone(), values: values_of(&self.lights) });
                                    write_saved(&self.saved);
                                    self.active = Some(format!("saved/{name}"));
                                    out = Some(self.sync());
                                }
                            }
                        }
                        for section in &self.sections {
                            ui.add_space(6.0);
                            ui.label(egui::RichText::new(section.title).color(accent));
                            for f in &section.fields {
                                if field_ui(ui, f, &mut self.lights, accent) {
                                    self.active = None;
                                    out = Some(LightsEvent::Changed(self.lights.clone()));
                                }
                            }
                        }
                        ui.add_space(6.0);
                        ui.label(egui::RichText::new("Values").color(accent));
                        ui.label(egui::RichText::new(values_text(&self.lights)).size(11.0).color(egui::Color32::from_gray(0xaa)));
                    });
                });
            },
        );
        if matches!(out, Some(LightsEvent::Changed(_))) {
            set_raw(CURRENT_KEY, Some(&Value::Object(values_of(&self.lights)).to_string()));
        }
        out
    }
}

fn preset_button(ui: &mut egui::Ui, label: &str, on: bool, accent: egui::Color32) -> bool {
    let text = egui::RichText::new(label).color(if on { accent } else { egui::Color32::from_gray(0xdd) });
    let b = egui::Button::new(text)
        .fill(egui::Color32::from_gray(0x22))
        .stroke(egui::Stroke::new(1.0, if on { accent } else { egui::Color32::from_gray(0x44) }))
        .corner_radius(0.0);
    ui.add(b).clicked()
}

fn field_ui(ui: &mut egui::Ui, f: &Field, lights: &mut IsoLights, accent: egui::Color32) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(f.label);
        let v = (f.get)(lights);
        match f.kind {
            Kind::Toggle => {
                let mut b = v != 0.0;
                if ui.checkbox(&mut b, "").changed() {
                    (f.set)(lights, if b { 1.0 } else { 0.0 });
                    changed = true;
                }
            }
            Kind::Colour => {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let c = v as u32;
                    let mut rgb = [(c >> 16) as u8, (c >> 8) as u8, c as u8];
                    if ui.color_edit_button_srgb(&mut rgb).changed() {
                        (f.set)(lights, ((rgb[0] as u32) << 16 | (rgb[1] as u32) << 8 | rgb[2] as u32) as f64);
                        changed = true;
                    }
                });
            }
            Kind::Number { min, max, step, names } => {
                let mut x = v;
                let shown = match names {
                    Some(n) => n.get(v as usize).copied().unwrap_or("").to_string(),
                    None => format!("{}", round6(v)),
                };
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_sized([40.0, 16.0], egui::Label::new(egui::RichText::new(shown).color(accent)));
                    let slider = egui::Slider::new(&mut x, min..=max).step_by(step).show_value(false);
                    ui.spacing_mut().slider_width = ui.available_width() - 8.0;
                    if ui.add(slider).changed() {
                        (f.set)(lights, x);
                        changed = true;
                    }
                });
            }
        }
    });
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_round_trip() {
        for name in LIGHTS_NAMES {
            let Some(l) = isorun_render::lights(name) else { continue };
            let back = with_values(&values_of(&l));
            assert_eq!(values_of(&back), values_of(&l), "{name}");
        }
    }

    #[test]
    fn wrong_kinds_are_passed_over() {
        let mut v = Values::new();
        v.insert("Overall/Exposure".into(), Value::Bool(true));
        v.insert("Bloom/On".into(), Value::from(3));
        v.insert("Fill light/Strength".into(), Value::from(1.5));
        let l = with_values(&v);
        let base = isorun_render::lights(BASE).unwrap();
        assert_eq!(l.exposure, base.exposure);
        assert_eq!(l.bloom.enabled, base.bloom.enabled);
        assert_eq!(l.ambient_intensity, 1.5);
    }

    #[test]
    fn upscale_by_name() {
        let mut v = Values::new();
        v.insert("Painted tiles/Upscale".into(), Value::from(3));
        assert_eq!(with_values(&v).paint.upscale, Upscale::ScaleFx);
    }
}
