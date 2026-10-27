use pi_fory_schema::*;
use std::collections::VecDeque;

#[test]
fn dense_fixed_integer_bodies_in_groups_and_user_fields() {
    for compatible in [false, true] {
        macro_rules! check {
            ($t:ty, $kind:ident, $encoding:ident, $read:ident, $values:expr) => {{
                #[derive(Debug, PartialEq, fory::ForyStruct)]
                struct Parent { items: Vec<[$t; 3]> }
                let values = $values;
                let element = TypeDescriptor { kind: TypeKind::Scalar(ScalarKind::$kind), integer_encoding: Some(IntegerEncoding::$encoding), nullable: false, track_ref: false };
                let array = ty(TypeKind::Array { element: Box::new(element), length: Some(3) }, false);
                let list = ty(TypeKind::List(Box::new(array.clone())), false);
                let mode = if compatible { SchemaMode::Compatible } else { SchemaMode::SameSchema { version_hash: false } };
                let mut f = fory::Fory::builder().xlang(true).compatible(compatible).check_struct_version(false).build();
                f.register::<Parent>(732).unwrap();
                for root in [list.clone(), ty(TypeKind::User(UserIdentity::Numeric(732)), false)] {
                    let is_user = matches!(root.kind, TypeKind::User(_));
                    if is_user && compatible { continue; }
                    let mut input = if is_user { f.serialize(&Parent { items: vec![values, values] }).unwrap() } else { f.serialize(&vec![values, values]).unwrap() };
                    if is_user {
                        // derive 字段省略已声明的子 typeinfo。替换为真实 root List
                        // writer 的显式 group body，保留实际 struct 包络和 Compatible metadata。
                        let body_len = 2 + 2 * (1 + std::mem::size_of_val(&values));
                        let body_start = input.len() - body_len;
                        assert_eq!(&input[body_start..body_start + 2], &[2, 12]);
                        let explicit = f.serialize(&vec![values, values]).unwrap();
                        input.truncate(body_start);
                        input.extend_from_slice(&explicit[3..]);
                    }
                    let s = Schema::compile(SchemaDescriptor { encoding: RootEncoding { protocol_version: ProtocolVersion::Fory1_7, mode, root_reference: RootReferenceMode::NullOnly }, key_semantics_version: KEY_SEMANTICS_VERSION, root, definitions: if is_user { vec![UserDefinition { identity: UserIdentity::Numeric(732), kind: UserDefinitionKind::Struct { fields: vec![FieldDescriptor { id: None, name: "items".into(), ty: list.clone() }], physical_field_order: vec![0] } }] } else { vec![] } }, &SchemaLimits::default()).unwrap();
                    let view = s.view(&input, InputFormat::XlangMessage, &ParseLimits::default()).unwrap();
                    let root = view.root().unwrap();
                    let container = if is_user { root.field_at(0).unwrap() } else { view.root().unwrap() };
                    assert_eq!(container.element_at(1).unwrap().element_at(2).unwrap().$read().unwrap(), values[2]);
                    assert_eq!(container.elements().unwrap().map(|n| n.unwrap().element_at(0).unwrap().$read().unwrap()).collect::<Vec<_>>(), [values[0], values[0]]);
                    let owned = container.element_at(1).unwrap().element_at(1).unwrap().to_owned().unwrap();
                    drop(container); drop(root);
                    let validated = view.validate().unwrap();
                    let root = validated.root().unwrap();
                    let container = if is_user { root.field_at(0).unwrap() } else { validated.root().unwrap() };
                    assert_eq!(container.element_at(0).unwrap().elements().unwrap().map(|n| n.unwrap().$read().unwrap()).collect::<Vec<_>>(), values);
                    drop(container); drop(root); drop(validated); drop(s); drop(input);
                    assert_eq!(owned.as_view().unwrap().$read().unwrap(), values[1]);
                }
                let root = ty(TypeKind::Map { key: Box::new(ty(TypeKind::Scalar(ScalarKind::String), false)), value: Box::new(array) }, false);
                let s = schema(root, mode, RootReferenceMode::NullOnly);
                let input = f.serialize(&std::collections::BTreeMap::from([("key".to_owned(), values)])).unwrap();
                let view = s.view(&input, InputFormat::XlangMessage, &ParseLimits::default()).unwrap();
                assert_eq!(view.root().unwrap().entries().unwrap().next().unwrap().unwrap().1.element_at(2).unwrap().$read().unwrap(), values[2]);
                let validated = view.validate().unwrap();
                assert_eq!(validated.root().unwrap().entries().unwrap().next().unwrap().unwrap().1.element_at(0).unwrap().$read().unwrap(), values[0]);
            }};
        }
        check!(i32, I32, Varint, read_i32, [i32::MIN, -129, i32::MAX]);
        check!(u32, U32, Varint, read_u32, [0u32, 128, u32::MAX]);
        check!(i64, I64, Varint, read_i64, [i64::MIN, -129, i64::MAX]);
        check!(i64, I64, Tagged, read_i64, [i64::MIN, -129, i64::MAX]);
        check!(u64, U64, Varint, read_u64, [0u64, 128, u64::MAX]);
        check!(u64, U64, Tagged, read_u64, [0u64, 128, u64::MAX]);
    }
}

fn ty(kind: TypeKind, nullable: bool) -> TypeDescriptor {
    TypeDescriptor {
        nullable,
        track_ref: false,
        integer_encoding: None,
        kind,
    }
}
fn schema(root: TypeDescriptor, mode: SchemaMode, reference: RootReferenceMode) -> Schema {
    Schema::compile(
        SchemaDescriptor {
            encoding: RootEncoding {
                protocol_version: ProtocolVersion::Fory1_7,
                mode,
                root_reference: reference,
            },
            key_semantics_version: KEY_SEMANTICS_VERSION,
            root,
            definitions: vec![],
        },
        &SchemaLimits::default(),
    )
    .unwrap()
}

fn map_wire(format: InputFormat, len: u32, body: &[u8]) -> Vec<u8> {
    let mut input = vec![];
    let mut writer = fory::Writer::from_buffer(&mut input);
    if matches!(format, InputFormat::XlangMessage) {
        writer.write_u8(1);
    }
    writer.write_i8(-1);
    writer.write_var_u32(24);
    writer.write_var_u32(len);
    writer.write_bytes(body);
    input
}

#[test]
fn map_chunk_wire_types_change_without_rewriting_the_logical_descriptor() {
    let s = schema(
        ty(
            TypeKind::Map {
                key: Box::new(ty(TypeKind::Scalar(ScalarKind::Bool), false)),
                value: Box::new(ty(
                    TypeKind::List(Box::new(ty(TypeKind::Scalar(ScalarKind::I16), false))),
                    false,
                )),
            },
            false,
        ),
        SchemaMode::Compatible,
        RootReferenceMode::NullOnly,
    );
    // Two identical keys remain two wire entries. The value group changes dense -> LIST.
    let input = map_wire(
        InputFormat::RootValue,
        2,
        &[0, 1, 1, 45, 1, 2, 7, 0, 0, 1, 1, 22, 1, 1, 8, 3, 9, 0],
    );
    let view = s
        .view(&input, InputFormat::RootValue, &ParseLimits::default())
        .unwrap();
    let root = view.root().unwrap();
    let actual: Vec<_> = root
        .entries()
        .unwrap()
        .map(|p| {
            let (k, v) = p.unwrap();
            assert!(k.read_bool().unwrap());
            assert!(matches!(v.node_type().kind, TypeKind::List(_)));
            assert_eq!(v.len().unwrap(), 1);
            v.element_at(0).unwrap().read_i16().unwrap()
        })
        .collect();
    assert_eq!(actual, vec![7, 9]);
    let validated = view.validate().unwrap();
    let root = validated.root().unwrap();
    let actual: Vec<_> = root
        .entries()
        .unwrap()
        .map(|p| p.unwrap().1.element_at(0).unwrap().read_i16().unwrap())
        .collect();
    assert_eq!(actual, vec![7, 9]);
    let set = schema(
        ty(
            TypeKind::Set(Box::new(ty(TypeKind::Scalar(ScalarKind::Bool), false))),
            false,
        ),
        SchemaMode::Compatible,
        RootReferenceMode::NullOnly,
    );
    let input = [255, 23, 3, 12, 1, 1, 0];
    let validated = set
        .validate(&input, InputFormat::RootValue, &ParseLimits::default())
        .unwrap();
    let root = validated.root().unwrap();
    assert_eq!(root.len().unwrap(), 3);
    assert!(!root.element_at(2).unwrap().read_bool().unwrap());
    assert_eq!(
        root.elements()
            .unwrap()
            .map(|n| n.unwrap().read_bool().unwrap())
            .collect::<Vec<_>>(),
        vec![true, true, false]
    );
}

#[test]
fn deep_nested_validation_uses_heap_frames_and_checks_inner_array_length() {
    let depth = 2048;
    let mut root = ty(TypeKind::Scalar(ScalarKind::Bool), false);
    for _ in 0..depth {
        root = ty(TypeKind::List(Box::new(root)), false);
    }
    let s = schema(root, SchemaMode::Compatible, RootReferenceMode::NullOnly);
    let mut input = vec![];
    let mut writer = fory::Writer::from_buffer(&mut input);
    writer.write_i8(-1);
    writer.write_var_u32(22);
    for _ in 0..depth {
        writer.write_var_u32(1);
        writer.write_u8(12);
    }
    writer.write_u8(1);
    assert_eq!(
        s.validate(&input, InputFormat::RootValue, &ParseLimits::default())
            .unwrap()
            .root()
            .unwrap()
            .len()
            .unwrap(),
        1
    );
    let fory = fory::Fory::builder().xlang(true).build();
    let data = vec![Some([true, false]), None, Some([false, true])];
    let input = fory.serialize(&data).unwrap();
    for expected in [2, 3] {
        let s = schema(
            ty(
                TypeKind::List(Box::new(ty(
                    TypeKind::Array {
                        element: Box::new(ty(TypeKind::Scalar(ScalarKind::Bool), false)),
                        length: Some(expected),
                    },
                    true,
                ))),
                false,
            ),
            SchemaMode::Compatible,
            RootReferenceMode::NullOnly,
        );
        if expected == 3 {
            assert!(matches!(
                s.validate(&input, InputFormat::XlangMessage, &ParseLimits::default()),
                Err(DataError::SchemaMismatch(_))
            ));
        } else {
            let validated = s
                .validate(&input, InputFormat::XlangMessage, &ParseLimits::default())
                .unwrap();
            let root = validated.root().unwrap();
            assert!(root.element_at(1).unwrap().is_null().unwrap());
            assert_eq!(root.element_at(2).unwrap().len().unwrap(), 2);
        }
    }
}

#[test]
fn maps_preserve_scalar_content_time_and_container_keys() {
    use std::collections::BTreeMap;
    let fory = fory::Fory::builder().xlang(true).build();
    macro_rules! check {
        ($kind:ident, $read:ident, $values:expr) => {{
            let values = $values;
            let input = fory.serialize(&values).unwrap();
            let s = schema(
                ty(
                    TypeKind::Map {
                        key: Box::new(ty(TypeKind::Scalar(ScalarKind::String), false)),
                        value: Box::new(ty(TypeKind::Scalar(ScalarKind::$kind), false)),
                    },
                    false,
                ),
                SchemaMode::Compatible,
                RootReferenceMode::NullOnly,
            );
            let view = s
                .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
                .unwrap();
            for (pair, (_, expected)) in view.root().unwrap().entries().unwrap().zip(values.iter())
            {
                assert_eq!(pair.unwrap().1.$read().unwrap(), *expected);
            }
            let validated = view.validate().unwrap();
            for (pair, (_, expected)) in validated
                .root()
                .unwrap()
                .entries()
                .unwrap()
                .zip(values.iter())
            {
                assert_eq!(pair.unwrap().1.$read().unwrap(), *expected);
            }
        }};
    }
    check!(Bool, read_bool, BTreeMap::from([("a".to_owned(), true)]));
    check!(
        Date,
        read_date,
        BTreeMap::from([("a".to_owned(), fory::Date::from_epoch_days(-1))])
    );
    check!(
        Timestamp,
        read_timestamp,
        BTreeMap::from([("a".to_owned(), fory::Timestamp::new(-1, 3).unwrap())])
    );
    check!(
        Duration,
        read_duration,
        BTreeMap::from([("a".to_owned(), fory::Duration::new(0, -1).unwrap())])
    );
    let values = BTreeMap::from([(vec![1i16, 2], vec![0u8, 255])]);
    let input = fory.serialize(&values).unwrap();
    let s = schema(
        ty(
            TypeKind::Map {
                key: Box::new(ty(
                    TypeKind::List(Box::new(ty(TypeKind::Scalar(ScalarKind::I16), false))),
                    false,
                )),
                value: Box::new(ty(TypeKind::Scalar(ScalarKind::Binary), false)),
            },
            false,
        ),
        SchemaMode::Compatible,
        RootReferenceMode::NullOnly,
    );
    let validated = s
        .validate(&input, InputFormat::XlangMessage, &ParseLimits::default())
        .unwrap();
    let root = validated.root().unwrap();
    let (key, value) = root.entries().unwrap().next().unwrap().unwrap();
    assert_eq!(key.element_at(1).unwrap().read_i16().unwrap(), 2);
    assert_eq!(value.read_binary().unwrap().as_bytes(), &[0, 255]);
}

#[test]
fn container_ids_are_exact_and_nested_dense_integer_uses_actual_fixed_body() {
    let fory = fory::Fory::builder().xlang(true).build();
    let element = || ty(TypeKind::Scalar(ScalarKind::Bool), false);
    for (kind, id) in [
        (TypeKind::List(Box::new(element())), 22),
        (TypeKind::Set(Box::new(element())), 23),
        (
            TypeKind::Map {
                key: Box::new(element()),
                value: Box::new(element()),
            },
            24,
        ),
    ] {
        let s = schema(
            ty(kind, false),
            SchemaMode::Compatible,
            RootReferenceMode::NullOnly,
        );
        for wire_id in [22, 23, 24] {
            let input = [255, wire_id, 0];
            if wire_id == id {
                s.validate(&input, InputFormat::RootValue, &ParseLimits::default())
                    .unwrap();
            } else {
                assert!(matches!(
                    s.view(&input, InputFormat::RootValue, &ParseLimits::default()),
                    Err(DataError::SchemaMismatch(_))
                ));
            }
        }
    }
    let mut integer = ty(TypeKind::Scalar(ScalarKind::I32), false);
    integer.integer_encoding = Some(IntegerEncoding::Varint);
    let s = schema(
        ty(
            TypeKind::List(Box::new(ty(TypeKind::List(Box::new(integer)), false))),
            false,
        ),
        SchemaMode::Compatible,
        RootReferenceMode::NullOnly,
    );
    let input = fory.serialize(&vec![vec![1i32]]).unwrap();
    let validated = s
        .validate(&input, InputFormat::XlangMessage, &ParseLimits::default())
        .unwrap();
    assert_eq!(
        validated
            .root()
            .unwrap()
            .element_at(0)
            .unwrap()
            .element_at(0)
            .unwrap()
            .read_i32()
            .unwrap(),
        1
    );
}

#[test]
fn map_chunks_nulls_declared_types_errors_and_first_failure() {
    for mode in [
        SchemaMode::Compatible,
        SchemaMode::SameSchema {
            version_hash: false,
        },
        SchemaMode::SameSchema { version_hash: true },
    ] {
        for reference in [RootReferenceMode::NullOnly, RootReferenceMode::Tracking] {
            for format in [InputFormat::RootValue, InputFormat::XlangMessage] {
                let boolean = || ty(TypeKind::Scalar(ScalarKind::Bool), true);
                let s = schema(
                    ty(
                        TypeKind::Map {
                            key: Box::new(boolean()),
                            value: Box::new(boolean()),
                        },
                        false,
                    ),
                    mode,
                    reference,
                );
                // Cross chunk declared -> typed, then both-null, key-null, value-null.
                let input = map_wire(
                    format,
                    6,
                    &[36, 1, 1, 0, 0, 2, 1, 1, 0, 1, 1, 0, 18, 34, 1, 20, 0],
                );
                let view = s
                    .view(
                        &input,
                        format,
                        &ParseLimits {
                            max_input_bytes: input.len(),
                        },
                    )
                    .unwrap();
                let root = view.root().unwrap();
                assert!(matches!(root.elements(), Err(DataError::AccessError(_))));
                assert!(matches!(root.element_at(0), Err(DataError::AccessError(_))));
                let actual: Vec<_> = root
                    .entries()
                    .unwrap()
                    .map(|pair| {
                        let (k, v) = pair.unwrap();
                        (
                            if k.is_null().unwrap() {
                                None
                            } else {
                                Some(k.read_bool().unwrap())
                            },
                            if v.is_null().unwrap() {
                                None
                            } else {
                                Some(v.read_bool().unwrap())
                            },
                        )
                    })
                    .collect();
                assert_eq!(
                    actual,
                    vec![
                        (Some(true), Some(false)),
                        (Some(false), Some(true)),
                        (Some(true), Some(false)),
                        (None, None),
                        (None, Some(true)),
                        (Some(false), None)
                    ]
                );
                let validated = view.validate().unwrap();
                let root = validated.root().unwrap();
                assert_eq!(
                    root.entries()
                        .unwrap()
                        .map(|p| {
                            p.unwrap();
                            1
                        })
                        .sum::<usize>(),
                    6
                );
                assert!(matches!(root.element_at(0), Err(DataError::AccessError(_))));
                assert!(matches!(
                    s.view(
                        &input,
                        format,
                        &ParseLimits {
                            max_input_bytes: input.len() - 1
                        }
                    ),
                    Err(DataError::ResourceLimit(_))
                ));
                for body in [
                    vec![36, 0],
                    vec![36, 2, 1, 0],
                    vec![128],
                    vec![0, 1, 2, 1, 0, 0],
                    vec![36, 1, 2, 0],
                    vec![36, 1, 1],
                ] {
                    let bad = map_wire(format, 1, &body);
                    let view = s.view(&bad, format, &ParseLimits::default()).unwrap();
                    let root = view.root().unwrap();
                    let mut iter = root.entries().unwrap();
                    let error = iter.next().unwrap().err().unwrap();
                    assert!(matches!(
                        error,
                        DataError::MalformedInput(_) | DataError::SchemaMismatch(_)
                    ));
                    assert!(iter.next().is_none());
                    assert_eq!(
                        format!("{error:?}"),
                        format!("{:?}", root.len().err().unwrap())
                    );
                    drop(iter);
                    drop(root);
                    assert_eq!(
                        format!("{error:?}"),
                        format!("{:?}", view.validate().err().unwrap())
                    );
                }
                for body in [vec![1, 1], vec![8, 1]] {
                    assert!(matches!(
                        s.validate(&map_wire(format, 1, &body), format, &ParseLimits::default()),
                        Err(DataError::NotImplemented(_))
                    ));
                }
                for end in 3..input.len() {
                    assert!(
                        s.validate(&input[..end], format, &ParseLimits::default())
                            .is_err()
                    );
                }
                let mut trailing = input.clone();
                trailing.push(0);
                assert!(matches!(
                    s.validate(&trailing, format, &ParseLimits::default()),
                    Err(DataError::MalformedInput(_))
                ));
                let empty = map_wire(format, 0, &[]);
                assert_eq!(
                    s.validate(&empty, format, &ParseLimits::default())
                        .unwrap()
                        .root()
                        .unwrap()
                        .entries()
                        .unwrap()
                        .count(),
                    0
                );
                let strict = schema(
                    ty(
                        TypeKind::Map {
                            key: Box::new(ty(TypeKind::Scalar(ScalarKind::Bool), false)),
                            value: Box::new(boolean()),
                        },
                        false,
                    ),
                    mode,
                    reference,
                );
                assert!(matches!(
                    strict.validate(&map_wire(format, 1, &[18]), format, &ParseLimits::default()),
                    Err(DataError::SchemaMismatch(_))
                ));
                let mut null = vec![];
                if matches!(format, InputFormat::XlangMessage) {
                    null.push(1);
                }
                null.push(253);
                let nullable = schema(
                    ty(
                        TypeKind::Map {
                            key: Box::new(boolean()),
                            value: Box::new(boolean()),
                        },
                        true,
                    ),
                    mode,
                    reference,
                );
                assert!(
                    nullable
                        .validate(&null, format, &ParseLimits::default())
                        .unwrap()
                        .root()
                        .unwrap()
                        .is_null()
                        .unwrap()
                );
            }
        }
    }
}

#[test]
fn actual_multichunk_map_and_nested_containers_keep_wire_order() {
    use std::collections::{BTreeMap, BTreeSet};
    let fory = fory::Fory::builder().xlang(true).build();
    let values: BTreeMap<_, _> = (0..1024)
        .map(|i| (format!("{i:04}"), BTreeSet::from([vec![i as i16], vec![]])))
        .collect();
    let input = fory.serialize(&values).unwrap();
    assert_eq!(
        fory.deserialize::<BTreeMap<String, BTreeSet<Vec<i16>>>>(&input)
            .unwrap(),
        values
    );
    let scalar = ty(TypeKind::Scalar(ScalarKind::I16), false);
    let s = schema(
        ty(
            TypeKind::Map {
                key: Box::new(ty(TypeKind::Scalar(ScalarKind::String), false)),
                value: Box::new(ty(
                    TypeKind::Set(Box::new(ty(
                        TypeKind::Array {
                            element: Box::new(scalar),
                            length: None,
                        },
                        false,
                    ))),
                    false,
                )),
            },
            false,
        ),
        SchemaMode::Compatible,
        RootReferenceMode::NullOnly,
    );
    let view = s
        .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
        .unwrap();
    let root = view.root().unwrap();
    for (index, pair) in root.entries().unwrap().enumerate() {
        let (key, value) = pair.unwrap();
        assert_eq!(key.read_string().unwrap().as_str(), format!("{index:04}"));
        assert_eq!(value.len().unwrap(), 2);
    }
    let validated = view.validate().unwrap();
    let root = validated.root().unwrap();
    let mut iter = root.entries().unwrap();
    let (_, v) = iter.next().unwrap().unwrap();
    let retained = v.to_owned().unwrap();
    assert_eq!(
        iter.map(|pair| {
            pair.unwrap();
            1
        })
        .sum::<usize>(),
        1023
    );
    drop(v);
    drop(root);
    drop(validated);
    drop(input);
    assert_eq!(retained.as_view().unwrap().len().unwrap(), 2);
}

#[test]
fn actual_sets_and_nullable_map_entries_survive_parents() {
    use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
    for mode in [
        SchemaMode::Compatible,
        SchemaMode::SameSchema { version_hash: true },
    ] {
        let fory = fory::Fory::builder()
            .xlang(true)
            .compatible(matches!(mode, SchemaMode::Compatible))
            .build();
        let text = || ty(TypeKind::Scalar(ScalarKind::String), true);
        let set_schema = schema(
            ty(TypeKind::Set(Box::new(text())), false),
            mode,
            RootReferenceMode::NullOnly,
        );
        let values = BTreeSet::from([None, Some("a".to_owned()), Some("b".to_owned())]);
        for input in [
            fory.serialize(&values).unwrap(),
            fory.serialize(&values.iter().cloned().collect::<HashSet<_>>())
                .unwrap(),
        ] {
            let view = set_schema
                .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
                .unwrap();
            let root = view.root().unwrap();
            assert_eq!(root.len().unwrap(), 3);
            assert_eq!(
                root.elements()
                    .unwrap()
                    .map(|n| n.unwrap().is_null().unwrap() as usize)
                    .sum::<usize>(),
                1
            );
            let validated = view.validate().unwrap();
            assert_eq!(validated.root().unwrap().elements().unwrap().count(), 3);
        }
        let map_schema = schema(
            ty(
                TypeKind::Map {
                    key: Box::new(text()),
                    value: Box::new(ty(
                        TypeKind::List(Box::new(ty(TypeKind::Scalar(ScalarKind::I16), false))),
                        true,
                    )),
                },
                false,
            ),
            mode,
            RootReferenceMode::NullOnly,
        );
        let values = BTreeMap::from([
            (None, None),
            (Some("a".to_owned()), Some(vec![1i16, 2])),
            (Some("b".to_owned()), None),
        ]);
        let another = BTreeMap::from([
            (None, Some(vec![3i16])),
            (Some("a".to_owned()), Some(vec![])),
        ]);
        for input in [
            fory.serialize(&values).unwrap(),
            fory.serialize(&another).unwrap(),
            fory.serialize(
                &values
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect::<HashMap<_, _>>(),
            )
            .unwrap(),
        ] {
            let view = map_schema
                .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
                .unwrap();
            let root = view.root().unwrap();
            assert!(matches!(root.element_at(0), Err(DataError::AccessError(_))));
            let len = root.len().unwrap();
            let mut iter = root.entries().unwrap();
            let (key, value) = iter.next().unwrap().unwrap();
            let key = key.to_owned().unwrap();
            let value = value.to_owned().unwrap();
            assert_eq!(
                iter.map(|pair| {
                    pair.unwrap();
                    1
                })
                .sum::<usize>()
                    + 1,
                len
            );
            drop(root);
            drop(view);
            let key = key.as_view().unwrap();
            if key.is_null().unwrap() {
                assert!(matches!(key.read_string(), Err(DataError::AccessError(_))));
            }
            let value = value.as_view().unwrap();
            if value.is_null().unwrap() {
                assert!(matches!(value.len(), Err(DataError::AccessError(_))));
            } else {
                value.len().unwrap();
            }
            let validated = map_schema
                .validate(&input, InputFormat::XlangMessage, &ParseLimits::default())
                .unwrap();
            let root = validated.root().unwrap();
            assert_eq!(root.len().unwrap(), len);
            for pair in root.entries().unwrap() {
                let (k, v) = pair.unwrap();
                if !k.is_null().unwrap() {
                    k.read_string().unwrap();
                }
                if !v.is_null().unwrap() {
                    v.elements().unwrap().for_each(|n| {
                        n.unwrap().read_i16().unwrap();
                    });
                }
            }
        }
    }
}

#[test]
fn nested_sequences_preserve_actual_group_wire_mode() {
    let fory = fory::Fory::builder().xlang(true).build();
    let dense = fory.serialize(&vec![vec![1i16, 2]]).unwrap();
    let generic = fory.serialize(&vec![VecDeque::from([1i16, 2])]).unwrap();
    assert_eq!(
        fory.deserialize::<Vec<Vec<i16>>>(&dense).unwrap(),
        vec![vec![1, 2]]
    );
    assert_eq!(
        fory.deserialize::<Vec<VecDeque<i16>>>(&generic).unwrap(),
        vec![VecDeque::from([1, 2])]
    );
    println!("dense={dense:?}; generic={generic:?}");
    let scalar = TypeDescriptor {
        nullable: false,
        track_ref: false,
        integer_encoding: None,
        kind: TypeKind::Scalar(ScalarKind::I16),
    };
    let sequence = |element| TypeDescriptor {
        nullable: false,
        track_ref: false,
        integer_encoding: None,
        kind: TypeKind::List(Box::new(element)),
    };
    let schema = Schema::compile(
        SchemaDescriptor {
            encoding: RootEncoding {
                protocol_version: ProtocolVersion::Fory1_7,
                mode: SchemaMode::SameSchema {
                    version_hash: false,
                },
                root_reference: RootReferenceMode::NullOnly,
            },
            key_semantics_version: KEY_SEMANTICS_VERSION,
            root: sequence(sequence(scalar)),
            definitions: vec![],
        },
        &SchemaLimits::default(),
    )
    .unwrap();
    for input in [dense, generic] {
        let view = schema
            .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
            .unwrap();
        let root = view.root().unwrap();
        let child = root.element_at(0).unwrap().to_owned().unwrap();
        drop(root);
        drop(view);
        let node = child.as_view().unwrap();
        assert_eq!(node.len().unwrap(), 2);
        assert_eq!(node.element_at(1).unwrap().read_i16().unwrap(), 2);
        let validated = schema
            .validate(&input, InputFormat::XlangMessage, &ParseLimits::default())
            .unwrap();
        let root = validated.root().unwrap();
        assert_eq!(root.element_at(0).unwrap().len().unwrap(), 2);
    }
}
