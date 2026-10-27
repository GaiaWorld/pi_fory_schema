//! Adapter boundary only; serializer bounds and registration are reserved.
use crate::{RootEncoding, SchemaDescriptor, SchemaError, SchemaRegistry};

#[path = "fory_adapter_inner.rs"]
mod fory_adapter_inner;

struct ForyAdapterInner;

/// Uses the same explicit registry as root description and Fory registration.
/// Serializer bounds remain reserved; no registration facts are inferred from T.
pub fn describe<T: crate::DescribeSchema + 'static>(
    _registry: &SchemaRegistry,
    _encoding: &RootEncoding,
) -> Result<SchemaDescriptor, SchemaError> {
    ForyAdapterInner::describe::<T>(_registry, _encoding)
}

/// Discard the Fory instance if applying registrations fails after any mutation.
pub fn apply_to_fory(
    _registry: &SchemaRegistry,
    _fory: &mut fory::Fory,
) -> Result<(), SchemaError> {
    ForyAdapterInner::apply_to_fory(_registry, _fory)
}
