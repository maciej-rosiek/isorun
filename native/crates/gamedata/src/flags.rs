//! Cell flag words, decoded as the server's `enums.js` `prepare_fg_flags` and
//! `prepare_bg_flags` decode them: the tables (`fg_flags`, `bg_flags`) are read out of the
//! script, and the decoding is `prepare_flags` in Rust, with JavaScript's 32-bit integer and
//! equality rules.

use std::sync::Arc;

use crate::js::{to_int32, to_number, Value};

/// A mask or a value in a flag table: a number for the first 32 bits, or an array of words
/// for more (`[lo, hi]`), as `prepare_flags` tells them apart with `isNaN`.
#[derive(Clone, Debug, PartialEq)]
enum Mask {
    Num(f64),
    Words(Vec<f64>),
}

/// One of `enums.js`'s flag tables (`{ flags, exclusive_flags, mask }`).
#[derive(Clone, Debug)]
pub(crate) struct FlagSpec {
    names: Arc<[String]>,
    flags: Vec<(usize, Mask)>,
    exclusive: Vec<(Mask, Vec<(usize, Mask)>)>,
    mask: f64,
}

/// A decoded cell flag word: `value` is the tile, `get` answers a flag by its `enums.js` name
/// (`PET`, `S_UNDER`, `MDAM_LIGHT`, `UNSEEN`, `WATER`, ...). `lo` and `hi` are the word's two
/// 32-bit halves.
#[derive(Clone, Debug)]
pub struct FlagWord {
    pub value: u32,
    pub lo: u32,
    pub hi: u32,
    bits: u128,
    names: Arc<[String]>,
}

impl PartialEq for FlagWord {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value && self.lo == other.lo && self.hi == other.hi && self.set_flags().eq(other.set_flags())
    }
}

impl FlagWord {
    /// Whether the flag of this name is set; false for a name the table does not have.
    pub fn get(&self, name: &str) -> bool {
        self.names.iter().position(|n| n == name).is_some_and(|i| self.is_set(i))
    }

    /// Whether the flag at this index of [`FlagWord::names`] is set.
    pub fn is_set(&self, index: usize) -> bool {
        index < 128 && self.bits & (1u128 << index) != 0
    }

    /// Every flag name the table knows, set or not.
    pub fn names(&self) -> &[String] {
        &self.names
    }

    /// The names of the flags that are set, in table order.
    pub fn set_flags(&self) -> impl Iterator<Item = &str> + '_ {
        self.names.iter().enumerate().filter(|(i, _)| self.is_set(*i)).map(|(_, n)| n.as_str())
    }

    /// The same word with its tile moved by `offset` (zero, "no tile", stays zero).
    pub(crate) fn moved(mut self, offset: i64) -> FlagWord {
        if offset != 0 && self.value != 0 {
            self.value = (self.value as i64 + offset) as u32;
        }
        self
    }
}

/// A mask as `prepare_flags` uses it: `isNaN(m)` picks the array path.
fn mask_of(v: &Value) -> Option<Mask> {
    match v {
        Value::Undef => None,
        Value::Arr(a) => {
            let a = a.borrow();
            match a.len() {
                0 => Some(Mask::Num(0.0)),
                1 => Some(Mask::Num(to_number(&a[0]))),
                _ => Some(Mask::Words(a.iter().map(to_number).collect())),
            }
        }
        Value::Obj(_) | Value::Func(_) | Value::Native(_) => Some(Mask::Words(Vec::new())),
        other => {
            let n = to_number(other);
            if n.is_nan() {
                // a string that is not a number: the array path, over its characters
                Some(Mask::Words(Vec::new()))
            } else {
                Some(Mask::Num(n))
            }
        }
    }
}

impl FlagSpec {
    /// Read a flag table out of the value `enums.js` built for it.
    pub(crate) fn from_value(v: &Value, what: &str, diags: &mut Vec<String>) -> Result<FlagSpec, String> {
        let obj = v.as_obj().ok_or_else(|| format!("{what} is not an object"))?;
        let obj = obj.borrow();
        let mut names: Vec<String> = Vec::new();
        let intern = |name: &str, names: &mut Vec<String>| -> usize {
            match names.iter().position(|n| n == name) {
                Some(i) => i,
                None => {
                    names.push(name.to_string());
                    names.len() - 1
                }
            }
        };
        let mut flags = Vec::new();
        if let Some(Value::Obj(f)) = obj.get("flags") {
            for (name, m) in f.borrow().entries() {
                match mask_of(m) {
                    Some(m) => flags.push((intern(name, &mut names), m)),
                    None => diags.push(format!("enums.js: {what}.flags.{name} has no mask; left out")),
                }
            }
        } else {
            diags.push(format!("enums.js: {what} has no flags"));
        }
        let mut exclusive = Vec::new();
        if let Some(Value::Arr(ex)) = obj.get("exclusive_flags") {
            for (i, group) in ex.borrow().iter().enumerate() {
                let Some(g) = group.as_obj() else {
                    diags.push(format!("enums.js: {what}.exclusive_flags[{i}] is not an object; left out"));
                    continue;
                };
                let g = g.borrow();
                let Some(mask) = g.get("mask").and_then(mask_of) else {
                    diags.push(format!("enums.js: {what}.exclusive_flags[{i}] has no mask; left out"));
                    continue;
                };
                let mut values = Vec::new();
                for (name, val) in g.entries() {
                    if &**name == "mask" {
                        continue;
                    }
                    match mask_of(val) {
                        Some(m) => values.push((intern(name, &mut names), m)),
                        None => diags.push(format!("enums.js: {what}.exclusive_flags[{i}].{name} has no value")),
                    }
                }
                exclusive.push((mask, values));
            }
        }
        let mask = obj.get("mask").map(to_number).unwrap_or(f64::NAN);
        if names.len() > 128 {
            diags.push(format!("enums.js: {what} has {} flags; only the first 128 are decoded", names.len()));
        }
        Ok(FlagSpec { names: names.into(), flags, exclusive, mask })
    }

    pub(crate) fn names(&self) -> &[String] {
        &self.names
    }

    /// `prepare_flags` for one word, its halves as JavaScript numbers (at least two).
    pub(crate) fn decode(&self, words: &[f64]) -> FlagWord {
        let w0 = to_int32(words[0]);
        let and = |m: &[f64]| -> Vec<i32> { words.iter().zip(m).map(|(a, b)| to_int32(*a) & to_int32(*b)).collect() };
        let mut bits = 0u128;
        let mut set = |i: usize, on: bool| {
            if i < 128 {
                if on {
                    bits |= 1 << i;
                } else {
                    bits &= !(1 << i);
                }
            }
        };
        for (i, m) in &self.flags {
            let on = match m {
                Mask::Num(m) => w0 & to_int32(*m) != 0,
                Mask::Words(m) => and(m).iter().any(|x| *x != 0),
            };
            set(*i, on);
        }
        for (mask, values) in &self.exclusive {
            let val: Vec<f64> = match mask {
                Mask::Words(m) => and(m).into_iter().map(f64::from).collect(),
                Mask::Num(m) => vec![(w0 & to_int32(*m)) as f64],
            };
            for (i, v) in values {
                let on = match v {
                    Mask::Words(v) => {
                        // array_equal: (a[i] || 0) != (b[i] || 0) over the longer of the two
                        let or0 = |x: Option<&f64>| x.copied().filter(|x| *x != 0.0 && !x.is_nan()).unwrap_or(0.0);
                        (0..val.len().max(v.len())).all(|k| or0(val.get(k)) == or0(v.get(k)))
                    }
                    Mask::Num(v) => val.first().is_some_and(|x| x == v),
                };
                set(*i, on);
            }
        }
        FlagWord { value: (w0 & to_int32(self.mask)) as u32, lo: w0 as u32, hi: to_int32(words[1]) as u32, bits, names: self.names.clone() }
    }
}

/// A JSON value as JavaScript's `Number()` reads it.
fn json_number(v: &serde_json::Value) -> f64 {
    use serde_json::Value as J;
    match v {
        J::Null => 0.0,
        J::Bool(b) => *b as u8 as f64,
        J::Number(n) => n.as_f64().unwrap_or(f64::NAN),
        J::String(s) => to_number(&Value::Str(s.as_str().into())),
        J::Array(a) => match a.len() {
            0 => 0.0,
            1 => match &a[0] {
                J::Null => 0.0,
                x => json_number(x),
            },
            _ => f64::NAN,
        },
        J::Object(_) => f64::NAN,
    }
}

/// A tile word as the server sends it (a number, or `[lo, hi]` past 32 bits) as the words
/// `prepare_flags` works on, padded to two.
pub(crate) fn words_of(v: &serde_json::Value) -> Vec<f64> {
    use serde_json::Value as J;
    let mut w = match v {
        J::Array(a) if a.len() != 1 => a.iter().map(json_number).collect(),
        J::Object(_) => vec![0.0],
        other => vec![json_number(other)],
    };
    while w.len() < 2 {
        w.push(0.0);
    }
    w
}
