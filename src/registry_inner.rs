use super::{
    DescribeSchema, DescribeSchemaInner, HashMap, RegistrationData, RootEncoding, SchemaDescriptor,
    SchemaError, SchemaRegistry, SchemaRegistryBuilder, SchemaRegistryBuilderInner,
    SchemaRegistryInner, UserDefinition, UserIdentity, collect_schema,
};
use super::{DescriptionCollection, PendingDefinition, RegistrationEntry};
use crate::{
    ErrorLocation, PathSegment, RegistryErrorKind, TypeDescriptor, TypeKind, UserDefinitionKind,
};
use std::any::TypeId;

fn registry_error(kind: RegistryErrorKind) -> SchemaError {
    SchemaError::Registry {
        kind,
        location: ErrorLocation::default(),
    }
}

impl SchemaRegistryBuilderInner {
    pub(super) fn default() -> SchemaRegistryBuilder {
        SchemaRegistryBuilder::new()
    }
}

impl SchemaRegistryBuilderInner {
    pub(super) fn new() -> SchemaRegistryBuilder {
        SchemaRegistryBuilder {
            _data: RegistrationData {
                _entries: Vec::new(),
                _entry_by_rust_type: HashMap::new(),
                _entry_by_identity: HashMap::new(),
            },
        }
    }

    pub(super) fn register_by_id<T>(
        _this: &mut SchemaRegistryBuilder,
        _id: u32,
    ) -> Result<(), SchemaError>
    where
        T: DescribeSchema + fory::StructSerializer<Target = T>,
    {
        Self::register::<T>(_this, UserIdentity::Numeric(_id))
    }

    pub(super) fn register_by_name<T>(
        _this: &mut SchemaRegistryBuilder,
        _namespace: &str,
        _type_name: &str,
    ) -> Result<(), SchemaError>
    where
        T: DescribeSchema + fory::StructSerializer<Target = T>,
    {
        if _type_name.is_empty()
            || _type_name.contains('.')
            || _namespace.len() > 32766
            || _type_name.len() > 32766
        {
            return Err(registry_error(RegistryErrorKind::InvalidIdentity));
        }
        Self::register::<T>(
            _this,
            UserIdentity::Name {
                namespace: _namespace.into(),
                type_name: _type_name.into(),
            },
        )
    }

    pub(super) fn build(_this: SchemaRegistryBuilder) -> Result<SchemaRegistry, SchemaError> {
        Ok(SchemaRegistry { _data: _this._data })
    }

    fn register<T>(
        this: &mut SchemaRegistryBuilder,
        identity: UserIdentity,
    ) -> Result<(), SchemaError>
    where
        T: DescribeSchema + fory::StructSerializer<Target = T>,
    {
        let rust_type = TypeId::of::<T>();
        if let Some(&index) = this._data._entry_by_rust_type.get(&rust_type) {
            return if this._data._entries[index]._identity == identity {
                Ok(())
            } else {
                Err(registry_error(RegistryErrorKind::TypeIdentityConflict))
            };
        }
        if this._data._entry_by_identity.contains_key(&identity) {
            return Err(registry_error(RegistryErrorKind::IdentityTypeConflict));
        }
        match T::static_type_id() {
            fory::TypeId::STRUCT | fory::TypeId::ENUM | fory::TypeId::UNION => {}
            _ => return Err(SchemaError::UnsupportedType(ErrorLocation::default())),
        }
        let index = this._data._entries.len();
        this._data._entry_by_rust_type.insert(rust_type, index);
        this._data
            ._entry_by_identity
            .insert(identity.clone(), index);
        this._data._entries.push(RegistrationEntry {
            _identity: identity,
            _describe_definition: SchemaRegistryInner::describe_registered_definition::<T>,
            _register_fory: SchemaRegistryInner::register_fory_type::<T>,
        });
        Ok(())
    }
}

impl SchemaRegistryInner {
    pub(super) fn identity_of<'this, T: 'static>(
        _this: &'this SchemaRegistry,
    ) -> Result<&'this UserIdentity, SchemaError> {
        let index = _this
            ._data
            ._entry_by_rust_type
            .get(&TypeId::of::<T>())
            .ok_or_else(|| registry_error(RegistryErrorKind::UnregisteredType))?;
        Ok(&_this._data._entries[*index]._identity)
    }

    pub(super) fn describe<T: DescribeSchema + 'static>(
        this: &SchemaRegistry,
        encoding: &RootEncoding,
    ) -> Result<SchemaDescriptor, SchemaError> {
        collect_schema::<T>(encoding, this)
    }

    pub(super) fn apply_to_fory(
        _this: &SchemaRegistry,
        _fory: &mut fory::Fory,
    ) -> Result<(), SchemaError> {
        for entry in &_this._data._entries {
            (entry._register_fory)(_fory, &entry._identity)?;
        }
        Ok(())
    }
}

impl SchemaRegistryInner {
    /// Checks one registered definition without expanding dependencies.
    /// Query the registered identity, call the trait once, and propagate its errors.
    /// Reject None, identity mismatch, and STRUCT/ENUM/UNION definition mismatch.
    /// Other serializer categories are unsupported. Success preserves the original Some.
    /// This does not replace schema compilation or prove arbitrary serializer bytes.
    pub(super) fn describe_registered_definition<T>(
        _encoding: &RootEncoding,
        _registry: &SchemaRegistry,
    ) -> Result<Option<UserDefinition>, SchemaError>
    where
        T: DescribeSchema + fory::StructSerializer<Target = T>,
    {
        let identity = _registry.identity_of::<T>()?;
        let definition = T::describe_definition(_encoding, _registry)?
            .ok_or_else(|| registry_error(RegistryErrorKind::MissingDefinition))?;
        if &definition.identity != identity {
            return Err(registry_error(
                RegistryErrorKind::DefinitionIdentityMismatch,
            ));
        }
        let matches = match T::static_type_id() {
            fory::TypeId::STRUCT => matches!(definition.kind, UserDefinitionKind::Struct { .. }),
            fory::TypeId::ENUM => matches!(definition.kind, UserDefinitionKind::Enum { .. }),
            fory::TypeId::UNION => matches!(definition.kind, UserDefinitionKind::Union { .. }),
            _ => return Err(SchemaError::UnsupportedType(ErrorLocation::default())),
        };
        if !matches {
            return Err(registry_error(
                RegistryErrorKind::DefinitionCategoryMismatch,
            ));
        }
        Ok(Some(definition))
    }
}

impl SchemaRegistryInner {
    /// Fixed Fory 1.7.3 adapter boundary; dispatch uses the static category.
    /// Use Serializer::static_type_id (doc-hidden): STRUCT/ENUM use ordinary
    /// registration, UNION uses union registration, and other categories fail.
    /// Never retry another category after failure or collect schema to dispatch.
    pub(super) fn register_fory_type<T>(
        _fory: &mut fory::Fory,
        _identity: &UserIdentity,
    ) -> Result<(), SchemaError>
    where
        T: fory::StructSerializer<Target = T>,
    {
        let union = match T::static_type_id() {
            fory::TypeId::STRUCT | fory::TypeId::ENUM => false,
            fory::TypeId::UNION => true,
            _ => return Err(SchemaError::UnsupportedType(ErrorLocation::default())),
        };
        let result = match _identity {
            UserIdentity::Numeric(id) => {
                if union {
                    _fory.register_union::<T>(*id)
                } else {
                    _fory.register::<T>(*id)
                }
            }
            UserIdentity::Name {
                namespace,
                type_name,
            } => {
                let name = if namespace.is_empty() {
                    type_name.clone()
                } else {
                    format!("{namespace}.{type_name}")
                };
                if union {
                    _fory.register_union_by_name::<T>(&name)
                } else {
                    _fory.register_by_name::<T>(&name)
                }
            }
        };
        result.map_err(|source| SchemaError::ForyRegistration {
            source,
            location: ErrorLocation::default(),
        })
    }
    pub(super) fn collect_schema<T: DescribeSchema + 'static>(
        _encoding: &RootEncoding,
        _registry: &SchemaRegistry,
    ) -> Result<SchemaDescriptor, SchemaError> {
        let root = T::describe_type(_encoding, _registry).map_err(|error| with_path(error, &[]))?;
        let mut collection = DescriptionCollection {
            _registry,
            _encoding,
            _root: root,
            _pending: Default::default(),
            _discovered: Default::default(),
            _definitions: Vec::new(),
        };
        discover_type(
            _registry,
            &collection._root,
            Vec::new(),
            &mut collection._discovered,
            &mut collection._pending,
        )?;
        while let Some(pending) = collection._pending.pop_front() {
            let entry = &collection._registry._data._entries[pending._entry_index];
            let definition =
                (entry._describe_definition)(collection._encoding, collection._registry)
                    .map_err(|error| with_path(error, &pending._path))?
                    .ok_or_else(|| {
                        with_path(
                            registry_error(RegistryErrorKind::MissingDefinition),
                            &pending._path,
                        )
                    })?;
            match &definition.kind {
                UserDefinitionKind::Struct { fields, .. } => {
                    for field in fields {
                        let mut path = pending._path.clone();
                        path.push(PathSegment::Field(field.name.clone()));
                        discover_type(
                            collection._registry,
                            &field.ty,
                            path,
                            &mut collection._discovered,
                            &mut collection._pending,
                        )?;
                    }
                }
                UserDefinitionKind::Enum { variants } | UserDefinitionKind::Union { variants } => {
                    for variant in variants {
                        if let Some(payload) = &variant.payload {
                            let mut path = pending._path.clone();
                            path.push(PathSegment::Variant(variant.name.clone()));
                            discover_type(
                                collection._registry,
                                payload,
                                path,
                                &mut collection._discovered,
                                &mut collection._pending,
                            )?;
                        }
                    }
                }
            }
            collection._definitions.push(definition);
        }
        Ok(SchemaDescriptor {
            encoding: *collection._encoding,
            key_semantics_version: crate::KEY_SEMANTICS_VERSION,
            root: collection._root,
            definitions: collection._definitions,
        })
    }
}

fn discover_type(
    registry: &SchemaRegistry,
    root: &TypeDescriptor,
    path: Vec<PathSegment>,
    discovered: &mut std::collections::HashSet<usize>,
    pending: &mut std::collections::VecDeque<PendingDefinition>,
) -> Result<(), SchemaError> {
    // Reverse-push children so an iterative walk preserves logical key/value order.
    let mut stack = vec![(root, path)];
    while let Some((ty, mut path)) = stack.pop() {
        match &ty.kind {
            TypeKind::Scalar(_) => {}
            TypeKind::List(element) | TypeKind::Array { element, .. } | TypeKind::Set(element) => {
                path.push(PathSegment::ElementType);
                stack.push((element, path));
            }
            TypeKind::Map { key, value } => {
                let mut value_path = path.clone();
                value_path.push(PathSegment::MapValueType);
                stack.push((value, value_path));
                path.push(PathSegment::MapKeyType);
                stack.push((key, path));
            }
            TypeKind::User(identity) => {
                let index = registry
                    ._data
                    ._entry_by_identity
                    .get(identity)
                    .ok_or_else(|| {
                        with_path(
                            registry_error(RegistryErrorKind::UnregisteredDependency),
                            &path,
                        )
                    })?;
                if discovered.insert(*index) {
                    pending.push_back(PendingDefinition {
                        _entry_index: *index,
                        _path: path,
                    });
                }
            }
        }
    }
    Ok(())
}

fn with_path(mut error: SchemaError, path: &[PathSegment]) -> SchemaError {
    let location = match &mut error {
        SchemaError::NotImplemented(_) => return error,
        SchemaError::Registry { location, .. }
        | SchemaError::ForyRegistration { location, .. }
        | SchemaError::InvalidSchema(location)
        | SchemaError::UnsupportedVersion(location)
        | SchemaError::UnsupportedType(location)
        | SchemaError::MalformedInput(location)
        | SchemaError::SchemaMismatch(location)
        | SchemaError::UnsupportedKeyCapability(location)
        | SchemaError::AccessError(location)
        | SchemaError::ResourceLimit(location) => location,
    };
    if location.path.is_none() {
        location.path = Some(path.to_vec());
    }
    error
}

impl DescribeSchemaInner {
    pub(super) fn describe_schema<T: DescribeSchema + 'static>(
        encoding: &RootEncoding,
        registry: &SchemaRegistry,
    ) -> Result<SchemaDescriptor, SchemaError> {
        registry.describe::<T>(encoding)
    }
}
