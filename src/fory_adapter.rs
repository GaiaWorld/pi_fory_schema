//! Adapter boundary only; serializer bounds and registration are reserved.
use crate::{RootEncoding, SchemaDescriptor, SchemaError, SchemaRegistry};

/// Uses the same explicit registry as root description and Fory registration.
/// Serializer bounds remain reserved; no registration facts are inferred from T.
pub fn describe<T: crate::DescribeSchema + 'static>(
    _registry: &SchemaRegistry,
    _encoding: &RootEncoding,
) -> Result<SchemaDescriptor, SchemaError> {
    Err(SchemaError::NotImplemented("fory_adapter::describe"))
}

/// Discard the Fory instance if applying registrations fails after any mutation.
pub fn apply_to_fory(
    _registry: &SchemaRegistry,
    _fory: &mut fory::Fory,
) -> Result<(), SchemaError> {
    Err(SchemaError::NotImplemented("fory_adapter::apply_to_fory"))
}
