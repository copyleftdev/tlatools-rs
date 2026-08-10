use std::collections::BTreeMap;
use std::path::Path;

use tla_eval::{Evaluator, Spec, State, Value};

pub fn spec(name: &str) -> Spec {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../specs")
        .join(format!("{name}.tla"));
    let src = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    Spec::parse(&src).unwrap_or_else(|e| panic!("{name}: {e}"))
}

pub fn evaluator<'a>(spec: &'a Spec, constants: &[(&str, Value)]) -> Evaluator<'a> {
    let map = constants
        .iter()
        .map(|(k, v)| ((*k).to_string(), v.clone()))
        .collect();
    Evaluator::new(spec, map).expect("constants cover the declarations")
}

pub fn st(pairs: &[(&str, Value)]) -> State {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), v.clone()))
        .collect()
}

pub fn rec(pairs: &[(&str, Value)]) -> Value {
    Value::Record(
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), v.clone()))
            .collect::<BTreeMap<_, _>>(),
    )
}

pub fn set<const N: usize>(items: [Value; N]) -> Value {
    Value::set(items)
}

pub fn seq<const N: usize>(items: [Value; N]) -> Value {
    Value::Seq(items.into_iter().collect())
}

pub fn s(text: &str) -> Value {
    Value::string(text)
}

pub fn n(value: i64) -> Value {
    Value::Int(value)
}

pub fn strs<const N: usize>(items: [&str; N]) -> Value {
    Value::set(items.map(Value::string))
}
