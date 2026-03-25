// =============================================================================
// Serde helper for distinguishing absent vs null in JSON.
//
// Standard serde treats both absent fields and explicit `null` as `None` for
// `Option<Option<T>>`. This module provides a custom deserializer that maps:
//   - absent field -> `None`        (field not touched)
//   - `null`       -> `Some(None)`  (field explicitly cleared)
//   - `"value"`    -> `Some(Some("value"))` (field set to value)
//
// Usage: `#[serde(default, deserialize_with = "deserialize_optional_field")]`
// =============================================================================

use serde::{Deserialize, Deserializer};

/// Deserializes an optional nullable field, distinguishing absent from null.
///
/// When the JSON key is absent (and `#[serde(default)]` is set), serde skips
/// the deserializer entirely and uses `Default::default()` = `None`.
/// When the key is present with `null`, this function returns `Some(None)`.
/// When the key is present with a value, this returns `Some(Some(value))`.
///
/// # Errors
///
/// Returns a deserialization error if the underlying value cannot be parsed
/// as `T`.
pub fn deserialize_optional_field<'de, T, D>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    // Deserialize the value. If the key is present, serde calls this function.
    // `Option::<T>::deserialize` maps `null` to `None` and a value to `Some(v)`.
    let value: Option<T> = Option::deserialize(deserializer)?;
    // Wrap in `Some(...)` to indicate the field was explicitly present.
    Ok(Some(value))
}
