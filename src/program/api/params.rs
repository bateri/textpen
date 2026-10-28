//! What the page sends is not trusted: each call reads its params here, and what does not fit is
//! answered `bad-params`, with words that say where (`ui.tabs.0: expected a string, got number`),
//! and never run.
//!
//! `null` and absent are different. A field the contract writes `T | null` is always there and may
//! be `null` ([`Fields::nullable`]); one it writes `?` may be left out and is never `null`
//! ([`Fields::optional`]); the others are there and are not `null` ([`Fields::required`]). Fields
//! the program does not know are ignored.
//!
//! This is api/'s: the files of api/ know the page, and project/, state/ and terminal/ do not.

use std::collections::BTreeMap;

use serde_json::{Map, Number, Value};

use crate::state::defaults::{
    LineNumbers, Place, SIDEBAR_WIDTHS, Settings, TAB_SIZES, UiState, Whitespace, clamp,
};

/// A value as the program takes it, or the words for what is wrong, led by where.
pub type Checked<T> = Result<T, String>;

/// What a value from JSON is, for the words of a refusal.
fn describe(value: Option<&Value>) -> &'static str {
    match value {
        None => "nothing",
        Some(Value::Null) => "null",
        Some(Value::Bool(_)) => "true or false",
        Some(Value::Number(_)) => "number",
        Some(Value::String(_)) => "string",
        Some(Value::Array(_)) => "list",
        Some(Value::Object(_)) => "object",
    }
}

/// The words for what is wrong at `at` (a path such as `ui.tabs.0`; empty for the params).
fn wrong<T>(at: &str, words: &str) -> Checked<T> {
    Err(if at.is_empty() {
        words.to_owned()
    } else {
        format!("{at}: {words}")
    })
}

fn expected<T>(at: &str, what: &str, got: Option<&Value>) -> Checked<T> {
    wrong(at, &format!("expected {what}, got {}", describe(got)))
}

/// Where a member of `at` is.
fn member(at: &str, key: &str) -> String {
    if at.is_empty() {
        key.to_owned()
    } else {
        format!("{at}.{key}")
    }
}

/// A value of one kind, read from JSON.
pub trait Param: Sized {
    /// What the value is, in the words of a refusal (`a string`).
    const WHAT: &'static str;

    fn read(value: &Value, at: &str) -> Checked<Self>;
}

impl Param for String {
    const WHAT: &'static str = "a string";

    fn read(value: &Value, at: &str) -> Checked<Self> {
        match value {
            Value::String(text) => Ok(text.clone()),
            other => expected(at, Self::WHAT, Some(other)),
        }
    }
}

impl Param for bool {
    const WHAT: &'static str = "true or false";

    fn read(value: &Value, at: &str) -> Checked<Self> {
        match value {
            Value::Bool(flag) => Ok(*flag),
            other => expected(at, Self::WHAT, Some(other)),
        }
    }
}

/// A whole number (`2.0` is one).
impl Param for i64 {
    const WHAT: &'static str = "an integer";

    fn read(value: &Value, at: &str) -> Checked<Self> {
        let whole = match value {
            Value::Number(number) => number.as_i64().or_else(|| {
                number
                    .as_f64()
                    .filter(|float| float.fract() == 0.0 && float.abs() < 9.0e15)
                    // Whole and well within the range of an i64.
                    .map(|float| {
                        #[allow(clippy::cast_possible_truncation)]
                        let whole = float as i64;
                        whole
                    })
            }),
            _ => None,
        };
        match whole {
            Some(whole) => Ok(whole),
            None => expected(at, Self::WHAT, Some(value)),
        }
    }
}

/// Any number, kept as it was written.
impl Param for Number {
    const WHAT: &'static str = "a number";

    fn read(value: &Value, at: &str) -> Checked<Self> {
        match value {
            Value::Number(number) => Ok(number.clone()),
            other => expected(at, Self::WHAT, Some(other)),
        }
    }
}

impl<T: Param> Param for Vec<T> {
    const WHAT: &'static str = "a list";

    fn read(value: &Value, at: &str) -> Checked<Self> {
        match value {
            Value::Array(items) => items
                .iter()
                .enumerate()
                .map(|(index, item)| T::read(item, &member(at, &index.to_string())))
                .collect(),
            other => expected(at, Self::WHAT, Some(other)),
        }
    }
}

/// An object whose every member is one kind, under any key. A key is a key and nothing more.
impl<T: Param> Param for BTreeMap<String, T> {
    const WHAT: &'static str = "an object";

    fn read(value: &Value, at: &str) -> Checked<Self> {
        match value {
            Value::Object(members) => members
                .iter()
                .map(|(key, item)| Ok((key.clone(), T::read(item, &member(at, key))?)))
                .collect(),
            other => expected(at, Self::WHAT, Some(other)),
        }
    }
}

/// An edit: `[from, to, insert]`.
impl Param for (i64, i64, String) {
    const WHAT: &'static str = "a list of 3";

    fn read(value: &Value, at: &str) -> Checked<Self> {
        match value {
            Value::Array(items) if items.len() == 3 => {
                let item = |index: usize| items.get(index).unwrap_or(&Value::Null);
                Ok((
                    i64::read(item(0), &member(at, "0"))?,
                    i64::read(item(1), &member(at, "1"))?,
                    String::read(item(2), &member(at, "2"))?,
                ))
            }
            other => expected(at, Self::WHAT, Some(other)),
        }
    }
}

/// The members of an object.
pub struct Fields<'a> {
    /// None for params that were not sent, which are `{}`.
    members: Option<&'a Map<String, Value>>,
    at: String,
}

impl<'a> Fields<'a> {
    /// The members of `value`, an object, at `at`.
    pub fn of(value: &'a Value, at: &str) -> Checked<Self> {
        match value {
            Value::Object(members) => Ok(Self {
                members: Some(members),
                at: at.to_owned(),
            }),
            other => expected(at, "an object", Some(other)),
        }
    }

    fn get(&self, key: &str) -> Option<&'a Value> {
        self.members.and_then(|members| members.get(key))
    }

    /// A member that is there and is not `null`.
    pub fn required<T: Param>(&self, key: &str) -> Checked<T> {
        let at = member(&self.at, key);
        match self.get(key) {
            Some(value) if !value.is_null() => T::read(value, &at),
            got => expected(&at, T::WHAT, got),
        }
    }

    /// A member that may be left out, and is never `null`.
    pub fn optional<T: Param>(&self, key: &str) -> Checked<Option<T>> {
        let at = member(&self.at, key);
        match self.get(key) {
            None => Ok(None),
            Some(Value::Null) => expected(&at, T::WHAT, Some(&Value::Null)),
            Some(value) => T::read(value, &at).map(Some),
        }
    }

    /// A member that is there, and may be `null`.
    pub fn nullable<T: Param>(&self, key: &str) -> Checked<Option<T>> {
        let at = member(&self.at, key);
        match self.get(key) {
            Some(Value::Null) => Ok(None),
            Some(value) => T::read(value, &at).map(Some),
            None => expected(&at, &format!("{} or null", T::WHAT), None),
        }
    }

    /// A string with at least one character.
    pub fn text(&self, key: &str) -> Checked<String> {
        let text: String = self.required(key)?;
        if text.is_empty() {
            return wrong(&member(&self.at, key), "expected at least 1 character");
        }
        Ok(text)
    }

    /// A place or a count: a whole number, 0 or more.
    pub fn count(&self, key: &str) -> Checked<usize> {
        let whole: i64 = self.required(key)?;
        self.at_least_zero(key, whole)
    }

    /// The same, for a member that may be left out.
    pub fn optional_count(&self, key: &str) -> Checked<Option<usize>> {
        match self.optional::<i64>(key)? {
            Some(whole) => self.at_least_zero(key, whole).map(Some),
            None => Ok(None),
        }
    }

    fn at_least_zero(&self, key: &str, whole: i64) -> Checked<usize> {
        match usize::try_from(whole) {
            Ok(count) => Ok(count),
            Err(_) => wrong(&member(&self.at, key), "expected at least 0"),
        }
    }

    /// One of `names`.
    pub fn one_of<'n>(&self, key: &str, names: &[&'n str]) -> Checked<&'n str> {
        let at = member(&self.at, key);
        let words = format!("one of {}", names.join(", "));
        match self.get(key) {
            Some(Value::String(name)) => match names.iter().find(|known| *known == name) {
                Some(known) => Ok(known),
                None => wrong(&at, &format!("expected {words}")),
            },
            got => expected(&at, &words, got),
        }
    }
}

/// The params of a call, as it reads them.
pub trait Params: Sized {
    fn read(fields: &Fields<'_>) -> Checked<Self>;
}

/// The params the page sent, as the call reads them; none sent is `{}`.
pub fn parse<P: Params>(params: &Value) -> Checked<P> {
    match params {
        Value::Null => P::read(&Fields {
            members: None,
            at: String::new(),
        }),
        value => P::read(&Fields::of(value, "")?),
    }
}

/// A call that asks for nothing.
impl Params for () {
    fn read(_: &Fields<'_>) -> Checked<Self> {
        Ok(())
    }
}

/// A call that asks for one path.
pub struct OnePath {
    pub path: String,
}

impl Params for OnePath {
    fn read(fields: &Fields<'_>) -> Checked<Self> {
        Ok(Self {
            path: fields.text("path")?,
        })
    }
}

impl Param for Place {
    const WHAT: &'static str = "an object";

    fn read(value: &Value, at: &str) -> Checked<Self> {
        let fields = Fields::of(value, at)?;
        Ok(Self {
            anchor: fields.required("anchor")?,
            head: fields.required("head")?,
            top: fields.required("top")?,
        })
    }
}

impl Param for UiState {
    const WHAT: &'static str = "an object";

    fn read(value: &Value, at: &str) -> Checked<Self> {
        let fields = Fields::of(value, at)?;
        Ok(Self {
            tabs: fields.required("tabs")?,
            active: fields.nullable("active")?,
            expanded: fields.required("expanded")?,
            places: fields.required("places")?,
            recent: fields.required("recent")?,
            preview: fields.required("preview")?,
        })
    }
}

/// Settings as the page sends them: every one there; a size is made whole and kept within its
/// bounds.
impl Param for Settings {
    const WHAT: &'static str = "an object";

    fn read(value: &Value, at: &str) -> Checked<Self> {
        let fields = Fields::of(value, at)?;
        let size = |key: &str, sizes| -> Checked<u32> {
            let number: Number = fields.required(key)?;
            Ok(clamp(number.as_f64().unwrap_or(0.0), sizes))
        };
        Ok(Self {
            wrap: fields.required("wrap")?,
            line_numbers: LineNumbers::named(fields.one_of("lineNumbers", &LineNumbers::NAMES)?)
                .unwrap_or(LineNumbers::On),
            tab_size: size("tabSize", TAB_SIZES)?,
            insert_spaces: fields.required("insertSpaces")?,
            whitespace: Whitespace::named(fields.one_of("whitespace", &Whitespace::NAMES)?)
                .unwrap_or(Whitespace::Selection),
            indent_guides: fields.required("indentGuides")?,
            auto_save: fields.required("autoSave")?,
            sidebar: fields.required("sidebar")?,
            sidebar_width: size("sidebarWidth", SIDEBAR_WIDTHS)?,
            diff_decorations: fields.required("diffDecorations")?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ui() -> Value {
        json!({ "tabs": [], "active": null, "expanded": [], "places": {}, "recent": [], "preview": [] })
    }

    fn ui_with(key: &str, value: Option<Value>) -> Value {
        let mut ui = ui();
        if let Value::Object(members) = &mut ui {
            match value {
                Some(value) => members.insert(key.to_owned(), value),
                None => members.remove(key),
            };
        }
        json!({ "ui": ui })
    }

    struct Ui(UiState);

    impl Params for Ui {
        fn read(fields: &Fields<'_>) -> Checked<Self> {
            Ok(Self(fields.required("ui")?))
        }
    }

    fn refusal(params: &Value) -> String {
        match parse::<Ui>(params) {
            Ok(_) => "taken".to_owned(),
            Err(why) => why,
        }
    }

    #[test]
    fn a_refusal_says_where_and_what() {
        assert!(parse::<Ui>(&ui_with("active", None)).is_err());
        assert_eq!(
            refusal(&ui_with("tabs", Some(json!(["a", 1])))),
            "ui.tabs.1: expected a string, got number"
        );
        assert_eq!(
            refusal(&ui_with("active", None)),
            "ui.active: expected a string or null, got nothing"
        );
        assert_eq!(
            refusal(&ui_with(
                "places",
                Some(json!({ "a": { "anchor": 0, "head": 0 } }))
            )),
            "ui.places.a.top: expected a number, got nothing"
        );
        assert_eq!(
            refusal(&ui_with("places", Some(json!([])))),
            "ui.places: expected an object, got list"
        );
        assert_eq!(refusal(&json!([])), "expected an object, got list");
        assert_eq!(refusal(&Value::Null), "ui: expected an object, got nothing");
    }

    #[test]
    fn null_is_not_absent() {
        let read = parse::<Ui>(&ui_with("active", Some(Value::Null))).unwrap();
        assert_eq!(read.0.active, None);
        struct From(Option<usize>);
        impl Params for From {
            fn read(fields: &Fields<'_>) -> Checked<Self> {
                Ok(Self(fields.optional_count("from")?))
            }
        }
        assert_eq!(parse::<From>(&json!({})).map(|from| from.0), Ok(None));
        assert_eq!(
            parse::<From>(&json!({ "from": 2.0 })).map(|from| from.0),
            Ok(Some(2))
        );
        assert_eq!(
            parse::<From>(&json!({ "from": null })).map(|from| from.0),
            Err("from: expected an integer, got null".to_owned())
        );
        assert_eq!(
            parse::<From>(&json!({ "from": -1 })).map(|from| from.0),
            Err("from: expected at least 0".to_owned())
        );
    }

    #[test]
    fn every_key_of_a_record_is_a_key() {
        let place = json!({ "anchor": 1, "head": 2, "top": 3 });
        let places = json!({ "__proto__": place, "constructor": place, "__index": place });
        let read = parse::<Ui>(&ui_with("places", Some(places))).unwrap();
        assert_eq!(
            read.0.places.keys().collect::<Vec<_>>(),
            ["__index", "__proto__", "constructor"]
        );
    }
}
