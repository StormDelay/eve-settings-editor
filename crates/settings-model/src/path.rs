//! Index-based addressing of nodes in a `Value` tree. Paths are sequences
//! of steps from the root; indices (not keys) because dict keys are
//! arbitrary `Value`s (tuples are real keys in these files) and entry order
//! is wire order, which mutations preserve.

use blue_marshal::Value;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "s", content = "i", rename_all = "snake_case")]
pub enum Step {
    Tuple(usize),
    List(usize),
    DictKey(usize),
    DictValue(usize),
    InstanceClass,
    InstanceState,
    ReduceCtor,
    ReduceItem(usize),
    ReducePairKey(usize),
    ReducePairValue(usize),
    SharedInner,
    StreamInner,
}

pub type NodePath = Vec<Step>;

pub fn resolve<'a>(root: &'a Value, path: &[Step]) -> Option<&'a Value> {
    let mut cur = root;
    for step in path {
        cur = match (step, cur) {
            (Step::Tuple(i), Value::Tuple(items)) => items.get(*i)?,
            (Step::List(i), Value::List(items)) => items.get(*i)?,
            (Step::DictKey(i), Value::Dict(entries)) => &entries.get(*i)?.0,
            (Step::DictValue(i), Value::Dict(entries)) => &entries.get(*i)?.1,
            (Step::InstanceClass, Value::Instance { class, .. }) => class,
            (Step::InstanceState, Value::Instance { state, .. }) => state,
            (Step::ReduceCtor, Value::Reduce { ctor, .. }) => ctor,
            (Step::ReduceItem(i), Value::Reduce { items, .. }) => items.get(*i)?,
            (Step::ReducePairKey(i), Value::Reduce { pairs, .. }) => &pairs.get(*i)?.0,
            (Step::ReducePairValue(i), Value::Reduce { pairs, .. }) => &pairs.get(*i)?.1,
            (Step::SharedInner, Value::Shared { value, .. }) => value,
            (Step::StreamInner, Value::Stream(inner)) => inner,
            _ => return None,
        };
    }
    Some(cur)
}

pub fn resolve_mut<'a>(root: &'a mut Value, path: &[Step]) -> Option<&'a mut Value> {
    let mut cur = root;
    for step in path {
        cur = match (step, cur) {
            (Step::Tuple(i), Value::Tuple(items)) => items.get_mut(*i)?,
            (Step::List(i), Value::List(items)) => items.get_mut(*i)?,
            (Step::DictKey(i), Value::Dict(entries)) => &mut entries.get_mut(*i)?.0,
            (Step::DictValue(i), Value::Dict(entries)) => &mut entries.get_mut(*i)?.1,
            (Step::InstanceClass, Value::Instance { class, .. }) => class,
            (Step::InstanceState, Value::Instance { state, .. }) => state,
            (Step::ReduceCtor, Value::Reduce { ctor, .. }) => ctor,
            (Step::ReduceItem(i), Value::Reduce { items, .. }) => items.get_mut(*i)?,
            (Step::ReducePairKey(i), Value::Reduce { pairs, .. }) => &mut pairs.get_mut(*i)?.0,
            (Step::ReducePairValue(i), Value::Reduce { pairs, .. }) => &mut pairs.get_mut(*i)?.1,
            (Step::SharedInner, Value::Shared { value, .. }) => value,
            (Step::StreamInner, Value::Stream(inner)) => inner,
            _ => return None,
        };
    }
    Some(cur)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Value {
        // { b"windows": ( 1, [ 2.5 ] ) } with the tuple shared as slot 1
        Value::Dict(vec![(
            Value::Bytes(b"windows".to_vec()),
            Value::Shared {
                slot: 1,
                value: Box::new(Value::Tuple(vec![
                    Value::Int(1),
                    Value::List(vec![Value::Float(2.5)]),
                ])),
            },
        )])
    }

    #[test]
    fn resolve_walks_every_step_kind_used_in_real_files() {
        let v = sample();
        assert_eq!(
            resolve(&v, &[Step::DictKey(0)]),
            Some(&Value::Bytes(b"windows".to_vec()))
        );
        let deep = [
            Step::DictValue(0),
            Step::SharedInner,
            Step::Tuple(1),
            Step::List(0),
        ];
        assert_eq!(resolve(&v, &deep), Some(&Value::Float(2.5)));
        assert_eq!(resolve(&v, &[Step::DictValue(1)]), None); // out of range
        assert_eq!(resolve(&v, &[Step::List(0)]), None); // kind mismatch
    }

    #[test]
    fn resolve_mut_reaches_the_same_node() {
        let mut v = sample();
        let deep = [
            Step::DictValue(0),
            Step::SharedInner,
            Step::Tuple(0),
        ];
        *resolve_mut(&mut v, &deep).unwrap() = Value::Int(42);
        assert_eq!(resolve(&v, &deep), Some(&Value::Int(42)));
    }

    #[test]
    fn step_serde_shape_is_stable() {
        // The UI stores and replays these — the wire shape is a contract.
        let json = serde_json::to_string(&Step::DictValue(3)).unwrap();
        assert_eq!(json, r#"{"s":"dict_value","i":3}"#);
        let json = serde_json::to_string(&Step::SharedInner).unwrap();
        assert_eq!(json, r#"{"s":"shared_inner"}"#);
        let back: Step = serde_json::from_str(r#"{"s":"tuple","i":2}"#).unwrap();
        assert_eq!(back, Step::Tuple(2));
    }
}
