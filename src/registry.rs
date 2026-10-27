//! Shared protocol registration facts and root-reachable schema extraction.
//! Registration, conflict checks, extraction, and application are unimplemented.
use crate::{
    PathSegment, RootEncoding, SchemaDescriptor, SchemaError, TypeDescriptor, UserDefinition,
    UserIdentity,
};
use std::{
    any::TypeId,
    collections::{HashMap, HashSet, VecDeque},
};

#[path = "registry_inner.rs"]
mod registry_inner;

struct DescribeSchemaInner;
struct SchemaRegistryBuilderInner;
struct SchemaRegistryInner;

/// Trusted in-process description code; no serializer bound is assumed.
/// Generation has no resource budget. Compile the returned descriptor with limits.
/// Registry deduplication does not bound arbitrary callback allocation or recursion.
pub trait DescribeSchema: Sized {
    /// Describes a type use; user types return identity references, not definitions.
    /// Derived user descriptions clone identity_of::<Self>() into TypeKind::User.
    /// They do not expand fields or collect definitions.
    fn describe_type(
        encoding: &RootEncoding,
        registry: &SchemaRegistry,
    ) -> Result<TypeDescriptor, SchemaError>;

    /// Extracts only this user definition; built-in types return None.
    /// Dependencies remain identity references rather than expanded roots.
    /// Derived user definitions clone their registered identity and describe only
    /// participating fields or payloads via describe_type, in logical order.
    /// They never call child describe_definition or describe_schema methods.
    fn describe_definition(
        encoding: &RootEncoding,
        registry: &SchemaRegistry,
    ) -> Result<Option<UserDefinition>, SchemaError>;

    /// Delegates root collection to the registry; derive does not override it.
    fn describe_schema(
        encoding: &RootEncoding,
        registry: &SchemaRegistry,
    ) -> Result<SchemaDescriptor, SchemaError>
    where
        Self: 'static,
    {
        DescribeSchemaInner::describe_schema::<Self>(encoding, registry)
    }
}

/// Shared across tables to detect conflicting type identities before building.
pub struct SchemaRegistryBuilder {
    _data: RegistrationData,
}
/// Immutable registration facts; no public constructor bypasses the builder.
pub struct SchemaRegistry {
    _data: RegistrationData,
}

/// Both indexes point into the single registration entry sequence.
/// Rust TypeId is local-only and never becomes a protocol identity.
struct RegistrationData {
    _entries: Vec<RegistrationEntry>,
    _entry_by_rust_type: HashMap<TypeId, usize>,
    _entry_by_identity: HashMap<UserIdentity, usize>,
}

/// Identity is passed to registration callbacks rather than captured separately.
struct RegistrationEntry {
    _identity: UserIdentity,
    _describe_definition: DefinitionCallback,
    _register_fory: ForyRegistrationCallback,
}

/// Extracts one definition. A registered user type returning None is an error.
/// Its identity must match the entry; dependencies are read from the definition.
/// Builder entries use describe_registered_definition::<T>, not the raw trait method.
type DefinitionCallback =
    fn(&RootEncoding, &SchemaRegistry) -> Result<Option<UserDefinition>, SchemaError>;

/// Registers one concrete type using the entry's sole protocol identity.
/// Does not serialize, generate descriptions, or explicitly freeze Fory.
/// The builder supplies a monomorphized register_fory_type::<T> callback.
/// Name mapping preserves the identity: use type_name for an empty namespace,
/// otherwise namespace + '.' + type_name. Type names are nonempty and dot-free.
/// Each component is at most 32766 original UTF-8 bytes; Unicode is allowed.
/// The joined name has no additional 32766-byte limit. Error conversion is TBD.
/// Failures from Fory-generated auxiliary names must still propagate.
type ForyRegistrationCallback = fn(&mut fory::Fory, &UserIdentity) -> Result<(), SchemaError>;

/// First-discovered root-relative type path, released after processing the item.
/// No byte or descriptor position is inferred from this registry entry index.
struct PendingDefinition {
    _entry_index: usize,
    _path: Vec<PathSegment>,
}

/// Per-description temporary state, never retained by the registry.
/// Queue and set positions index registration entries, not output definitions.
/// Mark a dependency discovered before enqueueing it to keep cycles finite.
struct DescriptionCollection<'a> {
    _registry: &'a SchemaRegistry,
    _encoding: &'a RootEncoding,
    _root: TypeDescriptor,
    _pending: VecDeque<PendingDefinition>,
    _discovered: HashSet<usize>,
    _definitions: Vec<UserDefinition>,
}

impl Default for SchemaRegistryBuilder {
    fn default() -> Self {
        SchemaRegistryBuilderInner::default()
    }
}

impl SchemaRegistryBuilder {
    /// Creates an empty placeholder, not a successfully registered registry.
    pub fn new() -> Self {
        SchemaRegistryBuilderInner::new()
    }

    /// Same type and identity are idempotent; different types sharing an ID conflict.
    /// A single builder is shared across tables. These checks are not implemented.
    pub fn register_by_id<T>(&mut self, _id: u32) -> Result<(), SchemaError>
    where
        T: DescribeSchema + fory::StructSerializer<Target = T>,
    {
        SchemaRegistryBuilderInner::register_by_id::<T>(self, _id)
    }

    /// Registers a namespace/name identity with the same idempotence and conflict
    /// policy as numeric registration; performs no registration in this skeleton.
    /// Requires a nonempty, dot-free type name; namespace may be empty or dotted.
    /// Preserve exact strings without trimming, normalization, or escaping.
    /// These checks and qualified-name construction are not implemented.
    pub fn register_by_name<T>(
        &mut self,
        _namespace: &str,
        _type_name: &str,
    ) -> Result<(), SchemaError>
    where
        T: DescribeSchema + fory::StructSerializer<Target = T>,
    {
        SchemaRegistryBuilderInner::register_by_name::<T>(self, _namespace, _type_name)
    }

    /// Finalizes registration facts; currently returns `SchemaError::NotImplemented`.
    pub fn build(self) -> Result<SchemaRegistry, SchemaError> {
        SchemaRegistryBuilderInner::build(self)
    }
}

impl SchemaRegistry {
    /// Borrows the explicitly registered protocol identity of this concrete type.
    /// Does not register, allocate identities, or expose local entry positions.
    /// Built-in descriptions do not need this query. Missing-type errors are TBD.
    pub fn identity_of<T: 'static>(&self) -> Result<&UserIdentity, SchemaError> {
        SchemaRegistryInner::identity_of::<T>(self)
    }

    /// Extracts only definitions reachable from the requested root.
    pub fn describe<T: DescribeSchema + 'static>(
        &self,
        encoding: &RootEncoding,
    ) -> Result<SchemaDescriptor, SchemaError> {
        SchemaRegistryInner::describe::<T>(self, encoding)
    }

    /// Discard the Fory instance on partial registration failure.
    pub fn apply_to_fory(&self, _fory: &mut fory::Fory) -> Result<(), SchemaError> {
        SchemaRegistryInner::apply_to_fory(self, _fory)
    }
}

/// The sole root collector: uses describe_type and single-definition callbacks.
/// Never calls T::describe_schema, which delegates back to the registry.
fn collect_schema<T: DescribeSchema + 'static>(
    _encoding: &RootEncoding,
    _registry: &SchemaRegistry,
) -> Result<SchemaDescriptor, SchemaError> {
    SchemaRegistryInner::collect_schema::<T>(_encoding, _registry)
}
