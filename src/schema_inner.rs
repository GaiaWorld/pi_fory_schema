use super::{
    DataError, InputFormat, KeyRequirement, OwnedValidatedValue, OwnedValueView, ParseLimits,
    Schema, SchemaDescriptor, SchemaError, SchemaInner, SchemaLimits, ValidatedValue, ValueView,
};
use super::{SchemaData, StructAccess, TypeAccess, VariantAccess};
use crate::{
    ErrorLocation, IntegerEncoding, ScalarKind, TypeDescriptor, TypeKind, UserDefinitionKind,
    UserIdentity,
};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

fn location(index: Option<usize>) -> ErrorLocation {
    ErrorLocation {
        descriptor_index: index,
        ..ErrorLocation::default()
    }
}

fn charge(
    counter: &mut usize,
    amount: usize,
    limit: usize,
    index: Option<usize>,
) -> Result<(), SchemaError> {
    *counter = counter
        .checked_add(amount)
        .filter(|total| *total <= limit)
        .ok_or_else(|| SchemaError::ResourceLimit(location(index)))?;
    Ok(())
}

fn charge_identity(
    identity: &UserIdentity,
    counter: &mut usize,
    limits: &SchemaLimits,
    index: Option<usize>,
) -> Result<(), SchemaError> {
    if let UserIdentity::Name {
        namespace,
        type_name,
    } = identity
    {
        charge(counter, namespace.len(), limits.max_metadata_bytes, index)?;
        charge(counter, type_name.len(), limits.max_metadata_bytes, index)?;
    }
    Ok(())
}

fn validate_identity(identity: &UserIdentity, index: Option<usize>) -> Result<(), SchemaError> {
    if let UserIdentity::Name {
        namespace,
        type_name,
    } = identity
    {
        if type_name.is_empty()
            || type_name.contains('.')
            || namespace.len() > 32766
            || type_name.len() > 32766
        {
            return Err(SchemaError::InvalidSchema(location(index)));
        }
    }
    Ok(())
}

fn push_children<'a>(
    ty: &'a TypeDescriptor,
    index: Option<usize>,
    stack: &mut Vec<(&'a TypeDescriptor, Option<usize>)>,
) {
    match &ty.kind {
        TypeKind::List(element) | TypeKind::Array { element, .. } | TypeKind::Set(element) => {
            stack.push((element, index))
        }
        TypeKind::Map { key, value } => {
            stack.push((value, index));
            stack.push((key, index));
        }
        TypeKind::Scalar(_) | TypeKind::User(_) => {}
    }
}

fn push_definition<'a>(
    kind: &'a UserDefinitionKind,
    index: Option<usize>,
    stack: &mut Vec<(&'a TypeDescriptor, Option<usize>)>,
) {
    match kind {
        UserDefinitionKind::Struct { fields, .. } => {
            for field in fields.iter().rev() {
                stack.push((&field.ty, index));
            }
        }
        UserDefinitionKind::Enum { variants } | UserDefinitionKind::Union { variants } => {
            for variant in variants.iter().rev() {
                if let Some(payload) = &variant.payload {
                    stack.push((payload, index));
                }
            }
        }
    }
}

#[cfg(test)]
#[test]
fn checked_budget_overflow_is_resource_limit_even_at_maximum_bound() {
    let mut counter = usize::MAX;
    assert!(matches!(
        charge(&mut counter, 1, usize::MAX, None),
        Err(SchemaError::ResourceLimit(_))
    ));
    assert_eq!(counter, usize::MAX);
}

#[cfg(test)]
#[test]
fn compiled_indexes_preserve_logical_positions_and_declared_encoding() {
    let encoding = crate::RootEncoding {
        protocol_version: crate::ProtocolVersion::Fory1_7,
        mode: crate::SchemaMode::SameSchema { version_hash: true },
        root_reference: crate::RootReferenceMode::Tracking,
    };
    let scalar = || TypeDescriptor {
        nullable: false,
        track_ref: false,
        integer_encoding: None,
        kind: TypeKind::Scalar(ScalarKind::Bool),
    };
    let descriptor = SchemaDescriptor {
        encoding,
        key_semantics_version: crate::KEY_SEMANTICS_VERSION,
        root: TypeDescriptor {
            kind: TypeKind::User(UserIdentity::Numeric(1)),
            ..scalar()
        },
        definitions: vec![
            crate::UserDefinition {
                identity: UserIdentity::Numeric(1),
                kind: UserDefinitionKind::Struct {
                    fields: vec![
                        crate::FieldDescriptor {
                            id: None,
                            name: "z".into(),
                            ty: scalar(),
                        },
                        crate::FieldDescriptor {
                            id: Some(9),
                            name: "a".into(),
                            ty: TypeDescriptor {
                                kind: TypeKind::User(UserIdentity::Numeric(2)),
                                ..scalar()
                            },
                        },
                    ],
                    physical_field_order: vec![1, 0],
                },
            },
            crate::UserDefinition {
                identity: UserIdentity::Numeric(2),
                kind: UserDefinitionKind::Enum {
                    variants: vec![
                        crate::VariantDescriptor {
                            id: 9,
                            name: "z".into(),
                            payload: None,
                        },
                        crate::VariantDescriptor {
                            id: 2,
                            name: "a".into(),
                            payload: None,
                        },
                    ],
                },
            },
        ],
    };
    let schema = SchemaInner::compile(descriptor, &SchemaLimits::default()).unwrap();
    let access = &schema._data._type_access;
    assert_eq!(access._definition_by_identity[&UserIdentity::Numeric(1)], 0);
    assert_eq!(access._definition_by_identity[&UserIdentity::Numeric(2)], 1);
    assert_eq!(access._struct_access[&0]._physical_field_order, vec![1, 0]);
    assert_eq!(access._struct_access[&0]._field_by_name["z"], 0);
    assert_eq!(access._struct_access[&0]._field_by_id[&9], 1);
    assert_eq!(access._struct_access[&0]._field_by_id.len(), 1);
    assert_eq!(access._variant_access[&1]._variant_by_id[&9], 0);
    assert_eq!(access._variant_access[&1]._variant_by_id[&2], 1);
    assert!(matches!(
        schema._data._descriptor.encoding.protocol_version,
        crate::ProtocolVersion::Fory1_7
    ));
    assert!(matches!(
        schema._data._descriptor.encoding.mode,
        crate::SchemaMode::SameSchema { version_hash: true }
    ));
    assert!(matches!(
        schema._data._descriptor.encoding.root_reference,
        crate::RootReferenceMode::Tracking
    ));
}

impl SchemaInner {
    pub(super) fn compile(
        descriptor: SchemaDescriptor,
        limits: &SchemaLimits,
    ) -> Result<Schema, SchemaError> {
        // Count the entire declaration before rejecting duplicate or unreachable entries.
        let mut definitions = 0;
        let mut members = 0;
        let mut nodes = 0;
        let mut metadata = 0;
        let mut stack = vec![(&descriptor.root, None)];
        for (index, definition) in descriptor.definitions.iter().enumerate() {
            charge(&mut definitions, 1, limits.max_definitions, Some(index))?;
            charge_identity(&definition.identity, &mut metadata, limits, Some(index))?;
            match &definition.kind {
                UserDefinitionKind::Struct { fields, .. } => {
                    for field in fields {
                        charge(&mut members, 1, limits.max_members, Some(index))?;
                        charge(
                            &mut metadata,
                            field.name.len(),
                            limits.max_metadata_bytes,
                            Some(index),
                        )?;
                        stack.push((&field.ty, Some(index)));
                    }
                }
                UserDefinitionKind::Enum { variants } | UserDefinitionKind::Union { variants } => {
                    for variant in variants {
                        charge(&mut members, 1, limits.max_members, Some(index))?;
                        charge(
                            &mut metadata,
                            variant.name.len(),
                            limits.max_metadata_bytes,
                            Some(index),
                        )?;
                        if let Some(payload) = &variant.payload {
                            stack.push((payload, Some(index)));
                        }
                    }
                }
            }
        }
        while let Some((ty, index)) = stack.pop() {
            charge(&mut nodes, 1, limits.max_type_nodes, index)?;
            if let TypeKind::User(identity) = &ty.kind {
                charge_identity(identity, &mut metadata, limits, index)?;
            }
            push_children(ty, index, &mut stack);
        }
        if descriptor.key_semantics_version != crate::KEY_SEMANTICS_VERSION {
            return Err(SchemaError::UnsupportedVersion(location(None)));
        }
        let mut access = TypeAccess {
            _definition_by_identity: HashMap::new(),
            _struct_access: HashMap::new(),
            _variant_access: HashMap::new(),
        };
        for (index, definition) in descriptor.definitions.iter().enumerate() {
            validate_identity(&definition.identity, Some(index))?;
            if access
                ._definition_by_identity
                .insert(definition.identity.clone(), index)
                .is_some()
            {
                return Err(SchemaError::InvalidSchema(location(Some(index))));
            }
            match &definition.kind {
                UserDefinitionKind::Struct {
                    fields,
                    physical_field_order,
                } => {
                    let mut seen = vec![false; fields.len()];
                    if physical_field_order.len() != fields.len() {
                        return Err(SchemaError::InvalidSchema(location(Some(index))));
                    }
                    for &position in physical_field_order {
                        let Some(slot) = seen.get_mut(position) else {
                            return Err(SchemaError::InvalidSchema(location(Some(index))));
                        };
                        if std::mem::replace(slot, true) {
                            return Err(SchemaError::InvalidSchema(location(Some(index))));
                        }
                    }
                    let mut field_access = StructAccess {
                        _physical_field_order: physical_field_order.clone(),
                        _field_by_id: HashMap::new(),
                        _field_by_name: HashMap::new(),
                    };
                    for (position, field) in fields.iter().enumerate() {
                        if field.name.is_empty()
                            || field_access
                                ._field_by_name
                                .insert(field.name.clone(), position)
                                .is_some()
                        {
                            return Err(SchemaError::InvalidSchema(location(Some(index))));
                        }
                        if let Some(id) = field.id {
                            if field_access._field_by_id.insert(id, position).is_some() {
                                return Err(SchemaError::InvalidSchema(location(Some(index))));
                            }
                        }
                    }
                    access._struct_access.insert(index, field_access);
                }
                UserDefinitionKind::Enum { variants } | UserDefinitionKind::Union { variants } => {
                    let mut names = HashSet::new();
                    let mut variant_access = VariantAccess {
                        _variant_by_id: HashMap::new(),
                    };
                    for (position, variant) in variants.iter().enumerate() {
                        if variant.name.is_empty()
                            || !names.insert(&variant.name)
                            || variant_access
                                ._variant_by_id
                                .insert(variant.id, position)
                                .is_some()
                            || (matches!(definition.kind, UserDefinitionKind::Enum { .. })
                                && variant.payload.is_some())
                        {
                            return Err(SchemaError::InvalidSchema(location(Some(index))));
                        }
                    }
                    access._variant_access.insert(index, variant_access);
                }
            }
        }
        // Validate every syntactic node; User references do not expand here.
        stack.push((&descriptor.root, None));
        for (index, definition) in descriptor.definitions.iter().enumerate() {
            push_definition(&definition.kind, Some(index), &mut stack);
        }
        while let Some((ty, index)) = stack.pop() {
            let valid_encoding = match ty.kind {
                TypeKind::Scalar(ScalarKind::I32 | ScalarKind::U32) => matches!(
                    ty.integer_encoding,
                    Some(IntegerEncoding::Fixed | IntegerEncoding::Varint)
                ),
                TypeKind::Scalar(ScalarKind::I64 | ScalarKind::U64) => {
                    ty.integer_encoding.is_some()
                }
                _ => ty.integer_encoding.is_none(),
            };
            if !valid_encoding {
                return Err(SchemaError::InvalidSchema(location(index)));
            }
            if let TypeKind::User(identity) = &ty.kind {
                validate_identity(identity, index)?;
                if !access._definition_by_identity.contains_key(identity) {
                    return Err(SchemaError::InvalidSchema(location(index)));
                }
            }
            push_children(ty, index, &mut stack);
        }
        let mut reachable = HashSet::new();
        stack.push((&descriptor.root, None));
        while let Some((ty, index)) = stack.pop() {
            if let TypeKind::User(identity) = &ty.kind {
                let target = access._definition_by_identity[identity];
                if reachable.insert(target) {
                    push_definition(
                        &descriptor.definitions[target].kind,
                        Some(target),
                        &mut stack,
                    );
                }
            }
            push_children(ty, index, &mut stack);
        }
        if let Some(index) =
            (0..descriptor.definitions.len()).find(|index| !reachable.contains(index))
        {
            return Err(SchemaError::InvalidSchema(location(Some(index))));
        }
        Ok(Schema {
            _data: Arc::new(SchemaData {
                _descriptor: descriptor,
                _type_access: access,
            }),
        })
    }

    pub(super) fn check_key_support(
        this: &Schema,
        _requirement: KeyRequirement,
    ) -> Result<(), SchemaError> {
        let descriptor = &this._data._descriptor;
        let mut visited = HashSet::new();
        let mut stack = vec![(&descriptor.root, None)];
        // All three requirements have the same confirmed support set.
        while let Some((ty, index)) = stack.pop() {
            if ty.track_ref
                || matches!(
                    ty.kind,
                    TypeKind::Map { .. }
                        | TypeKind::Set(_)
                        | TypeKind::Scalar(
                            ScalarKind::Date | ScalarKind::Timestamp | ScalarKind::Duration
                        )
                )
            {
                return Err(SchemaError::UnsupportedKeyCapability(location(index)));
            }
            if let TypeKind::User(identity) = &ty.kind {
                let target = this._data._type_access._definition_by_identity[identity];
                if visited.insert(target) {
                    push_definition(
                        &descriptor.definitions[target].kind,
                        Some(target),
                        &mut stack,
                    );
                }
            }
            push_children(ty, index, &mut stack);
        }
        Ok(())
    }

    pub(super) fn view<'a>(
        _this: &'a Schema,
        _input: &'a [u8],
        _format: InputFormat,
        _limits: &ParseLimits,
    ) -> Result<ValueView<'a>, DataError> {
        Ok(ValueView {
            _context: crate::value::RootState::open(_this, _input, _format, _limits)?,
            _backing: crate::value::RootBacking::Borrowed {
                _schema: _this,
                _input,
            },
        })
    }

    pub(super) fn validate<'a>(
        _this: &'a Schema,
        _input: &'a [u8],
        _format: InputFormat,
        _limits: &ParseLimits,
    ) -> Result<ValidatedValue<'a>, DataError> {
        Self::view(_this, _input, _format, _limits)?.validate()
    }

    pub(super) fn view_owned(
        _this: &Arc<Schema>,
        _input: bytes::Bytes,
        _format: InputFormat,
        _limits: &ParseLimits,
    ) -> Result<OwnedValueView, DataError> {
        Ok(OwnedValueView {
            _context: crate::value::RootState::open(_this, &_input, _format, _limits)?,
            _backing: crate::value::RetainedRoot {
                _schema: Arc::clone(_this),
                _input,
            },
        })
    }

    pub(super) fn validate_owned(
        _this: &Arc<Schema>,
        _input: bytes::Bytes,
        _format: InputFormat,
        _limits: &ParseLimits,
    ) -> Result<OwnedValidatedValue, DataError> {
        Self::view_owned(_this, _input, _format, _limits)?.validate()
    }

    pub(super) fn view_bytes(
        this: &Arc<Schema>,
        input: bytes::Bytes,
        format: InputFormat,
        limits: &ParseLimits,
    ) -> Result<OwnedValueView, DataError> {
        this.view_owned(input, format, limits)
    }

    pub(super) fn validate_bytes(
        this: &Arc<Schema>,
        input: bytes::Bytes,
        format: InputFormat,
        limits: &ParseLimits,
    ) -> Result<OwnedValidatedValue, DataError> {
        this.validate_owned(input, format, limits)
    }
}
