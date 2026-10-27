use super::{ForyAdapterInner, RootEncoding, SchemaDescriptor, SchemaError, SchemaRegistry};

impl ForyAdapterInner {
    pub(super) fn describe<T: crate::DescribeSchema + 'static>(
        _registry: &SchemaRegistry,
        _encoding: &RootEncoding,
    ) -> Result<SchemaDescriptor, SchemaError> {
        _registry.describe::<T>(_encoding)
    }

    pub(super) fn apply_to_fory(
        _registry: &SchemaRegistry,
        _fory: &mut fory::Fory,
    ) -> Result<(), SchemaError> {
        _registry.apply_to_fory(_fory)
    }
}
