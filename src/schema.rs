//! Compiled state cannot be forged through public constructors.
//! Root encoding and key semantics are carried by SchemaDescriptor.
use crate::{
    DataError, InputFormat, KeyRequirement, OwnedValidatedValue, OwnedValueView, ParseLimits,
    SchemaDescriptor, SchemaError, SchemaLimits, UserIdentity, ValidatedValue, ValueView,
};
use std::{collections::HashMap, sync::Arc};

/// Compiled, validated schema state; public construction goes through `compile`.
/// Reserved signature: `is_equivalent(&self, other: &Schema) -> bool`.
/// Equivalence means full schema equivalence, not merely a hash match.
/// No callable stub is supplied: a bool cannot report `NotImplemented`, and
/// returning a fabricated value or panicking would misrepresent the contract.
#[derive(Clone)]
pub struct Schema {
    /// Clones share compiled data without rebuilding descriptions or indexes.
    pub(crate) _data: Arc<SchemaData>,
}

/// Immutable compiled contract only; no input, parsing state, or graph identity.
pub(crate) struct SchemaData {
    /// Complete immutable contract, including root encoding and semantics.
    pub(crate) _descriptor: SchemaDescriptor,
    /// Private lookup data derived from the immutable descriptor.
    pub(crate) _type_access: TypeAccess,
}

/// Identity lookup only; values index SchemaDescriptor::definitions directly.
/// Map hashing and iteration order never define stable hashes or equivalence.
pub(crate) struct TypeAccess {
    pub(crate) _definition_by_identity: HashMap<UserIdentity, usize>,
    /// Keys index definitions; entries exist only for struct definitions.
    pub(crate) _struct_access: HashMap<usize, StructAccess>,
    /// Keys index definitions; entries exist only for enum or union definitions.
    pub(crate) _variant_access: HashMap<usize, VariantAccess>,
}

/// All positions index the descriptor's logical field sequence.
/// Input-specific offsets and remote metadata remain outside this structure.
pub(crate) struct StructAccess {
    /// Logical field positions in physical encoding order.
    pub(crate) _physical_field_order: Vec<usize>,
    /// Only explicitly declared IDs have entries.
    pub(crate) _field_by_id: HashMap<u32, usize>,
    pub(crate) _field_by_name: HashMap<String, usize>,
}

/// Protocol case IDs map to logical variant positions, never numeric ordering.
/// Names and payload descriptions remain in the original descriptor.
pub(crate) struct VariantAccess {
    pub(crate) _variant_by_id: HashMap<u32, usize>,
}

#[path = "schema_inner.rs"]
mod schema_inner;

struct SchemaInner;

impl Schema {
    /// Compiles and validates the descriptor within the supplied schema budget.
    pub fn compile(
        _descriptor: SchemaDescriptor,
        _limits: &SchemaLimits,
    ) -> Result<Self, SchemaError> {
        SchemaInner::compile(_descriptor, _limits)
    }
    /// Checks schema key capabilities without parsing data or consuming a parse budget.
    pub fn check_key_support(&self, _requirement: KeyRequirement) -> Result<(), SchemaError> {
        SchemaInner::check_key_support(self, _requirement)
    }
    /// Opens a borrowed, not-yet-validated root with a context-owned parse budget.
    pub fn view<'a>(
        &'a self,
        _input: &'a [u8],
        _format: InputFormat,
        _limits: &ParseLimits,
    ) -> Result<ValueView<'a>, DataError> {
        SchemaInner::view(self, _input, _format, _limits)
    }
    /// Opens and fully validates one borrowed root within the supplied budget.
    pub fn validate<'a>(
        &'a self,
        _input: &'a [u8],
        _format: InputFormat,
        _limits: &ParseLimits,
    ) -> Result<ValidatedValue<'a>, DataError> {
        SchemaInner::validate(self, _input, _format, _limits)
    }
    /// Retains the schema and stable input lease in an unvalidated root context.
    pub fn view_owned(
        self: &Arc<Self>,
        _input: bytes::Bytes,
        _format: InputFormat,
        _limits: &ParseLimits,
    ) -> Result<OwnedValueView, DataError> {
        SchemaInner::view_owned(self, _input, _format, _limits)
    }
    /// Retains the schema and input lease and validates within the supplied budget.
    pub fn validate_owned(
        self: &Arc<Self>,
        _input: bytes::Bytes,
        _format: InputFormat,
        _limits: &ParseLimits,
    ) -> Result<OwnedValidatedValue, DataError> {
        SchemaInner::validate_owned(self, _input, _format, _limits)
    }
    /// Convenience alias for the unified owned Bytes entry point.
    pub fn view_bytes(
        self: &Arc<Self>,
        _input: bytes::Bytes,
        _format: InputFormat,
        _limits: &ParseLimits,
    ) -> Result<OwnedValueView, DataError> {
        SchemaInner::view_bytes(self, _input, _format, _limits)
    }
    /// Convenience alias for the unified validated owned Bytes entry point.
    pub fn validate_bytes(
        self: &Arc<Self>,
        _input: bytes::Bytes,
        _format: InputFormat,
        _limits: &ParseLimits,
    ) -> Result<OwnedValidatedValue, DataError> {
        SchemaInner::validate_bytes(self, _input, _format, _limits)
    }
}
