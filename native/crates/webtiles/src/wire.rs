//! Reading the server's JSON as the official client's JavaScript does: leniently. A field of
//! the wrong type is reported, never a panic; JavaScript's `String()`, `Number()` and truthiness
//! are here for the places the TS relied on them.

use serde_json::{Map, Value};

/// A JSON object, keys in the order they arrived (serde_json's `preserve_order`).
pub type Obj = Map<String, Value>;

/// JavaScript truthiness: `null`, `false`, `0`, `""` are false; objects and arrays are true.
pub fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0 && !f.is_nan()),
        Value::String(s) => !s.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

/// [`truthy`] of an optional field (absent is `undefined`, false).
pub(crate) fn truthy_opt(v: Option<&Value>) -> bool {
    v.is_some_and(truthy)
}

/// JavaScript `Number(v)`: NaN for what does not convert.
pub(crate) fn js_number(v: &Value) -> f64 {
    match v {
        Value::Null => 0.0,
        Value::Bool(b) => f64::from(u8::from(*b)),
        Value::Number(n) => n.as_f64().unwrap_or(f64::NAN),
        Value::String(s) => {
            let t = s.trim();
            if t.is_empty() {
                0.0
            } else {
                t.parse::<f64>().ok().filter(|f| f.is_finite() || t.ends_with("Infinity")).unwrap_or(f64::NAN)
            }
        }
        Value::Array(a) if a.is_empty() => 0.0,
        Value::Array(a) if a.len() == 1 => js_number(&a[0]),
        _ => f64::NAN,
    }
}

fn js_number_string(f: f64) -> String {
    if f.is_nan() {
        "NaN".into()
    } else if f.is_infinite() {
        if f > 0.0 { "Infinity" } else { "-Infinity" }.into()
    } else if f.fract() == 0.0 && f.abs() < 1e21 {
        format!("{}", f as i128)
    } else {
        format!("{f}")
    }
}

/// JavaScript `String(v)`.
pub fn js_string(v: &Value) -> String {
    match v {
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => match n.as_i64() {
            Some(i) => i.to_string(),
            None => n.as_u64().map(|u| u.to_string()).unwrap_or_else(|| js_number_string(n.as_f64().unwrap_or(f64::NAN))),
        },
        Value::String(s) => s.clone(),
        Value::Array(a) => a.iter().map(|e| if e.is_null() { String::new() } else { js_string(e) }).collect::<Vec<_>>().join(","),
        Value::Object(_) => "[object Object]".into(),
    }
}

/// `typeof v === 'number'`, as a float.
pub(crate) fn num(v: Option<&Value>) -> Option<f64> {
    v.and_then(Value::as_f64)
}

/// `typeof v === 'number'`, as an integer (a fraction is cut off).
pub(crate) fn int(v: Option<&Value>) -> Option<i64> {
    let v = v?;
    v.as_i64().or_else(|| v.as_f64().filter(|f| f.is_finite()).map(|f| f as i64))
}

/// `typeof v === 'string'`.
pub(crate) fn string(v: Option<&Value>) -> Option<&str> {
    v.and_then(Value::as_str)
}

/// `(v as string) || fallback`: a falsy value gives the fallback, anything else its `String()`.
pub(crate) fn str_or(v: Option<&Value>, fallback: &str) -> String {
    match v {
        Some(v) if truthy(v) => js_string(v),
        _ => fallback.to_owned(),
    }
}

/// `(v as string) ?? null` read as a string: absent and null give `None`.
pub(crate) fn opt_string(v: Option<&Value>) -> Option<String> {
    match v {
        None | Some(Value::Null) => None,
        Some(v) => Some(js_string(v)),
    }
}

/// A value a typed field can hold, read from its wire form and written back to it.
pub trait Wire: Sized {
    /// `None` when the value has the wrong type for the field.
    fn from_wire(v: &Value) -> Option<Self>;
    fn to_wire(&self) -> Value;
}

impl Wire for i64 {
    fn from_wire(v: &Value) -> Option<Self> {
        int(Some(v))
    }
    fn to_wire(&self) -> Value {
        Value::from(*self)
    }
}

impl Wire for i32 {
    fn from_wire(v: &Value) -> Option<Self> {
        int(Some(v)).and_then(|i| i32::try_from(i).ok())
    }
    fn to_wire(&self) -> Value {
        Value::from(*self)
    }
}

impl Wire for f64 {
    fn from_wire(v: &Value) -> Option<Self> {
        v.as_f64()
    }
    fn to_wire(&self) -> Value {
        if self.fract() == 0.0 && self.abs() < 9.0e15 {
            Value::from(*self as i64)
        } else {
            Value::from(*self)
        }
    }
}

/// A boolean, or a number read as one (crawl writes some flags as 0 / 1).
impl Wire for bool {
    fn from_wire(v: &Value) -> Option<Self> {
        match v {
            Value::Bool(b) => Some(*b),
            Value::Number(_) => Some(truthy(v)),
            _ => None,
        }
    }
    fn to_wire(&self) -> Value {
        Value::Bool(*self)
    }
}

/// A string, or a number read as its text.
impl Wire for String {
    fn from_wire(v: &Value) -> Option<Self> {
        match v {
            Value::String(s) => Some(s.clone()),
            Value::Number(_) => Some(js_string(v)),
            _ => None,
        }
    }
    fn to_wire(&self) -> Value {
        Value::String(self.clone())
    }
}

impl Wire for Value {
    fn from_wire(v: &Value) -> Option<Self> {
        Some(v.clone())
    }
    fn to_wire(&self) -> Value {
        self.clone()
    }
}

impl Wire for Obj {
    fn from_wire(v: &Value) -> Option<Self> {
        v.as_object().cloned()
    }
    fn to_wire(&self) -> Value {
        Value::Object(self.clone())
    }
}

/// `null` is `None`; any other value must read as `T`.
impl<T: Wire> Wire for Option<T> {
    fn from_wire(v: &Value) -> Option<Self> {
        if v.is_null() {
            Some(None)
        } else {
            T::from_wire(v).map(Some)
        }
    }
    fn to_wire(&self) -> Value {
        self.as_ref().map_or(Value::Null, Wire::to_wire)
    }
}

impl<T: Wire> Wire for Vec<T> {
    fn from_wire(v: &Value) -> Option<Self> {
        v.as_array()?.iter().map(T::from_wire).collect()
    }
    fn to_wire(&self) -> Value {
        Value::Array(self.iter().map(Wire::to_wire).collect())
    }
}

impl<const N: usize> Wire for [i64; N] {
    fn from_wire(v: &Value) -> Option<Self> {
        let a = v.as_array()?;
        if a.len() < N {
            return None;
        }
        let mut out = [0; N];
        for (o, e) in out.iter_mut().zip(a) {
            *o = int(Some(e))?;
        }
        Some(out)
    }
    fn to_wire(&self) -> Value {
        Value::Array(self.iter().map(|&i| Value::from(i)).collect())
    }
}

/// What setting one field did.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Assign {
    /// The same value as before (JavaScript `===`: an object or array is never the same).
    Same,
    /// Set; the value it had, `null` when it had none.
    Changed(Value),
    /// The value has the wrong type for the field; the field kept its value.
    BadType,
    /// A key the struct does not keep.
    Unknown,
}

pub(crate) fn assign_field<T: Wire + PartialEq>(slot: &mut T, v: &Value) -> Assign {
    match T::from_wire(v) {
        None => Assign::BadType,
        Some(n) if *slot == n => Assign::Same,
        Some(n) => Assign::Changed(std::mem::replace(slot, n).to_wire()),
    }
}

pub(crate) fn assign_extra(extra: &mut Obj, k: &str, v: &Value) -> Assign {
    let shared = v.is_object() || v.is_array();
    match extra.insert(k.to_owned(), v.clone()) {
        Some(old) if !shared && old == *v => Assign::Same,
        old => Assign::Changed(old.unwrap_or(Value::Null)),
    }
}

/// The fields of a struct by their wire keys: `assign` (one key, the TS `obj[k] = v`), `merge`
/// (`Object.assign`), `get` and `to_object`. With `extra`, keys the struct does not name are kept
/// in its `extra` map, as the TS kept them on the object; `closed` drops them.
macro_rules! wire_fields {
    ($ty:ident { $($field:ident : $key:literal),* $(,)? }) => {
        wire_fields!(@impl $ty, true, { $($field : $key),* });
    };
    ($ty:ident closed { $($field:ident : $key:literal),* $(,)? }) => {
        wire_fields!(@impl $ty, false, { $($field : $key),* });
    };
    (@impl $ty:ident, $open:tt, { $($field:ident : $key:literal),* }) => {
        #[allow(dead_code)]
        impl $ty {
            pub(crate) fn assign(&mut self, key: &str, v: &serde_json::Value) -> $crate::wire::Assign {
                match key {
                    $($key => $crate::wire::assign_field(&mut self.$field, v),)*
                    _ => wire_fields!(@extra_assign $open, self, key, v),
                }
            }

            /// `Object.assign(self, diff)`; returns the keys whose value had the wrong type.
            pub(crate) fn merge(&mut self, diff: &$crate::wire::Obj) -> Vec<String> {
                let mut bad = Vec::new();
                for (k, v) in diff {
                    if self.assign(k, v) == $crate::wire::Assign::BadType {
                        bad.push(k.clone());
                    }
                }
                bad
            }

            /// A field by its wire key, as the server would send it (`null` for an unset one).
            pub fn get(&self, key: &str) -> Option<serde_json::Value> {
                match key {
                    $($key => Some($crate::wire::Wire::to_wire(&self.$field)),)*
                    _ => wire_fields!(@extra_get $open, self, key),
                }
            }

            /// The struct as a wire object; unset fields are left out.
            pub fn to_object(&self) -> $crate::wire::Obj {
                let mut o = $crate::wire::Obj::new();
                $(
                    let w = $crate::wire::Wire::to_wire(&self.$field);
                    if !w.is_null() {
                        o.insert($key.to_owned(), w);
                    }
                )*
                wire_fields!(@extra_out $open, self, o);
                o
            }
        }
    };
    (@extra_assign true, $s:ident, $k:ident, $v:ident) => { $crate::wire::assign_extra(&mut $s.extra, $k, $v) };
    (@extra_assign false, $s:ident, $k:ident, $v:ident) => {{ let _ = ($k, $v); $crate::wire::Assign::Unknown }};
    (@extra_get true, $s:ident, $k:ident) => { $s.extra.get($k).cloned() };
    (@extra_get false, $s:ident, $k:ident) => {{ let _ = $k; None }};
    (@extra_out true, $s:ident, $o:ident) => { for (k, v) in &$s.extra { $o.insert(k.clone(), v.clone()); } };
    (@extra_out false, $s:ident, $o:ident) => {};
}

/// `Wire` for a `wire_fields!` struct: an object merged over the default.
macro_rules! impl_wire {
    ($ty:ident) => {
        impl $crate::wire::Wire for $ty {
            fn from_wire(v: &serde_json::Value) -> Option<Self> {
                let mut s = Self::default();
                s.merge(v.as_object()?);
                Some(s)
            }
            fn to_wire(&self) -> serde_json::Value {
                serde_json::Value::Object(self.to_object())
            }
        }
    };
}

pub(crate) use {impl_wire, wire_fields};

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn javascript_conversions() {
        assert!(!truthy(&json!(0)) && !truthy(&json!("")) && truthy(&json!([])) && truthy(&json!({})));
        assert_eq!(js_string(&json!(true)), "true");
        assert_eq!(js_string(&json!(5.0)), "5");
        assert_eq!(js_string(&json!(2.5)), "2.5");
        assert_eq!(js_string(&json!([1, null, "a"])), "1,,a");
        assert_eq!(js_number(&json!(" 7 ")), 7.0);
        assert!(js_number(&json!("x")).is_nan());
        assert_eq!(js_number(&json!(true)), 1.0);
    }
}
