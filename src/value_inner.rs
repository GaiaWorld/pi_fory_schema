use super::{
    BinaryView, BinaryViewInner, DataError, KeyError, KeyRequirement, NodeEntries, NodeFields,
    NodeIter, NodeView, NodeViewInner, ObjectId, OwnedNodeView, OwnedNodeViewInner,
    OwnedValidatedKey, OwnedValidatedNodeView, OwnedValidatedNodeViewInner, OwnedValidatedValue,
    OwnedValidatedValueInner, OwnedValueView, OwnedValueViewInner, RootBacking, Schema, StringView,
    StringViewInner, TypeDescriptor, ValidatedKey, ValidatedNodeView, ValidatedNodeViewInner,
    ValidatedValue, ValidatedValueInner, ValueView, ValueViewInner, VariantDescriptor,
};
use super::{
    NodeLocation, NodeTypeSource, ObjectIdInner, RetainedRoot, RootProgress, RootState,
    ValidatedState,
};
use crate::{
    ErrorLocation, InputFormat, IntegerEncoding, ParseLimits, RootReferenceMode, ScalarKind,
    TypeKind,
};
use fory::Reader;
use std::cell::{OnceCell, RefCell};
use std::{hash::Hash, rc::Rc};

fn data_location(offset: usize) -> ErrorLocation {
    ErrorLocation {
        byte_offset: Some(offset),
        path: Some(vec![]),
        ..ErrorLocation::default()
    }
}

#[cfg(test)]
#[test]
fn key_operations_do_not_borrow_progress_or_replace_published_state() {
    let schema = std::sync::Arc::new(
        Schema::compile(
            crate::SchemaDescriptor {
                encoding: crate::RootEncoding {
                    protocol_version: crate::ProtocolVersion::Fory1_7,
                    mode: crate::SchemaMode::Compatible,
                    root_reference: RootReferenceMode::NullOnly,
                },
                key_semantics_version: crate::KEY_SEMANTICS_VERSION,
                root: TypeDescriptor {
                    kind: TypeKind::List(Box::new(TypeDescriptor {
                        kind: TypeKind::Scalar(ScalarKind::String),
                        nullable: true,
                        track_ref: false,
                        integer_encoding: None,
                    })),
                    nullable: false,
                    track_ref: false,
                    integer_encoding: None,
                },
                definitions: vec![],
            },
            &crate::SchemaLimits::default(),
        )
        .unwrap(),
    );
    let input = fory::Fory::builder()
        .xlang(true)
        .build()
        .serialize(&vec![None, Some("é中".to_owned()), Some(String::new())])
        .unwrap();
    let borrowed = schema
        .validate(&input, InputFormat::XlangMessage, &ParseLimits::default())
        .unwrap();
    let owned = schema
        .validate_owned(
            bytes::Bytes::from(input.clone()),
            InputFormat::XlangMessage,
            &ParseLimits::default(),
        )
        .unwrap();
    let ordered = schema
        .validate(&input, InputFormat::XlangMessage, &ParseLimits::default())
        .unwrap()
        .into_key(KeyRequirement::Ordered)
        .unwrap();
    let mut other_descriptor = schema._data._descriptor.clone();
    other_descriptor.root.kind = TypeKind::Scalar(ScalarKind::Bool);
    let other_schema = Schema::compile(other_descriptor, &crate::SchemaLimits::default()).unwrap();
    let other_input = fory::Fory::builder()
        .xlang(true)
        .build()
        .serialize(&true)
        .unwrap();
    let other = other_schema
        .validate(
            &other_input,
            InputFormat::XlangMessage,
            &ParseLimits::default(),
        )
        .unwrap()
        .into_key(KeyRequirement::HashAndOrdered)
        .unwrap();
    let ordered_context = Rc::clone(&ordered._value._context);
    let other_context = Rc::clone(&other._value._context);
    let _ordered_guard = ordered_context._progress.try_borrow_mut().unwrap();
    let _other_guard = other_context._progress.try_borrow_mut().unwrap();
    let left_context = Rc::clone(&borrowed._context);
    let right_context = Rc::clone(&owned._context);
    let left_state = Rc::clone(&borrowed._validated);
    let right_state = Rc::clone(&owned._validated);
    let left_guard = left_context._progress.try_borrow_mut().unwrap();
    let right_guard = right_context._progress.try_borrow_mut().unwrap();
    assert!(matches!(*left_guard, RootProgress::Published));
    assert!(matches!(*right_guard, RootProgress::Published));
    let borrowed = borrowed.into_key(KeyRequirement::HashAndOrdered).unwrap();
    let owned = owned.into_key(KeyRequirement::HashAndOrdered).unwrap();
    let expected = borrowed.stable_hash().unwrap();
    for _ in 0..3 {
        assert!(borrowed.try_eq(&owned).unwrap());
        assert!(owned.try_eq(&borrowed).unwrap());
        assert_eq!(borrowed.try_cmp(&owned).unwrap(), std::cmp::Ordering::Equal);
        assert_eq!(owned.try_cmp(&borrowed).unwrap(), std::cmp::Ordering::Equal);
        assert_eq!(owned.stable_hash().unwrap(), expected);
        assert!(matches!(
            ordered.stable_hash(),
            Err(KeyError::UnsupportedKeyCapability(_))
        ));
        assert!(matches!(
            borrowed.try_eq(&other),
            Err(KeyError::SchemaMismatch(_))
        ));
        assert!(matches!(
            owned.try_cmp(&other),
            Err(KeyError::SchemaMismatch(_))
        ));
        assert_eq!(borrowed.stable_hash().unwrap(), expected);
    }
    assert!(Rc::ptr_eq(
        left_context._validated.get().unwrap(),
        &left_state
    ));
    assert!(Rc::ptr_eq(
        right_context._validated.get().unwrap(),
        &right_state
    ));
    drop(left_guard);
    drop(right_guard);
    assert!(matches!(
        *left_context._progress.borrow(),
        RootProgress::Published
    ));
    assert!(matches!(
        *right_context._progress.borrow(),
        RootProgress::Published
    ));
    assert_eq!(owned.stable_hash().unwrap(), expected);
}

fn copy_location(location: &NodeLocation) -> NodeLocation {
    NodeLocation {
        _byte_offset: location._byte_offset,
        _parent_null: location._parent_null,
        _wire_type_id: location._wire_type_id,
        _reference_mode: location._reference_mode,
        _type_source: match location._type_source {
            NodeTypeSource::Schema => NodeTypeSource::Schema,
            NodeTypeSource::Envelope => NodeTypeSource::Envelope,
            NodeTypeSource::Inherited { _metadata_slot } => {
                NodeTypeSource::Inherited { _metadata_slot }
            }
        },
    }
}

#[cfg(test)]
#[test]
fn key_bridge_children_outlive_parent_without_copy_or_progress_borrow() {
    let schema = Schema::compile(
        crate::SchemaDescriptor {
            encoding: crate::RootEncoding {
                protocol_version: crate::ProtocolVersion::Fory1_7,
                mode: crate::SchemaMode::Compatible,
                root_reference: RootReferenceMode::NullOnly,
            },
            key_semantics_version: crate::KEY_SEMANTICS_VERSION,
            root: TypeDescriptor {
                kind: TypeKind::List(Box::new(TypeDescriptor {
                    kind: TypeKind::Scalar(ScalarKind::I64),
                    nullable: false,
                    track_ref: false,
                    integer_encoding: Some(IntegerEncoding::Tagged),
                })),
                nullable: false,
                track_ref: false,
                integer_encoding: None,
            },
            definitions: vec![],
        },
        &crate::SchemaLimits::default(),
    )
    .unwrap();
    let input = fory::Fory::builder()
        .xlang(true)
        .build()
        .serialize(&vec![i64::MIN, i64::MAX])
        .unwrap();
    let view = schema
        .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
        .unwrap();
    let context = Rc::clone(&view._context);
    let validated = view.validate().unwrap();
    let guard = context._progress.try_borrow_mut().unwrap();
    let root =
        ValidatedState::key_node(&validated._validated, &schema, &input, &context, None).unwrap();
    let child = root.element_at(1).unwrap();
    let location = Rc::clone(&child._location);
    let independent = ValidatedState::key_node(
        &validated._validated,
        &schema,
        &input,
        &context,
        Some(&child),
    )
    .unwrap();
    assert!(Rc::ptr_eq(&independent._location, &location));
    assert!(std::ptr::eq(independent._backing.input(), input.as_slice()));
    drop(child);
    drop(root);
    assert_eq!(independent.read_i64().unwrap(), i64::MAX);
    let copy = input.clone();
    assert!(matches!(
        ValidatedState::key_node(
            &validated._validated,
            &schema,
            &copy,
            &context,
            Some(&independent)
        ),
        Err(DataError::AccessError(_))
    ));
    let other = schema
        .validate(&input, InputFormat::XlangMessage, &ParseLimits::default())
        .unwrap();
    assert!(matches!(
        ValidatedState::key_node(
            &other._validated,
            &schema,
            &input,
            &other._context,
            Some(&independent)
        ),
        Err(DataError::AccessError(_))
    ));
    assert_eq!(independent.read_i64().unwrap(), i64::MAX);
    drop(guard);
}

#[cfg(test)]
#[test]
fn parent_null_has_no_body_and_preserves_schema_nullability() {
    let mut ty = TypeDescriptor {
        nullable: true,
        track_ref: false,
        integer_encoding: None,
        kind: TypeKind::Scalar(ScalarKind::String),
    };
    let location = NodeLocation {
        _byte_offset: 0,
        _parent_null: true,
        _wire_type_id: None,
        _reference_mode: None,
        _type_source: NodeTypeSource::Schema,
    };
    for validated in [false, true] {
        assert!(matches!(
            content_body(&[], &location, &ty, ScalarKind::String, validated),
            Err(DataError::AccessError(_))
        ));
    }
    assert_eq!(
        node_body(&[], &copy_location(&location), &ty).unwrap(),
        (true, 0)
    );
    ty.nullable = false;
    assert!(matches!(
        node_body(&[], &location, &ty),
        Err(DataError::SchemaMismatch(_))
    ));
    ty.nullable = true;
    let ordinary = NodeLocation {
        _parent_null: false,
        _reference_mode: Some(RootReferenceMode::NullOnly),
        ..copy_location(&location)
    };
    assert_eq!(node_body(&[253], &ordinary, &ty).unwrap(), (true, 1));
    assert_eq!(node_body(&[255], &ordinary, &ty).unwrap(), (false, 1));
}

fn content_range(
    input: &[u8],
    body: usize,
    string: bool,
) -> Result<(u8, std::ops::Range<usize>), DataError> {
    let mut reader = Reader::new(input);
    reader
        .skip(body)
        .map_err(|_| DataError::MalformedInput(data_location(body)))?;
    let header = if string {
        reader.read_var_u36_small()
    } else {
        read_var_u32(&mut reader).map(u64::from)
    }
    .map_err(|_| DataError::MalformedInput(data_location(body)))?;
    let tag = if string { (header & 3) as u8 } else { 0 };
    if tag == 3 {
        return Err(DataError::MalformedInput(data_location(body)));
    }
    let len = usize::try_from(if string { header >> 2 } else { header })
        .map_err(|_| DataError::MalformedInput(data_location(body)))?;
    let start = reader.get_cursor();
    let end = start
        .checked_add(len)
        .filter(|end| *end <= input.len())
        .ok_or_else(|| DataError::MalformedInput(data_location(body)))?;
    Ok((tag, start..end))
}

fn utf8_content(bytes: &[u8], offset: usize) -> Result<&str, DataError> {
    std::str::from_utf8(bytes)
        .map_err(|error| DataError::MalformedInput(data_location(offset + error.valid_up_to())))
}

fn walk_utf16(bytes: &[u8], offset: usize, mut emit: impl FnMut(char)) -> Result<(), DataError> {
    let mut units = bytes.chunks_exact(2).enumerate();
    while let Some((index, pair)) = units.next() {
        let unit = u16::from_le_bytes([pair[0], pair[1]]);
        let position = offset + index * 2;
        let scalar = if (0xd800..=0xdbff).contains(&unit) {
            // A missing/non-low partner is attributed to the initiating high surrogate.
            let (_, pair) = units
                .next()
                .ok_or_else(|| DataError::MalformedInput(data_location(position)))?;
            let low = u16::from_le_bytes([pair[0], pair[1]]);
            if !(0xdc00..=0xdfff).contains(&low) {
                return Err(DataError::MalformedInput(data_location(position)));
            }
            0x10000 + ((u32::from(unit) - 0xd800) << 10) + (u32::from(low) - 0xdc00)
        } else if (0xdc00..=0xdfff).contains(&unit) {
            return Err(DataError::MalformedInput(data_location(position)));
        } else {
            u32::from(unit)
        };
        emit(
            char::from_u32(scalar)
                .ok_or_else(|| DataError::MalformedInput(data_location(position)))?,
        );
    }
    if bytes.len() % 2 != 0 {
        return Err(DataError::MalformedInput(data_location(
            offset + bytes.len() - 1,
        )));
    }
    Ok(())
}

fn validate_string_payload(bytes: &[u8], tag: u8, offset: usize) -> Result<(), DataError> {
    match tag {
        0 => Ok(()),
        2 => utf8_content(bytes, offset).map(|_| ()),
        1 => walk_utf16(bytes, offset, |_| {}),
        _ => Err(DataError::MalformedInput(data_location(offset))),
    }
}

fn decode_string_content(input: &[u8], body: usize) -> Result<StringView<'_>, DataError> {
    let (tag, range) = content_range(input, body, true)?;
    let bytes = &input[range.clone()];
    let content: Box<dyn AsRef<str> + '_> = match tag {
        0 if bytes.is_ascii() => Box::new(utf8_content(bytes, range.start)?),
        0 => Box::new(
            bytes
                .iter()
                .map(|byte| char::from(*byte))
                .collect::<String>(),
        ),
        1 => {
            let mut string = String::new();
            walk_utf16(bytes, range.start, |value| string.push(value))?;
            Box::new(string)
        }
        2 => Box::new(utf8_content(bytes, range.start)?),
        _ => return Err(DataError::MalformedInput(data_location(body))),
    };
    Ok(StringView { content })
}

fn binary_content(input: &[u8], body: usize) -> Result<BinaryView<'_>, DataError> {
    let (_, range) = content_range(input, body, false)?;
    Ok(BinaryView {
        content: Box::new(&input[range]),
    })
}

// 只有 sequence_header 在核实 dense 父类型后，才将父 array ID 留给无 marker 的
// scalar body。普通 group/envelope 仍校验 scalar ID，不能借此放宽字段编码契约。
fn dense_scalar_id(location: &NodeLocation, ty: &TypeDescriptor) -> Option<u32> {
    if !matches!(location._type_source, NodeTypeSource::Schema)
        || location._reference_mode.is_some()
        || ty.nullable
        || ty.track_ref
    {
        return None;
    }
    match (&ty.kind, location._wire_type_id) {
        (TypeKind::Scalar(ScalarKind::I32), Some(46)) => Some(4),
        (TypeKind::Scalar(ScalarKind::I64), Some(47)) => Some(6),
        (TypeKind::Scalar(ScalarKind::U32), Some(50)) => Some(11),
        (TypeKind::Scalar(ScalarKind::U64), Some(51)) => Some(13),
        _ => None,
    }
}

fn integer_read_encoding(location: &NodeLocation, ty: &TypeDescriptor) -> Option<IntegerEncoding> {
    if dense_scalar_id(location, ty).is_some() {
        Some(IntegerEncoding::Fixed)
    } else {
        ty.integer_encoding
    }
}

fn node_body(
    input: &[u8],
    location: &NodeLocation,
    ty: &TypeDescriptor,
) -> Result<(bool, usize), DataError> {
    if let Some(id) = location
        ._wire_type_id
        .filter(|_| dense_scalar_id(location, ty).is_none())
    {
        check_wire_id(ty, id, location._byte_offset)?;
    }
    if location._parent_null {
        return if ty.nullable {
            Ok((true, location._byte_offset))
        } else {
            Err(DataError::SchemaMismatch(data_location(
                location._byte_offset,
            )))
        };
    }
    if matches!(location._type_source, NodeTypeSource::Envelope) {
        if matches!(location._reference_mode, Some(RootReferenceMode::Tracking))
            && input.get(location._byte_offset) == Some(&254)
        {
            return Err(DataError::AccessError(data_location(location._byte_offset)));
        }
        return envelope(
            input,
            location._byte_offset,
            ty,
            location
                ._reference_mode
                .ok_or(DataError::NotImplemented("envelope reference mode"))?,
        );
    }
    let mut reader = Reader::new(input);
    reader
        .skip(location._byte_offset)
        .map_err(|_| DataError::MalformedInput(data_location(location._byte_offset)))?;
    if let Some(mode) = location._reference_mode {
        let flag = reader
            .read_i8()
            .map_err(|_| DataError::MalformedInput(data_location(location._byte_offset)))?;
        match flag {
            -3 if ty.nullable => return Ok((true, reader.get_cursor())),
            -3 => {
                return Err(DataError::SchemaMismatch(data_location(
                    location._byte_offset,
                )));
            }
            -1 => {}
            0 if matches!(mode, RootReferenceMode::Tracking) => {}
            -2 if matches!(mode, RootReferenceMode::Tracking) => {
                return Err(DataError::AccessError(data_location(location._byte_offset)));
            }
            _ => {
                return Err(DataError::MalformedInput(data_location(
                    location._byte_offset,
                )));
            }
        }
    }
    Ok((false, reader.get_cursor()))
}

fn node_is_null(
    input: &[u8],
    location: &NodeLocation,
    ty: &TypeDescriptor,
) -> Result<bool, DataError> {
    if location._parent_null || location._reference_mode.is_none() {
        return node_body(input, location, ty).map(|(null, _)| null);
    }
    let offset = location._byte_offset;
    match input.get(offset) {
        Some(253) if ty.nullable => Ok(true),
        Some(253) => Err(DataError::SchemaMismatch(data_location(offset))),
        Some(255) => Ok(false),
        Some(0 | 254) if matches!(location._reference_mode, Some(RootReferenceMode::Tracking)) => {
            Ok(false)
        }
        _ => Err(DataError::MalformedInput(data_location(offset))),
    }
}

fn named_node_body(
    input: &[u8],
    location: &NodeLocation,
    ty: &TypeDescriptor,
    names: &mut Vec<(usize, String)>,
) -> Result<(bool, usize), DataError> {
    if !matches!(location._type_source, NodeTypeSource::Envelope) {
        return node_body(input, location, ty);
    }
    let (null, body) = node_body(input, location, ty)?;
    if !null && matches!(ty.kind, TypeKind::User(crate::UserIdentity::Name { .. })) {
        let mut reader = Reader::new(input);
        reader
            .skip(location._byte_offset + 1)
            .map_err(|_| DataError::MalformedInput(data_location(location._byte_offset)))?;
        let offset = reader.get_cursor();
        read_var_u32(&mut reader).map_err(|_| DataError::MalformedInput(data_location(offset)))?;
        read_user_identity(&mut reader, ty, offset, names)?;
    }
    Ok((null, body))
}

#[cfg(test)]
#[test]
fn same_schema_name_iteration_replays_no_root_per_item_and_validated_ignores_lock() {
    let user = TypeDescriptor {
        kind: TypeKind::User(crate::UserIdentity::Name {
            namespace: "x".into(),
            type_name: "Y".into(),
        }),
        nullable: false,
        track_ref: false,
        integer_encoding: None,
    };
    let list = |element| TypeDescriptor {
        kind: TypeKind::List(Box::new(element)),
        nullable: false,
        track_ref: false,
        integer_encoding: None,
    };
    let TypeKind::User(identity) = &user.kind else {
        unreachable!()
    };
    let schema = Schema::compile(
        crate::SchemaDescriptor {
            encoding: crate::RootEncoding {
                protocol_version: crate::ProtocolVersion::Fory1_7,
                mode: crate::SchemaMode::SameSchema {
                    version_hash: false,
                },
                root_reference: RootReferenceMode::NullOnly,
            },
            key_semantics_version: crate::KEY_SEMANTICS_VERSION,
            definitions: vec![crate::UserDefinition {
                identity: identity.clone(),
                kind: crate::UserDefinitionKind::Struct {
                    fields: vec![crate::FieldDescriptor {
                        id: None,
                        name: "flag".into(),
                        ty: TypeDescriptor {
                            kind: TypeKind::Scalar(ScalarKind::Bool),
                            nullable: false,
                            track_ref: false,
                            integer_encoding: None,
                        },
                    }],
                    physical_field_order: vec![0],
                },
            }],
            root: list(list(user)),
        },
        &crate::SchemaLimits::default(),
    )
    .unwrap();
    let mut input = vec![255, 22, 128, 8, 8, 22];
    input.extend_from_slice(&[1, 8, 29, 2, 0, b'x', 2, 0, b'Y', 1]);
    for _ in 1..1024 {
        input.extend_from_slice(&[1, 8, 29, 3, 5, 1]);
    }
    let view = schema
        .view(&input, InputFormat::RootValue, &ParseLimits::default())
        .unwrap();
    let context = Rc::clone(&view._context);
    let root = view.root().unwrap();
    SEQUENCE_OPERATIONS.with(|count| count.set((0, 0)));
    let mut count = 0;
    for node in root.elements().unwrap() {
        node.unwrap();
        count += 1;
    }
    assert_eq!(count, 1024);
    assert_eq!(SEQUENCE_OPERATIONS.with(|count| count.get()), (1025, 1024));
    drop(root);
    let validated = view.validate().unwrap();
    let guard = context._progress.try_borrow_mut().unwrap();
    let root = validated.root().unwrap();
    SEQUENCE_OPERATIONS.with(|count| count.set((0, 0)));
    for node in root.elements().unwrap() {
        node.unwrap();
    }
    assert_eq!(SEQUENCE_OPERATIONS.with(|count| count.get()), (1025, 1024));
    let child = root
        .element_at(1023)
        .unwrap()
        .element_at(0)
        .unwrap()
        .to_owned()
        .unwrap();
    assert!(
        child
            .as_view()
            .unwrap()
            .field_at(0)
            .unwrap()
            .read_bool()
            .unwrap()
    );
    drop(guard);
}

fn content_body(
    input: &[u8],
    location: &NodeLocation,
    ty: &TypeDescriptor,
    kind: ScalarKind,
    _validated: bool,
) -> Result<usize, DataError> {
    if !matches!(&ty.kind, TypeKind::Scalar(actual) if std::mem::discriminant(actual) == std::mem::discriminant(&kind))
    {
        return Err(DataError::AccessError(data_location(location._byte_offset)));
    }
    if location._parent_null {
        node_body(input, location, ty)?;
        return Err(DataError::AccessError(data_location(location._byte_offset)));
    }
    let (null, body) = node_body(input, location, ty)?;
    if null {
        return Err(DataError::AccessError(data_location(location._byte_offset)));
    }
    Ok(body)
}

fn read_validated_scalar<T>(
    this: &ValidatedNodeView<'_>,
    kind: ScalarKind,
    read: impl FnOnce(&mut Reader<'_>) -> Result<T, fory::Error>,
) -> Result<T, DataError> {
    if !matches!(&this.node_type.kind, TypeKind::Scalar(actual) if std::mem::discriminant(actual) == std::mem::discriminant(&kind))
    {
        return Err(DataError::AccessError(data_location(
            this._location._byte_offset,
        )));
    }
    if ValidatedNodeViewInner::is_null(this)? {
        return Err(DataError::AccessError(data_location(
            this._location._byte_offset,
        )));
    }
    // The immutable published root already checked these bytes. Decode without RefCell or revalidation.
    let mut reader = Reader::new(this._backing.input());
    let (_, body) = node_body(this._backing.input(), &this._location, this.node_type)?;
    reader
        .skip(body)
        .map_err(|_| DataError::MalformedInput(data_location(body)))?;
    let body = reader.get_cursor();
    read(&mut reader).map_err(|_| DataError::MalformedInput(data_location(body)))
}

// @inline-test-exception: checks private Envelope marker dispatch without any progress borrow.
#[cfg(test)]
#[test]
fn content_envelope_null_and_reference_markers_precede_typeinfo() {
    for kind in [ScalarKind::String, ScalarKind::Binary] {
        let mut ty = TypeDescriptor {
            kind: TypeKind::Scalar(kind),
            nullable: false,
            track_ref: true,
            integer_encoding: None,
        };
        let location = NodeLocation {
            _byte_offset: 0,
            _parent_null: false,
            _wire_type_id: None,
            _reference_mode: Some(RootReferenceMode::Tracking),
            _type_source: NodeTypeSource::Envelope,
        };
        for validated in [false, true] {
            assert!(matches!(
                content_body(&[254, 0, 0], &location, &ty, kind, validated),
                Err(DataError::AccessError(_))
            ));
            assert!(matches!(
                content_body(&[253], &location, &ty, kind, validated),
                Err(DataError::SchemaMismatch(_))
            ));
            ty.nullable = true;
            assert!(matches!(
                content_body(&[253], &location, &ty, kind, validated),
                Err(DataError::AccessError(_))
            ));
            ty.nullable = false;
            assert!(matches!(
                content_body(&[1, 0], &location, &ty, kind, validated),
                Err(DataError::MalformedInput(_))
            ));
        }
    }
    let schema = Schema::compile(
        crate::SchemaDescriptor {
            encoding: crate::RootEncoding {
                protocol_version: crate::ProtocolVersion::Fory1_7,
                mode: crate::SchemaMode::SameSchema {
                    version_hash: false,
                },
                root_reference: RootReferenceMode::Tracking,
            },
            key_semantics_version: crate::KEY_SEMANTICS_VERSION,
            root: TypeDescriptor {
                kind: TypeKind::List(Box::new(TypeDescriptor {
                    kind: TypeKind::Scalar(ScalarKind::String),
                    nullable: false,
                    track_ref: true,
                    integer_encoding: None,
                })),
                nullable: false,
                track_ref: false,
                integer_encoding: None,
            },
            definitions: vec![],
        },
        &crate::SchemaLimits::default(),
    )
    .unwrap();
    let shared = Rc::new(String::from("locked"));
    let input = fory::Fory::builder()
        .xlang(true)
        .build()
        .serialize(&vec![Rc::clone(&shared), shared])
        .unwrap();
    let view = schema
        .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
        .unwrap();
    let context = Rc::clone(&view._context);
    let validated = view.validate().unwrap();
    let _guard = context._progress.try_borrow_mut().unwrap();
    let root = validated.root().unwrap();
    let edge = root.element_at(1).unwrap();
    assert!(matches!(edge.read_string(), Err(DataError::AccessError(_))));
    let id = edge.reference_id().unwrap().unwrap();
    assert_eq!(
        edge.resolve_reference(&id)
            .unwrap()
            .read_string()
            .unwrap()
            .as_str(),
        "locked"
    );
    assert_eq!(
        root.element_at(0).unwrap().read_string().unwrap().as_str(),
        "locked"
    );
}

fn read_var_u32(reader: &mut Reader<'_>) -> Result<u32, fory::Error> {
    let mut value = 0u32;
    for shift in (0..=28).step_by(7) {
        let byte = reader.read_u8()?;
        if shift == 28 && byte > 15 {
            return Err(fory::Error::invalid_data("var-u32 overflow"));
        }
        value |= u32::from(byte & 0x7f) << shift;
        if byte < 128 {
            return Ok(value);
        }
    }
    Err(fory::Error::invalid_data("invalid var-u32"))
}

fn read_var_u64(reader: &mut Reader<'_>) -> Result<u64, fory::Error> {
    let mut value = 0u64;
    for shift in (0..56).step_by(7) {
        let byte = reader.read_u8()?;
        value |= u64::from(byte & 0x7f) << shift;
        if byte < 128 {
            return Ok(value);
        }
    }
    // Fory's ninth byte carries all eight high bits, unlike ten-byte LEB128.
    Ok(value | (u64::from(reader.read_u8()?) << 56))
}

// Fory 1.7.3 serializer/datetime.rs:65-139: standalone roots retain xlang bodies.
fn decode_date(reader: &mut Reader<'_>) -> Result<fory::Date, fory::Error> {
    read_var_u64(reader)
        .map(|value| fory::Date::from_epoch_days(((value >> 1) as i64) ^ -((value & 1) as i64)))
}

fn decode_timestamp(reader: &mut Reader<'_>) -> Result<fory::Timestamp, fory::Error> {
    fory::Timestamp::new(reader.read_i64()?, reader.read_u32()?)
}

fn decode_duration(reader: &mut Reader<'_>) -> Result<fory::Duration, fory::Error> {
    let value = read_var_u64(reader)?;
    fory::Duration::new(
        ((value >> 1) as i64) ^ -((value & 1) as i64),
        reader.read_i32()?,
    )
}

fn sequence_element(ty: &TypeDescriptor) -> Result<&TypeDescriptor, DataError> {
    match &ty.kind {
        TypeKind::List(element) | TypeKind::Array { element, .. } | TypeKind::Set(element) => {
            Ok(element)
        }
        _ => Err(DataError::AccessError(data_location(0))),
    }
}

fn dense_type_id(ty: &TypeDescriptor) -> Result<Option<u32>, DataError> {
    if !matches!(ty.kind, TypeKind::List(_) | TypeKind::Array { .. }) {
        return Ok(None);
    }
    let element = sequence_element(ty)?;
    if element.nullable || element.track_ref {
        return Ok(None);
    }
    let expected = match scalar_id(element)? {
        1 => 43,
        2 => 44,
        3 => 45,
        4 | 5 => 46,
        6 | 7 | 8 => 47,
        9 => 41,
        10 => 49,
        11 | 12 => 50,
        13 | 14 | 15 => 51,
        19 => 55,
        20 => 56,
        _ => 0,
    };
    Ok((expected != 0).then_some(expected))
}

fn dense_sequence_id(ty: &TypeDescriptor, id: u32) -> Result<bool, DataError> {
    Ok(dense_type_id(ty)? == Some(id))
}

#[cfg(test)]
std::thread_local! {
    static SEQUENCE_OPERATIONS: std::cell::Cell<(usize, usize)> = const { std::cell::Cell::new((0, 0)) };
}

fn scan_scalar(
    input: &[u8],
    body: usize,
    ty: &TypeDescriptor,
    location: &NodeLocation,
    validate: bool,
) -> Result<usize, DataError> {
    #[cfg(test)]
    SEQUENCE_OPERATIONS.with(|count| {
        let (headers, scans) = count.get();
        count.set((headers, scans + 1));
    });
    let mut reader = Reader::new(input);
    reader
        .skip(body)
        .map_err(|_| DataError::MalformedInput(data_location(body)))?;
    let result = match dense_scalar_id(location, ty).unwrap_or(scalar_id(ty)?) {
        1 => reader.read_u8().and_then(|value| {
            if !validate || value <= 1 {
                Ok(())
            } else {
                Err(fory::Error::invalid_data("invalid bool"))
            }
        }),
        2 | 9 => reader.skip(1),
        3 | 10 => reader.skip(2),
        4 | 11 | 19 => reader.skip(4),
        6 | 13 | 20 => reader.skip(8),
        5 | 12 => read_var_u32(&mut reader).map(|_| ()),
        7 | 14 => read_var_u64(&mut reader).map(|_| ()),
        8 => reader.read_tagged_i64().map(|_| ()),
        15 => reader.read_tagged_u64().map(|_| ()),
        39 => read_var_u64(&mut reader).map(|_| ()),
        38 if !validate => reader.skip(12),
        37 if !validate => read_var_u64(&mut reader).and_then(|_| reader.skip(4)),
        38 => decode_timestamp(&mut reader).map(|_| ()),
        37 => decode_duration(&mut reader).map(|_| ()),
        id @ (21 | 41) => {
            let (tag, range) = content_range(input, body, id == 21)?;
            if validate && id == 21 {
                validate_string_payload(&input[range.clone()], tag, range.start)?;
            }
            reader.skip(range.end - body)
        }
        _ => return Err(DataError::NotImplemented("sequence element scan")),
    };
    result.map_err(|_| DataError::MalformedInput(data_location(body)))?;
    Ok(reader.get_cursor())
}

fn sequence_header(
    names: &mut Vec<(usize, String)>,
    schema: &Schema,
    metadata: &mut Vec<(std::ops::Range<usize>, usize)>,
    input: &[u8],
    location: &NodeLocation,
    ty: &TypeDescriptor,
) -> Result<
    (
        usize,
        usize,
        Option<RootReferenceMode>,
        Option<usize>,
        NodeLocation,
    ),
    DataError,
> {
    #[cfg(test)]
    SEQUENCE_OPERATIONS.with(|count| {
        let (headers, scans) = count.get();
        count.set((headers + 1, scans));
    });
    let element = sequence_element(ty)?;
    let (null, body) = named_node_body(input, location, ty, names)?;
    if null {
        return Err(DataError::AccessError(data_location(location._byte_offset)));
    }
    let mut reader = Reader::new(input);
    reader
        .skip(body)
        .map_err(|_| DataError::MalformedInput(data_location(body)))?;
    let count = read_var_u32(&mut reader)
        .map_err(|_| DataError::MalformedInput(data_location(body)))? as usize;
    let id = actual_wire_id(input, location, ty)?;
    check_wire_id(ty, id, location._byte_offset)?;
    let dense = id != 22 && id != 23;
    let width = match id {
        41 | 43 | 44 => 1,
        45 | 49 => 2,
        46 | 50 | 55 => 4,
        47 | 51 | 56 => 8,
        _ => 1,
    };
    if dense && !dense_sequence_id(ty, id)? {
        return Err(DataError::SchemaMismatch(data_location(
            location._byte_offset + 1,
        )));
    }
    if dense && count % width != 0 {
        return Err(DataError::MalformedInput(data_location(body)));
    }
    let len = if dense { count / width } else { count };
    if let TypeKind::Array {
        length: Some(expected),
        ..
    } = ty.kind
    {
        if u64::try_from(len).ok() != Some(expected) {
            return Err(DataError::SchemaMismatch(data_location(body)));
        }
    }
    let mut mode = None;
    let mut template = body_location(0, None, None, false);
    if dense && matches!(id, 46 | 47 | 50 | 51) {
        template._wire_type_id = Some(id);
    }
    if !dense && len > 0 {
        let offset = reader.get_cursor();
        let header = reader
            .read_u8()
            .map_err(|_| DataError::MalformedInput(data_location(offset)))?;
        if header & !15 != 0 || header & 8 == 0 {
            return Err(DataError::MalformedInput(data_location(offset)));
        }
        if header & 1 != 0 {
            if !element.track_ref && !matches!(element.kind, TypeKind::User(_)) {
                return Err(DataError::NotImplemented("tracked non-user carrier"));
            }
            mode = Some(RootReferenceMode::Tracking);
        } else if header & 2 != 0 {
            mode = Some(RootReferenceMode::NullOnly);
        }
        if header & 4 == 0 {
            let offset = reader.get_cursor();
            let group = read_var_u32(&mut reader)
                .map_err(|_| DataError::MalformedInput(data_location(offset)))?;
            template = group_type(
                names,
                schema,
                input,
                &mut reader,
                element,
                group,
                offset,
                metadata,
            )?;
        } else if matches!(element.kind, TypeKind::Array { .. }) {
            // DECL_ELEMENT_TYPE 明确选用声明的 ArrayCodec，子 body 不再写 ID。
            template._wire_type_id = dense_type_id(element)?;
        }
    }
    Ok((
        len,
        reader.get_cursor(),
        mode,
        if dense { Some(width) } else { None },
        template,
    ))
}

// Random access scans only the prefix; nested bodies use the explicit-stack scanner.
fn sequence_scan(
    names: &mut Vec<(usize, String)>,
    metadata: &mut Vec<(std::ops::Range<usize>, usize)>,
    schema: &Schema,
    input: &[u8],
    location: &NodeLocation,
    ty: &TypeDescriptor,
    target: Option<usize>,
    complete: bool,
    validate: bool,
) -> Result<(usize, Option<NodeLocation>, usize), DataError> {
    let (len, start, mode, width, template) =
        sequence_header(names, schema, metadata, input, location, ty)?;
    let element = sequence_element(ty)?;
    let mut reader = Reader::new(input);
    reader
        .skip(start)
        .map_err(|_| DataError::MalformedInput(data_location(start)))?;
    if let Some(index) = target {
        if index >= len {
            return Err(DataError::AccessError(data_location(start)));
        }
        if let Some(width) = width {
            let offset = index
                .checked_mul(width)
                .and_then(|offset| start.checked_add(offset))
                .ok_or_else(|| DataError::MalformedInput(data_location(start)))?;
            // Keep target-body reads lazy, but reject a truncated preceding dense prefix.
            if offset > input.len() {
                return Err(DataError::MalformedInput(data_location(start)));
            }
            return Ok((
                len,
                Some(NodeLocation {
                    _byte_offset: offset,
                    _parent_null: false,
                    _reference_mode: mode,
                    ..copy_location(&template)
                }),
                offset,
            ));
        }
    }
    if target.is_none() && !complete {
        return Ok((len, None, reader.get_cursor()));
    }
    let mut objects = objects_for_node(schema, input, location, ty)?;
    for index in 0..len {
        let node = NodeLocation {
            _byte_offset: reader.get_cursor(),
            _parent_null: false,
            _reference_mode: mode,
            ..copy_location(&template)
        };
        if target == Some(index) {
            return Ok((len, Some(node), reader.get_cursor()));
        }
        let end = scan_objects(
            schema,
            input,
            &node,
            element,
            validate,
            metadata,
            names,
            None,
            &mut objects,
        )?;
        reader
            .skip(end - reader.get_cursor())
            .map_err(|_| DataError::MalformedInput(data_location(node._byte_offset)))?;
    }
    Ok((len, None, reader.get_cursor()))
}

fn map_header(
    input: &[u8],
    location: &NodeLocation,
    ty: &TypeDescriptor,
) -> Result<(usize, usize), DataError> {
    if !matches!(ty.kind, TypeKind::Map { .. }) {
        return Err(DataError::AccessError(data_location(location._byte_offset)));
    }
    let (null, body) = node_body(input, location, ty)?;
    if null {
        return Err(DataError::AccessError(data_location(location._byte_offset)));
    }
    let mut reader = Reader::new(input);
    reader
        .skip(body)
        .map_err(|_| DataError::MalformedInput(data_location(body)))?;
    let len =
        read_var_u32(&mut reader).map_err(|_| DataError::MalformedInput(data_location(body)))?;
    Ok((len as usize, reader.get_cursor()))
}

// Null entries are singletons without a chunk-size byte. Both type headers precede bodies.
fn map_chunk(
    names: &mut Vec<(usize, String)>,
    schema: &Schema,
    metadata: &mut Vec<(std::ops::Range<usize>, usize)>,
    input: &[u8],
    cursor: &mut usize,
    remaining: usize,
    key: &TypeDescriptor,
    value: &TypeDescriptor,
) -> Result<
    (
        usize,
        Option<NodeLocation>,
        Option<NodeLocation>,
        bool,
        bool,
    ),
    DataError,
> {
    let start = *cursor;
    let mut reader = Reader::new(input);
    reader
        .skip(start)
        .map_err(|_| DataError::MalformedInput(data_location(start)))?;
    let header = reader
        .read_u8()
        .map_err(|_| DataError::MalformedInput(data_location(start)))?;
    if header & !63 != 0 {
        return Err(DataError::MalformedInput(data_location(start)));
    }
    if (header & 1 != 0 && !key.track_ref && !matches!(key.kind, TypeKind::User(_)))
        || (header & 8 != 0 && !value.track_ref && !matches!(value.kind, TypeKind::User(_)))
    {
        return Err(DataError::NotImplemented("tracked non-user carrier"));
    }
    let kn = header & 2 != 0;
    let vn = header & 16 != 0;
    if (kn && !key.nullable) || (vn && !value.nullable) {
        return Err(DataError::SchemaMismatch(data_location(start)));
    }
    let count = if kn || vn {
        1
    } else {
        reader
            .read_u8()
            .map_err(|_| DataError::MalformedInput(data_location(start)))? as usize
    };
    if count == 0 || count > remaining {
        return Err(DataError::MalformedInput(data_location(start)));
    }
    let mut ids = [None, None];
    for (index, ty, null, declared) in [
        (0, key, kn, header & 4 != 0),
        (1, value, vn, header & 32 != 0),
    ] {
        if !null && !declared {
            let offset = reader.get_cursor();
            let id = read_var_u32(&mut reader)
                .map_err(|_| DataError::MalformedInput(data_location(offset)))?;
            ids[index] = Some(group_type(
                names,
                schema,
                input,
                &mut reader,
                ty,
                id,
                offset,
                metadata,
            )?);
        }
    }
    *cursor = reader.get_cursor();
    let [key, value] = ids;
    let mut key = key;
    let mut value = value;
    if header & 1 != 0 {
        key.get_or_insert_with(|| body_location(0, None, None, false))
            ._reference_mode = Some(RootReferenceMode::Tracking);
    }
    if header & 8 != 0 {
        value
            .get_or_insert_with(|| body_location(0, None, None, false))
            ._reference_mode = Some(RootReferenceMode::Tracking);
    }
    Ok((count, key, value, kn, vn))
}

// group/chunk 的 type info 仅出现一次；后续 body 保留真实 metadata 来源。
fn group_type(
    names: &mut Vec<(usize, String)>,
    schema: &Schema,
    input: &[u8],
    reader: &mut Reader<'_>,
    ty: &TypeDescriptor,
    id: u32,
    offset: usize,
    metadata: &mut Vec<(std::ops::Range<usize>, usize)>,
) -> Result<NodeLocation, DataError> {
    if matches!(
        user_definition(schema, ty).map(|definition| &definition.kind),
        Some(crate::UserDefinitionKind::Enum { .. } | crate::UserDefinitionKind::Union { .. })
    ) {
        if id != user_wire_id(schema, ty)? {
            return Err(DataError::SchemaMismatch(data_location(offset)));
        }
        if matches!(
            schema._data._descriptor.encoding.mode,
            crate::SchemaMode::Compatible
        ) && matches!(ty.kind, TypeKind::User(crate::UserIdentity::Name { .. }))
        {
            return compatible_type(schema, input, reader, ty, metadata);
        }
        read_user_identity(reader, ty, offset, names)?;
        return Ok(body_location(0, None, None, false));
    }
    if matches!(
        schema._data._descriptor.encoding.mode,
        crate::SchemaMode::Compatible
    ) && matches!(ty.kind, TypeKind::User(_))
    {
        let expected = if matches!(ty.kind, TypeKind::User(crate::UserIdentity::Name { .. })) {
            30
        } else {
            28
        };
        if id != expected {
            return Err(DataError::SchemaMismatch(data_location(offset)));
        }
        return compatible_type(schema, input, reader, ty, metadata);
    }
    check_wire_id(ty, id, offset)?;
    read_user_identity(reader, ty, offset, names)?;
    Ok(body_location(0, None, Some(id), false))
}

fn group_location(offset: usize, template: Option<&NodeLocation>, null: bool) -> NodeLocation {
    let mut location = template
        .map(copy_location)
        .unwrap_or_else(|| body_location(0, None, None, false));
    location._byte_offset = offset;
    location._parent_null = null;
    location
}

fn body_location(
    offset: usize,
    mode: Option<RootReferenceMode>,
    id: Option<u32>,
    null: bool,
) -> NodeLocation {
    NodeLocation {
        _byte_offset: offset,
        _parent_null: null,
        _wire_type_id: id,
        _reference_mode: mode,
        _type_source: NodeTypeSource::Schema,
    }
}

fn container_len(
    names: &mut Vec<(usize, String)>,
    schema: &Schema,
    metadata: &mut Vec<(std::ops::Range<usize>, usize)>,
    input: &[u8],
    location: &NodeLocation,
    ty: &TypeDescriptor,
) -> Result<usize, DataError> {
    if matches!(ty.kind, TypeKind::Map { .. }) {
        map_header(input, location, ty).map(|(len, _)| len)
    } else {
        sequence_header(names, schema, metadata, input, location, ty).map(|(len, ..)| len)
    }
}

fn map_locations<'a>(
    mut names: Vec<(usize, String)>,
    mut metadata: Vec<(std::ops::Range<usize>, usize)>,
    context: &'a RootState,
    schema: &'a Schema,
    input: &'a [u8],
    location: &NodeLocation,
    ty: &'a TypeDescriptor,
    validate: bool,
) -> Result<impl Iterator<Item = Result<(NodeLocation, NodeLocation), DataError>> + 'a, DataError> {
    let (mut remaining, mut cursor) = map_header(input, location, ty)?;
    let TypeKind::Map { key, value } = &ty.kind else {
        unreachable!()
    };
    let mut chunk = 0;
    let mut objects = objects_for_node(schema, input, location, ty)?;
    let (mut kid, mut vid, mut kn, mut vn) = (None, None, false, false);
    let mut stopped = false;
    Ok(std::iter::from_fn(move || {
        if stopped || remaining == 0 {
            return None;
        }
        let result = (|| {
            if chunk == 0 {
                (chunk, kid, vid, kn, vn) = map_chunk(
                    &mut names,
                    schema,
                    &mut metadata,
                    input,
                    &mut cursor,
                    remaining,
                    key,
                    value,
                )?;
            }
            let kl = group_location(cursor, kid.as_ref(), kn);
            cursor = scan_objects(
                schema,
                input,
                &kl,
                key,
                validate,
                &mut metadata,
                &mut names,
                None,
                &mut objects,
            )?;
            let vl = group_location(cursor, vid.as_ref(), vn);
            cursor = scan_objects(
                schema,
                input,
                &vl,
                value,
                validate,
                &mut metadata,
                &mut names,
                None,
                &mut objects,
            )?;
            context.store_metadata(&metadata)?;
            chunk -= 1;
            remaining -= 1;
            Ok((kl, vl))
        })();
        stopped = result.is_err();
        Some(result)
    }))
}

// 帧仅保存未完成的容器和待处理值，空间 O(depth)，不递归调用自身或物化业务对象。
fn scan_node_metadata(
    schema: &Schema,
    input: &[u8],
    location: &NodeLocation,
    ty: &TypeDescriptor,
    validate: bool,
    metadata: &mut Vec<(std::ops::Range<usize>, usize)>,
) -> Result<usize, DataError> {
    scan_named_node(
        schema,
        input,
        location,
        ty,
        validate,
        metadata,
        &mut vec![],
        None,
    )
}

fn scan_named_node(
    schema: &Schema,
    input: &[u8],
    location: &NodeLocation,
    ty: &TypeDescriptor,
    validate: bool,
    metadata: &mut Vec<(std::ops::Range<usize>, usize)>,
    names: &mut Vec<(usize, String)>,
    stop: Option<usize>,
) -> Result<usize, DataError> {
    let mut objects = objects_before(schema, input, location._byte_offset)?;
    scan_objects(
        schema,
        input,
        location,
        ty,
        validate,
        metadata,
        names,
        stop,
        &mut objects,
    )
}

fn objects_before(
    schema: &Schema,
    input: &[u8],
    offset: usize,
) -> Result<Vec<(TypeDescriptor, NodeLocation)>, DataError> {
    let mut objects = vec![];
    let mut types = vec![&schema._data._descriptor.root];
    for definition in &schema._data._descriptor.definitions {
        match &definition.kind {
            crate::UserDefinitionKind::Struct { fields, .. } => {
                types.extend(fields.iter().map(|field| &field.ty))
            }
            crate::UserDefinitionKind::Union { .. } => {
                types.clear();
                break;
            }
            _ => {}
        }
    }
    if !types.is_empty()
        && !matches!(
            schema._data._descriptor.encoding.root_reference,
            RootReferenceMode::Tracking
        )
    {
        let mut tracked = false;
        while let Some(ty) = types.pop() {
            if ty.track_ref {
                tracked = true;
                break;
            }
            match &ty.kind {
                TypeKind::List(element)
                | TypeKind::Set(element)
                | TypeKind::Array { element, .. } => types.push(element),
                TypeKind::Map { key, value } => {
                    types.push(key);
                    types.push(value);
                }
                _ => {}
            }
        }
        if !tracked {
            return Ok(objects);
        }
    }
    // 已验证的两种入口首字节互斥：xlang bitmap=1，RootValue 从 ref flag 开始。
    let start = usize::from(input.first() == Some(&1));
    if offset < start {
        return Ok(objects);
    }
    let root = NodeLocation {
        _byte_offset: start,
        _reference_mode: Some(schema._data._descriptor.encoding.root_reference),
        _type_source: NodeTypeSource::Envelope,
        _wire_type_id: None,
        _parent_null: false,
    };
    scan_objects(
        schema,
        input,
        &root,
        &schema._data._descriptor.root,
        false,
        &mut vec![],
        &mut vec![],
        Some(offset),
        &mut objects,
    )?;
    Ok(objects)
}

fn objects_for_node(
    schema: &Schema,
    input: &[u8],
    location: &NodeLocation,
    ty: &TypeDescriptor,
) -> Result<Vec<(TypeDescriptor, NodeLocation)>, DataError> {
    let mut objects = objects_before(schema, input, location._byte_offset)?;
    if !location._parent_null
        && matches!(location._reference_mode, Some(RootReferenceMode::Tracking))
        && input.get(location._byte_offset) == Some(&0)
    {
        reserve_object(&mut objects, ty, location)?;
    }
    Ok(objects)
}

fn same_type(left: &TypeDescriptor, right: &TypeDescriptor) -> bool {
    compare_type(left, right, false)
}

fn compare_type(left: &TypeDescriptor, right: &TypeDescriptor, object: bool) -> bool {
    let mut stack = vec![(left, right, object)];
    while let Some((a, b, outer)) = stack.pop() {
        if (!outer && (a.nullable != b.nullable || a.track_ref != b.track_ref))
            || a.integer_encoding != b.integer_encoding
        {
            return false;
        }
        match (&a.kind, &b.kind) {
            (TypeKind::User(a), TypeKind::User(b)) if a == b => {}
            (TypeKind::Scalar(a), TypeKind::Scalar(b))
                if std::mem::discriminant(a) == std::mem::discriminant(b) => {}
            (TypeKind::List(a), TypeKind::List(b)) | (TypeKind::Set(a), TypeKind::Set(b)) => {
                stack.push((a, b, false))
            }
            (
                TypeKind::Array {
                    element: a,
                    length: al,
                },
                TypeKind::Array {
                    element: b,
                    length: bl,
                },
            ) if al == bl => stack.push((a, b, false)),
            (TypeKind::Map { key: ak, value: av }, TypeKind::Map { key: bk, value: bv }) => {
                stack.push((ak, bk, false));
                stack.push((av, bv, false));
            }
            _ => return false,
        }
    }
    true
}

fn declared_type<'a>(
    schema: &'a Schema,
    target: &TypeDescriptor,
) -> Result<&'a TypeDescriptor, DataError> {
    let mut types = vec![&schema._data._descriptor.root];
    for definition in &schema._data._descriptor.definitions {
        match &definition.kind {
            crate::UserDefinitionKind::Struct { fields, .. } => {
                types.extend(fields.iter().map(|field| &field.ty))
            }
            crate::UserDefinitionKind::Union { variants } => types.extend(
                variants
                    .iter()
                    .filter_map(|variant| variant.payload.as_ref()),
            ),
            _ => {}
        }
    }
    while let Some(ty) = types.pop() {
        if same_type(ty, target) {
            return Ok(ty);
        }
        match &ty.kind {
            TypeKind::List(element) | TypeKind::Set(element) | TypeKind::Array { element, .. } => {
                types.push(element)
            }
            TypeKind::Map { key, value } => {
                types.push(key);
                types.push(value);
            }
            _ => {}
        }
    }
    Err(DataError::AccessError(data_location(0)))
}

fn same_object_type(left: &TypeDescriptor, right: &TypeDescriptor) -> bool {
    // Outer null/ref flags describe the occurrence, not the stored object's body.
    // Nested generic flags and integer codecs remain part of the exact contract.
    compare_type(left, right, true)
}

fn scan_objects(
    schema: &Schema,
    input: &[u8],
    location: &NodeLocation,
    ty: &TypeDescriptor,
    validate: bool,
    metadata: &mut Vec<(std::ops::Range<usize>, usize)>,
    names: &mut Vec<(usize, String)>,
    stop: Option<usize>,
    objects: &mut Vec<(TypeDescriptor, NodeLocation)>,
) -> Result<usize, DataError> {
    let mut cursor = location._byte_offset;
    // kind: 0=value, 1=sequence, 2=map, 3=struct fields, 4=struct exit.
    let mut active = std::collections::HashSet::new();
    let mut stack = vec![(
        0u8,
        ty,
        copy_location(location),
        0usize,
        0usize,
        None,
        None,
        false,
        false,
    )];
    while let Some((kind, ty, mut loc, remaining, chunk, kid, vid, kn, vn)) = stack.pop() {
        if kind == 4 {
            active.remove(&(remaining, chunk));
            continue;
        }
        // 边界必须在 continuation 读取下一个 chunk/group header 之前生效。
        if stop.is_some_and(|stop| cursor >= stop) {
            if stop == Some(cursor)
                && kind == 0
                && !loc._parent_null
                && matches!(loc._reference_mode, Some(RootReferenceMode::Tracking))
                && input.get(cursor) == Some(&0)
            {
                loc._byte_offset = cursor;
                reserve_object(objects, ty, &loc)?;
            }
            return Ok(cursor);
        }
        if kind == 1 {
            if remaining == 0 {
                continue;
            }
            stack.push((
                1,
                ty,
                copy_location(&loc),
                remaining - 1,
                0,
                None,
                None,
                false,
                false,
            ));
            stack.push((
                0,
                sequence_element(ty)?,
                loc,
                0,
                0,
                None,
                None,
                false,
                false,
            ));
            continue;
        }
        if kind == 2 {
            if remaining == 0 {
                continue;
            }
            let TypeKind::Map { key, value } = &ty.kind else {
                unreachable!()
            };
            let (count, kid, vid, kn, vn) = if chunk == 0 {
                map_chunk(
                    names,
                    schema,
                    metadata,
                    input,
                    &mut cursor,
                    remaining,
                    key,
                    value,
                )?
            } else {
                (chunk, kid, vid, kn, vn)
            };
            stack.push((
                2,
                ty,
                body_location(0, None, None, false),
                remaining - 1,
                count - 1,
                kid.as_ref().map(copy_location),
                vid.as_ref().map(copy_location),
                kn,
                vn,
            ));
            stack.push((
                0,
                value,
                group_location(0, vid.as_ref(), vn),
                0,
                0,
                None,
                None,
                false,
                false,
            ));
            stack.push((
                0,
                key,
                group_location(0, kid.as_ref(), kn),
                0,
                0,
                None,
                None,
                false,
                false,
            ));
            continue;
        }
        if kind == 3 {
            let (fields, order) = struct_fields(schema, ty)?;
            if remaining == order.len() {
                continue;
            }
            let field = &fields[order[remaining]];
            let mut field_loc = field_location(schema, cursor, &field.ty)?;
            if field.ty.track_ref
                && matches!(
                    schema._data._descriptor.encoding.mode,
                    crate::SchemaMode::Compatible
                )
                && matches!(
                    user_definition(schema, &field.ty).map(|definition| &definition.kind),
                    Some(crate::UserDefinitionKind::Struct { .. })
                )
            {
                field_loc._type_source = NodeTypeSource::Envelope;
            }
            stack.push((
                3,
                ty,
                body_location(0, None, None, false),
                remaining + 1,
                0,
                None,
                None,
                false,
                false,
            ));
            stack.push((0, &field.ty, field_loc, 0, 0, None, None, false, false));
            continue;
        }
        loc._byte_offset = cursor;
        if matches!(loc._reference_mode, Some(RootReferenceMode::Tracking)) && !loc._parent_null {
            let flag = *input
                .get(cursor)
                .ok_or_else(|| DataError::MalformedInput(data_location(cursor)))?;
            if flag == 254 {
                let mut reader = Reader::new(input);
                reader
                    .skip(cursor + 1)
                    .map_err(|_| DataError::MalformedInput(data_location(cursor)))?;
                let id = read_var_u32(&mut reader)
                    .map_err(|_| DataError::MalformedInput(data_location(cursor)))?
                    as usize;
                let (target, at) = objects
                    .get(id)
                    .ok_or_else(|| DataError::MalformedInput(data_location(cursor)))?;
                if at._byte_offset >= cursor {
                    return Err(DataError::MalformedInput(data_location(cursor)));
                }
                if !same_object_type(target, ty) {
                    return Err(DataError::SchemaMismatch(data_location(cursor)));
                }
                cursor = reader.get_cursor();
                continue;
            }
            if flag == 0 {
                reserve_object(objects, ty, &loc)?;
            }
            if stop.is_some_and(|stop| cursor + 1 >= stop) {
                return Ok(cursor);
            }
        }
        if matches!(
            user_definition(schema, ty).map(|definition| &definition.kind),
            Some(crate::UserDefinitionKind::Enum { .. } | crate::UserDefinitionKind::Union { .. })
        ) {
            if matches!(loc._type_source, NodeTypeSource::Envelope) {
                loc = variant_envelope(schema, input, cursor, ty, names, metadata)?;
            }
            let (null, body) = node_body(input, &loc, ty)?;
            if null {
                cursor = body;
                continue;
            }
            let (variant, payload_location, end) = variant_body(schema, input, &loc, ty)?;
            cursor = end;
            if let (Some(payload), Some(location)) = (&variant.payload, payload_location) {
                let location = bind_payload(schema, input, &location, payload, names, metadata)?;
                cursor = location._byte_offset;
                stack.push((0, payload, location, 0, 0, None, None, false, false));
            }
            continue;
        }
        let (null, mut body) = if matches!(loc._type_source, NodeTypeSource::Envelope)
            && matches!(
                schema._data._descriptor.encoding.mode,
                crate::SchemaMode::Compatible
            )
            && matches!(
                user_definition(schema, ty).map(|definition| &definition.kind),
                Some(crate::UserDefinitionKind::Struct { .. })
            ) {
            let start = loc._byte_offset;
            let mut reader = Reader::new(input);
            reader
                .skip(start + 1)
                .map_err(|_| DataError::MalformedInput(data_location(start)))?;
            if input.get(start) == Some(&253) {
                node_body(input, &loc, ty)?
            } else {
                let id = read_var_u32(&mut reader)
                    .map_err(|_| DataError::MalformedInput(data_location(start)))?;
                if id != user_wire_id(schema, ty)? {
                    return Err(DataError::SchemaMismatch(data_location(start)));
                }
                loc = compatible_type(schema, input, &mut reader, ty, metadata)?;
                (false, loc._byte_offset)
            }
        } else {
            named_node_body(input, &loc, ty, names)?
        };
        if !null
            && matches!(
                schema._data._descriptor.encoding.mode,
                crate::SchemaMode::Compatible
            )
            && matches!(
                user_definition(schema, ty).map(|definition| &definition.kind),
                Some(crate::UserDefinitionKind::Struct { .. })
            )
            && !matches!(loc._type_source, NodeTypeSource::Inherited { .. })
        {
            loc = compatible_field(schema, input, &loc, ty, metadata)?;
            body = loc._byte_offset;
        }
        cursor = body;
        if null {
            continue;
        }
        match &ty.kind {
            TypeKind::Scalar(_) => cursor = scan_scalar(input, body, ty, &loc, validate)?,
            TypeKind::List(_) | TypeKind::Array { .. } | TypeKind::Set(_) => {
                let (len, start, mode, _, mut template) =
                    sequence_header(names, schema, metadata, input, &loc, ty)?;
                template._reference_mode = mode;
                cursor = start;
                stack.push((1, ty, template, len, 0, None, None, false, false));
            }
            TypeKind::Map { .. } => {
                let (len, start) = map_header(input, &loc, ty)?;
                cursor = start;
                stack.push((
                    2,
                    ty,
                    body_location(0, None, None, false),
                    len,
                    0,
                    None,
                    None,
                    false,
                    false,
                ));
            }
            TypeKind::User(_) => {
                struct_fields(schema, ty)?;
                cursor = struct_body(schema, input, ty, body, validate)?;
                let TypeKind::User(identity) = &ty.kind else {
                    unreachable!()
                };
                let index = schema._data._type_access._definition_by_identity[identity];
                if !active.insert((index, body)) {
                    return Err(DataError::MalformedInput(data_location(body)));
                }
                stack.push((
                    4,
                    ty,
                    body_location(0, None, None, false),
                    index,
                    body,
                    None,
                    None,
                    false,
                    false,
                ));
                stack.push((
                    3,
                    ty,
                    body_location(0, None, None, false),
                    0,
                    0,
                    None,
                    None,
                    false,
                    false,
                ));
            }
        }
    }
    Ok(cursor)
}

fn reserve_object(
    objects: &mut Vec<(TypeDescriptor, NodeLocation)>,
    ty: &TypeDescriptor,
    location: &NodeLocation,
) -> Result<(), DataError> {
    let offset = location._byte_offset;
    // Fresh wire markers append; replays must find the original slot, never renumber it.
    let append = objects.last().is_none_or(|(_, at)| {
        #[cfg(test)]
        OBJECT_COMPARISONS.with(|count| count.set(count.get() + 1));
        at._byte_offset < offset
    });
    if append {
        objects.push((ty.clone(), copy_location(location)));
    } else if objects
        .binary_search_by_key(&offset, |(_, at)| {
            #[cfg(test)]
            OBJECT_COMPARISONS.with(|count| count.set(count.get() + 1));
            at._byte_offset
        })
        .is_err()
    {
        return Err(DataError::MalformedInput(data_location(offset)));
    }
    Ok(())
}

#[cfg(test)]
std::thread_local! {
    static OBJECT_COMPARISONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static IDENTITY_REPLAYS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

// @inline-test-exception: counts the private identity-prefix fallback, not wall time.
#[cfg(test)]
#[test]
fn registered_identity_queries_do_not_replay_and_future_slots_stay_invalid() {
    let child = TypeDescriptor {
        kind: TypeKind::Scalar(ScalarKind::String),
        nullable: false,
        track_ref: true,
        integer_encoding: None,
    };
    let schema = Schema::compile(
        crate::SchemaDescriptor {
            encoding: crate::RootEncoding {
                protocol_version: crate::ProtocolVersion::Fory1_7,
                mode: crate::SchemaMode::SameSchema {
                    version_hash: false,
                },
                root_reference: RootReferenceMode::Tracking,
            },
            key_semantics_version: crate::KEY_SEMANTICS_VERSION,
            root: TypeDescriptor {
                kind: TypeKind::List(Box::new(child.clone())),
                nullable: false,
                track_ref: false,
                integer_encoding: None,
            },
            definitions: vec![],
        },
        &crate::SchemaLimits::default(),
    )
    .unwrap();
    let mut values: Vec<_> = (0..128).map(|i| Rc::new(format!("v{i}"))).collect();
    values.push(Rc::clone(&values[0]));
    let input = fory::Fory::builder()
        .xlang(true)
        .build()
        .serialize(&values)
        .unwrap();
    let view = schema
        .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
        .unwrap();
    let root = view.root().unwrap();
    let nodes = root
        .elements()
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let id = nodes[128].reference_id().unwrap().unwrap();
    IDENTITY_REPLAYS.with(|count| count.set(0));
    for _ in 0..2 {
        for node in &nodes[..128] {
            assert!(node.object_id().unwrap().is_some());
        }
        assert_eq!(nodes[128].reference_id().unwrap().unwrap(), id);
    }
    assert_eq!(IDENTITY_REPLAYS.with(|count| count.get()), 0);
    let context = &view._context;
    let guard = context._progress.try_borrow_mut().unwrap();
    assert!(matches!(
        nodes[0].object_id(),
        Err(DataError::AccessError(_))
    ));
    drop(guard);
    assert!(nodes[0].object_id().unwrap().is_some());
    // A cached later slot must not make an earlier Ref legal.
    let mut bad = input.clone();
    let at = nodes[0]._location._byte_offset;
    bad[at] = 254;
    bad[at + 1] = 127;
    let location = body_location(at, Some(RootReferenceMode::Tracking), Some(21), false);
    assert!(matches!(
        context.identity(&schema, &bad, &location, &child, true),
        Err(DataError::MalformedInput(_))
    ));
}

// @inline-test-exception: published recursive graph access under an exclusive progress borrow.
#[cfg(test)]
#[test]
fn recursive_container_and_temporal_reads_ignore_published_progress_lock() {
    for map in [false, true] {
        let user = TypeDescriptor {
            kind: TypeKind::User(crate::UserIdentity::Numeric(540)),
            nullable: false,
            track_ref: true,
            integer_encoding: None,
        };
        let container = if map {
            TypeKind::Map {
                key: Box::new(TypeDescriptor {
                    kind: TypeKind::Scalar(ScalarKind::String),
                    nullable: false,
                    track_ref: false,
                    integer_encoding: None,
                }),
                value: Box::new(user.clone()),
            }
        } else {
            TypeKind::List(Box::new(user.clone()))
        };
        let schema = Schema::compile(
            crate::SchemaDescriptor {
                encoding: crate::RootEncoding {
                    protocol_version: crate::ProtocolVersion::Fory1_7,
                    mode: crate::SchemaMode::SameSchema {
                        version_hash: false,
                    },
                    root_reference: RootReferenceMode::Tracking,
                },
                key_semantics_version: crate::KEY_SEMANTICS_VERSION,
                root: user,
                definitions: vec![crate::UserDefinition {
                    identity: crate::UserIdentity::Numeric(540),
                    kind: crate::UserDefinitionKind::Struct {
                        fields: vec![crate::FieldDescriptor {
                            id: None,
                            name: "children".into(),
                            ty: TypeDescriptor {
                                kind: container,
                                nullable: false,
                                track_ref: false,
                                integer_encoding: None,
                            },
                        }],
                        physical_field_order: vec![0],
                    },
                }],
            },
            &crate::SchemaLimits::default(),
        )
        .unwrap();
        let mut input = vec![];
        let mut writer = fory::Writer::from_buffer(&mut input);
        writer.write_i8(0);
        writer.write_var_u32(27);
        writer.write_var_u32(540);
        writer.write_var_u32(1);
        if map {
            writer.write_u8(8);
            writer.write_u8(1);
            writer.write_var_u32(21);
            writer.write_var_u32(27);
            writer.write_var_u32(540);
            writer.write_var_u32(6);
            writer.write_u8(b'k');
        } else {
            writer.write_u8(9);
            writer.write_var_u32(27);
            writer.write_var_u32(540);
        }
        writer.write_i8(-2);
        writer.write_var_u32(0);
        let view = schema
            .view(&input, InputFormat::RootValue, &ParseLimits::default())
            .unwrap();
        let context = Rc::clone(&view._context);
        let validated = view.validate().unwrap();
        let _guard = context._progress.try_borrow_mut().unwrap();
        let root = validated.root().unwrap();
        let id = root.object_id().unwrap().unwrap();
        for (_, body) in root.fields().unwrap().map(Result::unwrap) {
            assert_eq!(body.len().unwrap(), 1);
            let edge = if map {
                body.entries().unwrap().next().unwrap().unwrap().1
            } else {
                body.elements().unwrap().next().unwrap().unwrap()
            };
            assert_eq!(edge.reference_id().unwrap().unwrap(), id);
            assert!(matches!(edge.field_at(0), Err(DataError::AccessError(_))));
            assert_eq!(
                edge.resolve_reference(&id)
                    .unwrap()
                    .field_at(0)
                    .unwrap()
                    .len()
                    .unwrap(),
                1
            );
        }
    }
    for kind in [
        ScalarKind::Date,
        ScalarKind::Timestamp,
        ScalarKind::Duration,
    ] {
        let schema = Schema::compile(
            crate::SchemaDescriptor {
                encoding: crate::RootEncoding {
                    protocol_version: crate::ProtocolVersion::Fory1_7,
                    mode: crate::SchemaMode::SameSchema {
                        version_hash: false,
                    },
                    root_reference: RootReferenceMode::Tracking,
                },
                key_semantics_version: crate::KEY_SEMANTICS_VERSION,
                root: TypeDescriptor {
                    kind: TypeKind::Scalar(kind),
                    nullable: false,
                    track_ref: true,
                    integer_encoding: None,
                },
                definitions: vec![],
            },
            &crate::SchemaLimits::default(),
        )
        .unwrap();
        let mut input = vec![];
        let mut writer = fory::Writer::from_buffer(&mut input);
        writer.write_i8(0);
        writer.write_var_u32(scalar_id(&schema._data._descriptor.root).unwrap());
        if matches!(kind, ScalarKind::Timestamp) {
            writer.write_i64(-2);
        } else {
            writer.write_var_i64(-2);
        }
        if !matches!(kind, ScalarKind::Date) {
            writer.write_i32(999_999_999);
        }
        let view = schema
            .view(&input, InputFormat::RootValue, &ParseLimits::default())
            .unwrap();
        let context = Rc::clone(&view._context);
        let validated = view.validate().unwrap();
        let _guard = context._progress.try_borrow_mut().unwrap();
        let root = validated.root().unwrap();
        assert!(root.object_id().unwrap().is_some());
        match kind {
            ScalarKind::Date => assert_eq!(root.read_date().unwrap().epoch_days(), -2),
            ScalarKind::Timestamp => {
                assert_eq!(root.read_timestamp().unwrap().subsec_nanos(), 999_999_999)
            }
            _ => assert_eq!(root.read_duration().unwrap().subsec_nanos(), 999_999_999),
        }
    }
}

// @inline-test-exception: deterministic probe count for the private slot lookup algorithm.
#[cfg(test)]
#[test]
fn unique_object_registration_comparisons_are_not_quadratic() {
    let user = TypeDescriptor {
        kind: TypeKind::User(crate::UserIdentity::Numeric(410)),
        nullable: false,
        track_ref: true,
        integer_encoding: None,
    };
    let schema = Schema::compile(
        crate::SchemaDescriptor {
            encoding: crate::RootEncoding {
                protocol_version: crate::ProtocolVersion::Fory1_7,
                mode: crate::SchemaMode::SameSchema {
                    version_hash: false,
                },
                root_reference: RootReferenceMode::Tracking,
            },
            key_semantics_version: crate::KEY_SEMANTICS_VERSION,
            root: TypeDescriptor {
                kind: TypeKind::List(Box::new(user)),
                nullable: false,
                track_ref: false,
                integer_encoding: None,
            },
            definitions: vec![crate::UserDefinition {
                identity: crate::UserIdentity::Numeric(410),
                kind: crate::UserDefinitionKind::Struct {
                    fields: vec![],
                    physical_field_order: vec![],
                },
            }],
        },
        &crate::SchemaLimits::default(),
    )
    .unwrap();
    let mut input = vec![];
    let mut writer = fory::Writer::from_buffer(&mut input);
    writer.write_i8(-1);
    writer.write_var_u32(22);
    writer.write_var_u32(4096);
    writer.write_u8(9);
    writer.write_var_u32(27);
    writer.write_var_u32(410);
    for _ in 0..4096 {
        writer.write_i8(0);
    }
    OBJECT_COMPARISONS.with(|count| count.set(0));
    schema
        .validate(&input, InputFormat::RootValue, &ParseLimits::default())
        .unwrap();
    let comparisons = OBJECT_COMPARISONS.with(|count| count.get());
    assert!(comparisons <= 4096 * 16, "{comparisons} marker comparisons");
    assert_eq!(comparisons, 4095);
    let mut descriptor = schema._data._descriptor.clone();
    descriptor.definitions.clear();
    descriptor.root.kind = TypeKind::List(Box::new(TypeDescriptor {
        kind: TypeKind::Scalar(ScalarKind::String),
        nullable: false,
        track_ref: true,
        integer_encoding: None,
    }));
    let schema = Schema::compile(descriptor, &crate::SchemaLimits::default()).unwrap();
    let values: Vec<_> = (0..4096).map(|_| Rc::new(String::new())).collect();
    let input = fory::Fory::builder()
        .xlang(true)
        .check_struct_version(false)
        .build()
        .serialize(&values)
        .unwrap();
    let view = schema
        .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
        .unwrap();
    let context = Rc::clone(&view._context);
    OBJECT_COMPARISONS.with(|count| count.set(0));
    let validated = view.validate().unwrap();
    assert_eq!(OBJECT_COMPARISONS.with(|count| count.get()), 4095);
    let _guard = context._progress.try_borrow_mut().unwrap();
    let root = validated.root().unwrap();
    for _ in 0..2 {
        let ids: std::collections::HashSet<_> = root
            .elements()
            .unwrap()
            .map(|node| {
                let node = node.unwrap();
                assert_eq!(node.read_string().unwrap().as_str(), "");
                let id = node.object_id().unwrap().unwrap();
                assert_eq!(
                    node.resolve_reference(&id)
                        .unwrap()
                        .read_string()
                        .unwrap()
                        .as_str(),
                    ""
                );
                id
            })
            .collect();
        assert_eq!(ids.len(), 4096);
    }
}

// @inline-test-exception: verifies published access while the private progress cell is locked.
#[cfg(test)]
#[test]
fn validated_self_cycle_fields_and_null_ignore_progress_lock() {
    let user = TypeDescriptor {
        kind: TypeKind::User(crate::UserIdentity::Numeric(420)),
        nullable: true,
        track_ref: true,
        integer_encoding: None,
    };
    let schema = Schema::compile(
        crate::SchemaDescriptor {
            encoding: crate::RootEncoding {
                protocol_version: crate::ProtocolVersion::Fory1_7,
                mode: crate::SchemaMode::SameSchema {
                    version_hash: false,
                },
                root_reference: RootReferenceMode::Tracking,
            },
            key_semantics_version: crate::KEY_SEMANTICS_VERSION,
            root: user.clone(),
            definitions: vec![crate::UserDefinition {
                identity: crate::UserIdentity::Numeric(420),
                kind: crate::UserDefinitionKind::Struct {
                    fields: vec![crate::FieldDescriptor {
                        id: None,
                        name: "next".into(),
                        ty: user,
                    }],
                    physical_field_order: vec![0],
                },
            }],
        },
        &crate::SchemaLimits::default(),
    )
    .unwrap();
    let mut input = vec![];
    let mut writer = fory::Writer::from_buffer(&mut input);
    writer.write_i8(0);
    writer.write_var_u32(27);
    writer.write_var_u32(420);
    writer.write_i8(-2);
    writer.write_var_u32(0);
    let view = schema
        .view(&input, InputFormat::RootValue, &ParseLimits::default())
        .unwrap();
    let context = Rc::clone(&view._context);
    let validated = view.validate().unwrap();
    let _guard = context._progress.try_borrow_mut().unwrap();
    let root = validated.root().unwrap();
    let fields = root
        .fields()
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert!(!root.is_null().unwrap());
    assert!(!fields[0].1.is_null().unwrap());
    assert_eq!(
        fields[0].1.reference_id().unwrap(),
        root.object_id().unwrap()
    );
}

fn scalar_id(ty: &TypeDescriptor) -> Result<u32, DataError> {
    let id = match (&ty.kind, ty.integer_encoding) {
        (TypeKind::List(_) | TypeKind::Array { .. }, _) => 22,
        (TypeKind::Set(_), _) => 23,
        (TypeKind::Map { .. }, _) => 24,
        (TypeKind::User(crate::UserIdentity::Numeric(_)), _) => 27,
        (TypeKind::User(crate::UserIdentity::Name { .. }), _) => 29,
        (TypeKind::Scalar(ScalarKind::Bool), _) => 1,
        (TypeKind::Scalar(ScalarKind::I8), _) => 2,
        (TypeKind::Scalar(ScalarKind::I16), _) => 3,
        (TypeKind::Scalar(ScalarKind::I32), Some(IntegerEncoding::Fixed)) => 4,
        (TypeKind::Scalar(ScalarKind::I32), Some(IntegerEncoding::Varint)) => 5,
        (TypeKind::Scalar(ScalarKind::I64), Some(IntegerEncoding::Fixed)) => 6,
        (TypeKind::Scalar(ScalarKind::I64), Some(IntegerEncoding::Varint)) => 7,
        (TypeKind::Scalar(ScalarKind::I64), Some(IntegerEncoding::Tagged)) => 8,
        (TypeKind::Scalar(ScalarKind::U8), _) => 9,
        (TypeKind::Scalar(ScalarKind::U16), _) => 10,
        (TypeKind::Scalar(ScalarKind::U32), Some(IntegerEncoding::Fixed)) => 11,
        (TypeKind::Scalar(ScalarKind::U32), Some(IntegerEncoding::Varint)) => 12,
        (TypeKind::Scalar(ScalarKind::U64), Some(IntegerEncoding::Fixed)) => 13,
        (TypeKind::Scalar(ScalarKind::U64), Some(IntegerEncoding::Varint)) => 14,
        (TypeKind::Scalar(ScalarKind::U64), Some(IntegerEncoding::Tagged)) => 15,
        (TypeKind::Scalar(ScalarKind::F32), _) => 19,
        (TypeKind::Scalar(ScalarKind::F64), _) => 20,
        (TypeKind::Scalar(ScalarKind::String), _) => 21,
        (TypeKind::Scalar(ScalarKind::Binary), _) => 41,
        (TypeKind::Scalar(ScalarKind::Date), _) => 39,
        (TypeKind::Scalar(ScalarKind::Timestamp), _) => 38,
        (TypeKind::Scalar(ScalarKind::Duration), _) => 37,
        _ => return Err(DataError::NotImplemented("root type wire adaptation")),
    };
    Ok(id)
}

fn check_wire_id(ty: &TypeDescriptor, id: u32, offset: usize) -> Result<(), DataError> {
    if id == scalar_id(ty)? || dense_sequence_id(ty, id)? {
        Ok(())
    } else {
        Err(DataError::SchemaMismatch(data_location(offset)))
    }
}

fn user_definition<'a>(
    schema: &'a Schema,
    ty: &TypeDescriptor,
) -> Option<&'a crate::UserDefinition> {
    let TypeKind::User(identity) = &ty.kind else {
        return None;
    };
    schema
        ._data
        ._type_access
        ._definition_by_identity
        .get(identity)
        .map(|index| &schema._data._descriptor.definitions[*index])
}

fn user_wire_id(schema: &Schema, ty: &TypeDescriptor) -> Result<u32, DataError> {
    let Some(definition) = user_definition(schema, ty) else {
        return scalar_id(ty);
    };
    let named = matches!(definition.identity, crate::UserIdentity::Name { .. });
    Ok(match definition.kind {
        crate::UserDefinitionKind::Enum { .. } => {
            if named {
                26
            } else {
                25
            }
        }
        crate::UserDefinitionKind::Union { .. } => {
            if named {
                35
            } else {
                34
            }
        }
        crate::UserDefinitionKind::Struct { .. } => {
            if matches!(
                schema._data._descriptor.encoding.mode,
                crate::SchemaMode::Compatible
            ) {
                if named { 30 } else { 28 }
            } else if named {
                29
            } else {
                27
            }
        }
    })
}

fn variant_body<'a>(
    schema: &'a Schema,
    input: &[u8],
    location: &NodeLocation,
    ty: &TypeDescriptor,
) -> Result<(&'a VariantDescriptor, Option<NodeLocation>, usize), DataError> {
    let definition = user_definition(schema, ty)
        .ok_or_else(|| DataError::AccessError(data_location(location._byte_offset)))?;
    let (variants, union) = match &definition.kind {
        crate::UserDefinitionKind::Enum { variants } => (variants, false),
        crate::UserDefinitionKind::Union { variants } => (variants, true),
        _ => return Err(DataError::AccessError(data_location(location._byte_offset))),
    };
    let bound;
    let location = if matches!(location._type_source, NodeTypeSource::Envelope) {
        let mut names = vec![];
        let mut metadata = vec![];
        let start = usize::from(input.first() == Some(&1));
        if location._byte_offset > start {
            let root = NodeLocation {
                _byte_offset: start,
                _parent_null: false,
                _wire_type_id: None,
                _reference_mode: Some(schema._data._descriptor.encoding.root_reference),
                _type_source: NodeTypeSource::Envelope,
            };
            scan_objects(
                schema,
                input,
                &root,
                &schema._data._descriptor.root,
                false,
                &mut metadata,
                &mut names,
                Some(location._byte_offset),
                &mut vec![],
            )?;
        }
        bound = variant_envelope(
            schema,
            input,
            location._byte_offset,
            ty,
            &mut names,
            &mut metadata,
        )?;
        &bound
    } else {
        location
    };
    let (null, body) = node_body(input, location, ty)?;
    if null {
        return Err(DataError::AccessError(data_location(location._byte_offset)));
    }
    let mut reader = Reader::new(input);
    reader
        .skip(body)
        .map_err(|_| DataError::MalformedInput(data_location(body)))?;
    let id =
        read_var_u32(&mut reader).map_err(|_| DataError::MalformedInput(data_location(body)))?;
    let index = schema._data._type_access._definition_by_identity[&definition.identity];
    let position = schema._data._type_access._variant_access[&index]
        ._variant_by_id
        .get(&id)
        .ok_or_else(|| DataError::SchemaMismatch(data_location(body)))?;
    let variant = &variants[*position];
    let payload = if variant.payload.is_some() {
        Some(NodeLocation {
            _byte_offset: reader.get_cursor(),
            _reference_mode: Some(RootReferenceMode::Tracking),
            _type_source: NodeTypeSource::Envelope,
            _wire_type_id: None,
            _parent_null: false,
        })
    } else {
        if union
            && reader
                .read_i8()
                .map_err(|_| DataError::MalformedInput(data_location(body)))?
                != -3
        {
            return Err(DataError::MalformedInput(data_location(body)));
        }
        None
    };
    Ok((variant, payload, reader.get_cursor()))
}

#[cfg(test)]
#[test]
fn validated_union_payload_access_ignores_progress_lock_and_deep_scan_is_iterative() {
    let identity = crate::UserIdentity::Numeric(300);
    let user = TypeDescriptor {
        kind: TypeKind::User(identity.clone()),
        nullable: false,
        track_ref: false,
        integer_encoding: None,
    };
    let schema = Schema::compile(
        crate::SchemaDescriptor {
            encoding: crate::RootEncoding {
                protocol_version: crate::ProtocolVersion::Fory1_7,
                mode: crate::SchemaMode::SameSchema { version_hash: true },
                root_reference: RootReferenceMode::NullOnly,
            },
            key_semantics_version: crate::KEY_SEMANTICS_VERSION,
            root: user.clone(),
            definitions: vec![crate::UserDefinition {
                identity,
                kind: crate::UserDefinitionKind::Union {
                    variants: vec![
                        VariantDescriptor {
                            id: 9,
                            name: "Unit".into(),
                            payload: None,
                        },
                        VariantDescriptor {
                            id: 3,
                            name: "Next".into(),
                            payload: Some(user),
                        },
                    ],
                },
            }],
        },
        &crate::SchemaLimits::default(),
    )
    .unwrap();
    let mut input = vec![255, 34, 172, 2];
    for _ in 0..2048 {
        input.extend_from_slice(&[3, 255, 34, 172, 2]);
    }
    input.extend_from_slice(&[9, 253]);
    let view = schema
        .view(&input, InputFormat::RootValue, &ParseLimits::default())
        .unwrap();
    let context = Rc::clone(&view._context);
    let validated = view.validate().unwrap();
    let guard = context._progress.try_borrow_mut().unwrap();
    let root = validated.root().unwrap();
    assert_eq!(root.variant().unwrap().id, 3);
    let owned = root.variant_payload().unwrap().unwrap().to_owned().unwrap();
    assert_eq!(owned.as_view().unwrap().variant().unwrap().id, 3);
    assert!(matches!(root.field_at(0), Err(DataError::AccessError(_))));
    drop(guard);
}

fn bind_payload(
    schema: &Schema,
    input: &[u8],
    location: &NodeLocation,
    ty: &TypeDescriptor,
    names: &mut Vec<(usize, String)>,
    metadata: &mut Vec<(std::ops::Range<usize>, usize)>,
) -> Result<NodeLocation, DataError> {
    let start = location._byte_offset;
    let malformed = || DataError::MalformedInput(data_location(start));
    let mut reader = Reader::new(input);
    reader.skip(start).map_err(|_| malformed())?;
    match reader.read_i8().map_err(|_| malformed())? {
        -3 if ty.nullable => return Ok(body_location(reader.get_cursor(), None, None, true)),
        -3 => return Err(DataError::SchemaMismatch(data_location(start))),
        -1 => {}
        0 | -2 => {
            return Ok(NodeLocation {
                _byte_offset: start,
                _reference_mode: Some(RootReferenceMode::Tracking),
                _type_source: NodeTypeSource::Envelope,
                _wire_type_id: None,
                _parent_null: false,
            });
        }
        _ => return Err(malformed()),
    }
    let offset = reader.get_cursor();
    let id = read_var_u32(&mut reader).map_err(|_| malformed())?;
    let mut result = group_type(names, schema, input, &mut reader, ty, id, offset, metadata)?;
    result._byte_offset = reader.get_cursor();
    Ok(result)
}

// 变体身份按定义类别解析；静态字段和父 group 已提供身份时不重读 envelope。
fn variant_envelope(
    schema: &Schema,
    input: &[u8],
    start: usize,
    ty: &TypeDescriptor,
    names: &mut Vec<(usize, String)>,
    metadata: &mut Vec<(std::ops::Range<usize>, usize)>,
) -> Result<NodeLocation, DataError> {
    let malformed = || DataError::MalformedInput(data_location(start));
    let mut reader = Reader::new(input);
    reader.skip(start).map_err(|_| malformed())?;
    match reader.read_i8().map_err(|_| malformed())? {
        -3 if ty.nullable => return Ok(body_location(reader.get_cursor(), None, None, true)),
        -3 => return Err(DataError::SchemaMismatch(data_location(start))),
        -1 | 0 => {}
        -2 => return Err(DataError::AccessError(data_location(start))),
        _ => return Err(malformed()),
    }
    let offset = reader.get_cursor();
    let id = read_var_u32(&mut reader).map_err(|_| malformed())?;
    if id != user_wire_id(schema, ty)? {
        return Err(DataError::SchemaMismatch(data_location(offset)));
    }
    if matches!(
        schema._data._descriptor.encoding.mode,
        crate::SchemaMode::Compatible
    ) && matches!(ty.kind, TypeKind::User(crate::UserIdentity::Name { .. }))
    {
        return compatible_type(schema, input, &mut reader, ty, metadata);
    }
    read_user_identity(&mut reader, ty, offset, names)?;
    Ok(body_location(reader.get_cursor(), None, None, false))
}

fn actual_wire_id(
    input: &[u8],
    location: &NodeLocation,
    ty: &TypeDescriptor,
) -> Result<u32, DataError> {
    if let Some(id) = location._wire_type_id {
        check_wire_id(ty, id, location._byte_offset)?;
        return Ok(id);
    }
    if matches!(location._type_source, NodeTypeSource::Envelope) {
        let mut reader = Reader::new(input);
        reader
            .skip(location._byte_offset)
            .and_then(|_| reader.read_i8())
            .map_err(|_| DataError::MalformedInput(data_location(location._byte_offset)))?;
        return read_var_u32(&mut reader)
            .map_err(|_| DataError::MalformedInput(data_location(location._byte_offset)));
    }
    scalar_id(ty)
}

fn supported_tree(schema: &Schema) -> Result<(), DataError> {
    let mut stack = vec![&schema._data._descriptor.root];
    let mut seen = std::collections::HashSet::new();
    while let Some(ty) = stack.pop() {
        match &ty.kind {
            TypeKind::User(identity) => {
                let definition = user_definition(schema, ty)
                    .ok_or_else(|| DataError::SchemaMismatch(data_location(0)))?;
                if let crate::UserDefinitionKind::Enum { variants }
                | crate::UserDefinitionKind::Union { variants } = &definition.kind
                {
                    if seen.insert(identity) {
                        for variant in variants {
                            if let Some(payload) = &variant.payload {
                                stack.push(payload);
                            }
                        }
                    }
                    continue;
                }
                match schema._data._descriptor.encoding.mode {
                    crate::SchemaMode::SameSchema {
                        version_hash: false,
                    } => {}
                    crate::SchemaMode::SameSchema { version_hash: true } => {
                        struct_version_hash(schema, ty)?;
                    }
                    crate::SchemaMode::Compatible => {}
                }
                if seen.insert(identity) {
                    let (fields, _) = struct_fields(schema, ty)?;
                    for field in fields {
                        stack.push(&field.ty);
                    }
                }
            }
            TypeKind::Map { key, value } => {
                stack.push(value);
                stack.push(key);
            }
            TypeKind::List(element) | TypeKind::Set(element) | TypeKind::Array { element, .. } => {
                stack.push(element)
            }
            TypeKind::Scalar(_) => {
                scalar_id(ty)?;
            }
        }
    }
    Ok(())
}

fn struct_fields<'a>(
    schema: &'a Schema,
    ty: &TypeDescriptor,
) -> Result<(&'a [crate::FieldDescriptor], &'a [usize]), DataError> {
    let TypeKind::User(identity) = &ty.kind else {
        return Err(DataError::AccessError(data_location(0)));
    };
    let index = schema
        ._data
        ._type_access
        ._definition_by_identity
        .get(identity)
        .ok_or_else(|| DataError::SchemaMismatch(data_location(0)))?;
    let crate::UserDefinitionKind::Struct {
        fields,
        physical_field_order,
    } = &schema._data._descriptor.definitions[*index].kind
    else {
        return Err(DataError::AccessError(data_location(0)));
    };
    Ok((fields, physical_field_order))
}

// 只接受完整相同的 scalar 字段契约；hash 仅检验 metadata framing，不代替字段比较。
fn checked_meta_name(
    reader: &mut Reader<'_>,
    type_name: bool,
    offset: usize,
) -> Result<String, DataError> {
    let malformed = || DataError::MalformedInput(data_location(offset));
    let header = reader.read_u8().map_err(|_| malformed())?;
    let encodings = if type_name {
        fory::__private::meta::TYPE_NAME_ENCODINGS
    } else {
        fory::__private::meta::NAMESPACE_ENCODINGS
    };
    let encoding = *encodings.get((header & 3) as usize).ok_or_else(malformed)?;
    let mut len = (header >> 2) as usize;
    if len == 63 {
        len = len
            .checked_add(read_var_u32(reader).map_err(|_| malformed())? as usize)
            .ok_or_else(malformed)?;
    }
    let bytes = reader.read_bytes(len).map_err(|_| malformed())?;
    let decoder = if type_name {
        &fory::__private::meta::TYPE_NAME_DECODER
    } else {
        &fory::__private::meta::NAMESPACE_DECODER
    };
    Ok(decoder
        .decode(bytes, encoding)
        .map_err(|_| malformed())?
        .original)
}

fn compatible_root(
    schema: &Schema,
    input: &[u8],
    start: usize,
) -> Result<(NodeLocation, Vec<(std::ops::Range<usize>, usize)>), DataError> {
    let ty = &schema._data._descriptor.root;
    let malformed = || DataError::MalformedInput(data_location(start));
    let mismatch = || DataError::SchemaMismatch(data_location(start));
    let mut reader = Reader::new(input);
    reader.skip(start).map_err(|_| malformed())?;
    let flag = reader.read_i8().map_err(|_| malformed())?;
    if flag == -3 {
        if !ty.nullable {
            return Err(mismatch());
        }
        return Ok((body_location(reader.get_cursor(), None, None, true), vec![]));
    }
    if flag != -1
        && !(flag == 0
            && matches!(
                schema._data._descriptor.encoding.root_reference,
                RootReferenceMode::Tracking
            ))
    {
        return Err(if flag == 0 || flag == -2 {
            DataError::NotImplemented("compatible tracked struct")
        } else {
            malformed()
        });
    }
    let named = matches!(ty.kind, TypeKind::User(crate::UserIdentity::Name { .. }));
    if read_var_u32(&mut reader).map_err(|_| malformed())? != if named { 30 } else { 28 } {
        return Err(mismatch());
    }
    let mut metadata = vec![];
    let location = compatible_type(schema, input, &mut reader, ty, &mut metadata)?;
    Ok((location, metadata))
}

fn compatible_field(
    schema: &Schema,
    input: &[u8],
    location: &NodeLocation,
    ty: &TypeDescriptor,
    metadata: &mut Vec<(std::ops::Range<usize>, usize)>,
) -> Result<NodeLocation, DataError> {
    let (null, body) = node_body(input, location, ty)?;
    if null {
        return Ok(body_location(body, None, None, true));
    }
    let mut reader = Reader::new(input);
    reader
        .skip(body)
        .map_err(|_| DataError::MalformedInput(data_location(body)))?;
    let id =
        read_var_u32(&mut reader).map_err(|_| DataError::MalformedInput(data_location(body)))?;
    let expected = if matches!(ty.kind, TypeKind::User(crate::UserIdentity::Name { .. })) {
        30
    } else {
        28
    };
    if id != expected {
        return Err(DataError::SchemaMismatch(data_location(body)));
    }
    compatible_type(schema, input, &mut reader, ty, metadata)
}

#[cfg(test)]
#[test]
fn named_metadata_binding_survives_validated_progress_lock() {
    let identity = crate::UserIdentity::Name {
        namespace: "x".into(),
        type_name: "x".into(),
    };
    let schema = Schema::compile(
        crate::SchemaDescriptor {
            encoding: crate::RootEncoding {
                protocol_version: crate::ProtocolVersion::Fory1_7,
                mode: crate::SchemaMode::Compatible,
                root_reference: RootReferenceMode::NullOnly,
            },
            key_semantics_version: crate::KEY_SEMANTICS_VERSION,
            root: TypeDescriptor {
                kind: TypeKind::User(identity.clone()),
                nullable: false,
                track_ref: false,
                integer_encoding: None,
            },
            definitions: vec![crate::UserDefinition {
                identity,
                kind: crate::UserDefinitionKind::Struct {
                    fields: vec![crate::FieldDescriptor {
                        id: Some(7),
                        name: "flag".into(),
                        ty: TypeDescriptor {
                            kind: TypeKind::Scalar(ScalarKind::Bool),
                            nullable: false,
                            track_ref: false,
                            integer_encoding: None,
                        },
                    }],
                    physical_field_order: vec![0],
                },
            }],
        },
        &crate::SchemaLimits::default(),
    )
    .unwrap();
    // TypeMeta 内两个相同名字仍各自 inline；它们不是动态元字符串槽。
    let body = [0xe1, 4, b'x', 4, b'x', 0xdc, 1];
    let mut hashed = body.to_vec();
    hashed.extend_from_slice(&[7, 0]);
    let hash = (fory::__private::util::murmurhash3_x64_128(&hashed, 47).0 as i64)
        .wrapping_shl(12)
        .wrapping_abs() as u64;
    let mut input = vec![255, 30, 0];
    input.extend_from_slice(&((hash & !0xfff) | 7).to_le_bytes());
    input.extend_from_slice(&body);
    input.push(1);
    let view = schema
        .view(&input, InputFormat::RootValue, &ParseLimits::default())
        .unwrap();
    let context = Rc::clone(&view._context);
    let validated = view.validate().unwrap();
    let guard = context._progress.try_borrow_mut().unwrap();
    let owned = validated.root().unwrap().to_owned().unwrap();
    assert!(
        owned
            .as_view()
            .unwrap()
            .field_at(0)
            .unwrap()
            .read_bool()
            .unwrap()
    );
    assert_eq!(validated._validated._metadata, vec![(3..18, 0)]);
    drop(guard);
}

fn compatible_type(
    schema: &Schema,
    input: &[u8],
    reader: &mut Reader<'_>,
    ty: &TypeDescriptor,
    metadata: &mut Vec<(std::ops::Range<usize>, usize)>,
) -> Result<NodeLocation, DataError> {
    let start = reader.get_cursor();
    let malformed = || DataError::MalformedInput(data_location(start));
    let mismatch = || DataError::SchemaMismatch(data_location(start));
    let variant_kind = match user_definition(schema, ty).map(|definition| &definition.kind) {
        Some(crate::UserDefinitionKind::Enum { .. }) => Some(1),
        Some(crate::UserDefinitionKind::Union { .. }) => Some(5),
        _ => None,
    };
    let (fields, order) = if variant_kind.is_some() {
        (&[][..], &[][..])
    } else {
        struct_fields(schema, ty)?
    };
    let TypeKind::User(identity) = &ty.kind else {
        unreachable!()
    };
    let definition = schema._data._type_access._definition_by_identity[identity];
    let named = matches!(identity, crate::UserIdentity::Name { .. });
    let token = read_var_u32(reader).map_err(|_| malformed())?;
    let slot = (token >> 1) as usize;
    if token & 1 != 0 {
        let (range, index) = metadata.get(slot).ok_or_else(malformed)?;
        if *index != definition {
            return Err(mismatch());
        }
        if range.start >= start {
            return Err(malformed());
        }
        let mut location = body_location(reader.get_cursor(), None, None, false);
        location._type_source = NodeTypeSource::Inherited {
            _metadata_slot: slot,
        };
        return Ok(location);
    }
    let meta_start = reader.get_cursor();
    let header = reader.read_i64().map_err(|_| malformed())?;
    if header & 0xf00 != 0 {
        return Err(malformed());
    }
    let mut len = (header & 255) as usize;
    if len == 255 {
        len = len
            .checked_add(read_var_u32(reader).map_err(|_| malformed())? as usize)
            .ok_or_else(malformed)?;
    }
    let meta_end = reader.get_cursor().checked_add(len).ok_or_else(malformed)?;
    let bytes = reader.read_bytes(len).map_err(|_| malformed())?;
    let mut hash_input = Vec::with_capacity(len.checked_add(2).ok_or_else(malformed)?);
    hash_input.extend_from_slice(bytes);
    hash_input.extend_from_slice(&((header as u64 & 0xfff) as u16).to_le_bytes());
    let hash = (fory::__private::util::murmurhash3_x64_128(&hash_input, 47).0 as i64)
        .wrapping_shl(12)
        .wrapping_abs() as u64;
    if header as u64 & !0xfff != hash & !0xfff {
        return Err(malformed());
    }
    let mut meta = Reader::new(bytes);
    let flags = meta.read_u8().map_err(|_| malformed())?;
    if if let Some(kind) = variant_kind {
        flags != kind
    } else {
        flags & 0xe0 != if named { 0xe0 } else { 0xc0 }
    } {
        return Err(mismatch());
    }
    let mut count = if variant_kind.is_some() {
        0
    } else {
        (flags & 31) as usize
    };
    if count == 31 {
        count = count
            .checked_add(read_var_u32(&mut meta).map_err(|_| malformed())? as usize)
            .ok_or_else(malformed)?;
    }
    if count != fields.len() {
        return Err(mismatch());
    }
    let TypeKind::User(identity) = &ty.kind else {
        unreachable!()
    };
    match identity {
        crate::UserIdentity::Numeric(id) => {
            if read_var_u32(&mut meta).map_err(|_| malformed())? != *id {
                return Err(mismatch());
            }
        }
        crate::UserIdentity::Name {
            namespace,
            type_name,
        } => {
            let ns = checked_meta_name(&mut meta, false, start)?;
            let name = checked_meta_name(&mut meta, true, start)?;
            if ns != *namespace || name != *type_name {
                return Err(mismatch());
            }
        }
    }
    for &index in order {
        let field = &fields[index];
        let header = meta.read_u8().map_err(|_| malformed())?;
        if (header & 2 != 0) != field.ty.nullable || (header & 1 != 0) != field.ty.track_ref {
            return Err(mismatch());
        }
        let encoding = header >> 6;
        let mut size = u32::from((header >> 2) & 15);
        if size == 15 {
            size = size
                .checked_add(read_var_u32(&mut meta).map_err(|_| malformed())?)
                .ok_or_else(malformed)?;
        }
        if encoding == 3 {
            if size > (1 << 29) - 1 {
                return Err(malformed());
            }
            if field.id != Some(size) {
                return Err(mismatch());
            }
        } else if field.id.is_some() {
            return Err(mismatch());
        }
        compatible_field_type(schema, &mut meta, &field.ty, start)?;
        if encoding != 3 {
            let name_bytes = meta
                .read_bytes((size as usize).checked_add(1).ok_or_else(malformed)?)
                .map_err(|_| malformed())?;
            let encoding = match encoding {
                0 => fory::__private::meta::Encoding::Utf8,
                1 => fory::__private::meta::Encoding::AllToLowerSpecial,
                2 => fory::__private::meta::Encoding::LowerUpperDigitSpecial,
                _ => unreachable!(),
            };
            let name = fory::__private::meta::FIELD_NAME_DECODER
                .decode(name_bytes, encoding)
                .map_err(|_| malformed())?;
            if name.original != field.name {
                return Err(mismatch());
            }
        }
    }
    if meta.get_cursor() != bytes.len() {
        return Err(malformed());
    }
    if let Some((range, index)) = metadata.get(slot) {
        if *index != definition || *range != (meta_start..meta_end) {
            return Err(malformed());
        }
    } else {
        if slot != metadata.len() {
            return Err(malformed());
        }
        metadata.push((meta_start..meta_end, definition));
    }
    let mut location = body_location(meta_end, None, None, false);
    location._type_source = NodeTypeSource::Inherited {
        _metadata_slot: slot,
    };
    let _ = input;
    Ok(location)
}

// 泛型树逐项精确比较，不调用上游递归解析，也不接受缺失泛型或隐式默认值。
fn compatible_field_type(
    schema: &Schema,
    reader: &mut Reader<'_>,
    ty: &TypeDescriptor,
    offset: usize,
) -> Result<(), DataError> {
    let mut stack = vec![(ty, false)];
    while let Some((ty, flags)) = stack.pop() {
        let header = if flags {
            read_var_u32(reader)
        } else {
            reader.read_u8().map(u32::from)
        }
        .map_err(|_| DataError::MalformedInput(data_location(offset)))?;
        if flags && ((header & 2 != 0) != ty.nullable || (header & 1 != 0) != ty.track_ref) {
            return Err(DataError::SchemaMismatch(data_location(offset)));
        }
        let id = if flags { header >> 2 } else { header };
        let expected = match user_definition(schema, ty).map(|definition| &definition.kind) {
            Some(crate::UserDefinitionKind::Enum { .. }) => 25,
            Some(crate::UserDefinitionKind::Union { .. }) => 33,
            _ => match &ty.kind {
                TypeKind::User(crate::UserIdentity::Numeric(_)) => 28,
                TypeKind::User(crate::UserIdentity::Name { .. }) => 30,
                TypeKind::Array { .. } => dense_type_id(ty)?.unwrap_or(22),
                _ => scalar_id(ty)?,
            },
        };
        if id != expected {
            return Err(DataError::SchemaMismatch(data_location(offset)));
        }
        if matches!(ty.kind, TypeKind::Array { .. }) && dense_type_id(ty)?.is_some() {
            continue;
        }
        match &ty.kind {
            TypeKind::List(element) | TypeKind::Set(element) | TypeKind::Array { element, .. } => {
                stack.push((element, true))
            }
            TypeKind::Map { key, value } => {
                stack.push((value, true));
                stack.push((key, true));
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
#[test]
fn compatible_metadata_publishes_real_range_and_validated_access_ignores_progress_lock() {
    let identity = crate::UserIdentity::Numeric(100);
    let schema = Schema::compile(
        crate::SchemaDescriptor {
            encoding: crate::RootEncoding {
                protocol_version: crate::ProtocolVersion::Fory1_7,
                mode: crate::SchemaMode::Compatible,
                root_reference: RootReferenceMode::NullOnly,
            },
            key_semantics_version: crate::KEY_SEMANTICS_VERSION,
            root: TypeDescriptor {
                kind: TypeKind::User(identity.clone()),
                nullable: false,
                track_ref: false,
                integer_encoding: None,
            },
            definitions: vec![crate::UserDefinition {
                identity,
                kind: crate::UserDefinitionKind::Struct {
                    fields: vec![crate::FieldDescriptor {
                        id: Some(7),
                        name: "flag".into(),
                        ty: TypeDescriptor {
                            kind: TypeKind::Scalar(ScalarKind::Bool),
                            nullable: false,
                            track_ref: false,
                            integer_encoding: None,
                        },
                    }],
                    physical_field_order: vec![0],
                },
            }],
        },
        &crate::SchemaLimits::default(),
    )
    .unwrap();
    // TypeMeta body: compatible numeric struct, one tagged bool field.
    let body = [0xc1, 100, 0xdc, 1];
    let mut hashed = body.to_vec();
    hashed.extend_from_slice(&[4, 0]);
    let hash = (fory::__private::util::murmurhash3_x64_128(&hashed, 47).0 as i64)
        .wrapping_shl(12)
        .wrapping_abs() as u64;
    let mut input = vec![255, 28, 0];
    input.extend_from_slice(&((hash & !0xfff) | 4).to_le_bytes());
    input.extend_from_slice(&body);
    input.push(1);
    let view = schema
        .view(&input, InputFormat::RootValue, &ParseLimits::default())
        .unwrap();
    let context = Rc::clone(&view._context);
    let validated = view.validate().unwrap();
    assert_eq!(validated._validated._metadata, vec![(3..15, 0)]);
    assert!(matches!(
        validated._validated._root._type_source,
        NodeTypeSource::Inherited { _metadata_slot: 0 }
    ));
    let guard = context._progress.try_borrow_mut().unwrap();
    let root = validated.root().unwrap();
    assert!(root.field_by_id(7).unwrap().read_bool().unwrap());
    assert!(
        root.fields()
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .1
            .read_bool()
            .unwrap()
    );
    assert!(Rc::ptr_eq(
        &context.validate(&schema, &input).unwrap(),
        &validated._validated
    ));
    drop(guard);
}

#[cfg(test)]
#[test]
fn nested_metadata_publication_moves_all_slots_and_validated_needs_no_progress_borrow() {
    let leaf = crate::UserIdentity::Numeric(200);
    let parent = crate::UserIdentity::Numeric(201);
    let user = |identity: crate::UserIdentity| TypeDescriptor {
        kind: TypeKind::User(identity),
        nullable: false,
        track_ref: false,
        integer_encoding: None,
    };
    let schema = Schema::compile(
        crate::SchemaDescriptor {
            encoding: crate::RootEncoding {
                protocol_version: crate::ProtocolVersion::Fory1_7,
                mode: crate::SchemaMode::Compatible,
                root_reference: RootReferenceMode::NullOnly,
            },
            key_semantics_version: crate::KEY_SEMANTICS_VERSION,
            root: user(parent.clone()),
            definitions: vec![
                crate::UserDefinition {
                    identity: parent,
                    kind: crate::UserDefinitionKind::Struct {
                        fields: vec![crate::FieldDescriptor {
                            id: Some(7),
                            name: "child".into(),
                            ty: user(leaf.clone()),
                        }],
                        physical_field_order: vec![0],
                    },
                },
                crate::UserDefinition {
                    identity: leaf,
                    kind: crate::UserDefinitionKind::Struct {
                        fields: vec![crate::FieldDescriptor {
                            id: Some(7),
                            name: "flag".into(),
                            ty: TypeDescriptor {
                                kind: TypeKind::Scalar(ScalarKind::Bool),
                                nullable: false,
                                track_ref: false,
                                integer_encoding: None,
                            },
                        }],
                        physical_field_order: vec![0],
                    },
                },
            ],
        },
        &crate::SchemaLimits::default(),
    )
    .unwrap();
    let mut input = vec![255, 28, 0];
    for (index, body) in [[0xc1, 201, 1, 0xdc, 28], [0xc1, 200, 1, 0xdc, 1]]
        .iter()
        .enumerate()
    {
        if index == 1 {
            input.extend_from_slice(&[28, 2]);
        }
        let mut hashed = body.to_vec();
        hashed.extend_from_slice(&[5, 0]);
        let hash = (fory::__private::util::murmurhash3_x64_128(&hashed, 47).0 as i64)
            .wrapping_shl(12)
            .wrapping_abs() as u64;
        input.extend_from_slice(&((hash & !0xfff) | 5).to_le_bytes());
        input.extend_from_slice(body);
    }
    input.push(1);
    let view = schema
        .view(&input, InputFormat::RootValue, &ParseLimits::default())
        .unwrap();
    let context = Rc::clone(&view._context);
    let root = view.root().unwrap();
    assert!(
        root.field_at(0)
            .unwrap()
            .field_at(0)
            .unwrap()
            .read_bool()
            .unwrap()
    );
    assert_eq!(context.metadata().unwrap().len(), 2);
    drop(root);
    let validated = view.validate().unwrap();
    assert_eq!(validated._validated._metadata.len(), 2);
    let guard = context._progress.try_borrow_mut().unwrap();
    let child = validated
        .root()
        .unwrap()
        .field_at(0)
        .unwrap()
        .to_owned()
        .unwrap();
    assert!(
        child
            .as_view()
            .unwrap()
            .field_at(0)
            .unwrap()
            .read_bool()
            .unwrap()
    );
    assert!(Rc::ptr_eq(
        &context.validate(&schema, &input).unwrap(),
        &validated._validated
    ));
    drop(guard);
    let mut descriptor = schema._data._descriptor.clone();
    descriptor.root = TypeDescriptor {
        kind: TypeKind::List(Box::new(descriptor.root)),
        nullable: false,
        track_ref: false,
        integer_encoding: None,
    };
    let schema = Schema::compile(descriptor, &crate::SchemaLimits::default()).unwrap();
    let mut list = vec![255, 22, 1, 8, 28, 0];
    list.extend_from_slice(&input[3..]);
    let view = schema
        .view(&list, InputFormat::RootValue, &ParseLimits::default())
        .unwrap();
    let context = Rc::clone(&view._context);
    let validated = view.validate().unwrap();
    assert_eq!(validated._validated._metadata.len(), 2);
    let guard = context._progress.try_borrow_mut().unwrap();
    let root = validated.root().unwrap();
    assert_eq!(root.len().unwrap(), 1);
    assert!(
        root.element_at(0)
            .unwrap()
            .field_at(0)
            .unwrap()
            .field_at(0)
            .unwrap()
            .read_bool()
            .unwrap()
    );
    assert!(
        root.elements()
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .field_at(0)
            .unwrap()
            .field_at(0)
            .unwrap()
            .read_bool()
            .unwrap()
    );
    drop(guard);
}

// Canonical Binary 是 #[fory(bytes)] Vec<u8>，List 是普通 Vec<T>，Array 是
// [T; N]。独立于实际 wire ID；alias/custom 不尝试备用指纹。
fn type_fingerprint(ty: &TypeDescriptor) -> Result<String, DataError> {
    let mut text = String::new();
    // action 0 写节点，1/2 写闭括号/Map 分隔符；用户只写 UNKNOWN，
    // 不展开定义。泛型参数的 ref/nullable 位按 derive 强制为零。
    let mut stack = vec![(0u8, ty, true)];
    while let Some((action, ty, outer)) = stack.pop() {
        if action != 0 {
            text.push(if action == 1 { ']' } else { '|' });
            continue;
        }
        let id = match &ty.kind {
            TypeKind::User(_) => 0,
            TypeKind::Array { element, .. } if !element.nullable => match element.kind {
                TypeKind::Scalar(ScalarKind::Bool) => 43,
                TypeKind::Scalar(ScalarKind::I8) => 44,
                TypeKind::Scalar(ScalarKind::I16) => 45,
                TypeKind::Scalar(ScalarKind::I32) => 46,
                TypeKind::Scalar(ScalarKind::I64) => 47,
                TypeKind::Scalar(ScalarKind::U16) => 49,
                TypeKind::Scalar(ScalarKind::U32) => 50,
                TypeKind::Scalar(ScalarKind::U64) => 51,
                TypeKind::Scalar(ScalarKind::F32) => 55,
                TypeKind::Scalar(ScalarKind::F64) => 56,
                _ => 22,
            },
            _ => scalar_id(ty)?,
        };
        text.push_str(&format!(
            "{id},{},{}",
            u8::from(outer && ty.track_ref),
            u8::from(outer && ty.nullable)
        ));
        match &ty.kind {
            TypeKind::List(element) | TypeKind::Set(element) | TypeKind::Array { element, .. }
                if matches!(id, 22 | 23) =>
            {
                text.push('[');
                stack.push((1, ty, false));
                stack.push((0, element, false));
            }
            TypeKind::Map { key, value } => {
                text.push('[');
                stack.push((1, ty, false));
                stack.push((0, value, false));
                stack.push((2, ty, false));
                stack.push((0, key, false));
            }
            _ => {}
        }
    }
    Ok(text)
}

// Schema 规范指纹；调用方负责使用相同的声明语法，别名不额外尝试 UNKNOWN 指纹。
fn struct_version_hash(schema: &Schema, ty: &TypeDescriptor) -> Result<i32, DataError> {
    let (fields, _) = struct_fields(schema, ty)?;
    let mut parts = Vec::with_capacity(fields.len());
    for field in fields {
        let name = field
            .id
            .map(|id| id.to_string())
            .unwrap_or_else(|| fory::__private::util::to_snake_case(&field.name));
        let text = format!("{name},{};", type_fingerprint(&field.ty)?);
        parts.push((field.id, name, text));
    }
    parts.sort_unstable_by(|a, b| match (a.0, b.0) {
        (Some(a), Some(b)) => a.cmp(&b),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.1.cmp(&b.1),
    });
    let fingerprint: String = parts.into_iter().map(|(_, _, text)| text).collect();
    Ok(
        (fory::__private::util::murmurhash3_x64_128(fingerprint.as_bytes(), 47).0 & 0xffff_ffff)
            as i32,
    )
}

#[cfg(test)]
#[test]
fn recursive_canonical_versions_match_derive_and_published_binary_ignores_lock() {
    let scalar = |kind, integer_encoding| TypeDescriptor {
        kind: TypeKind::Scalar(kind),
        nullable: false,
        track_ref: false,
        integer_encoding,
    };
    let list = |element| TypeDescriptor {
        kind: TypeKind::List(Box::new(element)),
        nullable: false,
        track_ref: false,
        integer_encoding: None,
    };
    let user = |id| TypeDescriptor {
        kind: TypeKind::User(crate::UserIdentity::Numeric(id)),
        nullable: false,
        track_ref: false,
        integer_encoding: None,
    };
    let root = user(735);
    let schema = Schema::compile(
        crate::SchemaDescriptor {
            encoding: crate::RootEncoding {
                protocol_version: crate::ProtocolVersion::Fory1_7,
                mode: crate::SchemaMode::SameSchema { version_hash: true },
                root_reference: RootReferenceMode::NullOnly,
            },
            key_semantics_version: crate::KEY_SEMANTICS_VERSION,
            root: root.clone(),
            definitions: vec![crate::UserDefinition {
                identity: crate::UserIdentity::Numeric(735),
                kind: crate::UserDefinitionKind::Struct {
                    fields: vec![crate::FieldDescriptor {
                        id: None,
                        name: "value".into(),
                        ty: scalar(ScalarKind::Binary, None),
                    }],
                    physical_field_order: vec![0],
                },
            }],
        },
        &crate::SchemaLimits::default(),
    )
    .unwrap();
    let mut input = vec![1, 255, 27, 223, 5];
    input.extend_from_slice(&struct_version_hash(&schema, &root).unwrap().to_le_bytes());
    input.extend_from_slice(&[3, 0, 128, 255]);
    let view = schema
        .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
        .unwrap();
    let context = Rc::clone(&view._context);
    let validated = view.validate().unwrap();
    let guard = context._progress.try_borrow_mut().unwrap();
    let root = validated.root().unwrap();
    assert_eq!(
        root.field_at(0).unwrap().read_binary().unwrap().as_bytes(),
        &[0, 128, 255]
    );
    assert_eq!(
        struct_version_hash(&schema, root.node_type).unwrap(),
        i32::from_le_bytes(input[5..9].try_into().unwrap())
    );
    assert_eq!(
        type_fingerprint(&list(TypeDescriptor {
            nullable: true,
            track_ref: true,
            ..scalar(ScalarKind::I64, Some(IntegerEncoding::Tagged))
        }))
        .unwrap(),
        "22,0,0[8,0,0]"
    );
    assert_eq!(
        type_fingerprint(&TypeDescriptor {
            nullable: true,
            track_ref: true,
            ..list(scalar(ScalarKind::String, None))
        })
        .unwrap(),
        "22,1,1[21,0,0]"
    );
    let owned = root.field_at(0).unwrap().to_owned().unwrap();
    assert_eq!(
        owned.as_view().unwrap().read_binary().unwrap().as_bytes(),
        &[0, 128, 255]
    );
    drop(guard);
    // 深层泛型只经显式栈；User 终止于 UNKNOWN，不追入递归定义。
    let mut deep = user(735);
    for _ in 0..2048 {
        deep = list(deep);
    }
    let fingerprint = type_fingerprint(&deep).unwrap();
    assert_eq!(fingerprint.matches('[').count(), 2048);
    assert!(fingerprint.contains("[0,0,0]"));
}

fn struct_body(
    schema: &Schema,
    input: &[u8],
    ty: &TypeDescriptor,
    body: usize,
    _validate: bool,
) -> Result<usize, DataError> {
    if !matches!(
        schema._data._descriptor.encoding.mode,
        crate::SchemaMode::SameSchema { version_hash: true }
    ) {
        return Ok(body);
    }
    let mut reader = Reader::new(input);
    reader
        .skip(body)
        .map_err(|_| DataError::MalformedInput(data_location(body)))?;
    let actual = reader
        .read_i32()
        .map_err(|_| DataError::MalformedInput(data_location(body)))?;
    // 版本也是静态 canonical List codec 的选择证据，lazy 字段读取不能跳过。
    if actual != struct_version_hash(schema, ty)? {
        return Err(DataError::SchemaMismatch(data_location(body)));
    }
    Ok(reader.get_cursor())
}

#[cfg(test)]
#[test]
fn struct_fields_preserve_publication_and_access_without_progress_borrows() {
    let identity = crate::UserIdentity::Numeric(100);
    let schema = Schema::compile(
        crate::SchemaDescriptor {
            encoding: crate::RootEncoding {
                protocol_version: crate::ProtocolVersion::Fory1_7,
                mode: crate::SchemaMode::SameSchema {
                    version_hash: false,
                },
                root_reference: RootReferenceMode::NullOnly,
            },
            key_semantics_version: crate::KEY_SEMANTICS_VERSION,
            root: TypeDescriptor {
                kind: TypeKind::User(identity.clone()),
                nullable: false,
                track_ref: false,
                integer_encoding: None,
            },
            definitions: vec![crate::UserDefinition {
                identity,
                kind: crate::UserDefinitionKind::Struct {
                    fields: vec![crate::FieldDescriptor {
                        id: Some(7),
                        name: "flag".into(),
                        ty: TypeDescriptor {
                            kind: TypeKind::Scalar(ScalarKind::Bool),
                            nullable: false,
                            track_ref: false,
                            integer_encoding: None,
                        },
                    }],
                    physical_field_order: vec![0],
                },
            }],
        },
        &crate::SchemaLimits::default(),
    )
    .unwrap();
    let input = [255, 27, 100, 1];
    let view = schema
        .view(&input, InputFormat::RootValue, &ParseLimits::default())
        .unwrap();
    let context = Rc::clone(&view._context);
    let root = view.root().unwrap();
    let mut fields = root.fields().unwrap();
    let guard = context._progress.try_borrow_mut().unwrap();
    assert!(matches!(
        fields.next().unwrap(),
        Err(DataError::AccessError(_))
    ));
    assert!(fields.next().is_none());
    assert!(matches!(root.field_at(0), Err(DataError::AccessError(_))));
    drop(guard);
    drop(fields);
    drop(root);
    let validated = view.validate().unwrap();
    let state = context.validate(&schema, &input).unwrap();
    assert!(Rc::ptr_eq(&state, &validated._validated));
    let guard = context._progress.try_borrow_mut().unwrap();
    let root = validated.root().unwrap();
    let child = root.field_by_id(7).unwrap();
    assert!(child.read_bool().unwrap());
    assert!(Rc::ptr_eq(&child._context, &context));
    assert!(Rc::ptr_eq(&child._validated, &state));
    assert!(root.field_by_name("flag").unwrap().read_bool().unwrap());
    assert!(
        root.fields()
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .1
            .read_bool()
            .unwrap()
    );
    assert!(
        child
            .to_owned()
            .unwrap()
            .as_view()
            .unwrap()
            .read_bool()
            .unwrap()
    );
    drop(guard);
}

#[cfg(test)]
#[test]
fn canonical_static_list_published_reads_and_bad_headers_keep_error_contract() {
    let user = TypeDescriptor {
        kind: TypeKind::User(crate::UserIdentity::Numeric(747)),
        nullable: false,
        track_ref: false,
        integer_encoding: None,
    };
    let list = TypeDescriptor {
        kind: TypeKind::List(Box::new(TypeDescriptor {
            kind: TypeKind::Scalar(ScalarKind::I64),
            integer_encoding: Some(IntegerEncoding::Tagged),
            nullable: false,
            track_ref: false,
        })),
        nullable: false,
        track_ref: false,
        integer_encoding: None,
    };
    let schema = Schema::compile(
        crate::SchemaDescriptor {
            encoding: crate::RootEncoding {
                protocol_version: crate::ProtocolVersion::Fory1_7,
                mode: crate::SchemaMode::SameSchema { version_hash: true },
                root_reference: RootReferenceMode::NullOnly,
            },
            key_semantics_version: crate::KEY_SEMANTICS_VERSION,
            root: user.clone(),
            definitions: vec![crate::UserDefinition {
                identity: crate::UserIdentity::Numeric(747),
                kind: crate::UserDefinitionKind::Struct {
                    fields: vec![crate::FieldDescriptor {
                        id: None,
                        name: "value".into(),
                        ty: list,
                    }],
                    physical_field_order: vec![0],
                },
            }],
        },
        &crate::SchemaLimits::default(),
    )
    .unwrap();
    let mut input = vec![255, 27, 235, 5];
    input.extend_from_slice(&struct_version_hash(&schema, &user).unwrap().to_le_bytes());
    input.extend_from_slice(&[2, 12]);
    let mut writer = fory::Writer::from_buffer(&mut input);
    writer.write_tagged_i64(i64::MIN);
    writer.write_tagged_i64(i64::MAX);
    let view = schema
        .view(&input, InputFormat::RootValue, &ParseLimits::default())
        .unwrap();
    let context = Rc::clone(&view._context);
    let validated = view.validate().unwrap();
    let guard = context._progress.try_borrow_mut().unwrap();
    let root = validated.root().unwrap();
    let list = root.field_at(0).unwrap();
    assert_eq!(
        list.elements()
            .unwrap()
            .map(|n| n.unwrap().read_i64().unwrap())
            .collect::<Vec<_>>(),
        [i64::MIN, i64::MAX]
    );
    let owned = list.element_at(1).unwrap().to_owned().unwrap();
    assert_eq!(owned.as_view().unwrap().read_i64().unwrap(), i64::MAX);
    drop(guard);
    drop(list);
    drop(root);
    drop(validated);
    for (count, header) in [(2, 128), (3, 12)] {
        let mut bad = input.clone();
        bad[8] = count;
        bad[9] = header;
        let view = schema
            .view(&bad, InputFormat::RootValue, &ParseLimits::default())
            .unwrap();
        let survivor = view.root().unwrap().to_owned().unwrap();
        let error = view.validate().err().unwrap();
        assert!(matches!(error, DataError::MalformedInput(_)));
        assert_eq!(
            format!("{error:?}"),
            format!("{:?}", survivor.as_view().err().unwrap())
        );
    }
}

fn field_location(
    schema: &Schema,
    offset: usize,
    ty: &TypeDescriptor,
) -> Result<NodeLocation, DataError> {
    if ty.track_ref
        && matches!(
            ty.kind,
            TypeKind::Scalar(ScalarKind::I32 | ScalarKind::I64 | ScalarKind::U32 | ScalarKind::U64)
        )
    {
        // Plain integer codecs omit the flag; Rc/Arc codecs own one. The schema
        // does not retain that carrier distinction for a static field.
        return Err(DataError::NotImplemented(
            "ambiguous tracked static integer field framing",
        ));
    }
    // SameSchema 的静态字段不写 type info；nullable 独立控制实际 ref marker。
    if let TypeKind::List(element) = &ty.kind {
        // 无字段 codec 信息时，primitive List 可来自 generic carrier 或显式 dense Vec。
        if matches!(
            schema._data._descriptor.encoding.mode,
            crate::SchemaMode::SameSchema {
                version_hash: false
            }
        ) && !element.nullable
            && matches!(
                element.kind,
                TypeKind::Scalar(
                    ScalarKind::Bool
                        | ScalarKind::I8
                        | ScalarKind::I16
                        | ScalarKind::I32
                        | ScalarKind::I64
                        | ScalarKind::U8
                        | ScalarKind::U16
                        | ScalarKind::U32
                        | ScalarKind::U64
                        | ScalarKind::F32
                        | ScalarKind::F64
                )
            )
        {
            return Err(DataError::NotImplemented(
                "ambiguous static primitive list field wire selection",
            ));
        }
    }
    // Canonical [T; N] 的 primitive codec 唯一确定，字段省略 typeinfo。
    let wire = if matches!(ty.kind, TypeKind::Array { .. }) {
        dense_type_id(ty)?
    } else {
        None
    };
    Ok(body_location(
        offset,
        if ty.track_ref {
            Some(RootReferenceMode::Tracking)
        } else if ty.nullable {
            Some(RootReferenceMode::NullOnly)
        } else {
            None
        },
        wire,
        false,
    ))
}

#[cfg(test)]
#[test]
fn static_dense_array_publication_keeps_fixed_reads_without_progress_borrow() {
    let array = TypeDescriptor {
        kind: TypeKind::Array {
            element: Box::new(TypeDescriptor {
                kind: TypeKind::Scalar(ScalarKind::I64),
                nullable: false,
                track_ref: false,
                integer_encoding: Some(IntegerEncoding::Tagged),
            }),
            length: Some(2),
        },
        nullable: true,
        track_ref: false,
        integer_encoding: None,
    };
    let user = TypeDescriptor {
        kind: TypeKind::User(crate::UserIdentity::Numeric(743)),
        nullable: false,
        track_ref: false,
        integer_encoding: None,
    };
    let schema = Schema::compile(
        crate::SchemaDescriptor {
            encoding: crate::RootEncoding {
                protocol_version: crate::ProtocolVersion::Fory1_7,
                mode: crate::SchemaMode::SameSchema {
                    version_hash: false,
                },
                root_reference: RootReferenceMode::NullOnly,
            },
            key_semantics_version: crate::KEY_SEMANTICS_VERSION,
            root: user,
            definitions: vec![crate::UserDefinition {
                identity: crate::UserIdentity::Numeric(743),
                kind: crate::UserDefinitionKind::Struct {
                    fields: vec![crate::FieldDescriptor {
                        id: None,
                        name: "value".into(),
                        ty: array,
                    }],
                    physical_field_order: vec![0],
                },
            }],
        },
        &crate::SchemaLimits::default(),
    )
    .unwrap();
    let mut input = vec![255, 27, 231, 5, 255, 16];
    input.extend_from_slice(&i64::MIN.to_le_bytes());
    input.extend_from_slice(&i64::MAX.to_le_bytes());
    let view = schema
        .view(&input, InputFormat::RootValue, &ParseLimits::default())
        .unwrap();
    let context = Rc::clone(&view._context);
    let validated = view.validate().unwrap();
    let guard = context._progress.try_borrow_mut().unwrap();
    let root = validated.root().unwrap();
    let array = root.field_at(0).unwrap();
    assert_eq!(array._location._wire_type_id, Some(47));
    assert_eq!(array.element_at(1).unwrap().read_i64().unwrap(), i64::MAX);
    assert_eq!(
        array
            .elements()
            .unwrap()
            .map(|n| n.unwrap().read_i64().unwrap())
            .collect::<Vec<_>>(),
        [i64::MIN, i64::MAX]
    );
    let owned = array.element_at(0).unwrap().to_owned().unwrap();
    assert_eq!(owned.as_view().unwrap().read_i64().unwrap(), i64::MIN);
    drop(guard);
    drop(array);
    drop(root);
    drop(validated);
    // 字节长度必须整除宽度，错误一旦发现后保留到所有后续访问。
    input[5] = 15;
    let view = schema
        .view(&input, InputFormat::RootValue, &ParseLimits::default())
        .unwrap();
    let root = view.root().unwrap();
    let child = root.field_at(0).unwrap();
    let error = child.len().unwrap_err();
    assert!(matches!(error, DataError::MalformedInput(_)));
    assert_eq!(
        format!("{error:?}"),
        format!("{:?}", root.field_at(0).err().unwrap())
    );
    drop(child);
    drop(root);
    assert_eq!(
        format!("{error:?}"),
        format!("{:?}", view.validate().err().unwrap())
    );
}

fn field_locations<'a>(
    target: Option<usize>,
    names: &mut Vec<(usize, String)>,
    metadata: &mut Vec<(std::ops::Range<usize>, usize)>,
    schema: &'a Schema,
    input: &[u8],
    location: &NodeLocation,
    ty: &TypeDescriptor,
    validate: bool,
) -> Result<Vec<(&'a crate::FieldDescriptor, NodeLocation)>, DataError> {
    let (fields, order) = struct_fields(schema, ty)?;
    let mut objects = objects_for_node(schema, input, location, ty)?;
    let bound;
    let location = if matches!(location._type_source, NodeTypeSource::Envelope)
        && matches!(
            schema._data._descriptor.encoding.mode,
            crate::SchemaMode::Compatible
        ) {
        let start = location._byte_offset;
        if input.get(start) == Some(&254) {
            return Err(DataError::AccessError(data_location(start)));
        }
        if input.get(start) != Some(&253) {
            let mut reader = Reader::new(input);
            reader
                .skip(start + 1)
                .map_err(|_| DataError::MalformedInput(data_location(start)))?;
            let id = read_var_u32(&mut reader)
                .map_err(|_| DataError::MalformedInput(data_location(start)))?;
            if id != user_wire_id(schema, ty)? {
                return Err(DataError::SchemaMismatch(data_location(start)));
            }
            bound = compatible_type(schema, input, &mut reader, ty, metadata)?;
            &bound
        } else {
            location
        }
    } else {
        location
    };
    let (null, mut cursor) = named_node_body(input, location, ty, names)?;
    if null {
        return Err(DataError::AccessError(data_location(location._byte_offset)));
    }
    cursor = struct_body(schema, input, ty, cursor, validate)?;
    let mut locations = Vec::with_capacity(if target.is_some() { 1 } else { fields.len() });
    for &index in order {
        let field = &fields[index];
        let mut location = field_location(schema, cursor, &field.ty)?;
        if field.ty.track_ref
            && matches!(
                schema._data._descriptor.encoding.mode,
                crate::SchemaMode::Compatible
            )
            && matches!(
                user_definition(schema, &field.ty).map(|definition| &definition.kind),
                Some(crate::UserDefinitionKind::Struct { .. })
            )
        {
            location._type_source = NodeTypeSource::Envelope;
        }
        if matches!(
            schema._data._descriptor.encoding.mode,
            crate::SchemaMode::Compatible
        ) && !matches!(location._reference_mode, Some(RootReferenceMode::Tracking))
            && matches!(
                user_definition(schema, &field.ty).map(|definition| &definition.kind),
                Some(crate::UserDefinitionKind::Struct { .. })
            )
        {
            location = compatible_field(schema, input, &location, &field.ty, metadata)?;
        }
        if target == Some(index) {
            return Ok(vec![(field, location)]);
        }
        cursor = scan_objects(
            schema,
            input,
            &location,
            &field.ty,
            validate,
            metadata,
            names,
            None,
            &mut objects,
        )?;
        if target.is_none() {
            locations.push((index, location));
        }
    }
    locations.sort_unstable_by_key(|(index, _)| *index);
    Ok(locations
        .into_iter()
        .map(|(index, location)| (&fields[index], location))
        .collect())
}

#[cfg(test)]
#[test]
fn map_iteration_is_linear_and_publication_needs_no_progress_borrow() {
    let scalar = || TypeDescriptor {
        nullable: false,
        track_ref: false,
        integer_encoding: None,
        kind: TypeKind::Scalar(ScalarKind::String),
    };
    let schema = Schema::compile(
        crate::SchemaDescriptor {
            encoding: crate::RootEncoding {
                protocol_version: crate::ProtocolVersion::Fory1_7,
                mode: crate::SchemaMode::Compatible,
                root_reference: RootReferenceMode::NullOnly,
            },
            key_semantics_version: crate::KEY_SEMANTICS_VERSION,
            root: TypeDescriptor {
                nullable: false,
                track_ref: false,
                integer_encoding: None,
                kind: TypeKind::Map {
                    key: Box::new(scalar()),
                    value: Box::new(scalar()),
                },
            },
            definitions: vec![],
        },
        &crate::SchemaLimits::default(),
    )
    .unwrap();
    let values: std::collections::BTreeMap<_, _> = (0..4096)
        .map(|i| (format!("{i:04}"), format!("v{i}")))
        .collect();
    let input = fory::Fory::builder()
        .xlang(true)
        .build()
        .serialize(&values)
        .unwrap();
    let view = schema
        .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
        .unwrap();
    let context = Rc::clone(&view._context);
    let root = view.root().unwrap();
    let mut iter = root.entries().unwrap();
    let guard = context._progress.try_borrow_mut().unwrap();
    assert!(matches!(
        iter.next().unwrap(),
        Err(DataError::AccessError(_))
    ));
    assert!(iter.next().is_none());
    drop(guard);
    drop(iter);
    SEQUENCE_OPERATIONS.with(|count| count.set((0, 0)));
    for pair in root.entries().unwrap() {
        let (key, value) = pair.unwrap();
        assert!(Rc::ptr_eq(&key._context, &value._context));
        assert!(Rc::ptr_eq(&key._context, &context));
    }
    assert_eq!(SEQUENCE_OPERATIONS.with(|count| count.get()), (0, 8192));
    drop(root);
    let validated = view.validate().unwrap();
    let state = context.validate(&schema, &input).unwrap();
    assert!(Rc::ptr_eq(&state, &validated._validated));
    let guard = context._progress.try_borrow_mut().unwrap();
    assert!(Rc::ptr_eq(
        &context.validate(&schema, &input).unwrap(),
        &state
    ));
    let root = validated.root().unwrap();
    assert_eq!(root.len().unwrap(), 4096);
    SEQUENCE_OPERATIONS.with(|count| count.set((0, 0)));
    for pair in root.entries().unwrap() {
        let (key, value) = pair.unwrap();
        key.read_string().unwrap();
        value.read_string().unwrap();
        assert!(Rc::ptr_eq(&key._validated, &value._validated));
    }
    assert_eq!(SEQUENCE_OPERATIONS.with(|count| count.get()), (0, 8192));
    drop(guard);
}

// Fory 1.7.3 core.rs: ordinary scalar roots always carry a ref flag and type ID.
// Compatible metadata and same-schema version hashes occur only in structural codecs.
fn envelope(
    input: &[u8],
    start: usize,
    ty: &TypeDescriptor,
    mode: RootReferenceMode,
) -> Result<(bool, usize), DataError> {
    let mut reader = Reader::new(input);
    reader
        .skip(start)
        .map_err(|_| DataError::MalformedInput(data_location(start)))?;
    let flag = reader
        .read_i8()
        .map_err(|_| DataError::MalformedInput(data_location(start)))?;
    if flag == -3 {
        if !ty.nullable {
            return Err(DataError::SchemaMismatch(data_location(start)));
        }
        return Ok((true, reader.get_cursor()));
    }
    match (mode, flag) {
        (_, -1) => {}
        (RootReferenceMode::Tracking, 0) => {}
        (RootReferenceMode::Tracking, -2) => {
            return Err(DataError::MalformedInput(data_location(start)));
        }
        _ => return Err(DataError::MalformedInput(data_location(start))),
    }
    let offset = reader.get_cursor();
    let id =
        read_var_u32(&mut reader).map_err(|_| DataError::MalformedInput(data_location(offset)))?;
    check_wire_id(ty, id, offset)?;
    read_user_identity(&mut reader, ty, offset, &mut vec![])?;
    Ok((false, reader.get_cursor()))
}

fn read_user_identity(
    reader: &mut Reader<'_>,
    ty: &TypeDescriptor,
    offset: usize,
    names: &mut Vec<(usize, String)>,
) -> Result<(), DataError> {
    match &ty.kind {
        TypeKind::User(crate::UserIdentity::Numeric(expected)) => {
            let id = read_var_u32(reader)
                .map_err(|_| DataError::MalformedInput(data_location(offset)))?;
            if id != *expected {
                return Err(DataError::SchemaMismatch(data_location(offset)));
            }
        }
        TypeKind::User(crate::UserIdentity::Name {
            namespace,
            type_name,
        }) => {
            for expected in [namespace, type_name] {
                let position = reader.get_cursor();
                let header = read_var_u32(reader)
                    .map_err(|_| DataError::MalformedInput(data_location(offset)))?;
                if header & 1 != 0 {
                    let slot = (header >> 1)
                        .checked_sub(1)
                        .ok_or_else(|| DataError::MalformedInput(data_location(offset)))?
                        as usize;
                    let (defined, name) = names
                        .get(slot)
                        .ok_or_else(|| DataError::MalformedInput(data_location(offset)))?;
                    if *defined >= position {
                        return Err(DataError::MalformedInput(data_location(offset)));
                    }
                    if name != expected {
                        return Err(DataError::SchemaMismatch(data_location(offset)));
                    }
                    continue;
                }
                let len = (header >> 1) as usize;
                let wire_hash = if len > 16 {
                    Some(
                        reader
                            .read_i64()
                            .map_err(|_| DataError::MalformedInput(data_location(offset)))?,
                    )
                } else {
                    None
                };
                let encoding = if let Some(hash) = wire_hash {
                    Ok(hash as u8)
                } else if len > 0 {
                    reader.read_u8()
                } else {
                    Ok(0)
                }
                .map_err(|_| DataError::MalformedInput(data_location(offset)))?;
                let encoding = match encoding {
                    0 => fory::__private::meta::Encoding::Utf8,
                    1 => fory::__private::meta::Encoding::LowerSpecial,
                    2 => fory::__private::meta::Encoding::LowerUpperDigitSpecial,
                    3 => fory::__private::meta::Encoding::FirstToLowerSpecial,
                    4 => fory::__private::meta::Encoding::AllToLowerSpecial,
                    _ => return Err(DataError::MalformedInput(data_location(offset))),
                };
                let bytes = reader
                    .read_bytes(len)
                    .map_err(|_| DataError::MalformedInput(data_location(offset)))?;
                if let Some(wire_hash) = wire_hash {
                    // 固定 1.7.3 meta_string_resolver:55-64；先校验原编码字节，再解码并比较完整身份。
                    let mut hash = (fory::__private::util::murmurhash3_x64_128(bytes, 47).0 as i64)
                        .wrapping_abs();
                    if hash == 0 {
                        hash = 256;
                    }
                    let expected_hash =
                        ((hash as u64 & 0xffff_ffff_ffff_ff00) as i64) | encoding as i64;
                    if wire_hash != expected_hash {
                        return Err(DataError::MalformedInput(data_location(offset)));
                    }
                }
                let name = fory::__private::meta::NAMESPACE_DECODER
                    .decode(bytes, encoding)
                    .map_err(|_| DataError::MalformedInput(data_location(offset)))?;
                if name.original != *expected {
                    return Err(DataError::SchemaMismatch(data_location(offset)));
                }
                // 重读同一 wire 事件不新增槽；重复内容的不同 inline 事件仍各占一槽。
                if let Err(index) = names.binary_search_by_key(&position, |(defined, _)| *defined) {
                    names.insert(index, (position, name.original));
                }
            }
        }
        _ => {}
    }
    Ok(())
}

impl RootState {
    fn objects_at(
        &self,
        schema: &Schema,
        input: &[u8],
        offset: usize,
    ) -> Result<Vec<(TypeDescriptor, NodeLocation)>, DataError> {
        #[cfg(test)]
        IDENTITY_REPLAYS.with(|count| count.set(count.get() + 1));
        if let Some(state) = self._validated.get() {
            return Ok(state
                ._objects
                .iter()
                .map(|(ty, at)| (ty.clone(), copy_location(at)))
                .collect());
        }
        let start = match self._format {
            InputFormat::XlangMessage => 1,
            InputFormat::RootValue => 0,
        };
        let root = NodeLocation {
            _byte_offset: start,
            _reference_mode: Some(schema._data._descriptor.encoding.root_reference),
            _type_source: NodeTypeSource::Envelope,
            _wire_type_id: None,
            _parent_null: false,
        };
        let mut objects = vec![];
        scan_objects(
            schema,
            input,
            &root,
            &schema._data._descriptor.root,
            false,
            &mut vec![],
            &mut vec![],
            Some(
                offset
                    .checked_add(1)
                    .ok_or_else(|| DataError::MalformedInput(data_location(offset)))?,
            ),
            &mut objects,
        )?;
        let mut progress = self
            ._progress
            .try_borrow_mut()
            .map_err(|_| DataError::AccessError(data_location(offset)))?;
        if let RootProgress::Parsing { _objects, .. } = &mut *progress {
            if objects.len() > _objects.len() {
                let known = _objects.len();
                _objects.extend(
                    objects[known..]
                        .iter()
                        .map(|(ty, at)| (ty.clone(), copy_location(at))),
                );
            }
        }
        Ok(objects)
    }

    fn identity(
        &self,
        schema: &Schema,
        input: &[u8],
        location: &NodeLocation,
        ty: &TypeDescriptor,
        reference: bool,
    ) -> Result<Option<usize>, DataError> {
        if location._parent_null
            || !matches!(location._reference_mode, Some(RootReferenceMode::Tracking))
        {
            return Ok(None);
        }
        let flag = *input
            .get(location._byte_offset)
            .ok_or_else(|| DataError::MalformedInput(data_location(location._byte_offset)))?;
        if flag != if reference { 254 } else { 0 } {
            return Ok(None);
        }
        if self._validated.get().is_none() {
            let progress = self
                ._progress
                .try_borrow()
                .map_err(|_| DataError::AccessError(data_location(location._byte_offset)))?;
            if let RootProgress::Parsing { _objects, .. } = &*progress {
                if reference {
                    let mut reader = Reader::new(input);
                    reader.skip(location._byte_offset + 1).map_err(|_| {
                        DataError::MalformedInput(data_location(location._byte_offset))
                    })?;
                    let id = read_var_u32(&mut reader).map_err(|_| {
                        DataError::MalformedInput(data_location(location._byte_offset))
                    })? as usize;
                    if let Some((target, at)) = _objects.get(id) {
                        if at._byte_offset >= location._byte_offset {
                            return Err(DataError::MalformedInput(data_location(
                                location._byte_offset,
                            )));
                        }
                        if !same_object_type(target, ty) {
                            return Err(DataError::SchemaMismatch(data_location(
                                location._byte_offset,
                            )));
                        }
                        return Ok(Some(id));
                    }
                } else if let Ok(id) =
                    _objects.binary_search_by_key(&location._byte_offset, |(_, at)| at._byte_offset)
                {
                    if !same_object_type(&_objects[id].0, ty) {
                        return Err(DataError::SchemaMismatch(data_location(
                            location._byte_offset,
                        )));
                    }
                    return Ok(Some(id));
                }
            }
        }
        let pending;
        let objects = if let Some(state) = self._validated.get() {
            &state._objects
        } else {
            pending = self.objects_at(schema, input, location._byte_offset)?;
            &pending
        };
        if reference {
            let mut reader = Reader::new(input);
            reader
                .skip(location._byte_offset + 1)
                .map_err(|_| DataError::MalformedInput(data_location(location._byte_offset)))?;
            let id = read_var_u32(&mut reader)
                .map_err(|_| DataError::MalformedInput(data_location(location._byte_offset)))?
                as usize;
            let (target, at) = objects
                .get(id)
                .ok_or_else(|| DataError::MalformedInput(data_location(location._byte_offset)))?;
            if at._byte_offset >= location._byte_offset {
                return Err(DataError::MalformedInput(data_location(
                    location._byte_offset,
                )));
            }
            if !same_object_type(target, ty) {
                return Err(DataError::SchemaMismatch(data_location(
                    location._byte_offset,
                )));
            }
            Ok(Some(id))
        } else {
            Ok(objects
                .binary_search_by_key(&location._byte_offset, |(_, at)| at._byte_offset)
                .ok())
        }
    }
    // 每个访问边界最多重建一次前缀；迭代器随后持有局部事件表，next 不重放根。
    fn names_before(
        &self,
        schema: &Schema,
        input: &[u8],
        location: &NodeLocation,
    ) -> Result<Vec<(usize, String)>, DataError> {
        let mut names = vec![];
        if !matches!(
            schema._data._descriptor.encoding.mode,
            crate::SchemaMode::SameSchema { .. }
        ) || !schema
            ._data
            ._descriptor
            .definitions
            .iter()
            .any(|definition| matches!(definition.identity, crate::UserIdentity::Name { .. }))
        {
            return Ok(names);
        }
        let start = match self._format {
            InputFormat::XlangMessage => 1,
            InputFormat::RootValue => 0,
        };
        let root = NodeLocation {
            _byte_offset: start,
            _reference_mode: Some(schema._data._descriptor.encoding.root_reference),
            _type_source: NodeTypeSource::Envelope,
            _wire_type_id: None,
            _parent_null: false,
        };
        scan_named_node(
            schema,
            input,
            &root,
            &schema._data._descriptor.root,
            false,
            &mut vec![],
            &mut names,
            Some(location._byte_offset),
        )?;
        Ok(names)
    }
    fn metadata(&self) -> Result<Vec<(std::ops::Range<usize>, usize)>, DataError> {
        if let Some(state) = self._validated.get() {
            return Ok(state._metadata.clone());
        }
        let progress = self
            ._progress
            .try_borrow()
            .map_err(|_| DataError::AccessError(data_location(0)))?;
        match &*progress {
            RootProgress::Parsing { _metadata, .. } => Ok(_metadata.clone()),
            RootProgress::Failed(error) => Err(error.clone()),
            RootProgress::Published => Err(DataError::NotImplemented("missing published metadata")),
        }
    }
    fn store_metadata(
        &self,
        metadata: &[(std::ops::Range<usize>, usize)],
    ) -> Result<(), DataError> {
        if self._validated.get().is_some() {
            return Ok(());
        }
        let mut progress = self
            ._progress
            .try_borrow_mut()
            .map_err(|_| DataError::AccessError(data_location(0)))?;
        if let RootProgress::Parsing { _metadata, .. } = &mut *progress {
            if metadata.len() > _metadata.len() {
                _metadata.extend_from_slice(&metadata[_metadata.len()..]);
            }
        }
        Ok(())
    }
    fn validate(&self, schema: &Schema, input: &[u8]) -> Result<Rc<ValidatedState>, DataError> {
        if let Some(state) = self._validated.get() {
            return Ok(Rc::clone(state));
        }
        self.check()?;
        let start = match self._format {
            InputFormat::XlangMessage => 1,
            InputFormat::RootValue => 0,
        };
        let ty = &schema._data._descriptor.root;
        let result = (|| {
            if !matches!(ty.kind, TypeKind::Scalar(_))
                || (matches!(
                    schema._data._descriptor.encoding.root_reference,
                    RootReferenceMode::Tracking
                ) && input.get(start) == Some(&0))
            {
                let location = NodeLocation {
                    _byte_offset: start,
                    _reference_mode: Some(schema._data._descriptor.encoding.root_reference),
                    _type_source: NodeTypeSource::Envelope,
                    _wire_type_id: None,
                    _parent_null: false,
                };
                let mut metadata = vec![];
                let mut objects = vec![];
                let end = scan_objects(
                    schema,
                    input,
                    &location,
                    ty,
                    true,
                    &mut metadata,
                    &mut vec![],
                    None,
                    &mut objects,
                )?;
                if end != input.len() {
                    return Err(DataError::MalformedInput(data_location(end)));
                }
                let mut progress = self
                    ._progress
                    .try_borrow_mut()
                    .map_err(|_| DataError::AccessError(data_location(start)))?;
                if let RootProgress::Parsing {
                    _metadata,
                    _objects,
                    ..
                } = &mut *progress
                {
                    *_metadata = metadata;
                    *_objects = objects;
                }
                return Ok(());
            }
            if matches!(
                user_definition(schema, ty).map(|definition| &definition.kind),
                Some(
                    crate::UserDefinitionKind::Enum { .. }
                        | crate::UserDefinitionKind::Union { .. }
                )
            ) {
                let mut metadata = vec![];
                let mut names = vec![];
                let location =
                    variant_envelope(schema, input, start, ty, &mut names, &mut metadata)?;
                let end = scan_named_node(
                    schema,
                    input,
                    &location,
                    ty,
                    true,
                    &mut metadata,
                    &mut names,
                    None,
                )?;
                if end != input.len() {
                    return Err(DataError::MalformedInput(data_location(end)));
                }
                self.store_metadata(&metadata)?;
                return Ok(());
            }
            if matches!(
                schema._data._descriptor.encoding.mode,
                crate::SchemaMode::Compatible
            ) && matches!(ty.kind, TypeKind::User(_))
            {
                let (location, mut metadata) = compatible_root(schema, input, start)?;
                let end = scan_node_metadata(schema, input, &location, ty, true, &mut metadata)?;
                if end != input.len() {
                    return Err(DataError::MalformedInput(data_location(end)));
                }
                let mut progress = self
                    ._progress
                    .try_borrow_mut()
                    .map_err(|_| DataError::AccessError(data_location(start)))?;
                if let RootProgress::Parsing { _metadata, .. } = &mut *progress {
                    *_metadata = metadata;
                }
                return Ok(());
            }
            let (null, body) = envelope(
                input,
                start,
                ty,
                schema._data._descriptor.encoding.root_reference,
            )?;
            let mut reader = Reader::new(input);
            reader
                .skip(body)
                .map_err(|_| DataError::MalformedInput(data_location(body)))?;
            if !null {
                let result = match scalar_id(ty)? {
                    22 | 23 | 24 | 27 | 29 => {
                        let location = NodeLocation {
                            _byte_offset: start,
                            _parent_null: false,
                            _wire_type_id: None,
                            _reference_mode: Some(schema._data._descriptor.encoding.root_reference),
                            _type_source: NodeTypeSource::Envelope,
                        };
                        let mut metadata = vec![];
                        let end =
                            scan_node_metadata(schema, input, &location, ty, true, &mut metadata)?;
                        self.store_metadata(&metadata)?;
                        reader.skip(end - body)
                    }
                    1 => reader.read_u8().and_then(|value| {
                        if value <= 1 {
                            Ok(())
                        } else {
                            Err(fory::Error::invalid_data("invalid bool"))
                        }
                    }),
                    2 | 9 => reader.skip(1),
                    3 | 10 => reader.skip(2),
                    4 | 11 | 19 => reader.skip(4),
                    6 | 13 | 20 => reader.skip(8),
                    5 | 12 => read_var_u32(&mut reader).map(|_| ()),
                    7 | 14 => read_var_u64(&mut reader).map(|_| ()),
                    8 => reader.read_tagged_i64().map(|_| ()),
                    15 => reader.read_tagged_u64().map(|_| ()),
                    39 => decode_date(&mut reader).map(|_| ()),
                    38 => decode_timestamp(&mut reader).map(|_| ()),
                    37 => decode_duration(&mut reader).map(|_| ()),
                    21 | 41 => {
                        let (tag, range) = content_range(input, body, scalar_id(ty)? == 21)?;
                        if scalar_id(ty)? == 21 {
                            validate_string_payload(&input[range.clone()], tag, range.start)?;
                        }
                        reader.skip(range.end - body)
                    }
                    _ => return Err(DataError::NotImplemented("scalar validation")),
                };
                result.map_err(|_| DataError::MalformedInput(data_location(body)))?;
            }
            if reader.get_cursor() != input.len() {
                return Err(DataError::MalformedInput(data_location(
                    reader.get_cursor(),
                )));
            }
            Ok(())
        })();
        self.finish(result)?;
        let mut progress = self
            ._progress
            .try_borrow_mut()
            .map_err(|_| DataError::AccessError(data_location(start)))?;
        match &*progress {
            RootProgress::Failed(error) => return Err(error.clone()),
            RootProgress::Parsing { _root: Some(_), .. } => {}
            _ => return Err(DataError::NotImplemented("validation progress")),
        }
        // All checks succeeded. Move the tables exactly once; no mutable duplicate survives.
        let RootProgress::Parsing {
            _root,
            _objects,
            _metadata,
        } = &mut *progress
        else {
            return Err(DataError::NotImplemented("validation progress"));
        };
        let root = _root
            .take()
            .ok_or(DataError::NotImplemented("missing root location"))?;
        let objects = std::mem::take(_objects);
        let metadata = std::mem::take(_metadata);
        *progress = RootProgress::Published;
        let state = self._validated.get_or_init(|| {
            Rc::new(ValidatedState {
                _root: root,
                _objects: objects,
                _metadata: metadata,
            })
        });
        Ok(Rc::clone(state))
    }

    pub(crate) fn open(
        schema: &Schema,
        input: &[u8],
        format: InputFormat,
        limits: &ParseLimits,
    ) -> Result<Rc<Self>, DataError> {
        if input.len() > limits.max_input_bytes {
            return Err(DataError::ResourceLimit(data_location(0)));
        }
        supported_tree(schema)?;
        let start = match format {
            InputFormat::XlangMessage => {
                if input.first() != Some(&1) {
                    return Err(DataError::MalformedInput(data_location(0)));
                }
                1
            }
            InputFormat::RootValue => 0,
        };
        let mode = schema._data._descriptor.encoding.root_reference;
        let root_ty = &schema._data._descriptor.root;
        let (root, metadata) = if matches!(
            user_definition(schema, root_ty).map(|definition| &definition.kind),
            Some(crate::UserDefinitionKind::Enum { .. } | crate::UserDefinitionKind::Union { .. })
        ) {
            let mut metadata = vec![];
            if input.get(start) == Some(&254)
                || (input.get(start) == Some(&0) && !matches!(mode, RootReferenceMode::Tracking))
            {
                return Err(DataError::MalformedInput(data_location(start)));
            }
            let location =
                variant_envelope(schema, input, start, root_ty, &mut vec![], &mut metadata)?;
            if input.get(start) == Some(&0) && matches!(mode, RootReferenceMode::Tracking) {
                (
                    NodeLocation {
                        _byte_offset: start,
                        _parent_null: false,
                        _wire_type_id: None,
                        _reference_mode: Some(mode),
                        _type_source: NodeTypeSource::Envelope,
                    },
                    metadata,
                )
            } else {
                (location, metadata)
            }
        } else if matches!(
            schema._data._descriptor.encoding.mode,
            crate::SchemaMode::Compatible
        ) && matches!(root_ty.kind, TypeKind::User(_))
        {
            let (location, metadata) = compatible_root(schema, input, start)?;
            if input.get(start) == Some(&0) {
                (
                    NodeLocation {
                        _byte_offset: start,
                        _reference_mode: Some(mode),
                        _type_source: NodeTypeSource::Envelope,
                        _wire_type_id: None,
                        _parent_null: false,
                    },
                    metadata,
                )
            } else {
                (location, metadata)
            }
        } else {
            envelope(input, start, root_ty, mode)?;
            (
                NodeLocation {
                    _byte_offset: start,
                    _parent_null: false,
                    _wire_type_id: None,
                    _reference_mode: Some(mode),
                    _type_source: NodeTypeSource::Envelope,
                },
                vec![],
            )
        };
        Ok(Rc::new(Self {
            _format: format,
            _limits: limits.clone(),
            _progress: RefCell::new(RootProgress::Parsing {
                _root: Some(root),
                _objects: vec![],
                _metadata: metadata,
            }),
            _validated: OnceCell::new(),
        }))
    }

    fn check(&self) -> Result<(), DataError> {
        if self._validated.get().is_some() {
            return Ok(());
        }
        let progress = self
            ._progress
            .try_borrow()
            .map_err(|_| DataError::AccessError(data_location(0)))?;
        match &*progress {
            RootProgress::Failed(error) => Err(error.clone()),
            _ => Ok(()),
        }
    }

    fn finish<T>(&self, result: Result<T, DataError>) -> Result<T, DataError> {
        if self._validated.get().is_some() {
            return result;
        }
        if let Err(error @ (DataError::MalformedInput(_) | DataError::SchemaMismatch(_))) = &result
        {
            let mut progress = self
                ._progress
                .try_borrow_mut()
                .map_err(|_| DataError::AccessError(data_location(0)))?;
            if let RootProgress::Failed(first) = &*progress {
                return Err(first.clone());
            }
            *progress = RootProgress::Failed(error.clone());
        }
        result
    }
}

fn read_scalar<T>(
    this: &NodeView<'_>,
    kind: ScalarKind,
    read: impl FnOnce(&mut Reader<'_>) -> Result<T, fory::Error>,
) -> Result<T, DataError> {
    this._context.check()?;
    if !matches!(&this.node_type.kind, TypeKind::Scalar(actual) if std::mem::discriminant(actual) == std::mem::discriminant(&kind))
    {
        return Err(DataError::AccessError(data_location(
            this._location._byte_offset,
        )));
    }
    let result = (|| {
        let (null, body) = node_body(this._backing.input(), &this._location, this.node_type)?;
        if null {
            return Err(DataError::AccessError(data_location(
                this._location._byte_offset,
            )));
        }
        let mut reader = Reader::new(this._backing.input());
        reader
            .skip(body)
            .map_err(|_| DataError::MalformedInput(data_location(body)))?;
        read(&mut reader).map_err(|_| DataError::MalformedInput(data_location(body)))
    })();
    this._context.finish(result)
}

impl ObjectIdInner {
    pub(super) fn fmt(this: &ObjectId, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ObjectId")
            .field("local", &this._local)
            .finish_non_exhaustive()
    }

    pub(super) fn eq(this: &ObjectId, other: &ObjectId) -> bool {
        Rc::ptr_eq(&this._context, &other._context) && this._local == other._local
    }

    pub(super) fn hash<H: std::hash::Hasher>(this: &ObjectId, state: &mut H) {
        // The retained owner prevents address reuse while this identity remains alive.
        Rc::as_ptr(&this._context).hash(state);
        this._local.hash(state);
    }
}

#[cfg(test)]
#[test]
fn identities_retain_distinct_root_owners_and_hash_by_association() {
    use std::collections::{HashSet, hash_map::DefaultHasher};
    use std::hash::Hasher;
    let make = || {
        Rc::new(RootState {
            _format: InputFormat::RootValue,
            _limits: ParseLimits::default(),
            _progress: RefCell::new(RootProgress::Parsing {
                _root: None,
                _objects: vec![],
                _metadata: vec![],
            }),
            _validated: OnceCell::new(),
        })
    };
    let root = make();
    let weak = Rc::downgrade(&root);
    let id = ObjectId {
        _context: Rc::clone(&root),
        _local: 7,
    };
    let clone = id.clone();
    let other = ObjectId {
        _context: make(),
        _local: 7,
    };
    let local = ObjectId {
        _context: Rc::clone(&root),
        _local: 8,
    };
    assert_eq!(id, clone);
    assert_ne!(id, other);
    assert_ne!(id, local);
    let hash = |id: &ObjectId| {
        let mut state = DefaultHasher::new();
        id.hash(&mut state);
        state.finish()
    };
    assert_eq!(hash(&id), hash(&clone));
    let mut expected = DefaultHasher::new();
    Rc::as_ptr(&root).hash(&mut expected);
    7u64.hash(&mut expected);
    assert_eq!(hash(&id), expected.finish());
    let mut ids = HashSet::new();
    ids.insert(id.clone());
    ids.insert(clone.clone());
    ids.insert(other.clone());
    ids.insert(local.clone());
    assert_eq!(ids.len(), 3);
    // Debug is safe even with an outstanding mutable progress borrow.
    let guard = root._progress.try_borrow_mut().unwrap();
    assert_eq!(format!("{id:?}"), "ObjectId { local: 7, .. }");
    drop(guard);
    drop(root);
    drop(local);
    drop(ids);
    drop(id);
    assert!(weak.upgrade().is_some());
    drop(clone);
    assert!(weak.upgrade().is_none());
}

#[cfg(test)]
#[test]
fn sequence_iteration_counts_one_header_and_one_scan_per_element() {
    for kind in [ScalarKind::Bool, ScalarKind::String] {
        let schema = Schema::compile(
            crate::SchemaDescriptor {
                encoding: crate::RootEncoding {
                    protocol_version: crate::ProtocolVersion::Fory1_7,
                    mode: crate::SchemaMode::Compatible,
                    root_reference: RootReferenceMode::NullOnly,
                },
                key_semantics_version: crate::KEY_SEMANTICS_VERSION,
                root: TypeDescriptor {
                    nullable: false,
                    track_ref: false,
                    integer_encoding: None,
                    kind: TypeKind::List(Box::new(TypeDescriptor {
                        nullable: false,
                        track_ref: false,
                        integer_encoding: None,
                        kind: TypeKind::Scalar(kind),
                    })),
                },
                definitions: vec![],
            },
            &crate::SchemaLimits::default(),
        )
        .unwrap();
        let fory = fory::Fory::builder().xlang(true).build();
        let count = 4096;
        let input = if matches!(kind, ScalarKind::Bool) {
            fory.serialize(&vec![true; count]).unwrap()
        } else {
            fory.serialize(&vec!["value".to_owned(); count]).unwrap()
        };
        let view = schema
            .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
            .unwrap();
        let root = view.root().unwrap();
        SEQUENCE_OPERATIONS.with(|counts| counts.set((0, 0)));
        assert_eq!(
            root.elements().unwrap().map(|node| node.unwrap()).count(),
            count
        );
        assert_eq!(SEQUENCE_OPERATIONS.with(|counts| counts.get()), (1, count));
        if matches!(kind, ScalarKind::Bool) {
            SEQUENCE_OPERATIONS.with(|counts| counts.set((0, 0)));
            assert!(root.element_at(count - 1).unwrap().read_bool().unwrap());
            assert_eq!(SEQUENCE_OPERATIONS.with(|counts| counts.get()), (1, 0));
        }
        drop(root);
        let validated = view.validate().unwrap();
        let root = validated.root().unwrap();
        let guard = validated._context._progress.try_borrow_mut().unwrap();
        SEQUENCE_OPERATIONS.with(|counts| counts.set((0, 0)));
        assert_eq!(
            root.elements().unwrap().map(|node| node.unwrap()).count(),
            count
        );
        assert_eq!(SEQUENCE_OPERATIONS.with(|counts| counts.get()), (1, count));
        drop(guard);
    }
}

#[cfg(test)]
#[test]
fn dense_integer_reads_keep_wire_provenance_without_published_progress() {
    let element = TypeDescriptor {
        kind: TypeKind::Scalar(ScalarKind::I64),
        nullable: false,
        track_ref: false,
        integer_encoding: Some(IntegerEncoding::Tagged),
    };
    let schema = Schema::compile(
        crate::SchemaDescriptor {
            encoding: crate::RootEncoding {
                protocol_version: crate::ProtocolVersion::Fory1_7,
                mode: crate::SchemaMode::Compatible,
                root_reference: RootReferenceMode::NullOnly,
            },
            key_semantics_version: crate::KEY_SEMANTICS_VERSION,
            root: TypeDescriptor {
                kind: TypeKind::List(Box::new(element.clone())),
                nullable: false,
                track_ref: false,
                integer_encoding: None,
            },
            definitions: vec![],
        },
        &crate::SchemaLimits::default(),
    )
    .unwrap();
    let input = fory::Fory::builder()
        .xlang(true)
        .build()
        .serialize(&vec![i64::MIN; 4096])
        .unwrap();
    let view = schema
        .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
        .unwrap();
    let context = Rc::clone(&view._context);
    let root = view.root().unwrap();
    SEQUENCE_OPERATIONS.with(|count| count.set((0, 0)));
    let owned = root.element_at(4095).unwrap().to_owned().unwrap();
    assert_eq!(SEQUENCE_OPERATIONS.with(|count| count.get()), (1, 0));
    let mut fixed = element.clone();
    fixed.integer_encoding = Some(IntegerEncoding::Fixed);
    assert!(!same_object_type(&element, &fixed));
    drop(root);
    let validated = view.validate().unwrap();
    let guard = context._progress.try_borrow_mut().unwrap();
    let root = validated.root().unwrap();
    assert_eq!(root.element_at(4095).unwrap().read_i64().unwrap(), i64::MIN);
    SEQUENCE_OPERATIONS.with(|count| count.set((0, 0)));
    for node in root.elements().unwrap() {
        let node = node.unwrap();
        assert_eq!(node.read_i64().unwrap(), i64::MIN);
        assert!(node.object_id().unwrap().is_none());
        assert_eq!(
            node.node_type.integer_encoding,
            Some(IntegerEncoding::Tagged)
        );
    }
    assert_eq!(SEQUENCE_OPERATIONS.with(|count| count.get()), (1, 4096));
    assert_eq!(owned.as_view().unwrap().read_i64().unwrap(), i64::MIN);
    let edge = NodeLocation {
        _byte_offset: 0,
        _parent_null: false,
        _wire_type_id: Some(47),
        _reference_mode: Some(RootReferenceMode::Tracking),
        _type_source: NodeTypeSource::Envelope,
    };
    assert!(dense_scalar_id(&edge, &element).is_none());
    let edge = NodeLocation {
        _wire_type_id: None,
        ..edge
    };
    assert!(matches!(
        node_body(&[254, 0], &edge, &element),
        Err(DataError::AccessError(_))
    ));
    drop(guard);
    drop(root);
    drop(validated);
    drop(input);
    drop(schema);
    assert_eq!(owned.as_view().unwrap().read_i64().unwrap(), i64::MIN);
}

#[cfg(test)]
#[test]
fn sequence_children_keep_declaration_context_and_published_access() {
    let element = TypeDescriptor {
        nullable: true,
        track_ref: false,
        integer_encoding: None,
        kind: TypeKind::Scalar(ScalarKind::String),
    };
    let schema = std::sync::Arc::new(
        Schema::compile(
            crate::SchemaDescriptor {
                encoding: crate::RootEncoding {
                    protocol_version: crate::ProtocolVersion::Fory1_7,
                    mode: crate::SchemaMode::Compatible,
                    root_reference: RootReferenceMode::NullOnly,
                },
                key_semantics_version: crate::KEY_SEMANTICS_VERSION,
                root: TypeDescriptor {
                    nullable: false,
                    track_ref: false,
                    integer_encoding: None,
                    kind: TypeKind::List(Box::new(element)),
                },
                definitions: vec![],
            },
            &crate::SchemaLimits::default(),
        )
        .unwrap(),
    );
    let input = bytes::Bytes::from(
        fory::Fory::builder()
            .xlang(true)
            .build()
            .serialize(&vec![Some("one".to_owned()), None])
            .unwrap(),
    );
    let pointer = input.as_ptr();
    let owner = schema
        .view_owned(input, InputFormat::XlangMessage, &ParseLimits::default())
        .unwrap();
    let context = Rc::clone(&owner._context);
    let view = owner.as_view().unwrap();
    let root = view.root().unwrap();
    let child = root.element_at(0).unwrap();
    assert!(std::ptr::eq(
        child.node_type(),
        sequence_element(root.node_type()).unwrap()
    ));
    assert!(Rc::ptr_eq(&context, &child._context));
    assert!(matches!(
        child._location._type_source,
        NodeTypeSource::Schema
    ));
    let retained = child.to_owned().unwrap();
    assert_eq!(retained._backing._input.as_ptr(), pointer);
    let guard = context._progress.try_borrow_mut().unwrap();
    assert!(matches!(root.len(), Err(DataError::AccessError(_))));
    drop(guard);
    assert_eq!(root.len().unwrap(), 2);
    drop(child);
    drop(root);
    drop(view);
    let validated = owner.validate().unwrap();
    let guard = context._progress.try_borrow_mut().unwrap();
    let view = validated.as_view().unwrap();
    let root = view.root().unwrap();
    assert_eq!(root.len().unwrap(), 2);
    assert_eq!(root.elements().unwrap().count(), 2);
    let child = root.element_at(0).unwrap();
    assert_eq!(child.read_string().unwrap().as_str(), "one");
    assert!(Rc::ptr_eq(&context, &child._context));
    assert!(Rc::ptr_eq(&child._validated, &validated._validated));
    assert!(root.element_at(1).unwrap().is_null().unwrap());
    assert_eq!(
        retained.as_view().unwrap().read_string().unwrap().as_str(),
        "one"
    );
    drop(guard);
}

#[cfg(test)]
#[test]
fn temporal_publication_preserves_context_and_decodes_without_progress_borrows() {
    for kind in [
        ScalarKind::Date,
        ScalarKind::Timestamp,
        ScalarKind::Duration,
    ] {
        let schema = Schema::compile(
            crate::SchemaDescriptor {
                encoding: crate::RootEncoding {
                    protocol_version: crate::ProtocolVersion::Fory1_7,
                    mode: crate::SchemaMode::Compatible,
                    root_reference: RootReferenceMode::NullOnly,
                },
                key_semantics_version: crate::KEY_SEMANTICS_VERSION,
                root: TypeDescriptor {
                    nullable: false,
                    track_ref: false,
                    integer_encoding: None,
                    kind: TypeKind::Scalar(kind),
                },
                definitions: vec![],
            },
            &crate::SchemaLimits::default(),
        )
        .unwrap();
        let fory = fory::Fory::builder().xlang(true).build();
        let input = match kind {
            ScalarKind::Date => fory.serialize(&fory::Date::from_epoch_days(-1)).unwrap(),
            ScalarKind::Timestamp => fory
                .serialize(&fory::Timestamp::new(-1, 999_999_999).unwrap())
                .unwrap(),
            _ => fory
                .serialize(&fory::Duration::new(0, -1).unwrap())
                .unwrap(),
        };
        let mut limits = ParseLimits {
            max_input_bytes: input.len(),
        };
        let view = schema
            .view(&input, InputFormat::XlangMessage, &limits)
            .unwrap();
        let context = Rc::clone(&view._context);
        limits.max_input_bytes = 0;
        assert_eq!(limits.max_input_bytes, 0);
        assert_eq!(context._limits.max_input_bytes, input.len());
        let guard = context._progress.try_borrow_mut().unwrap();
        assert!(matches!(
            context.validate(&schema, &input),
            Err(DataError::AccessError(_))
        ));
        drop(guard);
        let validated = view.validate().unwrap();
        assert!(Rc::ptr_eq(&context, &validated._context));
        let owned = validated.root().unwrap().to_owned().unwrap();
        assert!(Rc::ptr_eq(&context, &owned._context));
        assert!(matches!(
            *context._progress.try_borrow().unwrap(),
            RootProgress::Published
        ));
        let guard = context._progress.try_borrow_mut().unwrap();
        assert!(Rc::ptr_eq(
            &context.validate(&schema, &input).unwrap(),
            &validated._validated
        ));
        let node = owned.as_view().unwrap();
        match kind {
            ScalarKind::Date => assert_eq!(node.read_date().unwrap().epoch_days(), -1),
            ScalarKind::Timestamp => assert_eq!(node.read_timestamp().unwrap().seconds(), -1),
            _ => assert_eq!(node.read_duration().unwrap().seconds(), -1),
        }
        drop(guard);
    }
}

#[cfg(test)]
#[test]
fn content_holders_publication_and_borrow_conflicts() {
    for kind in [ScalarKind::String, ScalarKind::Binary] {
        let schema = Schema::compile(
            crate::SchemaDescriptor {
                encoding: crate::RootEncoding {
                    protocol_version: crate::ProtocolVersion::Fory1_7,
                    mode: crate::SchemaMode::Compatible,
                    root_reference: RootReferenceMode::NullOnly,
                },
                key_semantics_version: crate::KEY_SEMANTICS_VERSION,
                root: TypeDescriptor {
                    nullable: false,
                    track_ref: false,
                    integer_encoding: None,
                    kind: TypeKind::Scalar(kind),
                },
                definitions: vec![],
            },
            &crate::SchemaLimits::default(),
        )
        .unwrap();
        let fory = fory::Fory::builder().xlang(true).build();
        let input = if matches!(kind, ScalarKind::String) {
            fory.serialize(&"borrowed".to_owned()).unwrap()
        } else {
            fory.serialize(&vec![0u8, 128, 255]).unwrap()
        };
        let view = schema
            .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
            .unwrap();
        let context = Rc::clone(&view._context);
        let node = view.root().unwrap();
        let guard = context._progress.try_borrow_mut().unwrap();
        if matches!(kind, ScalarKind::String) {
            assert!(matches!(node.read_string(), Err(DataError::AccessError(_))));
        } else {
            assert!(matches!(node.read_binary(), Err(DataError::AccessError(_))));
        }
        drop(guard);
        drop(node);
        let validated = view.validate().unwrap();
        let guard = context._progress.try_borrow_mut().unwrap();
        let node = validated.root().unwrap();
        if matches!(kind, ScalarKind::String) {
            let content = node.read_string().unwrap();
            assert_eq!(content.as_str(), "borrowed");
            assert_eq!(content.as_str().as_ptr(), input[input.len() - 8..].as_ptr());
        } else {
            assert_eq!(node.read_binary().unwrap().as_bytes(), &[0, 128, 255]);
        }
        drop(guard);
        let mut trailing = input.clone();
        trailing.push(0);
        let view = schema
            .view(
                &trailing,
                InputFormat::XlangMessage,
                &ParseLimits::default(),
            )
            .unwrap();
        let context = Rc::clone(&view._context);
        let node = view.root().unwrap();
        if matches!(kind, ScalarKind::String) {
            let holder = node.read_string().unwrap();
            assert!(matches!(
                context.validate(&schema, &trailing),
                Err(DataError::MalformedInput(_))
            ));
            assert_eq!(holder.as_str(), "borrowed");
        } else {
            let holder = node.read_binary().unwrap();
            assert!(matches!(
                context.validate(&schema, &trailing),
                Err(DataError::MalformedInput(_))
            ));
            assert_eq!(holder.as_bytes(), &[0, 128, 255]);
        }
        assert!(matches!(node.is_null(), Err(DataError::MalformedInput(_))));
    }
}

#[cfg(test)]
#[test]
fn validation_publication_reuses_owner_and_moves_tables() {
    let schema = Schema::compile(
        crate::SchemaDescriptor {
            encoding: crate::RootEncoding {
                protocol_version: crate::ProtocolVersion::Fory1_7,
                mode: crate::SchemaMode::Compatible,
                root_reference: RootReferenceMode::NullOnly,
            },
            key_semantics_version: crate::KEY_SEMANTICS_VERSION,
            root: TypeDescriptor {
                nullable: false,
                track_ref: false,
                integer_encoding: None,
                kind: TypeKind::Scalar(ScalarKind::Bool),
            },
            definitions: vec![],
        },
        &crate::SchemaLimits::default(),
    )
    .unwrap();
    let input = fory::Fory::builder()
        .xlang(true)
        .build()
        .serialize(&true)
        .unwrap();
    let view = schema
        .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
        .unwrap();
    let context = Rc::clone(&view._context);
    let id = ObjectId {
        _context: Rc::clone(&context),
        _local: 1,
    };
    let survivor = view.root().unwrap().to_owned().unwrap();
    let second = ValueView {
        _backing: RootBacking::Borrowed {
            _schema: &schema,
            _input: &input,
        },
        _context: Rc::clone(&context),
    };
    let guard = context._progress.try_borrow_mut().unwrap();
    assert!(matches!(
        context.validate(&schema, &input),
        Err(DataError::AccessError(_))
    ));
    assert!(context._validated.get().is_none());
    drop(guard);
    let validated_second = second.validate().unwrap();
    let validated = view.validate().unwrap();
    assert!(Rc::ptr_eq(&validated._context, &id._context));
    assert!(Rc::ptr_eq(
        &validated._validated,
        &validated_second._validated
    ));
    assert!(matches!(
        *context._progress.try_borrow().unwrap(),
        RootProgress::Published
    ));
    assert!(validated._validated._objects.is_empty());
    assert!(validated._validated._metadata.is_empty());
    let owned = validated.root().unwrap().to_owned().unwrap();
    assert!(Rc::ptr_eq(&owned._validated, &validated._validated));
    let guard = context._progress.try_borrow_mut().unwrap();
    assert!(validated.root().unwrap().read_bool().unwrap());
    assert!(owned.as_view().unwrap().read_bool().unwrap());
    assert!(survivor.as_view().unwrap().read_bool().unwrap());
    let ordinary = ValueView {
        _backing: RootBacking::Borrowed {
            _schema: &schema,
            _input: &input,
        },
        _context: Rc::clone(&context),
    };
    assert!(ordinary.root().unwrap().read_bool().unwrap());
    let explicitly_upgraded = ordinary.validate().unwrap();
    assert!(Rc::ptr_eq(
        &explicitly_upgraded._validated,
        &validated._validated
    ));
    assert!(Rc::ptr_eq(
        &context.validate(&schema, &input).unwrap(),
        &validated._validated
    ));
    assert!(matches!(
        validated.root().unwrap().read_i8(),
        Err(DataError::AccessError(_))
    ));
    drop(guard);
    let bad = [1u8, 255, 1, 2];
    let view = schema
        .view(&bad, InputFormat::XlangMessage, &ParseLimits::default())
        .unwrap();
    let context = Rc::clone(&view._context);
    let error = view.validate().err().unwrap();
    assert!(context._validated.get().is_none());
    assert_eq!(
        format!("{error:?}"),
        format!("{:?}", context.validate(&schema, &bad).err().unwrap())
    );
    assert!(matches!(
        *context._progress.try_borrow().unwrap(),
        RootProgress::Failed(_)
    ));
}

#[cfg(test)]
#[test]
fn root_limits_snapshot_borrow_conflicts_and_owned_propagation() {
    let schema = Schema::compile(
        crate::SchemaDescriptor {
            encoding: crate::RootEncoding {
                protocol_version: crate::ProtocolVersion::Fory1_7,
                mode: crate::SchemaMode::Compatible,
                root_reference: RootReferenceMode::NullOnly,
            },
            key_semantics_version: crate::KEY_SEMANTICS_VERSION,
            root: TypeDescriptor {
                nullable: false,
                track_ref: false,
                integer_encoding: None,
                kind: TypeKind::Scalar(ScalarKind::Bool),
            },
            definitions: vec![],
        },
        &crate::SchemaLimits::default(),
    )
    .unwrap();
    let input = fory::Fory::builder()
        .xlang(true)
        .build()
        .serialize(&true)
        .unwrap();
    let mut limits = ParseLimits {
        max_input_bytes: input.len(),
    };
    let view = schema
        .view(&input, InputFormat::XlangMessage, &limits)
        .unwrap();
    limits.max_input_bytes = 0;
    assert_eq!(limits.max_input_bytes, 0);
    assert_eq!(view._context._limits.max_input_bytes, input.len());
    let node = view.root().unwrap();
    assert!(Rc::ptr_eq(&view._context, &node._context));
    assert!(std::ptr::eq(
        node.node_type(),
        &schema._data._descriptor.root
    ));
    let owned = node.to_owned().unwrap();
    assert!(Rc::ptr_eq(&owned._context, &node._context));
    assert!(std::sync::Arc::ptr_eq(
        &owned._backing._schema._data,
        &schema._data
    ));
    let guard = view._context._progress.try_borrow_mut().unwrap();
    assert!(matches!(node.read_bool(), Err(DataError::AccessError(_))));
    assert!(matches!(view.root(), Err(DataError::AccessError(_))));
    drop(guard);
    assert!(node.read_bool().unwrap());
    assert!(owned.as_view().unwrap().read_bool().unwrap());
    let schema = std::sync::Arc::new(schema);
    let bytes = bytes::Bytes::from(input);
    let pointer = bytes.as_ptr();
    let retained = schema
        .view_owned(
            bytes.clone(),
            InputFormat::XlangMessage,
            &ParseLimits::default(),
        )
        .unwrap();
    let view = retained.as_view().unwrap();
    let node = view.root().unwrap();
    let owned = node.to_owned().unwrap();
    assert_eq!(owned._backing._input.as_ptr(), pointer);
    assert!(Rc::ptr_eq(&owned._context, &retained._context));
}

impl RootBacking<'_> {
    fn reborrow(&self) -> RootBacking<'_> {
        match self {
            Self::Borrowed { _schema, _input } => Self::Borrowed { _schema, _input },
            Self::Retained(backing) => Self::Retained(RetainedRoot {
                _schema: std::sync::Arc::clone(&backing._schema),
                _input: backing._input.clone(),
            }),
        }
    }

    fn retain(&self) -> RetainedRoot {
        match self {
            Self::Borrowed { _schema, _input } => RetainedRoot {
                _schema: std::sync::Arc::new((*_schema).clone()),
                _input: bytes::Bytes::copy_from_slice(_input),
            },
            Self::Retained(backing) => RetainedRoot {
                _schema: std::sync::Arc::clone(&backing._schema),
                _input: backing._input.clone(),
            },
        }
    }
    pub(crate) fn schema(&self) -> &Schema {
        match self {
            Self::Borrowed { _schema, .. } => _schema,
            Self::Retained(backing) => &backing._schema,
        }
    }

    pub(crate) fn input(&self) -> &[u8] {
        match self {
            Self::Borrowed { _input, .. } => _input,
            Self::Retained(backing) => &backing._input,
        }
    }
}

impl StringViewInner {
    pub(super) fn as_str<'this, 'a>(this: &'this StringView<'a>) -> &'this str {
        this.content.as_ref().as_ref()
    }
}

impl BinaryViewInner {
    pub(super) fn as_bytes<'this, 'a>(this: &'this BinaryView<'a>) -> &'this [u8] {
        this.content.as_ref().as_ref()
    }
}

impl ValueViewInner {
    pub(super) fn validate<'a>(_this: ValueView<'a>) -> Result<ValidatedValue<'a>, DataError> {
        let state = _this
            ._context
            .validate(_this._backing.schema(), _this._backing.input())?;
        Ok(ValidatedValue {
            _backing: _this._backing,
            _context: _this._context,
            _validated: state,
        })
    }
}

impl OwnedValueViewInner {
    pub(super) fn validate(_this: OwnedValueView) -> Result<OwnedValidatedValue, DataError> {
        let state = _this
            ._context
            .validate(&_this._backing._schema, &_this._backing._input)?;
        Ok(OwnedValidatedValue {
            _backing: _this._backing,
            _context: _this._context,
            _validated: state,
        })
    }
}

impl ValidatedValueInner {
    pub(super) fn into_key<'a>(
        _this: ValidatedValue<'a>,
        _requirement: KeyRequirement,
    ) -> Result<ValidatedKey<'a>, KeyError> {
        _this
            ._backing
            .schema()
            .check_key_support(_requirement)
            .map_err(crate::key::key_inner::schema_error)?;
        Ok(ValidatedKey {
            _value: _this,
            _requirement,
        })
    }
}

impl OwnedValidatedValueInner {
    pub(super) fn into_key(
        _this: OwnedValidatedValue,
        _requirement: KeyRequirement,
    ) -> Result<OwnedValidatedKey, KeyError> {
        _this
            ._backing
            ._schema
            .check_key_support(_requirement)
            .map_err(crate::key::key_inner::schema_error)?;
        Ok(OwnedValidatedKey {
            _value: _this,
            _requirement,
        })
    }
}

impl OwnedValueViewInner {
    pub(super) fn as_view<'this>(
        _this: &'this OwnedValueView,
    ) -> Result<ValueView<'this>, DataError> {
        _this._context.check()?;
        Ok(ValueView {
            _backing: RootBacking::Retained(RetainedRoot {
                _schema: std::sync::Arc::clone(&_this._backing._schema),
                _input: _this._backing._input.clone(),
            }),
            _context: Rc::clone(&_this._context),
        })
    }
}

impl OwnedValidatedValueInner {
    pub(super) fn as_view<'this>(
        _this: &'this OwnedValidatedValue,
    ) -> Result<ValidatedValue<'this>, DataError> {
        Ok(ValidatedValue {
            _backing: RootBacking::Retained(RetainedRoot {
                _schema: std::sync::Arc::clone(&_this._backing._schema),
                _input: _this._backing._input.clone(),
            }),
            _context: Rc::clone(&_this._context),
            _validated: Rc::clone(&_this._validated),
        })
    }
}

impl OwnedNodeViewInner {
    pub(super) fn as_view<'this>(
        _this: &'this OwnedNodeView,
    ) -> Result<NodeView<'this>, DataError> {
        _this._context.check()?;
        Ok(NodeView {
            node_type: &_this._node_type,
            _backing: RootBacking::Retained(RetainedRoot {
                _schema: std::sync::Arc::clone(&_this._backing._schema),
                _input: _this._backing._input.clone(),
            }),
            _context: Rc::clone(&_this._context),
            _location: Rc::clone(&_this._location),
        })
    }
}

impl OwnedValidatedNodeViewInner {
    pub(super) fn as_view<'this>(
        _this: &'this OwnedValidatedNodeView,
    ) -> Result<ValidatedNodeView<'this>, DataError> {
        Ok(ValidatedNodeView {
            node_type: &_this._node_type,
            _backing: RootBacking::Retained(RetainedRoot {
                _schema: std::sync::Arc::clone(&_this._backing._schema),
                _input: _this._backing._input.clone(),
            }),
            _context: Rc::clone(&_this._context),
            _validated: Rc::clone(&_this._validated),
            _location: Rc::clone(&_this._location),
        })
    }
}

impl ValueViewInner {
    pub(super) fn root<'this, 'a>(
        _this: &'this ValueView<'a>,
    ) -> Result<NodeView<'this>, DataError> {
        _this._context.check()?;
        let location = if let Some(state) = _this._context._validated.get() {
            copy_location(&state._root)
        } else {
            let progress = _this
                ._context
                ._progress
                .try_borrow()
                .map_err(|_| DataError::AccessError(data_location(0)))?;
            let location = match &*progress {
                RootProgress::Parsing {
                    _root: Some(root), ..
                } => copy_location(root),
                RootProgress::Failed(error) => return Err(error.clone()),
                _ => return Err(DataError::NotImplemented("published root access")),
            };
            location
        };
        let backing = match &_this._backing {
            RootBacking::Borrowed { _schema, _input } => RootBacking::Borrowed { _schema, _input },
            RootBacking::Retained(backing) => RootBacking::Retained(RetainedRoot {
                _schema: std::sync::Arc::clone(&backing._schema),
                _input: backing._input.clone(),
            }),
        };
        Ok(NodeView {
            node_type: &_this._backing.schema()._data._descriptor.root,
            _backing: backing,
            _context: Rc::clone(&_this._context),
            _location: Rc::new(location),
        })
    }
}

impl ValidatedValueInner {
    pub(super) fn root<'this, 'a>(
        _this: &'this ValidatedValue<'a>,
    ) -> Result<ValidatedNodeView<'this>, DataError> {
        Ok(ValidatedNodeView {
            node_type: &_this._backing.schema()._data._descriptor.root,
            _backing: _this._backing.reborrow(),
            _context: Rc::clone(&_this._context),
            _validated: Rc::clone(&_this._validated),
            _location: Rc::new(copy_location(&_this._validated._root)),
        })
    }
}

impl NodeViewInner {
    pub(super) fn to_owned<'this, 'a>(
        _this: &'this NodeView<'a>,
    ) -> Result<OwnedNodeView, DataError> {
        _this._context.check()?;
        let backing = match &_this._backing {
            RootBacking::Retained(backing) => RetainedRoot {
                _schema: std::sync::Arc::clone(&backing._schema),
                _input: backing._input.clone(),
            },
            RootBacking::Borrowed { _schema, _input } => RetainedRoot {
                _schema: std::sync::Arc::new((*_schema).clone()),
                _input: bytes::Bytes::copy_from_slice(_input),
            },
        };
        Ok(OwnedNodeView {
            _backing: backing,
            _context: Rc::clone(&_this._context),
            _node_type: _this.node_type.clone(),
            _location: Rc::clone(&_this._location),
        })
    }

    pub(super) fn node_type<'this, 'a>(this: &'this NodeView<'a>) -> &'this TypeDescriptor {
        this.node_type
    }

    pub(super) fn variant<'this, 'a>(
        _this: &'this NodeView<'a>,
    ) -> Result<&'this VariantDescriptor, DataError> {
        _this._context.check()?;
        _this._context.finish(
            variant_body(
                _this._backing.schema(),
                _this._backing.input(),
                &_this._location,
                _this.node_type,
            )
            .map(|(variant, _, _)| variant),
        )
    }

    pub(super) fn variant_payload<'this, 'a>(
        _this: &'this NodeView<'a>,
    ) -> Result<Option<NodeView<'this>>, DataError> {
        _this._context.check()?;
        let (variant, location, _) = _this._context.finish(variant_body(
            _this._backing.schema(),
            _this._backing.input(),
            &_this._location,
            _this.node_type,
        ))?;
        let Some((ty, location)) = variant.payload.as_ref().zip(location) else {
            return Ok(None);
        };
        let mut metadata = _this._context.metadata()?;
        let mut names = _this._context.finish(_this._context.names_before(
            _this._backing.schema(),
            _this._backing.input(),
            &_this._location,
        ))?;
        let location = _this._context.finish(bind_payload(
            _this._backing.schema(),
            _this._backing.input(),
            &location,
            ty,
            &mut names,
            &mut metadata,
        ))?;
        _this._context.store_metadata(&metadata)?;
        Ok(Some(NodeView {
            node_type: ty,
            _backing: _this._backing.reborrow(),
            _context: Rc::clone(&_this._context),
            _location: Rc::new(location),
        }))
    }

    pub(super) fn is_null<'this, 'a>(_this: &'this NodeView<'a>) -> Result<bool, DataError> {
        _this._context.check()?;
        _this._context.finish(node_is_null(
            _this._backing.input(),
            &_this._location,
            _this.node_type,
        ))
    }

    pub(super) fn reference_id<'this, 'a>(
        _this: &'this NodeView<'a>,
    ) -> Result<Option<ObjectId>, DataError> {
        _this._context.check()?;
        let id = _this._context.finish(_this._context.identity(
            _this._backing.schema(),
            _this._backing.input(),
            &_this._location,
            _this.node_type,
            true,
        ))?;
        Ok(id.map(|id| ObjectId {
            _context: Rc::clone(&_this._context),
            _local: id as u64,
        }))
    }

    pub(super) fn object_id<'this, 'a>(
        _this: &'this NodeView<'a>,
    ) -> Result<Option<ObjectId>, DataError> {
        _this._context.check()?;
        let id = _this._context.finish(_this._context.identity(
            _this._backing.schema(),
            _this._backing.input(),
            &_this._location,
            _this.node_type,
            false,
        ))?;
        Ok(id.map(|id| ObjectId {
            _context: Rc::clone(&_this._context),
            _local: id as u64,
        }))
    }

    pub(super) fn resolve_reference<'this, 'a>(
        _this: &'this NodeView<'a>,
        _id: &ObjectId,
    ) -> Result<NodeView<'this>, DataError> {
        _this._context.check()?;
        if !Rc::ptr_eq(&_this._context, &_id._context) {
            return Err(DataError::AccessError(data_location(
                _this._location._byte_offset,
            )));
        }
        let (ty, location) = if let Some(state) = _this._context._validated.get() {
            let (ty, location) = state._objects.get(_id._local as usize).ok_or_else(|| {
                DataError::AccessError(data_location(_this._location._byte_offset))
            })?;
            (
                declared_type(_this._backing.schema(), ty)?,
                copy_location(location),
            )
        } else {
            let progress =
                _this._context._progress.try_borrow().map_err(|_| {
                    DataError::AccessError(data_location(_this._location._byte_offset))
                })?;
            let RootProgress::Parsing { _objects, .. } = &*progress else {
                return Err(DataError::AccessError(data_location(
                    _this._location._byte_offset,
                )));
            };
            let (ty, location) = _objects.get(_id._local as usize).ok_or_else(|| {
                DataError::AccessError(data_location(_this._location._byte_offset))
            })?;
            (
                declared_type(_this._backing.schema(), ty)?,
                copy_location(location),
            )
        };
        Ok(NodeView {
            node_type: ty,
            _backing: _this._backing.reborrow(),
            _context: Rc::clone(&_this._context),
            _location: Rc::new(location),
        })
    }

    pub(super) fn field_by_id<'this, 'a>(
        _this: &'this NodeView<'a>,
        _id: u32,
    ) -> Result<NodeView<'this>, DataError> {
        _this._context.check()?;
        let (fields, _) = struct_fields(_this._backing.schema(), _this.node_type)?;
        let index = fields
            .iter()
            .position(|f| f.id == Some(_id))
            .ok_or_else(|| DataError::AccessError(data_location(_this._location._byte_offset)))?;
        Self::field_at(_this, index)
    }

    pub(super) fn field_by_name<'this, 'a>(
        _this: &'this NodeView<'a>,
        _name: &str,
    ) -> Result<NodeView<'this>, DataError> {
        _this._context.check()?;
        let (fields, _) = struct_fields(_this._backing.schema(), _this.node_type)?;
        let index = fields
            .iter()
            .position(|f| f.name == _name)
            .ok_or_else(|| DataError::AccessError(data_location(_this._location._byte_offset)))?;
        Self::field_at(_this, index)
    }

    pub(super) fn field_at<'this, 'a>(
        _this: &'this NodeView<'a>,
        _index: usize,
    ) -> Result<NodeView<'this>, DataError> {
        _this._context.check()?;
        let (fields, _) = struct_fields(_this._backing.schema(), _this.node_type)?;
        if _index >= fields.len() {
            return Err(DataError::AccessError(data_location(
                _this._location._byte_offset,
            )));
        }
        let mut metadata = _this._context.metadata()?;
        let mut names = _this._context.finish(_this._context.names_before(
            _this._backing.schema(),
            _this._backing.input(),
            &_this._location,
        ))?;
        let locations = _this._context.finish(field_locations(
            Some(_index),
            &mut names,
            &mut metadata,
            _this._backing.schema(),
            _this._backing.input(),
            &_this._location,
            _this.node_type,
            true,
        ))?;
        _this._context.store_metadata(&metadata)?;
        let (field, location) = locations
            .into_iter()
            .next()
            .ok_or_else(|| DataError::AccessError(data_location(_this._location._byte_offset)))?;
        Ok(NodeView {
            node_type: &field.ty,
            _backing: _this._backing.reborrow(),
            _context: Rc::clone(&_this._context),
            _location: Rc::new(location),
        })
    }

    pub(super) fn fields<'this, 'a>(
        _this: &'this NodeView<'a>,
    ) -> Result<NodeFields<'this, NodeView<'this>>, DataError> {
        _this._context.check()?;
        let mut metadata = _this._context.metadata()?;
        let locations = _this._context.finish(field_locations(
            None,
            &mut _this._context.finish(_this._context.names_before(
                _this._backing.schema(),
                _this._backing.input(),
                &_this._location,
            ))?,
            &mut metadata,
            _this._backing.schema(),
            _this._backing.input(),
            &_this._location,
            _this.node_type,
            true,
        ))?;
        _this._context.store_metadata(&metadata)?;
        let mut locations = locations.into_iter();
        let mut stopped = false;
        Ok(Box::new(std::iter::from_fn(move || {
            if stopped {
                return None;
            }
            if let Err(error) = _this._context.check() {
                stopped = true;
                return Some(Err(error));
            }
            let (field, location) = locations.next()?;
            Some(Ok((
                field,
                NodeView {
                    node_type: &field.ty,
                    _backing: _this._backing.reborrow(),
                    _context: Rc::clone(&_this._context),
                    _location: Rc::new(location),
                },
            )))
        })))
    }

    pub(super) fn len<'this, 'a>(_this: &'this NodeView<'a>) -> Result<usize, DataError> {
        _this._context.check()?;
        let mut metadata = _this._context.metadata()?;
        let result = container_len(
            &mut _this._context.finish(_this._context.names_before(
                _this._backing.schema(),
                _this._backing.input(),
                &_this._location,
            ))?,
            _this._backing.schema(),
            &mut metadata,
            _this._backing.input(),
            &_this._location,
            _this.node_type,
        );
        let len = _this._context.finish(result)?;
        _this._context.store_metadata(&metadata)?;
        Ok(len)
    }

    pub(super) fn element_at<'this, 'a>(
        _this: &'this NodeView<'a>,
        _index: usize,
    ) -> Result<NodeView<'this>, DataError> {
        _this._context.check()?;
        let mut metadata = _this._context.metadata()?;
        let result = sequence_scan(
            &mut _this._context.finish(_this._context.names_before(
                _this._backing.schema(),
                _this._backing.input(),
                &_this._location,
            ))?,
            &mut metadata,
            _this._backing.schema(),
            _this._backing.input(),
            &_this._location,
            _this.node_type,
            Some(_index),
            false,
            true,
        );
        let (_, location, _) = _this._context.finish(result)?;
        _this._context.store_metadata(&metadata)?;
        let location = location.ok_or(DataError::AccessError(data_location(
            _this._location._byte_offset,
        )))?;
        Ok(NodeView {
            node_type: sequence_element(_this.node_type)?,
            _backing: _this._backing.reborrow(),
            _context: Rc::clone(&_this._context),
            _location: Rc::new(location),
        })
    }

    pub(super) fn elements<'this, 'a>(
        _this: &'this NodeView<'a>,
    ) -> Result<NodeIter<'this, NodeView<'this>>, DataError> {
        _this._context.check()?;
        let mut metadata = _this._context.metadata()?;
        let mut names = _this._context.finish(_this._context.names_before(
            _this._backing.schema(),
            _this._backing.input(),
            &_this._location,
        ))?;
        let (len, mut cursor, mode, _, template) = _this._context.finish(sequence_header(
            &mut names,
            _this._backing.schema(),
            &mut metadata,
            _this._backing.input(),
            &_this._location,
            _this.node_type,
        ))?;
        _this._context.store_metadata(&metadata)?;
        let element = sequence_element(_this.node_type)?;
        let mut objects = _this._context.finish(objects_for_node(
            _this._backing.schema(),
            _this._backing.input(),
            &_this._location,
            _this.node_type,
        ))?;
        let mut index = 0usize;
        let mut stopped = false;
        Ok(Box::new(std::iter::from_fn(move || {
            if stopped || index == len {
                return None;
            }
            let result = (|| {
                _this._context.check()?;
                let location = NodeLocation {
                    _byte_offset: cursor,
                    _parent_null: false,
                    _reference_mode: mode,
                    ..copy_location(&template)
                };
                cursor = scan_objects(
                    _this._backing.schema(),
                    _this._backing.input(),
                    &location,
                    element,
                    true,
                    &mut metadata,
                    &mut names,
                    None,
                    &mut objects,
                )?;
                _this._context.store_metadata(&metadata)?;
                Ok(NodeView {
                    node_type: element,
                    _backing: _this._backing.reborrow(),
                    _context: Rc::clone(&_this._context),
                    _location: Rc::new(location),
                })
            })();
            let item = _this._context.finish(result);
            index += 1;
            stopped = item.is_err();
            Some(item)
        })))
    }

    pub(super) fn entries<'this, 'a>(
        _this: &'this NodeView<'a>,
    ) -> Result<NodeEntries<'this, NodeView<'this>>, DataError> {
        _this._context.check()?;
        let mut locations = _this._context.finish(map_locations(
            _this._context.finish(_this._context.names_before(
                _this._backing.schema(),
                _this._backing.input(),
                &_this._location,
            ))?,
            _this._context.metadata()?,
            &_this._context,
            _this._backing.schema(),
            _this._backing.input(),
            &_this._location,
            _this.node_type,
            true,
        ))?;
        let TypeKind::Map { key, value } = &_this.node_type.kind else {
            unreachable!()
        };
        let mut stopped = false;
        Ok(Box::new(std::iter::from_fn(move || {
            if stopped {
                return None;
            }
            let result = match _this._context.check() {
                Err(error) => Err(error),
                Ok(()) => locations.next()?,
            };
            let result = _this._context.finish(result).map(|(kl, vl)| {
                (
                    NodeView {
                        node_type: key,
                        _backing: _this._backing.reborrow(),
                        _context: Rc::clone(&_this._context),
                        _location: Rc::new(kl),
                    },
                    NodeView {
                        node_type: value,
                        _backing: _this._backing.reborrow(),
                        _context: Rc::clone(&_this._context),
                        _location: Rc::new(vl),
                    },
                )
            });
            stopped = result.is_err();
            Some(result)
        })))
    }

    pub(super) fn read_string<'this, 'a>(
        _this: &'this NodeView<'a>,
    ) -> Result<StringView<'this>, DataError> {
        _this._context.check()?;
        let result = content_body(
            _this._backing.input(),
            &_this._location,
            _this.node_type,
            ScalarKind::String,
            false,
        )
        .and_then(|body| decode_string_content(_this._backing.input(), body));
        _this._context.finish(result)
    }

    pub(super) fn read_binary<'this, 'a>(
        _this: &'this NodeView<'a>,
    ) -> Result<BinaryView<'this>, DataError> {
        _this._context.check()?;
        let result = content_body(
            _this._backing.input(),
            &_this._location,
            _this.node_type,
            ScalarKind::Binary,
            false,
        )
        .and_then(|body| binary_content(_this._backing.input(), body));
        _this._context.finish(result)
    }

    pub(super) fn read_date<'this, 'a>(
        _this: &'this NodeView<'a>,
    ) -> Result<fory::Date, DataError> {
        _this._context.check()?;
        read_scalar(_this, ScalarKind::Date, decode_date)
    }

    pub(super) fn read_timestamp<'this, 'a>(
        _this: &'this NodeView<'a>,
    ) -> Result<fory::Timestamp, DataError> {
        _this._context.check()?;
        read_scalar(_this, ScalarKind::Timestamp, decode_timestamp)
    }

    pub(super) fn read_duration<'this, 'a>(
        _this: &'this NodeView<'a>,
    ) -> Result<fory::Duration, DataError> {
        _this._context.check()?;
        read_scalar(_this, ScalarKind::Duration, decode_duration)
    }

    pub(super) fn read_bool<'this, 'a>(_this: &'this NodeView<'a>) -> Result<bool, DataError> {
        _this._context.check()?;
        let value = read_scalar(_this, ScalarKind::Bool, |reader| reader.read_u8())?;
        match value {
            0 => Ok(false),
            1 => Ok(true),
            _ => {
                let (_, body) =
                    node_body(_this._backing.input(), &_this._location, _this.node_type)?;
                _this
                    ._context
                    .finish(Err(DataError::MalformedInput(data_location(body))))
            }
        }
    }

    pub(super) fn read_i8<'this, 'a>(_this: &'this NodeView<'a>) -> Result<i8, DataError> {
        read_scalar(_this, ScalarKind::I8, |reader| reader.read_i8())
    }

    pub(super) fn read_i16<'this, 'a>(_this: &'this NodeView<'a>) -> Result<i16, DataError> {
        read_scalar(_this, ScalarKind::I16, |reader| reader.read_i16())
    }

    pub(super) fn read_i32<'this, 'a>(_this: &'this NodeView<'a>) -> Result<i32, DataError> {
        read_scalar(
            _this,
            ScalarKind::I32,
            |reader| match integer_read_encoding(&_this._location, _this.node_type) {
                Some(IntegerEncoding::Fixed) => reader.read_i32(),
                _ => {
                    read_var_u32(reader).map(|value| ((value >> 1) as i32) ^ -((value & 1) as i32))
                }
            },
        )
    }

    pub(super) fn read_i64<'this, 'a>(_this: &'this NodeView<'a>) -> Result<i64, DataError> {
        read_scalar(
            _this,
            ScalarKind::I64,
            |reader| match integer_read_encoding(&_this._location, _this.node_type) {
                Some(IntegerEncoding::Fixed) => reader.read_i64(),
                Some(IntegerEncoding::Tagged) => reader.read_tagged_i64(),
                _ => {
                    read_var_u64(reader).map(|value| ((value >> 1) as i64) ^ -((value & 1) as i64))
                }
            },
        )
    }

    pub(super) fn read_u8<'this, 'a>(_this: &'this NodeView<'a>) -> Result<u8, DataError> {
        read_scalar(_this, ScalarKind::U8, |reader| reader.read_u8())
    }

    pub(super) fn read_u16<'this, 'a>(_this: &'this NodeView<'a>) -> Result<u16, DataError> {
        read_scalar(_this, ScalarKind::U16, |reader| reader.read_u16())
    }

    pub(super) fn read_u32<'this, 'a>(_this: &'this NodeView<'a>) -> Result<u32, DataError> {
        read_scalar(
            _this,
            ScalarKind::U32,
            |reader| match integer_read_encoding(&_this._location, _this.node_type) {
                Some(IntegerEncoding::Fixed) => reader.read_u32(),
                _ => read_var_u32(reader),
            },
        )
    }

    pub(super) fn read_u64<'this, 'a>(_this: &'this NodeView<'a>) -> Result<u64, DataError> {
        read_scalar(
            _this,
            ScalarKind::U64,
            |reader| match integer_read_encoding(&_this._location, _this.node_type) {
                Some(IntegerEncoding::Fixed) => reader.read_u64(),
                Some(IntegerEncoding::Tagged) => reader.read_tagged_u64(),
                _ => read_var_u64(reader),
            },
        )
    }

    pub(super) fn read_f32<'this, 'a>(_this: &'this NodeView<'a>) -> Result<f32, DataError> {
        read_scalar(_this, ScalarKind::F32, |reader| reader.read_f32())
    }

    pub(super) fn read_f64<'this, 'a>(_this: &'this NodeView<'a>) -> Result<f64, DataError> {
        read_scalar(_this, ScalarKind::F64, |reader| reader.read_f64())
    }
}

impl ValidatedNodeViewInner {
    pub(super) fn key_string<'a>(
        node: &ValidatedNodeView<'a>,
    ) -> Result<StringView<'a>, DataError> {
        let RootBacking::Borrowed { _input, .. } = &node._backing else {
            return Err(DataError::AccessError(data_location(0)));
        };
        let body = content_body(
            _input,
            &node._location,
            node.node_type,
            ScalarKind::String,
            true,
        )?;
        decode_string_content(_input, body)
    }
    pub(super) fn key_binary<'a>(
        node: &ValidatedNodeView<'a>,
    ) -> Result<BinaryView<'a>, DataError> {
        let RootBacking::Borrowed { _input, .. } = &node._backing else {
            return Err(DataError::AccessError(data_location(0)));
        };
        let body = content_body(
            _input,
            &node._location,
            node.node_type,
            ScalarKind::Binary,
            true,
        )?;
        binary_content(_input, body)
    }
    pub(super) fn key_node<'a>(
        state: &Rc<ValidatedState>,
        schema: &'a Schema,
        input: &'a [u8],
        context: &Rc<RootState>,
        child: Option<&ValidatedNodeView<'_>>,
    ) -> Result<ValidatedNodeView<'a>, DataError> {
        let access = || DataError::AccessError(data_location(0));
        if !context
            ._validated
            .get()
            .is_some_and(|published| Rc::ptr_eq(published, state))
        {
            return Err(access());
        }
        let (node_type, location) = if let Some(child) = child {
            if !Rc::ptr_eq(&child._context, context)
                || !Rc::ptr_eq(&child._validated, state)
                || !std::sync::Arc::ptr_eq(&child._backing.schema()._data, &schema._data)
                || !std::ptr::eq(child._backing.input(), input)
            {
                return Err(access());
            }
            (
                declared_type(schema, child.node_type)?,
                Rc::clone(&child._location),
            )
        } else {
            (
                &schema._data._descriptor.root,
                Rc::new(copy_location(&state._root)),
            )
        };
        Ok(ValidatedNodeView {
            node_type,
            _backing: RootBacking::Borrowed {
                _schema: schema,
                _input: input,
            },
            _context: Rc::clone(context),
            _validated: Rc::clone(state),
            _location: location,
        })
    }

    pub(super) fn to_owned<'this, 'a>(
        _this: &'this ValidatedNodeView<'a>,
    ) -> Result<OwnedValidatedNodeView, DataError> {
        Ok(OwnedValidatedNodeView {
            _backing: _this._backing.retain(),
            _context: Rc::clone(&_this._context),
            _validated: Rc::clone(&_this._validated),
            _node_type: _this.node_type.clone(),
            _location: Rc::clone(&_this._location),
        })
    }

    pub(super) fn node_type<'this, 'a>(
        this: &'this ValidatedNodeView<'a>,
    ) -> &'this TypeDescriptor {
        this.node_type
    }

    pub(super) fn variant<'this, 'a>(
        _this: &'this ValidatedNodeView<'a>,
    ) -> Result<&'this VariantDescriptor, DataError> {
        variant_body(
            _this._backing.schema(),
            _this._backing.input(),
            &_this._location,
            _this.node_type,
        )
        .map(|(variant, _, _)| variant)
    }

    pub(super) fn variant_payload<'this, 'a>(
        _this: &'this ValidatedNodeView<'a>,
    ) -> Result<Option<ValidatedNodeView<'this>>, DataError> {
        let (variant, location, _) = variant_body(
            _this._backing.schema(),
            _this._backing.input(),
            &_this._location,
            _this.node_type,
        )?;
        let Some((ty, location)) = variant.payload.as_ref().zip(location) else {
            return Ok(None);
        };
        let location = bind_payload(
            _this._backing.schema(),
            _this._backing.input(),
            &location,
            ty,
            &mut _this._context.names_before(
                _this._backing.schema(),
                _this._backing.input(),
                &_this._location,
            )?,
            &mut _this._validated._metadata.clone(),
        )?;
        Ok(Some(ValidatedNodeView {
            node_type: ty,
            _backing: _this._backing.reborrow(),
            _context: Rc::clone(&_this._context),
            _validated: Rc::clone(&_this._validated),
            _location: Rc::new(location),
        }))
    }

    pub(super) fn is_null<'this, 'a>(
        _this: &'this ValidatedNodeView<'a>,
    ) -> Result<bool, DataError> {
        node_is_null(_this._backing.input(), &_this._location, _this.node_type)
    }

    pub(super) fn reference_id<'this, 'a>(
        _this: &'this ValidatedNodeView<'a>,
    ) -> Result<Option<ObjectId>, DataError> {
        Ok(_this
            ._context
            .identity(
                _this._backing.schema(),
                _this._backing.input(),
                &_this._location,
                _this.node_type,
                true,
            )?
            .map(|id| ObjectId {
                _context: Rc::clone(&_this._context),
                _local: id as u64,
            }))
    }

    pub(super) fn object_id<'this, 'a>(
        _this: &'this ValidatedNodeView<'a>,
    ) -> Result<Option<ObjectId>, DataError> {
        Ok(_this
            ._context
            .identity(
                _this._backing.schema(),
                _this._backing.input(),
                &_this._location,
                _this.node_type,
                false,
            )?
            .map(|id| ObjectId {
                _context: Rc::clone(&_this._context),
                _local: id as u64,
            }))
    }

    pub(super) fn resolve_reference<'this, 'a>(
        _this: &'this ValidatedNodeView<'a>,
        _id: &ObjectId,
    ) -> Result<ValidatedNodeView<'this>, DataError> {
        if !Rc::ptr_eq(&_this._context, &_id._context) {
            return Err(DataError::AccessError(data_location(
                _this._location._byte_offset,
            )));
        }
        let (ty, location) = _this
            ._validated
            ._objects
            .get(_id._local as usize)
            .ok_or_else(|| DataError::AccessError(data_location(_this._location._byte_offset)))?;
        Ok(ValidatedNodeView {
            node_type: ty,
            _backing: _this._backing.reborrow(),
            _context: Rc::clone(&_this._context),
            _validated: Rc::clone(&_this._validated),
            _location: Rc::new(copy_location(location)),
        })
    }

    pub(super) fn field_by_id<'this, 'a>(
        _this: &'this ValidatedNodeView<'a>,
        _id: u32,
    ) -> Result<ValidatedNodeView<'this>, DataError> {
        let (fields, _) = struct_fields(_this._backing.schema(), _this.node_type)?;
        let index = fields
            .iter()
            .position(|f| f.id == Some(_id))
            .ok_or_else(|| DataError::AccessError(data_location(_this._location._byte_offset)))?;
        Self::field_at(_this, index)
    }

    pub(super) fn field_by_name<'this, 'a>(
        _this: &'this ValidatedNodeView<'a>,
        _name: &str,
    ) -> Result<ValidatedNodeView<'this>, DataError> {
        let (fields, _) = struct_fields(_this._backing.schema(), _this.node_type)?;
        let index = fields
            .iter()
            .position(|f| f.name == _name)
            .ok_or_else(|| DataError::AccessError(data_location(_this._location._byte_offset)))?;
        Self::field_at(_this, index)
    }

    pub(super) fn field_at<'this, 'a>(
        _this: &'this ValidatedNodeView<'a>,
        _index: usize,
    ) -> Result<ValidatedNodeView<'this>, DataError> {
        let (fields, _) = struct_fields(_this._backing.schema(), _this.node_type)?;
        if _index >= fields.len() {
            return Err(DataError::AccessError(data_location(
                _this._location._byte_offset,
            )));
        }
        let locations = field_locations(
            Some(_index),
            &mut _this._context.names_before(
                _this._backing.schema(),
                _this._backing.input(),
                &_this._location,
            )?,
            &mut _this._validated._metadata.clone(),
            _this._backing.schema(),
            _this._backing.input(),
            &_this._location,
            _this.node_type,
            false,
        )?;
        let (field, location) = locations
            .into_iter()
            .next()
            .ok_or_else(|| DataError::AccessError(data_location(_this._location._byte_offset)))?;
        Ok(ValidatedNodeView {
            node_type: &field.ty,
            _backing: _this._backing.reborrow(),
            _context: Rc::clone(&_this._context),
            _validated: Rc::clone(&_this._validated),
            _location: Rc::new(location),
        })
    }

    pub(super) fn fields<'this, 'a>(
        _this: &'this ValidatedNodeView<'a>,
    ) -> Result<NodeFields<'this, ValidatedNodeView<'this>>, DataError> {
        let locations = field_locations(
            None,
            &mut _this._context.names_before(
                _this._backing.schema(),
                _this._backing.input(),
                &_this._location,
            )?,
            &mut _this._validated._metadata.clone(),
            _this._backing.schema(),
            _this._backing.input(),
            &_this._location,
            _this.node_type,
            false,
        )?;
        Ok(Box::new(locations.into_iter().map(
            move |(field, location)| {
                Ok((
                    field,
                    ValidatedNodeView {
                        node_type: &field.ty,
                        _backing: _this._backing.reborrow(),
                        _context: Rc::clone(&_this._context),
                        _validated: Rc::clone(&_this._validated),
                        _location: Rc::new(location),
                    },
                ))
            },
        )))
    }

    pub(super) fn len<'this, 'a>(_this: &'this ValidatedNodeView<'a>) -> Result<usize, DataError> {
        container_len(
            &mut _this._context.names_before(
                _this._backing.schema(),
                _this._backing.input(),
                &_this._location,
            )?,
            _this._backing.schema(),
            &mut _this._validated._metadata.clone(),
            _this._backing.input(),
            &_this._location,
            _this.node_type,
        )
    }

    pub(super) fn element_at<'this, 'a>(
        _this: &'this ValidatedNodeView<'a>,
        _index: usize,
    ) -> Result<ValidatedNodeView<'this>, DataError> {
        let result = sequence_scan(
            &mut _this._context.names_before(
                _this._backing.schema(),
                _this._backing.input(),
                &_this._location,
            )?,
            &mut _this._validated._metadata.clone(),
            _this._backing.schema(),
            _this._backing.input(),
            &_this._location,
            _this.node_type,
            Some(_index),
            false,
            false,
        );
        let (_, location, _) = result?;
        let location = location.ok_or(DataError::AccessError(data_location(
            _this._location._byte_offset,
        )))?;
        Ok(ValidatedNodeView {
            node_type: sequence_element(_this.node_type)?,
            _backing: _this._backing.reborrow(),
            _context: Rc::clone(&_this._context),
            _location: Rc::new(location),
            _validated: Rc::clone(&_this._validated),
        })
    }

    pub(super) fn elements<'this, 'a>(
        _this: &'this ValidatedNodeView<'a>,
    ) -> Result<NodeIter<'this, ValidatedNodeView<'this>>, DataError> {
        let mut metadata = _this._validated._metadata.clone();
        let mut names = _this._context.names_before(
            _this._backing.schema(),
            _this._backing.input(),
            &_this._location,
        )?;
        let (len, mut cursor, mode, _, template) = sequence_header(
            &mut names,
            _this._backing.schema(),
            &mut metadata,
            _this._backing.input(),
            &_this._location,
            _this.node_type,
        )?;
        let element = sequence_element(_this.node_type)?;
        let mut objects = objects_for_node(
            _this._backing.schema(),
            _this._backing.input(),
            &_this._location,
            _this.node_type,
        )?;
        let mut index = 0usize;
        let mut stopped = false;
        Ok(Box::new(std::iter::from_fn(move || {
            if stopped || index == len {
                return None;
            }
            let result = (|| {
                let location = NodeLocation {
                    _byte_offset: cursor,
                    _parent_null: false,
                    _reference_mode: mode,
                    ..copy_location(&template)
                };
                cursor = scan_objects(
                    _this._backing.schema(),
                    _this._backing.input(),
                    &location,
                    element,
                    false,
                    &mut metadata,
                    &mut names,
                    None,
                    &mut objects,
                )?;
                Ok(ValidatedNodeView {
                    node_type: element,
                    _backing: _this._backing.reborrow(),
                    _context: Rc::clone(&_this._context),
                    _location: Rc::new(location),
                    _validated: Rc::clone(&_this._validated),
                })
            })();
            let item = result;
            index += 1;
            stopped = item.is_err();
            Some(item)
        })))
    }

    pub(super) fn entries<'this, 'a>(
        _this: &'this ValidatedNodeView<'a>,
    ) -> Result<NodeEntries<'this, ValidatedNodeView<'this>>, DataError> {
        let locations = map_locations(
            _this._context.names_before(
                _this._backing.schema(),
                _this._backing.input(),
                &_this._location,
            )?,
            _this._validated._metadata.clone(),
            &_this._context,
            _this._backing.schema(),
            _this._backing.input(),
            &_this._location,
            _this.node_type,
            false,
        )?;
        let TypeKind::Map { key, value } = &_this.node_type.kind else {
            unreachable!()
        };
        Ok(Box::new(locations.map(move |result| {
            result.map(|(kl, vl)| {
                (
                    ValidatedNodeView {
                        node_type: key,
                        _backing: _this._backing.reborrow(),
                        _context: Rc::clone(&_this._context),
                        _validated: Rc::clone(&_this._validated),
                        _location: Rc::new(kl),
                    },
                    ValidatedNodeView {
                        node_type: value,
                        _backing: _this._backing.reborrow(),
                        _context: Rc::clone(&_this._context),
                        _validated: Rc::clone(&_this._validated),
                        _location: Rc::new(vl),
                    },
                )
            })
        })))
    }

    pub(super) fn read_string<'this, 'a>(
        _this: &'this ValidatedNodeView<'a>,
    ) -> Result<StringView<'this>, DataError> {
        let body = content_body(
            _this._backing.input(),
            &_this._location,
            _this.node_type,
            ScalarKind::String,
            true,
        )?;
        decode_string_content(_this._backing.input(), body)
    }

    pub(super) fn read_binary<'this, 'a>(
        _this: &'this ValidatedNodeView<'a>,
    ) -> Result<BinaryView<'this>, DataError> {
        let body = content_body(
            _this._backing.input(),
            &_this._location,
            _this.node_type,
            ScalarKind::Binary,
            true,
        )?;
        binary_content(_this._backing.input(), body)
    }

    pub(super) fn read_date<'this, 'a>(
        _this: &'this ValidatedNodeView<'a>,
    ) -> Result<fory::Date, DataError> {
        read_validated_scalar(_this, ScalarKind::Date, decode_date)
    }

    pub(super) fn read_timestamp<'this, 'a>(
        _this: &'this ValidatedNodeView<'a>,
    ) -> Result<fory::Timestamp, DataError> {
        read_validated_scalar(_this, ScalarKind::Timestamp, decode_timestamp)
    }

    pub(super) fn read_duration<'this, 'a>(
        _this: &'this ValidatedNodeView<'a>,
    ) -> Result<fory::Duration, DataError> {
        read_validated_scalar(_this, ScalarKind::Duration, decode_duration)
    }

    pub(super) fn read_bool<'this, 'a>(
        _this: &'this ValidatedNodeView<'a>,
    ) -> Result<bool, DataError> {
        read_validated_scalar(_this, ScalarKind::Bool, |reader| {
            reader.read_u8().map(|value| value == 1)
        })
    }

    pub(super) fn read_i8<'this, 'a>(_this: &'this ValidatedNodeView<'a>) -> Result<i8, DataError> {
        read_validated_scalar(_this, ScalarKind::I8, |reader| reader.read_i8())
    }

    pub(super) fn read_i16<'this, 'a>(
        _this: &'this ValidatedNodeView<'a>,
    ) -> Result<i16, DataError> {
        read_validated_scalar(_this, ScalarKind::I16, |reader| reader.read_i16())
    }

    pub(super) fn read_i32<'this, 'a>(
        _this: &'this ValidatedNodeView<'a>,
    ) -> Result<i32, DataError> {
        read_validated_scalar(
            _this,
            ScalarKind::I32,
            |reader| match integer_read_encoding(&_this._location, _this.node_type) {
                Some(IntegerEncoding::Fixed) => reader.read_i32(),
                _ => {
                    read_var_u32(reader).map(|value| ((value >> 1) as i32) ^ -((value & 1) as i32))
                }
            },
        )
    }

    pub(super) fn read_i64<'this, 'a>(
        _this: &'this ValidatedNodeView<'a>,
    ) -> Result<i64, DataError> {
        read_validated_scalar(
            _this,
            ScalarKind::I64,
            |reader| match integer_read_encoding(&_this._location, _this.node_type) {
                Some(IntegerEncoding::Fixed) => reader.read_i64(),
                Some(IntegerEncoding::Tagged) => reader.read_tagged_i64(),
                _ => {
                    read_var_u64(reader).map(|value| ((value >> 1) as i64) ^ -((value & 1) as i64))
                }
            },
        )
    }

    pub(super) fn read_u8<'this, 'a>(_this: &'this ValidatedNodeView<'a>) -> Result<u8, DataError> {
        read_validated_scalar(_this, ScalarKind::U8, |reader| reader.read_u8())
    }

    pub(super) fn read_u16<'this, 'a>(
        _this: &'this ValidatedNodeView<'a>,
    ) -> Result<u16, DataError> {
        read_validated_scalar(_this, ScalarKind::U16, |reader| reader.read_u16())
    }

    pub(super) fn read_u32<'this, 'a>(
        _this: &'this ValidatedNodeView<'a>,
    ) -> Result<u32, DataError> {
        read_validated_scalar(
            _this,
            ScalarKind::U32,
            |reader| match integer_read_encoding(&_this._location, _this.node_type) {
                Some(IntegerEncoding::Fixed) => reader.read_u32(),
                _ => read_var_u32(reader),
            },
        )
    }

    pub(super) fn read_u64<'this, 'a>(
        _this: &'this ValidatedNodeView<'a>,
    ) -> Result<u64, DataError> {
        read_validated_scalar(
            _this,
            ScalarKind::U64,
            |reader| match integer_read_encoding(&_this._location, _this.node_type) {
                Some(IntegerEncoding::Fixed) => reader.read_u64(),
                Some(IntegerEncoding::Tagged) => reader.read_tagged_u64(),
                _ => read_var_u64(reader),
            },
        )
    }

    pub(super) fn read_f32<'this, 'a>(
        _this: &'this ValidatedNodeView<'a>,
    ) -> Result<f32, DataError> {
        read_validated_scalar(_this, ScalarKind::F32, |reader| reader.read_f32())
    }

    pub(super) fn read_f64<'this, 'a>(
        _this: &'this ValidatedNodeView<'a>,
    ) -> Result<f64, DataError> {
        read_validated_scalar(_this, ScalarKind::F64, |reader| reader.read_f64())
    }
}
