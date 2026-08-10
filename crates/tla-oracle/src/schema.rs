use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;
use serde_json::Value as Json;
use tla_eval::Value;

/// How to read a JSON value as a TLA+ one.
///
/// The schema is explicit rather than inferred because JSON cannot tell a set
/// from a sequence — both arrive as an array, and reading one as the other
/// silently changes what the oracle checks.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Schema {
    Int,
    Bool,
    Str,
    Set {
        of: Box<Schema>,
    },
    Seq {
        of: Box<Schema>,
    },
    /// An object with arbitrary keys — a function from strings.
    Map {
        of: Box<Schema>,
    },
    /// An object whose keys are fixed by the schema.
    Rec {
        fields: BTreeMap<String, Schema>,
    },
    /// Alternatives tried in order; used for sets whose members differ in
    /// shape, such as a message set carrying several kinds of message.
    Union {
        of: Vec<Schema>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodeError {
    pub path: String,
    pub message: String,
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.path, self.message)
    }
}

impl std::error::Error for DecodeError {}

type Result<T> = std::result::Result<T, DecodeError>;

fn fail<T>(path: &str, message: impl Into<String>) -> Result<T> {
    Err(DecodeError {
        path: path.to_string(),
        message: message.into(),
    })
}

pub fn decode(json: &Json, schema: &Schema, path: &str) -> Result<Value> {
    match schema {
        Schema::Int => match json.as_i64() {
            Some(n) if !json.is_boolean() => Ok(Value::Int(n)),
            _ => fail(path, format!("expected an integer, got {}", kind_of(json))),
        },
        Schema::Bool => match json.as_bool() {
            Some(b) => Ok(Value::Bool(b)),
            None => fail(path, format!("expected a boolean, got {}", kind_of(json))),
        },
        Schema::Str => match json.as_str() {
            Some(s) => Ok(Value::string(s)),
            None => fail(path, format!("expected a string, got {}", kind_of(json))),
        },
        Schema::Set { of } => {
            let items = array(json, path)?;
            let mut out = BTreeSet::new();
            for (i, item) in items.iter().enumerate() {
                let value = decode(item, of, &format!("{path}[{i}]"))?;
                if !out.insert(value) {
                    return fail(
                        path,
                        format!("element {i} is a duplicate; a set has no repeats"),
                    );
                }
            }
            Ok(Value::Set(out))
        }
        Schema::Seq { of } => {
            let items = array(json, path)?;
            let mut out = Vec::with_capacity(items.len());
            for (i, item) in items.iter().enumerate() {
                out.push(decode(item, of, &format!("{path}[{i}]"))?);
            }
            Ok(Value::Seq(out))
        }
        Schema::Map { of } => {
            let fields = object(json, path)?;
            let mut out = BTreeMap::new();
            for (k, v) in fields {
                out.insert(k.clone(), decode(v, of, &format!("{path}.{k}"))?);
            }
            Ok(Value::Record(out))
        }
        Schema::Rec { fields } => {
            let given = object(json, path)?;
            let expected: BTreeSet<&String> = fields.keys().collect();
            let actual: BTreeSet<&String> = given.keys().collect();
            if expected != actual {
                return fail(
                    path,
                    format!(
                        "field mismatch (missing={:?}, unexpected={:?})",
                        sorted_diff(&expected, &actual),
                        sorted_diff(&actual, &expected)
                    ),
                );
            }
            let mut out = BTreeMap::new();
            for (name, field_schema) in fields {
                let v = &given[name];
                out.insert(
                    name.clone(),
                    decode(v, field_schema, &format!("{path}.{name}"))?,
                );
            }
            Ok(Value::Record(out))
        }
        Schema::Union { of } => {
            let mut reasons = Vec::new();
            for alternative in of {
                match decode(json, alternative, path) {
                    Ok(v) => return Ok(v),
                    Err(e) => reasons.push(e.message),
                }
            }
            fail(
                path,
                format!("matches no alternative ({})", reasons.join("; ")),
            )
        }
    }
}

/// Read a whole state: exactly the specification's variables, nothing else.
pub fn decode_state(
    json: &Json,
    schema: &BTreeMap<String, Schema>,
    path: &str,
) -> Result<tla_eval::State> {
    let given = object(json, path)?;
    let expected: BTreeSet<&String> = schema.keys().collect();
    let actual: BTreeSet<&String> = given.keys().collect();
    if expected != actual {
        return fail(
            path,
            format!(
                "state keys must be exactly the specification's variables \
                 (missing={:?}, unexpected={:?})",
                sorted_diff(&expected, &actual),
                sorted_diff(&actual, &expected)
            ),
        );
    }
    let mut state = tla_eval::State::new();
    for (name, field) in schema {
        state.insert(
            name.clone(),
            decode(&given[name], field, &format!("{path}.{name}"))?,
        );
    }
    Ok(state)
}

fn array<'a>(json: &'a Json, path: &str) -> Result<&'a Vec<Json>> {
    json.as_array().map_or_else(
        || fail(path, format!("expected an array, got {}", kind_of(json))),
        Ok,
    )
}

fn object<'a>(json: &'a Json, path: &str) -> Result<&'a serde_json::Map<String, Json>> {
    json.as_object().map_or_else(
        || fail(path, format!("expected an object, got {}", kind_of(json))),
        Ok,
    )
}

fn sorted_diff(a: &BTreeSet<&String>, b: &BTreeSet<&String>) -> Vec<String> {
    a.difference(b).map(|s| (*s).clone()).collect()
}

fn kind_of(json: &Json) -> &'static str {
    match json {
        Json::Null => "null",
        Json::Bool(_) => "a boolean",
        Json::Number(_) => "a number",
        Json::String(_) => "a string",
        Json::Array(_) => "an array",
        Json::Object(_) => "an object",
    }
}
