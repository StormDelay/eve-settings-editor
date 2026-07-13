use blue_marshal::Value;

/// True if any node in the subtree is a `Shared` store. Removing such a
/// subtree would orphan its slot (encode fails SlotOutOfRange) or dangle
/// Refs elsewhere — so removal is blocked at the mutation layer.
pub fn subtree_contains_shared(v: &Value) -> bool {
    match v {
        Value::Shared { .. } => true,
        Value::Tuple(items) | Value::List(items) => items.iter().any(subtree_contains_shared),
        Value::Dict(entries) => entries
            .iter()
            .any(|(k, val)| subtree_contains_shared(k) || subtree_contains_shared(val)),
        Value::Stream(inner) => subtree_contains_shared(inner),
        Value::Instance { class, state } => {
            subtree_contains_shared(class) || subtree_contains_shared(state)
        }
        Value::Reduce { ctor, items, pairs } => {
            subtree_contains_shared(ctor)
                || items.iter().any(subtree_contains_shared)
                || pairs
                    .iter()
                    .any(|(k, v)| subtree_contains_shared(k) || subtree_contains_shared(v))
        }
        _ => false,
    }
}
