use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// A TLA+ value.
///
/// Sequences, records and functions are all functions in TLA+, and the same
/// value must not have two representations or equality would depend on how it
/// was written. [`Value::function`] is the only way to build one, and it picks
/// the representation from the domain: `1..n` gives a sequence, all-string
/// gives a record, anything else stays a general function.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Value {
    Bool(bool),
    Int(i64),
    Str(String),
    Seq(Vec<Value>),
    Set(BTreeSet<Value>),
    Record(BTreeMap<String, Value>),
    Func(BTreeMap<Value, Value>),
    /// A set too large to enumerate. Membership is decidable; iteration is not.
    Infinite(Infinite),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Infinite {
    Nat,
    Int,
    Strings,
    /// `Seq(S)` — every finite sequence over `S`.
    Sequences(Box<Value>),
}

/// A length or cardinality as a TLA+ integer. No collection can hold more than
/// `isize::MAX` elements, so this never loses information.
pub(crate) fn count(n: usize) -> i64 {
    i64::try_from(n).expect("no collection exceeds isize::MAX elements")
}

impl Value {
    pub fn set(items: impl IntoIterator<Item = Value>) -> Value {
        Value::Set(items.into_iter().collect())
    }

    pub fn string(s: impl Into<String>) -> Value {
        Value::Str(s.into())
    }

    pub fn interval(lo: i64, hi: i64) -> Value {
        Value::Set((lo..=hi).map(Value::Int).collect())
    }

    pub fn record(fields: impl IntoIterator<Item = (String, Value)>) -> Value {
        Value::Record(fields.into_iter().collect())
    }

    /// Build a function, choosing the representation its domain implies.
    pub fn function(entries: BTreeMap<Value, Value>) -> Value {
        if entries.is_empty() {
            return Value::Seq(Vec::new());
        }
        if entries
            .keys()
            .enumerate()
            .all(|(i, k)| matches!(k, Value::Int(n) if *n == count(i) + 1))
        {
            return Value::Seq(entries.into_values().collect());
        }
        if entries.keys().all(|k| matches!(k, Value::Str(_))) {
            return Value::Record(
                entries
                    .into_iter()
                    .map(|(k, v)| match k {
                        Value::Str(s) => (s, v),
                        _ => unreachable!("keys checked to be strings"),
                    })
                    .collect(),
            );
        }
        Value::Func(entries)
    }

    /// The function's graph, for values that are functions.
    pub fn entries(&self) -> Option<BTreeMap<Value, Value>> {
        match self {
            Value::Seq(items) => Some(
                items
                    .iter()
                    .enumerate()
                    .map(|(i, v)| (Value::Int(count(i) + 1), v.clone()))
                    .collect(),
            ),
            Value::Record(fields) => Some(
                fields
                    .iter()
                    .map(|(k, v)| (Value::Str(k.clone()), v.clone()))
                    .collect(),
            ),
            Value::Func(map) => Some(map.clone()),
            _ => None,
        }
    }

    pub fn domain(&self) -> Option<BTreeSet<Value>> {
        match self {
            Value::Seq(items) => Some((1..=count(items.len())).map(Value::Int).collect()),
            Value::Record(fields) => Some(fields.keys().cloned().map(Value::Str).collect()),
            Value::Func(map) => Some(map.keys().cloned().collect()),
            _ => None,
        }
    }

    pub fn apply(&self, key: &Value) -> Option<Value> {
        match (self, key) {
            (Value::Seq(items), Value::Int(i)) => usize::try_from(*i)
                .ok()?
                .checked_sub(1)
                .and_then(|i| items.get(i).cloned()),
            (Value::Record(fields), Value::Str(k)) => fields.get(k).cloned(),
            (Value::Func(map), k) => map.get(k).cloned(),
            _ => None,
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Bool(_) => "a boolean",
            Value::Int(_) => "an integer",
            Value::Str(_) => "a string",
            Value::Seq(_) => "a sequence",
            Value::Set(_) | Value::Infinite(_) => "a set",
            Value::Record(_) => "a record",
            Value::Func(_) => "a function",
        }
    }

    pub fn is_set(&self) -> bool {
        matches!(self, Value::Set(_) | Value::Infinite(_))
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Bool(b) => f.write_str(if *b { "TRUE" } else { "FALSE" }),
            Value::Int(n) => write!(f, "{n}"),
            Value::Str(s) => write!(f, "{s:?}"),
            Value::Seq(items) => write!(f, "<<{}>>", join(items.iter())),
            Value::Set(items) => write!(f, "{{{}}}", join(items.iter())),
            Value::Record(fields) => {
                let body: Vec<String> =
                    fields.iter().map(|(k, v)| format!("{k} |-> {v}")).collect();
                write!(f, "[{}]", body.join(", "))
            }
            Value::Func(map) => {
                let body: Vec<String> = map.iter().map(|(k, v)| format!("{k} :> {v}")).collect();
                write!(f, "({})", body.join(" @@ "))
            }
            Value::Infinite(Infinite::Nat) => f.write_str("Nat"),
            Value::Infinite(Infinite::Int) => f.write_str("Int"),
            Value::Infinite(Infinite::Strings) => f.write_str("STRING"),
            Value::Infinite(Infinite::Sequences(s)) => write!(f, "Seq({s})"),
        }
    }
}

fn join<'a>(items: impl Iterator<Item = &'a Value>) -> String {
    items
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}
