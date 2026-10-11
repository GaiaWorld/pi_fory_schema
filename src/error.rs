//! Classified errors with structural locations only; never embed raw KV data.
use std::{error::Error, fmt};

/// Structural steps from the root. Names come from schema metadata, not values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PathSegment {
    Field(String),
    Element(usize),
    Variant(String),
    MapKey(usize),
    MapValue(usize),
    /// Element type in a list, array, or set; not an input element position.
    ElementType,
    /// Map key type; not an input entry position.
    MapKeyType,
    /// Map value type; not an input entry position.
    MapValueType,
}

/// Optional structural coordinates; never includes raw input contents.
#[derive(Clone, Debug, Default)]
pub struct ErrorLocation {
    /// Byte position when available.
    pub byte_offset: Option<usize>,
    /// Descriptor position when available.
    pub descriptor_index: Option<usize>,
    /// None means unavailable; Some(empty) identifies the root itself.
    /// Map entries use positions, never the actual key contents.
    pub path: Option<Vec<PathSegment>>,
}

/// Machine-readable registry failures without local type IDs or entry indexes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegistryErrorKind {
    UnregisteredType,
    UnregisteredDependency,
    TypeIdentityConflict,
    IdentityTypeConflict,
    InvalidIdentity,
    MissingDefinition,
    DefinitionIdentityMismatch,
    DefinitionCategoryMismatch,
}

/// Descriptor, document, compilation, and registry failures.
#[derive(Debug)]
pub enum SchemaError {
    NotImplemented(&'static str),
    Registry {
        kind: RegistryErrorKind,
        location: ErrorLocation,
    },
    /// Preserves an error from a Fory registration call, not KV serialization.
    ForyRegistration {
        source: fory::Error,
        location: ErrorLocation,
    },
    InvalidSchema(ErrorLocation),
    UnsupportedVersion(ErrorLocation),
    UnsupportedType(ErrorLocation),
    MalformedInput(ErrorLocation),
    SchemaMismatch(ErrorLocation),
    UnsupportedKeyCapability(ErrorLocation),
    AccessError(ErrorLocation),
    ResourceLimit(ErrorLocation),
}

impl fmt::Display for SchemaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl Error for SchemaError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::ForyRegistration { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// Input parsing, validation, and view-access failures.
/// Clone preserves the first terminal parsing error and its original location.
#[derive(Clone, Debug)]
pub enum DataError {
    NotImplemented(&'static str),
    InvalidSchema(ErrorLocation),
    UnsupportedVersion(ErrorLocation),
    UnsupportedType(ErrorLocation),
    MalformedInput(ErrorLocation),
    SchemaMismatch(ErrorLocation),
    UnsupportedKeyCapability(ErrorLocation),
    AccessError(ErrorLocation),
    ResourceLimit(ErrorLocation),
}

impl fmt::Display for DataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl Error for DataError {}

/// Key capability and logical-operation failures.
/// Normal key operations use validated data and do not revalidate or check
/// resource budgets. `MalformedInput` and `ResourceLimit` remain declared;
/// which construction entry points can produce them is a pending decision.
#[derive(Debug)]
pub enum KeyError {
    NotImplemented(&'static str),
    InvalidSchema(ErrorLocation),
    UnsupportedVersion(ErrorLocation),
    UnsupportedType(ErrorLocation),
    MalformedInput(ErrorLocation),
    SchemaMismatch(ErrorLocation),
    UnsupportedKeyCapability(ErrorLocation),
    AccessError(ErrorLocation),
    ResourceLimit(ErrorLocation),
}

impl fmt::Display for KeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl Error for KeyError {}
