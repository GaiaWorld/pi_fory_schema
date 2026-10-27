use pi_fory_schema::*;

#[test]
fn tracked_plain_primitive_list_markers_and_float_bits_are_preserved() {
    macro_rules! check {
        ($rust:ident, $kind:ident, $read:ident, $bits:expr) => {{
            #[derive(fory::ForyStruct)]
            struct Plain {
                #[fory(ref = true)]
                value: Vec<$rust>,
            }
            for mode in [
                SchemaMode::SameSchema { version_hash: true },
                SchemaMode::Compatible,
            ] {
                let mut f = fory::Fory::builder()
                    .xlang(true)
                    .compatible(matches!(mode, SchemaMode::Compatible))
                    .check_struct_version(matches!(
                        mode,
                        SchemaMode::SameSchema { version_hash: true }
                    ))
                    .build();
                f.register::<Plain>(749).unwrap();
                let bits = $bits;
                let input = f
                    .serialize(&Plain {
                        value: bits.map($rust::from_bits).to_vec(),
                    })
                    .unwrap();
                let s = Schema::compile(
                    SchemaDescriptor {
                        encoding: RootEncoding {
                            protocol_version: ProtocolVersion::Fory1_7,
                            mode,
                            root_reference: RootReferenceMode::NullOnly,
                        },
                        key_semantics_version: KEY_SEMANTICS_VERSION,
                        root: ty(TypeKind::User(UserIdentity::Numeric(749)), false),
                        definitions: vec![UserDefinition {
                            identity: UserIdentity::Numeric(749),
                            kind: UserDefinitionKind::Struct {
                                fields: vec![FieldDescriptor {
                                    id: None,
                                    name: "value".into(),
                                    ty: TypeDescriptor {
                                        kind: TypeKind::List(Box::new(ty(
                                            TypeKind::Scalar(ScalarKind::$kind),
                                            false,
                                        ))),
                                        nullable: false,
                                        track_ref: true,
                                        integer_encoding: None,
                                    },
                                }],
                                physical_field_order: vec![0],
                            },
                        }],
                    },
                    &SchemaLimits::default(),
                )
                .unwrap();
                let view = s
                    .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
                    .unwrap();
                let root = view.root().unwrap();
                let list = root.field_at(0).unwrap();
                assert!(list.object_id().unwrap().is_none());
                assert!(list.reference_id().unwrap().is_none());
                assert_eq!(
                    list.elements()
                        .unwrap()
                        .map(|n| n.unwrap().$read().unwrap().to_bits())
                        .collect::<Vec<_>>(),
                    bits
                );
                let owned = list.element_at(1).unwrap().to_owned().unwrap();
                drop(list);
                drop(root);
                let validated = view.validate().unwrap();
                let root = validated.root().unwrap();
                let list = root.field_at(0).unwrap();
                for i in (0..bits.len()).rev() {
                    assert_eq!(
                        list.element_at(i).unwrap().$read().unwrap().to_bits(),
                        bits[i]
                    );
                }
                assert!(list.object_id().unwrap().is_none());
                let retained = list.element_at(0).unwrap().to_owned().unwrap();
                drop(list);
                drop(root);
                drop(validated);
                drop(input);
                drop(s);
                assert_eq!(owned.as_view().unwrap().$read().unwrap().to_bits(), bits[1]);
                assert_eq!(
                    retained.as_view().unwrap().$read().unwrap().to_bits(),
                    bits[0]
                );
            }
        }};
    }
    check!(
        f32,
        F32,
        read_f32,
        [0x8000_0000u32, 0x7fc0_1234, 0xff80_0000, 0x0000_0001]
    );
    check!(
        f64,
        F64,
        read_f64,
        [
            0x8000_0000_0000_0000u64,
            0x7ff8_0000_0000_1234,
            0xfff0_0000_0000_0000,
            1
        ]
    );
}

#[test]
fn compatible_static_shared_primitive_lists_preserve_reference_edges() {
    macro_rules! check {
        ($owner:ident, $module:ident) => {{
            #[derive(fory::ForyStruct)]
            struct Shared {
                first: std::$module::$owner<Vec<i32>>,
                second: std::$module::$owner<Vec<i32>>,
            }
            for named in [false, true] {
                let mut f = fory::Fory::builder()
                    .xlang(true)
                    .compatible(true)
                    .check_struct_version(false)
                    .build();
                let identity = if named {
                    f.register_by_name::<Shared>("lists.Shared").unwrap();
                    UserIdentity::Name {
                        namespace: "lists".into(),
                        type_name: "Shared".into(),
                    }
                } else {
                    f.register::<Shared>(748).unwrap();
                    UserIdentity::Numeric(748)
                };
                let list = TypeDescriptor {
                    kind: TypeKind::List(Box::new(TypeDescriptor {
                        kind: TypeKind::Scalar(ScalarKind::I32),
                        nullable: false,
                        track_ref: false,
                        integer_encoding: Some(IntegerEncoding::Varint),
                    })),
                    nullable: false,
                    track_ref: true,
                    integer_encoding: None,
                };
                let s = Schema::compile(
                    SchemaDescriptor {
                        encoding: RootEncoding {
                            protocol_version: ProtocolVersion::Fory1_7,
                            mode: SchemaMode::Compatible,
                            root_reference: RootReferenceMode::NullOnly,
                        },
                        key_semantics_version: KEY_SEMANTICS_VERSION,
                        root: ty(TypeKind::User(identity.clone()), false),
                        definitions: vec![UserDefinition {
                            identity,
                            kind: UserDefinitionKind::Struct {
                                fields: vec![
                                    FieldDescriptor {
                                        id: None,
                                        name: "first".into(),
                                        ty: list.clone(),
                                    },
                                    FieldDescriptor {
                                        id: None,
                                        name: "second".into(),
                                        ty: list,
                                    },
                                ],
                                physical_field_order: vec![0, 1],
                            },
                        }],
                    },
                    &SchemaLimits::default(),
                )
                .unwrap();
                let shared = std::$module::$owner::new(vec![i32::MIN, -129, i32::MAX]);
                let input = f
                    .serialize(&Shared {
                        first: shared.clone(),
                        second: shared,
                    })
                    .unwrap();
                let view = s
                    .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
                    .unwrap();
                let root = view.root().unwrap();
                let first = root.field_at(0).unwrap();
                let edge = root.field_at(1).unwrap();
                let id = first.object_id().unwrap().unwrap();
                assert_eq!(edge.reference_id().unwrap().unwrap(), id);
                assert!(matches!(edge.len(), Err(DataError::AccessError(_))));
                assert_eq!(
                    edge.resolve_reference(&id)
                        .unwrap()
                        .element_at(2)
                        .unwrap()
                        .read_i32()
                        .unwrap(),
                    i32::MAX
                );
                assert_eq!(first.element_at(0).unwrap().read_i32().unwrap(), i32::MIN);
                let owned = edge.to_owned().unwrap();
                drop(edge);
                drop(first);
                drop(root);
                let validated = view.validate().unwrap();
                let root = validated.root().unwrap();
                let edge = root.field_at(1).unwrap();
                assert!(matches!(edge.elements(), Err(DataError::AccessError(_))));
                assert_eq!(
                    edge.resolve_reference(&id)
                        .unwrap()
                        .elements()
                        .unwrap()
                        .map(|n| n.unwrap().read_i32().unwrap())
                        .collect::<Vec<_>>(),
                    [i32::MIN, -129, i32::MAX]
                );
                drop(edge);
                drop(root);
                drop(validated);
                drop(input);
                drop(s);
                assert_eq!(
                    owned
                        .as_view()
                        .unwrap()
                        .resolve_reference(&id)
                        .unwrap()
                        .element_at(1)
                        .unwrap()
                        .read_i32()
                        .unwrap(),
                    -129
                );
            }
        }};
    }
    check!(Rc, rc);
    check!(Arc, sync);
}

#[test]
fn static_list_type_proof_rejects_dense_and_preserves_hashless_boundary() {
    #[derive(fory::ForyStruct)]
    struct Generic {
        value: Vec<i32>,
    }
    #[derive(fory::ForyStruct)]
    struct Dense {
        #[fory(array)]
        value: Vec<i32>,
    }
    #[derive(fory::ForyStruct)]
    struct Bytes {
        #[fory(bytes)]
        value: Vec<u8>,
    }
    for mode in [
        SchemaMode::SameSchema { version_hash: true },
        SchemaMode::Compatible,
        SchemaMode::SameSchema {
            version_hash: false,
        },
    ] {
        let descriptor = SchemaDescriptor {
            encoding: RootEncoding {
                protocol_version: ProtocolVersion::Fory1_7,
                mode,
                root_reference: RootReferenceMode::NullOnly,
            },
            key_semantics_version: KEY_SEMANTICS_VERSION,
            root: ty(TypeKind::User(UserIdentity::Numeric(745)), false),
            definitions: vec![UserDefinition {
                identity: UserIdentity::Numeric(745),
                kind: UserDefinitionKind::Struct {
                    fields: vec![FieldDescriptor {
                        id: None,
                        name: "value".into(),
                        ty: ty(
                            TypeKind::List(Box::new(TypeDescriptor {
                                kind: TypeKind::Scalar(ScalarKind::I32),
                                integer_encoding: Some(IntegerEncoding::Varint),
                                nullable: false,
                                track_ref: false,
                            })),
                            false,
                        ),
                    }],
                    physical_field_order: vec![0],
                },
            }],
        };
        let s = Schema::compile(descriptor.clone(), &SchemaLimits::default()).unwrap();
        let make = || {
            fory::Fory::builder()
                .xlang(true)
                .compatible(matches!(mode, SchemaMode::Compatible))
                .check_struct_version(matches!(
                    mode,
                    SchemaMode::SameSchema { version_hash: true }
                ))
                .build()
        };
        let mut generic = make();
        generic.register::<Generic>(745).unwrap();
        let mut dense = make();
        dense.register::<Dense>(745).unwrap();
        let input = dense
            .serialize(&Dense {
                value: vec![1, 128],
            })
            .unwrap();
        if matches!(
            mode,
            SchemaMode::SameSchema {
                version_hash: false
            }
        ) {
            let real = generic.serialize(&Generic { value: vec![1] }).unwrap();
            assert!(matches!(
                s.validate(&real, InputFormat::XlangMessage, &ParseLimits::default()),
                Err(DataError::NotImplemented(_))
            ));
            assert!(matches!(
                s.validate(&input, InputFormat::XlangMessage, &ParseLimits::default()),
                Err(DataError::NotImplemented(_))
            ));
        } else {
            assert!(matches!(
                s.validate(&input, InputFormat::XlangMessage, &ParseLimits::default()),
                Err(DataError::SchemaMismatch(_))
            ));
            if let Ok(view) = s.view(&input, InputFormat::XlangMessage, &ParseLimits::default()) {
                let root = view.root().unwrap();
                let error = root.field_at(0).err().unwrap();
                assert!(matches!(error, DataError::SchemaMismatch(_)));
                assert_eq!(
                    format!("{error:?}"),
                    format!("{:?}", root.fields().err().unwrap())
                );
            }
            let mut bytes = make();
            bytes.register::<Bytes>(745).unwrap();
            let input = bytes
                .serialize(&Bytes {
                    value: vec![0, 255],
                })
                .unwrap();
            let mut d = descriptor;
            if let UserDefinitionKind::Struct { fields, .. } = &mut d.definitions[0].kind {
                fields[0].ty = ty(
                    TypeKind::List(Box::new(ty(TypeKind::Scalar(ScalarKind::U8), false))),
                    false,
                );
            }
            assert!(matches!(
                Schema::compile(d, &SchemaLimits::default())
                    .unwrap()
                    .validate(&input, InputFormat::XlangMessage, &ParseLimits::default()),
                Err(DataError::SchemaMismatch(_))
            ));
        }
    }
}

#[test]
fn canonical_nested_static_lists_and_nullable_groups_keep_declared_codecs() {
    #[derive(Debug, PartialEq, fory::ForyStruct)]
    struct Nested {
        a: Option<Vec<i32>>,
        b: Vec<Option<i64>>,
        c: Vec<Vec<i32>>,
        d: std::collections::VecDeque<u16>,
        e: std::collections::LinkedList<i8>,
    }
    let scalar = |kind, integer_encoding, nullable| TypeDescriptor {
        kind: TypeKind::Scalar(kind),
        integer_encoding,
        nullable,
        track_ref: false,
    };
    let list = |element| ty(TypeKind::List(Box::new(element)), false);
    for mode in [
        SchemaMode::SameSchema { version_hash: true },
        SchemaMode::Compatible,
    ] {
        let s = Schema::compile(
            SchemaDescriptor {
                encoding: RootEncoding {
                    protocol_version: ProtocolVersion::Fory1_7,
                    mode,
                    root_reference: RootReferenceMode::NullOnly,
                },
                key_semantics_version: KEY_SEMANTICS_VERSION,
                root: ty(TypeKind::User(UserIdentity::Numeric(746)), false),
                definitions: vec![UserDefinition {
                    identity: UserIdentity::Numeric(746),
                    kind: UserDefinitionKind::Struct {
                        fields: vec![
                            FieldDescriptor {
                                id: None,
                                name: "a".into(),
                                ty: TypeDescriptor {
                                    nullable: true,
                                    ..list(scalar(
                                        ScalarKind::I32,
                                        Some(IntegerEncoding::Varint),
                                        false,
                                    ))
                                },
                            },
                            FieldDescriptor {
                                id: None,
                                name: "b".into(),
                                ty: list(scalar(
                                    ScalarKind::I64,
                                    Some(IntegerEncoding::Varint),
                                    true,
                                )),
                            },
                            FieldDescriptor {
                                id: None,
                                name: "c".into(),
                                ty: list(list(scalar(
                                    ScalarKind::I32,
                                    Some(IntegerEncoding::Varint),
                                    false,
                                ))),
                            },
                            FieldDescriptor {
                                id: None,
                                name: "d".into(),
                                ty: list(scalar(ScalarKind::U16, None, false)),
                            },
                            FieldDescriptor {
                                id: None,
                                name: "e".into(),
                                ty: list(scalar(ScalarKind::I8, None, false)),
                            },
                        ],
                        physical_field_order: vec![0, 1, 2, 3, 4],
                    },
                }],
            },
            &SchemaLimits::default(),
        )
        .unwrap();
        let mut f = fory::Fory::builder()
            .xlang(true)
            .compatible(matches!(mode, SchemaMode::Compatible))
            .check_struct_version(matches!(
                mode,
                SchemaMode::SameSchema { version_hash: true }
            ))
            .build();
        f.register::<Nested>(746).unwrap();
        for a in [None, Some(vec![i32::MIN, i32::MAX])] {
            let expected = Nested {
                a,
                b: vec![None, Some(i64::MAX)],
                c: vec![vec![i32::MIN, -129]],
                d: [u16::MAX].into(),
                e: [i8::MIN].into(),
            };
            let input = f.serialize(&expected).unwrap();
            assert_eq!(f.deserialize::<Nested>(&input).unwrap(), expected);
            let validated = s
                .validate(&input, InputFormat::XlangMessage, &ParseLimits::default())
                .unwrap();
            let root = validated.root().unwrap();
            assert_eq!(
                root.field_at(0).unwrap().is_null().unwrap(),
                expected.a.is_none()
            );
            assert!(
                root.field_at(1)
                    .unwrap()
                    .element_at(0)
                    .unwrap()
                    .is_null()
                    .unwrap()
            );
            assert_eq!(
                root.field_at(1)
                    .unwrap()
                    .element_at(1)
                    .unwrap()
                    .read_i64()
                    .unwrap(),
                i64::MAX
            );
            assert_eq!(
                root.field_at(2)
                    .unwrap()
                    .element_at(0)
                    .unwrap()
                    .element_at(1)
                    .unwrap()
                    .read_i32()
                    .unwrap(),
                -129
            );
            assert_eq!(
                root.field_at(3)
                    .unwrap()
                    .element_at(0)
                    .unwrap()
                    .read_u16()
                    .unwrap(),
                u16::MAX
            );
            assert_eq!(
                root.field_at(4)
                    .unwrap()
                    .element_at(0)
                    .unwrap()
                    .read_i8()
                    .unwrap(),
                i8::MIN
            );
        }
    }
}

#[test]
fn canonical_static_primitive_lists_use_generic_bodies_after_type_proof() {
    macro_rules! check {
        ($(#[$attr:meta])* {$($rust:tt)*}, $kind:ident, $encoding:expr, $read:ident, $values:expr) => {{
            #[derive(Debug, PartialEq, fory::ForyStruct)]
            struct Lists { $(#[$attr])* value: Vec<$($rust)*> }
            for mode in [SchemaMode::SameSchema { version_hash: true }, SchemaMode::Compatible] {
                for named in [false, true] {
                    let mut f = fory::Fory::builder().xlang(true).compatible(matches!(mode, SchemaMode::Compatible)).check_struct_version(matches!(mode, SchemaMode::SameSchema { version_hash: true })).build();
                    let identity = if named { f.register_by_name::<Lists>("lists.Primitive").unwrap(); UserIdentity::Name { namespace: "lists".into(), type_name: "Primitive".into() } } else { f.register::<Lists>(744).unwrap(); UserIdentity::Numeric(744) };
                    let descriptor = SchemaDescriptor {
                        encoding: RootEncoding { protocol_version: ProtocolVersion::Fory1_7, mode, root_reference: RootReferenceMode::NullOnly }, key_semantics_version: KEY_SEMANTICS_VERSION,
                        root: ty(TypeKind::User(identity.clone()), false),
                        definitions: vec![UserDefinition { identity, kind: UserDefinitionKind::Struct { fields: vec![FieldDescriptor { id: None, name: "value".into(), ty: ty(TypeKind::List(Box::new(TypeDescriptor { kind: TypeKind::Scalar(ScalarKind::$kind), nullable: false, track_ref: false, integer_encoding: $encoding })), false) }], physical_field_order: vec![0] } }],
                    };
                    let s = Schema::compile(descriptor, &SchemaLimits::default()).unwrap();
                    let values = $values;
                    for values in [values.to_vec(), vec![]] {
                        let expected = Lists { value: values.clone() };
                        let input = f.serialize(&expected).unwrap();
                        assert_eq!(f.deserialize::<Lists>(&input).unwrap(), expected);
                        let view = s.view(&input, InputFormat::XlangMessage, &ParseLimits::default()).unwrap();
                        let root = view.root().unwrap(); let list = root.field_at(0).unwrap();
                        assert_eq!(list.len().unwrap(), values.len());
                        for i in (0..values.len()).rev() { assert_eq!(list.element_at(i).unwrap().$read().unwrap(), values[i]); }
                        assert_eq!(list.elements().unwrap().map(|n| n.unwrap().$read().unwrap()).collect::<Vec<_>>(), values);
                        let owned = list.to_owned().unwrap(); drop(list); drop(root);
                        let validated = view.validate().unwrap();
                        let retained = validated.root().unwrap().field_at(0).unwrap().to_owned().unwrap();
                        assert_eq!(retained.as_view().unwrap().elements().unwrap().map(|n| n.unwrap().$read().unwrap()).collect::<Vec<_>>(), values);
                        drop(validated);
                        for end in 0..input.len() { assert!(s.validate(&input[..end], InputFormat::XlangMessage, &ParseLimits::default()).is_err()); }
                        let mut trailing = input.clone(); trailing.push(0);
                        assert!(matches!(s.validate(&trailing, InputFormat::XlangMessage, &ParseLimits::default()), Err(DataError::MalformedInput(_))));
                        drop(input);
                        assert_eq!(owned.as_view().unwrap().elements().unwrap().map(|n| n.unwrap().$read().unwrap()).collect::<Vec<_>>(), values);
                    }
                }
            }
        }};
    }
    check!({ bool }, Bool, None, read_bool, [false, true]);
    check!({ i8 }, I8, None, read_i8, [i8::MIN, i8::MAX]);
    check!({ i16 }, I16, None, read_i16, [i16::MIN, i16::MAX]);
    check!({ u8 }, U8, None, read_u8, [0, 128, u8::MAX]);
    check!({ u16 }, U16, None, read_u16, [0, u16::MAX]);
    check!(
        { i32 },
        I32,
        Some(IntegerEncoding::Varint),
        read_i32,
        [i32::MIN, -129, i32::MAX]
    );
    check!(
        { i64 },
        I64,
        Some(IntegerEncoding::Varint),
        read_i64,
        [i64::MIN, -129, i64::MAX]
    );
    check!(
        { u32 },
        U32,
        Some(IntegerEncoding::Varint),
        read_u32,
        [0, 128, u32::MAX]
    );
    check!(
        { u64 },
        U64,
        Some(IntegerEncoding::Varint),
        read_u64,
        [0, 128, u64::MAX]
    );
    check!(
        #[fory(list(element(encoding = "fixed")))]
        {
            i32
        },
        I32,
        Some(IntegerEncoding::Fixed),
        read_i32,
        [i32::MIN, i32::MAX]
    );
    check!(
        #[fory(list(element(encoding = "tagged")))]
        {
            i64
        },
        I64,
        Some(IntegerEncoding::Tagged),
        read_i64,
        [i64::MIN, i64::MAX]
    );
    check!(
        #[fory(list(element(encoding = "tagged")))]
        {
            u64
        },
        U64,
        Some(IntegerEncoding::Tagged),
        read_u64,
        [0, u64::MAX]
    );
    check!(
        { f32 },
        F32,
        None,
        read_f32,
        [f32::NEG_INFINITY, -0.0, f32::MAX]
    );
    check!(
        { f64 },
        F64,
        None,
        read_f64,
        [f64::NEG_INFINITY, -0.0, f64::MAX]
    );
}

#[test]
fn canonical_static_primitive_array_fields_read_actual_dense_bodies() {
    macro_rules! check {
        ($rust:ident, $kind:ident, $encoding:expr, $read:ident, $values:expr) => {{
            #[derive(Debug, PartialEq, fory::ForyStruct)]
            struct Arrays {
                value: [$rust; 3],
            }
            for mode in [
                SchemaMode::SameSchema {
                    version_hash: false,
                },
                SchemaMode::SameSchema { version_hash: true },
                SchemaMode::Compatible,
            ] {
                for named in [false, true] {
                    let mut f = fory::Fory::builder()
                        .xlang(true)
                        .compatible(matches!(mode, SchemaMode::Compatible))
                        .check_struct_version(matches!(
                            mode,
                            SchemaMode::SameSchema { version_hash: true }
                        ))
                        .build();
                    let identity = if named {
                        f.register_by_name::<Arrays>("arrays.Primitive").unwrap();
                        UserIdentity::Name {
                            namespace: "arrays".into(),
                            type_name: "Primitive".into(),
                        }
                    } else {
                        f.register::<Arrays>(741).unwrap();
                        UserIdentity::Numeric(741)
                    };
                    let descriptor = SchemaDescriptor {
                        encoding: RootEncoding {
                            protocol_version: ProtocolVersion::Fory1_7,
                            mode,
                            root_reference: RootReferenceMode::NullOnly,
                        },
                        key_semantics_version: KEY_SEMANTICS_VERSION,
                        root: ty(TypeKind::User(identity.clone()), false),
                        definitions: vec![UserDefinition {
                            identity,
                            kind: UserDefinitionKind::Struct {
                                fields: vec![FieldDescriptor {
                                    id: None,
                                    name: "value".into(),
                                    ty: ty(
                                        TypeKind::Array {
                                            element: Box::new(TypeDescriptor {
                                                kind: TypeKind::Scalar(ScalarKind::$kind),
                                                nullable: false,
                                                track_ref: false,
                                                integer_encoding: $encoding,
                                            }),
                                            length: Some(3),
                                        },
                                        false,
                                    ),
                                }],
                                physical_field_order: vec![0],
                            },
                        }],
                    };
                    let s = Schema::compile(descriptor.clone(), &SchemaLimits::default()).unwrap();
                    let values = $values;
                    let input = f.serialize(&Arrays { value: values }).unwrap();
                    let view = s
                        .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
                        .unwrap();
                    let root = view.root().unwrap();
                    let array = root.field_at(0).unwrap();
                    assert_eq!(array.len().unwrap(), 3);
                    for i in (0..3).rev() {
                        assert_eq!(array.element_at(i).unwrap().$read().unwrap(), values[i]);
                    }
                    assert_eq!(
                        array
                            .elements()
                            .unwrap()
                            .map(|n| n.unwrap().$read().unwrap())
                            .collect::<Vec<_>>(),
                        values
                    );
                    let owned = array.element_at(1).unwrap().to_owned().unwrap();
                    drop(array);
                    drop(root);
                    let validated = view.validate().unwrap();
                    assert_eq!(
                        validated
                            .root()
                            .unwrap()
                            .field_at(0)
                            .unwrap()
                            .elements()
                            .unwrap()
                            .map(|n| n.unwrap().$read().unwrap())
                            .collect::<Vec<_>>(),
                        values
                    );
                    let retained = validated
                        .root()
                        .unwrap()
                        .field_at(0)
                        .unwrap()
                        .element_at(2)
                        .unwrap()
                        .to_owned()
                        .unwrap();
                    drop(validated);
                    for end in 0..input.len() {
                        assert!(
                            s.validate(
                                &input[..end],
                                InputFormat::XlangMessage,
                                &ParseLimits::default()
                            )
                            .is_err()
                        );
                    }
                    let mut trailing = input.clone();
                    trailing.push(0);
                    assert!(matches!(
                        s.validate(
                            &trailing,
                            InputFormat::XlangMessage,
                            &ParseLimits::default()
                        ),
                        Err(DataError::MalformedInput(_))
                    ));
                    let mut wrong = descriptor;
                    if let UserDefinitionKind::Struct { fields, .. } =
                        &mut wrong.definitions[0].kind
                    {
                        if let TypeKind::Array { length, .. } = &mut fields[0].ty.kind {
                            *length = Some(2);
                        }
                    }
                    assert!(matches!(
                        Schema::compile(wrong, &SchemaLimits::default())
                            .unwrap()
                            .validate(&input, InputFormat::XlangMessage, &ParseLimits::default()),
                        Err(DataError::SchemaMismatch(_))
                    ));
                    drop(input);
                    drop(s);
                    assert_eq!(owned.as_view().unwrap().$read().unwrap(), values[1]);
                    assert_eq!(retained.as_view().unwrap().$read().unwrap(), values[2]);
                }
            }
        }};
    }
    check!(bool, Bool, None, read_bool, [false, true, false]);
    check!(i8, I8, None, read_i8, [i8::MIN, 0, i8::MAX]);
    check!(i16, I16, None, read_i16, [i16::MIN, 0, i16::MAX]);
    check!(u8, U8, None, read_u8, [0, 128, u8::MAX]);
    check!(u16, U16, None, read_u16, [0, 128, u16::MAX]);
    check!(
        i32,
        I32,
        Some(IntegerEncoding::Varint),
        read_i32,
        [i32::MIN, -129, i32::MAX]
    );
    check!(
        i64,
        I64,
        Some(IntegerEncoding::Varint),
        read_i64,
        [i64::MIN, -129, i64::MAX]
    );
    check!(
        u32,
        U32,
        Some(IntegerEncoding::Varint),
        read_u32,
        [0, 128, u32::MAX]
    );
    check!(
        u64,
        U64,
        Some(IntegerEncoding::Tagged),
        read_u64,
        [0, 128, u64::MAX]
    );
    check!(
        f32,
        F32,
        None,
        read_f32,
        [f32::NEG_INFINITY, -0.0, f32::MAX]
    );
    check!(
        f64,
        F64,
        None,
        read_f64,
        [f64::NEG_INFINITY, -0.0, f64::MAX]
    );
}

#[test]
fn static_array_nullable_empty_and_declared_nested_bodies() {
    #[derive(Debug, PartialEq, fory::ForyStruct)]
    struct Arrays {
        a: Option<[i32; 2]>,
        b: [Option<i32>; 2],
        c: [u8; 0],
        d: [[i64; 2]; 2],
        e: Vec<[u32; 2]>,
    }
    let scalar = |kind, integer_encoding, nullable| TypeDescriptor {
        kind: TypeKind::Scalar(kind),
        nullable,
        track_ref: false,
        integer_encoding,
    };
    let array = |element, length| {
        ty(
            TypeKind::Array {
                element: Box::new(element),
                length: Some(length),
            },
            false,
        )
    };
    for mode in [
        SchemaMode::SameSchema {
            version_hash: false,
        },
        SchemaMode::SameSchema { version_hash: true },
        SchemaMode::Compatible,
    ] {
        let mut f = fory::Fory::builder()
            .xlang(true)
            .compatible(matches!(mode, SchemaMode::Compatible))
            .check_struct_version(matches!(
                mode,
                SchemaMode::SameSchema { version_hash: true }
            ))
            .build();
        f.register::<Arrays>(742).unwrap();
        let s = Schema::compile(
            SchemaDescriptor {
                encoding: RootEncoding {
                    protocol_version: ProtocolVersion::Fory1_7,
                    mode,
                    root_reference: RootReferenceMode::NullOnly,
                },
                key_semantics_version: KEY_SEMANTICS_VERSION,
                root: ty(TypeKind::User(UserIdentity::Numeric(742)), false),
                definitions: vec![UserDefinition {
                    identity: UserIdentity::Numeric(742),
                    kind: UserDefinitionKind::Struct {
                        fields: vec![
                            FieldDescriptor {
                                id: None,
                                name: "a".into(),
                                ty: TypeDescriptor {
                                    nullable: true,
                                    ..array(
                                        scalar(
                                            ScalarKind::I32,
                                            Some(IntegerEncoding::Varint),
                                            false,
                                        ),
                                        2,
                                    )
                                },
                            },
                            FieldDescriptor {
                                id: None,
                                name: "b".into(),
                                ty: array(
                                    scalar(ScalarKind::I32, Some(IntegerEncoding::Varint), true),
                                    2,
                                ),
                            },
                            FieldDescriptor {
                                id: None,
                                name: "c".into(),
                                ty: array(scalar(ScalarKind::U8, None, false), 0),
                            },
                            FieldDescriptor {
                                id: None,
                                name: "d".into(),
                                ty: array(
                                    array(
                                        scalar(
                                            ScalarKind::I64,
                                            Some(IntegerEncoding::Tagged),
                                            false,
                                        ),
                                        2,
                                    ),
                                    2,
                                ),
                            },
                            FieldDescriptor {
                                id: None,
                                name: "e".into(),
                                ty: ty(
                                    TypeKind::List(Box::new(array(
                                        scalar(
                                            ScalarKind::U32,
                                            Some(IntegerEncoding::Varint),
                                            false,
                                        ),
                                        2,
                                    ))),
                                    false,
                                ),
                            },
                        ],
                        physical_field_order: vec![0, 1, 2, 3, 4],
                    },
                }],
            },
            &SchemaLimits::default(),
        )
        .unwrap();
        for a in [None, Some([i32::MIN, i32::MAX])] {
            let value = Arrays {
                a,
                b: [None, Some(-129)],
                c: [],
                d: [[i64::MIN, -1], [0, i64::MAX]],
                e: vec![[0, u32::MAX]],
            };
            let input = f.serialize(&value).unwrap();
            assert_eq!(f.deserialize::<Arrays>(&input).unwrap(), value);
            let view = s
                .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
                .unwrap();
            let root = view.root().unwrap();
            assert_eq!(root.field_at(0).unwrap().is_null().unwrap(), a.is_none());
            if a.is_some() {
                assert_eq!(
                    root.field_at(0)
                        .unwrap()
                        .element_at(1)
                        .unwrap()
                        .read_i32()
                        .unwrap(),
                    i32::MAX
                );
            }
            assert!(
                root.field_at(1)
                    .unwrap()
                    .element_at(0)
                    .unwrap()
                    .is_null()
                    .unwrap()
            );
            assert_eq!(
                root.field_at(1)
                    .unwrap()
                    .element_at(1)
                    .unwrap()
                    .read_i32()
                    .unwrap(),
                -129
            );
            assert_eq!(root.field_at(2).unwrap().len().unwrap(), 0);
            assert_eq!(
                root.field_at(3)
                    .unwrap()
                    .element_at(1)
                    .unwrap()
                    .element_at(1)
                    .unwrap()
                    .read_i64()
                    .unwrap(),
                i64::MAX
            );
            assert_eq!(
                root.field_at(4)
                    .unwrap()
                    .element_at(0)
                    .unwrap()
                    .element_at(1)
                    .unwrap()
                    .read_u32()
                    .unwrap(),
                u32::MAX
            );
            drop(root);
            let validated = view.validate().unwrap();
            assert_eq!(
                validated
                    .root()
                    .unwrap()
                    .field_at(3)
                    .unwrap()
                    .element_at(0)
                    .unwrap()
                    .elements()
                    .unwrap()
                    .map(|n| n.unwrap().read_i64().unwrap())
                    .collect::<Vec<_>>(),
                [i64::MIN, -1]
            );
        }
    }
}

#[test]
fn canonical_binary_and_recursive_container_versions_match_real_derive() {
    macro_rules! check {
        ($(#[$attr:meta])* {$($rust:tt)*}, $descriptor:expr, $value:expr, $node:ident => $body:block) => {{
            #[derive(Debug, PartialEq, fory::ForyStruct)]
            struct Canonical { $(#[$attr])* value: $($rust)* }
            for named in [false, true] {
                let mut f = fory::Fory::builder().xlang(true).compatible(false).check_struct_version(true).build();
                let identity = if named {
                    f.register_by_name::<Canonical>("versions.Canonical").unwrap();
                    UserIdentity::Name { namespace: "versions".into(), type_name: "Canonical".into() }
                } else { f.register::<Canonical>(733).unwrap(); UserIdentity::Numeric(733) };
                let s = Schema::compile(SchemaDescriptor {
                    encoding: RootEncoding { protocol_version: ProtocolVersion::Fory1_7, mode: SchemaMode::SameSchema { version_hash: true }, root_reference: RootReferenceMode::NullOnly },
                    key_semantics_version: KEY_SEMANTICS_VERSION, root: ty(TypeKind::User(identity.clone()), false),
                    definitions: vec![UserDefinition { identity, kind: UserDefinitionKind::Struct { fields: vec![FieldDescriptor { id: None, name: "value".into(), ty: $descriptor }], physical_field_order: vec![0] } }],
                }, &SchemaLimits::default()).unwrap();
                let expected = Canonical { value: $value };
                let input = f.serialize(&expected).unwrap();
                assert_eq!(f.deserialize::<Canonical>(&input).unwrap(), expected);
                let view = s.view(&input, InputFormat::XlangMessage, &ParseLimits::default()).unwrap();
                let root = view.root().unwrap();
                let $node = root.field_at(0).unwrap(); $body
                let owned = $node.to_owned().unwrap();
                drop($node); drop(root);
                let mut bad_hash = input.clone();
                // 单字段 body 紧跟四字节版本；由关闭版本检查的真实 writer 定位。
                let mut plain = fory::Fory::builder().xlang(true).compatible(false).check_struct_version(false).build();
                if named { plain.register_by_name::<Canonical>("versions.Canonical").unwrap(); }
                else { plain.register::<Canonical>(733).unwrap(); }
                let no_hash = plain.serialize(&expected).unwrap();
                let hash_at = input.iter().zip(&no_hash).position(|(a, b)| a != b).unwrap();
                bad_hash[hash_at] ^= 1;
                let bad_view = s.view(&bad_hash, InputFormat::XlangMessage, &ParseLimits::default()).unwrap();
                let survivor = bad_view.root().unwrap().to_owned().unwrap();
                let error = bad_view.validate().err().unwrap();
                assert!(matches!(error, DataError::SchemaMismatch(_)));
                assert_eq!(format!("{error:?}"), format!("{:?}", survivor.as_view().err().unwrap()));
                let validated = view.validate().unwrap();
                let root = validated.root().unwrap();
                let $node = root.field_at(0).unwrap(); $body
                let retained = $node.to_owned().unwrap();
                drop($node); drop(root); drop(validated);
                for end in 0..input.len() { assert!(s.validate(&input[..end], InputFormat::XlangMessage, &ParseLimits::default()).is_err()); }
                let mut bad = input.clone(); bad.push(0);
                assert!(matches!(s.validate(&bad, InputFormat::XlangMessage, &ParseLimits::default()), Err(DataError::MalformedInput(_))));
                drop(input); drop(s);
                let $node = owned.as_view().unwrap(); $body
                let $node = retained.as_view().unwrap(); $body
            }
        }};
    }
    let string = || ty(TypeKind::Scalar(ScalarKind::String), false);
    let list = |element| ty(TypeKind::List(Box::new(element)), false);
    check!(#[fory(bytes)] {Vec<u8>}, ty(TypeKind::Scalar(ScalarKind::Binary), false), vec![0, 128, 255], node => {
        assert_eq!(node.read_binary().unwrap().as_bytes(), &[0, 128, 255]);
    });
    check!({Vec<Option<String>>}, list(ty(TypeKind::Scalar(ScalarKind::String), true)), vec![None, Some("list".into())], node => {
        assert!(node.element_at(0).unwrap().is_null().unwrap());
        assert_eq!(node.element_at(1).unwrap().read_string().unwrap().as_str(), "list");
    });
    check!({Option<Vec<String>>}, ty(TypeKind::List(Box::new(string())), true), Some(vec!["optional".into()]), node => {
        assert_eq!(node.element_at(0).unwrap().read_string().unwrap().as_str(), "optional");
    });
    check!({Option<Vec<String>>}, ty(TypeKind::List(Box::new(string())), true), None, node => {
        assert!(node.is_null().unwrap());
    });
    check!({Vec<fory::Date>}, list(ty(TypeKind::Scalar(ScalarKind::Date), false)), vec![fory::Date::from_epoch_days(-1)], node => {
        assert_eq!(node.element_at(0).unwrap().read_date().unwrap(), fory::Date::from_epoch_days(-1));
    });
    check!({Vec<fory::Duration>}, list(ty(TypeKind::Scalar(ScalarKind::Duration), false)), vec![fory::Duration::new(-2, -1).unwrap()], node => {
        assert_eq!(node.element_at(0).unwrap().read_duration().unwrap(), fory::Duration::new(-2, -1).unwrap());
    });
    check!({[String; 2]}, ty(TypeKind::Array { element: Box::new(string()), length: Some(2) }, false), ["array".into(), "tail".into()], node => {
        assert_eq!(node.element_at(1).unwrap().read_string().unwrap().as_str(), "tail");
    });
    check!({std::collections::BTreeSet<String>}, ty(TypeKind::Set(Box::new(string())), false), std::collections::BTreeSet::from(["set".into()]), node => {
        assert_eq!(node.elements().unwrap().next().unwrap().unwrap().read_string().unwrap().as_str(), "set");
    });
    check!({std::collections::BTreeMap<String, Vec<Option<String>>>}, ty(TypeKind::Map { key: Box::new(string()), value: Box::new(list(ty(TypeKind::Scalar(ScalarKind::String), true))) }, false), std::collections::BTreeMap::from([("key".into(), vec![Some("nested".into()), None])]), node => {
        let mut entries = node.entries().unwrap();
        let (key, value) = entries.next().unwrap().unwrap();
        assert_eq!(key.read_string().unwrap().as_str(), "key");
        assert_eq!(value.element_at(0).unwrap().read_string().unwrap().as_str(), "nested");
        assert!(value.element_at(1).unwrap().is_null().unwrap());
    });
    check!({Vec<fory::Timestamp>}, list(ty(TypeKind::Scalar(ScalarKind::Timestamp), false)), vec![fory::Timestamp::new(-1, 999_999_999).unwrap()], node => {
        assert_eq!(node.element_at(0).unwrap().read_timestamp().unwrap(), fory::Timestamp::new(-1, 999_999_999).unwrap());
    });
    check!(#[fory(list(element(ref = true)))] {Vec<Option<String>>}, list(TypeDescriptor { track_ref: true, ..ty(TypeKind::Scalar(ScalarKind::String), true) }), vec![None, Some("flags".into())], node => {
        assert!(node.element_at(0).unwrap().is_null().unwrap());
        assert_eq!(node.element_at(1).unwrap().read_string().unwrap().as_str(), "flags");
    });
    check!(#[fory(list(element(bytes)))] {Vec<Vec<u8>>}, list(ty(TypeKind::Scalar(ScalarKind::Binary), false)), vec![vec![0, 255]], node => {
        assert_eq!(node.element_at(0).unwrap().read_binary().unwrap().as_bytes(), &[0, 255]);
    });
}

#[derive(Debug, PartialEq, fory::ForyStruct)]
struct StaticStringArrays {
    a: [String; 2],
    b: [Option<String>; 2],
    c: [[String; 2]; 1],
    d: [String; 0],
}

#[test]
fn canonical_primitive_container_hashes_do_not_claim_ambiguous_framing() {
    macro_rules! check {
        ($(#[$attr:meta])* {$($rust:tt)*}, $ty:expr, $value:expr, $fingerprint:expr, $error:ident) => {{
            #[derive(Debug, PartialEq, fory::ForyStruct)]
            struct Primitive { $(#[$attr])* value: $($rust)* }
            let mut f = fory::Fory::builder().xlang(true).compatible(false).check_struct_version(true).build();
            f.register::<Primitive>(734).unwrap();
            let input = f.serialize(&Primitive { value: $value }).unwrap();
            // Numeric 734 的包络是 message flag、ref、struct ID、二字节 user ID。
            let actual = i32::from_le_bytes(input[5..9].try_into().unwrap());
            let expected = fory::__private::util::murmurhash3_x64_128($fingerprint.as_bytes(), 47).0 as i32;
            assert_eq!(actual, expected);
            let s = Schema::compile(SchemaDescriptor {
                encoding: RootEncoding { protocol_version: ProtocolVersion::Fory1_7, mode: SchemaMode::SameSchema { version_hash: true }, root_reference: RootReferenceMode::NullOnly },
                key_semantics_version: KEY_SEMANTICS_VERSION, root: ty(TypeKind::User(UserIdentity::Numeric(734)), false),
                definitions: vec![UserDefinition { identity: UserIdentity::Numeric(734), kind: UserDefinitionKind::Struct { fields: vec![FieldDescriptor { id: None, name: "value".into(), ty: $ty }], physical_field_order: vec![0] } }],
            }, &SchemaLimits::default()).unwrap();
            let result = s.validate(&input, InputFormat::XlangMessage, &ParseLimits::default());
            if stringify!($error) == "NotImplemented" {
                let validated = result.unwrap();
                assert_eq!(validated.root().unwrap().field_at(0).unwrap().len().unwrap(), ($value).len());
            } else {
                assert!(matches!(result, Err(DataError::$error(_))));
            }
            let mut corrupt = input.clone(); corrupt[5] ^= 1;
            assert!(matches!(s.validate(&corrupt, InputFormat::XlangMessage, &ParseLimits::default()), Err(DataError::SchemaMismatch(_))));
        }};
    }
    let scalar = |kind, integer_encoding| TypeDescriptor {
        kind: TypeKind::Scalar(kind),
        nullable: false,
        track_ref: false,
        integer_encoding,
    };
    let list = |element| ty(TypeKind::List(Box::new(element)), false);
    let array = |element| {
        ty(
            TypeKind::Array {
                element: Box::new(element),
                length: Some(2),
            },
            false,
        )
    };
    check!({Vec<u8>}, list(scalar(ScalarKind::U8, None)), vec![0, 255], "value,22,0,0[9,0,0];", NotImplemented);
    check!(
        { [u8; 2] },
        array(scalar(ScalarKind::U8, None)),
        [0, 255],
        "value,22,0,0[9,0,0];",
        NotImplemented
    );
    check!(
        { [i32; 2] },
        array(scalar(ScalarKind::I32, Some(IntegerEncoding::Varint))),
        [i32::MIN, i32::MAX],
        "value,46,0,0;",
        NotImplemented
    );
    check!(#[fory(list(element(encoding = "fixed")))] {Vec<i32>}, list(scalar(ScalarKind::I32, Some(IntegerEncoding::Fixed))), vec![i32::MIN], "value,22,0,0[4,0,0];", NotImplemented);
    check!(#[fory(list(element(encoding = "tagged")))] {Vec<i64>}, list(scalar(ScalarKind::I64, Some(IntegerEncoding::Tagged))), vec![i64::MIN], "value,22,0,0[8,0,0];", NotImplemented);
    check!(
        { [u64; 2] },
        array(scalar(ScalarKind::U64, Some(IntegerEncoding::Tagged))),
        [0, u64::MAX],
        "value,51,0,0;",
        NotImplemented
    );
    check!({[Option<i32>; 2]}, array(TypeDescriptor { nullable: true, ..scalar(ScalarKind::I32, Some(IntegerEncoding::Varint)) }), [None, Some(7)], "value,22,0,0[5,0,0];", NotImplemented);
    check!(#[fory(array)] {Vec<i32>}, list(scalar(ScalarKind::I32, Some(IntegerEncoding::Varint))), vec![1, 2], "value,46,0,0;", SchemaMismatch);
}

#[test]
fn canonical_user_container_tag_and_name_order_matches_derive() {
    #[derive(fory::ForyEnum)]
    enum Choice {
        A,
    }
    #[derive(fory::ForyUnion)]
    enum Payload {
        #[fory(default)]
        A(String),
        #[fory(unknown)]
        Unknown(fory::UnknownCase),
    }
    #[derive(fory::ForyStruct)]
    struct Ordered {
        z_values: Vec<Choice>,
        #[fory(id = 9, bytes)]
        last: Vec<u8>,
        a_values: Vec<Payload>,
        #[fory(id = 2)]
        first: Vec<fory::Duration>,
    }
    let mut f = fory::Fory::builder()
        .xlang(true)
        .compatible(false)
        .check_struct_version(true)
        .build();
    f.register::<Ordered>(738).unwrap();
    f.register::<Choice>(739).unwrap();
    let input = f
        .serialize(&Ordered {
            z_values: vec![],
            last: vec![1],
            a_values: vec![],
            first: vec![],
        })
        .unwrap();
    let expected = "2,22,0,0[37,0,0];9,41,0,0;a_values,22,0,0[0,0,0];z_values,22,0,0[0,0,0];";
    assert_eq!(
        i32::from_le_bytes(input[5..9].try_into().unwrap()),
        fory::__private::util::murmurhash3_x64_128(expected.as_bytes(), 47).0 as i32
    );
}

#[test]
fn static_nonprimitive_scalar_arrays_use_list_bodies() {
    let array = |element, length| {
        ty(
            TypeKind::Array {
                element: Box::new(element),
                length: Some(length),
            },
            false,
        )
    };
    for compatible in [false, true] {
        for named in [false, true] {
            let identity = if named {
                UserIdentity::Name {
                    namespace: "arrays".into(),
                    type_name: "Strings".into(),
                }
            } else {
                UserIdentity::Numeric(730)
            };
            let root_type = ty(TypeKind::User(identity.clone()), false);
            let schema = Schema::compile(
                SchemaDescriptor {
                    encoding: RootEncoding {
                        protocol_version: ProtocolVersion::Fory1_7,
                        mode: if compatible {
                            SchemaMode::Compatible
                        } else {
                            SchemaMode::SameSchema {
                                version_hash: false,
                            }
                        },
                        root_reference: RootReferenceMode::NullOnly,
                    },
                    key_semantics_version: KEY_SEMANTICS_VERSION,
                    root: root_type,
                    definitions: vec![UserDefinition {
                        identity,
                        kind: UserDefinitionKind::Struct {
                            fields: vec![
                                FieldDescriptor {
                                    id: None,
                                    name: "a".into(),
                                    ty: array(ty(TypeKind::Scalar(ScalarKind::String), false), 2),
                                },
                                FieldDescriptor {
                                    id: None,
                                    name: "b".into(),
                                    ty: array(ty(TypeKind::Scalar(ScalarKind::String), true), 2),
                                },
                                FieldDescriptor {
                                    id: None,
                                    name: "c".into(),
                                    ty: array(
                                        array(ty(TypeKind::Scalar(ScalarKind::String), false), 2),
                                        1,
                                    ),
                                },
                                FieldDescriptor {
                                    id: None,
                                    name: "d".into(),
                                    ty: array(ty(TypeKind::Scalar(ScalarKind::String), false), 0),
                                },
                            ],
                            physical_field_order: vec![0, 1, 2, 3],
                        },
                    }],
                },
                &SchemaLimits::default(),
            )
            .unwrap();
            let mut f = fory::Fory::builder()
                .xlang(true)
                .compatible(compatible)
                .check_struct_version(false)
                .build();
            if named {
                f.register_by_name::<StaticStringArrays>("arrays.Strings")
                    .unwrap();
            } else {
                f.register::<StaticStringArrays>(730).unwrap();
            }
            let value = StaticStringArrays {
                a: ["first".into(), "second".into()],
                b: [None, Some("present".into())],
                c: [["nested".into(), "end".into()]],
                d: [],
            };
            let input = f.serialize(&value).unwrap();
            assert_eq!(f.deserialize::<StaticStringArrays>(&input).unwrap(), value);
            let view = schema
                .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
                .unwrap();
            let root = view.root().unwrap();
            let a = root.field_by_name("a").unwrap();
            assert_eq!(a.len().unwrap(), 2);
            assert_eq!(
                a.element_at(1).unwrap().read_string().unwrap().as_str(),
                "second"
            );
            let items = a
                .elements()
                .unwrap()
                .map(|item| item.unwrap().read_string().unwrap().as_str().to_owned())
                .collect::<Vec<_>>();
            assert_eq!(items, ["first", "second"]);
            assert!(
                root.field_by_name("b")
                    .unwrap()
                    .element_at(0)
                    .unwrap()
                    .is_null()
                    .unwrap()
            );
            assert_eq!(
                root.field_by_name("c")
                    .unwrap()
                    .element_at(0)
                    .unwrap()
                    .element_at(0)
                    .unwrap()
                    .read_string()
                    .unwrap()
                    .as_str(),
                "nested"
            );
            assert_eq!(root.field_by_name("d").unwrap().len().unwrap(), 0);
            let retained = a.to_owned().unwrap();
            drop(a);
            drop(root);
            let validated = view.validate().unwrap();
            let owned = validated
                .root()
                .unwrap()
                .field_by_name("b")
                .unwrap()
                .element_at(1)
                .unwrap()
                .to_owned()
                .unwrap();
            assert_eq!(
                validated
                    .root()
                    .unwrap()
                    .field_by_name("a")
                    .unwrap()
                    .element_at(0)
                    .unwrap()
                    .read_string()
                    .unwrap()
                    .as_str(),
                "first"
            );
            for end in 0..input.len() {
                assert!(
                    schema
                        .validate(
                            &input[..end],
                            InputFormat::XlangMessage,
                            &ParseLimits::default()
                        )
                        .is_err()
                );
            }
            let mut trailing = input.clone();
            trailing.push(0);
            assert!(matches!(
                schema.validate(
                    &trailing,
                    InputFormat::XlangMessage,
                    &ParseLimits::default()
                ),
                Err(DataError::MalformedInput(_))
            ));
            drop(validated);
            drop(input);
            drop(schema);
            assert_eq!(
                retained
                    .as_view()
                    .unwrap()
                    .element_at(1)
                    .unwrap()
                    .read_string()
                    .unwrap()
                    .as_str(),
                "second"
            );
            assert_eq!(
                owned.as_view().unwrap().read_string().unwrap().as_str(),
                "present"
            );
        }
    }
}

type Count = i32;
#[derive(Debug, PartialEq, fory::ForyStruct)]
struct NormalCount {
    count: i32,
}
#[derive(Debug, PartialEq, fory::ForyStruct)]
struct AliasCount {
    count: Count,
}

#[derive(Debug, PartialEq, fory::ForyStruct)]
struct CanonicalParent {
    child: NormalCount,
    optional: Option<NormalCount>,
}

#[derive(Debug, PartialEq, fory::ForyStruct)]
struct CompatibleCollections {
    children: Vec<Option<CanonicalParent>>,
    lookup: std::collections::HashMap<String, Option<NormalCount>>,
}

fn named_count_descriptor() -> SchemaDescriptor {
    let mut descriptor = count_descriptor_for_nested();
    let child = UserIdentity::Name {
        namespace: "shared_namespace_long_enough_for_hash_validation".into(),
        type_name: "SharedLongChildTypeNameForWireValidation".into(),
    };
    let parent = UserIdentity::Name {
        namespace: "shared_namespace_long_enough_for_hash_validation".into(),
        type_name: "SharedLongParentTypeNameForWireValidation".into(),
    };
    descriptor.root.kind = TypeKind::User(parent.clone());
    for definition in &mut descriptor.definitions {
        definition.identity = if definition.identity == UserIdentity::Numeric(200) {
            child.clone()
        } else {
            parent.clone()
        };
        if let UserDefinitionKind::Struct { fields, .. } = &mut definition.kind {
            for field in fields {
                if matches!(field.ty.kind, TypeKind::User(_)) {
                    field.ty.kind = TypeKind::User(child.clone());
                }
            }
        }
    }
    descriptor
}

fn named_count_fory(compatible: bool) -> fory::Fory {
    let mut f = fory::Fory::builder()
        .xlang(true)
        .compatible(compatible)
        .check_struct_version(false)
        .build();
    f.register_by_name::<NormalCount>(
        "shared_namespace_long_enough_for_hash_validation.SharedLongChildTypeNameForWireValidation",
    )
    .unwrap();
    f.register_by_name::<CanonicalParent>("shared_namespace_long_enough_for_hash_validation.SharedLongParentTypeNameForWireValidation").unwrap();
    f
}

#[test]
fn same_schema_root_meta_string_backref_uses_only_preceding_inline_slot() {
    let component = "SharedLongNamespaceAndTypeNameForMetaStringReferences";
    let mut descriptor = count_schema_descriptor();
    descriptor.encoding.mode = SchemaMode::SameSchema {
        version_hash: false,
    };
    let identity = UserIdentity::Name {
        namespace: component.into(),
        type_name: component.into(),
    };
    descriptor.root.kind = TypeKind::User(identity.clone());
    descriptor.definitions[0].identity = identity;
    let schema = Schema::compile(descriptor.clone(), &SchemaLimits::default()).unwrap();
    let mut f = fory::Fory::builder()
        .xlang(true)
        .compatible(false)
        .check_struct_version(false)
        .build();
    f.register_by_name::<NormalCount>(&format!("{component}.{component}"))
        .unwrap();
    let input = f.serialize(&NormalCount { count: -19 }).unwrap();
    assert_eq!(
        f.deserialize::<NormalCount>(&input).unwrap(),
        NormalCount { count: -19 }
    );
    let mut reader = fory::Reader::new(&input);
    reader.skip(3).unwrap();
    let len = reader.read_var_u32().unwrap() >> 1;
    assert!(len > 16);
    reader.skip(8 + len as usize).unwrap();
    let reference = reader.get_cursor();
    assert_eq!(reader.read_var_u32().unwrap(), 3);
    let view = schema
        .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
        .unwrap();
    let owned = view.root().unwrap().to_owned().unwrap();
    let validated = view.validate().unwrap();
    assert_eq!(
        validated
            .root()
            .unwrap()
            .field_at(0)
            .unwrap()
            .read_i32()
            .unwrap(),
        -19
    );
    for token in [1, 5, 127] {
        let mut bad = input.clone();
        bad[reference] = token;
        assert!(matches!(
            schema.view(&bad, InputFormat::XlangMessage, &ParseLimits::default()),
            Err(DataError::MalformedInput(_))
        ));
    }
    let mut bad = input.clone();
    bad[5] ^= 1;
    assert!(matches!(
        schema.view(&bad, InputFormat::XlangMessage, &ParseLimits::default()),
        Err(DataError::MalformedInput(_))
    ));
    let TypeKind::User(UserIdentity::Name { type_name, .. }) = &mut descriptor.root.kind else {
        unreachable!()
    };
    type_name.push('X');
    descriptor.definitions[0].identity = match &descriptor.root.kind {
        TypeKind::User(identity) => identity.clone(),
        _ => unreachable!(),
    };
    let wrong = Schema::compile(descriptor, &SchemaLimits::default()).unwrap();
    assert!(matches!(
        wrong.view(&input, InputFormat::XlangMessage, &ParseLimits::default()),
        Err(DataError::SchemaMismatch(_))
    ));
    for end in 1..input.len() {
        assert!(
            schema
                .validate(
                    &input[..end],
                    InputFormat::XlangMessage,
                    &ParseLimits::default()
                )
                .is_err()
        );
    }
    drop(input);
    assert_eq!(
        owned
            .as_view()
            .unwrap()
            .field_at(0)
            .unwrap()
            .read_i32()
            .unwrap(),
        -19
    );
}

#[test]
fn same_schema_list_group_zero_name_reference_is_terminal_malformed() {
    assert_zero_group_name_reference(false);
}

#[test]
fn same_schema_map_chunk_zero_name_reference_is_terminal_malformed() {
    assert_zero_group_name_reference(true);
}

fn assert_zero_group_name_reference(map: bool) {
    let mut descriptor = count_schema_descriptor();
    descriptor.encoding.mode = SchemaMode::SameSchema {
        version_hash: false,
    };
    let identity = UserIdentity::Name {
        namespace: "names".into(),
        type_name: "Count".into(),
    };
    descriptor.definitions[0].identity = identity.clone();
    let element = ty(TypeKind::User(identity), false);
    descriptor.root = if map {
        ty(
            TypeKind::Map {
                key: Box::new(ty(TypeKind::Scalar(ScalarKind::String), false)),
                value: Box::new(element),
            },
            false,
        )
    } else {
        ty(TypeKind::List(Box::new(element)), false)
    };
    let schema = Schema::compile(descriptor, &SchemaLimits::default()).unwrap();
    let mut f = fory::Fory::builder()
        .xlang(true)
        .compatible(false)
        .check_struct_version(false)
        .build();
    f.register_by_name::<NormalCount>("names.Count").unwrap();
    let mut input = if map {
        let value = std::collections::HashMap::from([("key".to_owned(), NormalCount { count: 7 })]);
        let input = f.serialize(&value).unwrap();
        assert_eq!(
            f.deserialize::<std::collections::HashMap<String, NormalCount>>(&input)
                .unwrap(),
            value
        );
        input
    } else {
        let value = vec![NormalCount { count: 7 }];
        let input = f.serialize(&value).unwrap();
        assert_eq!(f.deserialize::<Vec<NormalCount>>(&input).unwrap(), value);
        input
    };
    let mut reader = fory::Reader::new(&input);
    assert_eq!(reader.read_u8().unwrap(), 1);
    assert_eq!(reader.read_i8().unwrap(), -1);
    assert_eq!(reader.read_u8().unwrap(), if map { 24 } else { 22 });
    assert_eq!(reader.read_var_u32().unwrap(), 1);
    let header = reader.read_u8().unwrap();
    if map {
        assert_eq!(header & 63, 0);
        assert_eq!(reader.read_u8().unwrap(), 1);
        assert_eq!(reader.read_u8().unwrap(), 21);
    } else {
        assert_eq!(header, 8);
    }
    assert_eq!(reader.read_u8().unwrap(), 29);
    let name_offset = reader.get_cursor();
    assert_eq!(reader.read_var_u32().unwrap() & 1, 0);
    input[name_offset] = 1;
    assert!(if map {
        f.deserialize::<std::collections::HashMap<String, NormalCount>>(&input)
            .is_err()
    } else {
        f.deserialize::<Vec<NormalCount>>(&input).is_err()
    });
    let view = schema
        .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
        .unwrap();
    let root = view.root().unwrap();
    // 错误访问可恢复，不能在解析坏名字之前把上下文标为 Failed。
    assert!(matches!(root.read_i32(), Err(DataError::AccessError(_))));
    let error = if map {
        assert_eq!(root.len().unwrap(), 1);
        root.entries().unwrap().next().unwrap().err().unwrap()
    } else {
        root.len().unwrap_err()
    };
    assert!(matches!(error, DataError::MalformedInput(_)), "{error:?}");
    assert_eq!(
        format!("{error:?}"),
        format!("{:?}", root.len().unwrap_err())
    );
    assert_eq!(
        format!("{error:?}"),
        format!("{:?}", root.read_i32().unwrap_err())
    );
    drop(root);
    assert_eq!(
        format!("{error:?}"),
        format!("{:?}", view.validate().err().unwrap())
    );
    // 独立完整校验也必须由相同 wire 位置给出 MalformedInput，而非 NotImplemented。
    let error_from_validate = schema
        .validate(&input, InputFormat::XlangMessage, &ParseLimits::default())
        .err()
        .unwrap();
    assert!(matches!(error_from_validate, DataError::MalformedInput(_)));
    assert_eq!(format!("{error:?}"), format!("{error_from_validate:?}"));
}

#[test]
fn compatible_named_refs_are_type_slots_not_dynamic_string_slots() {
    let schema = Schema::compile(named_count_descriptor(), &SchemaLimits::default()).unwrap();
    let f = named_count_fory(true);
    let value = CanonicalParent {
        child: NormalCount { count: 1 },
        optional: Some(NormalCount { count: 2 }),
    };
    let input = f.serialize(&value).unwrap();
    let mut reader = fory::Reader::new(&input);
    reader.skip(4).unwrap();
    skip_type_meta(&mut reader);
    assert_eq!(reader.read_u8().unwrap(), 30);
    let new_offset = reader.get_cursor();
    assert_eq!(reader.read_var_u32().unwrap(), 2);
    let meta_start = reader.get_cursor();
    skip_type_meta(&mut reader);
    let meta_end = reader.get_cursor();
    assert_eq!(reader.read_var_i32().unwrap(), 1);
    assert_eq!(reader.read_i8().unwrap(), -1);
    assert_eq!(reader.read_u8().unwrap(), 30);
    let ref_offset = reader.get_cursor();
    assert_eq!(reader.read_var_u32().unwrap(), 3);
    for (offset, token) in [
        (new_offset, 3),
        (ref_offset, 1),
        (ref_offset, 5),
        (ref_offset, 127),
    ] {
        let mut bad = input.clone();
        bad[offset] = token;
        let view = schema
            .view(&bad, InputFormat::XlangMessage, &ParseLimits::default())
            .unwrap();
        let root = view.root().unwrap();
        let error = root.field_at(1).err().unwrap();
        assert!(matches!(
            error,
            DataError::MalformedInput(_) | DataError::SchemaMismatch(_)
        ));
        assert_eq!(
            format!("{error:?}"),
            format!("{:?}", root.field_at(0).err().unwrap())
        );
        drop(root);
        assert_eq!(
            format!("{error:?}"),
            format!("{:?}", view.validate().err().unwrap())
        );
    }
    let mut repeated = input[..ref_offset].to_vec();
    repeated.push(4);
    repeated.extend_from_slice(&input[meta_start..meta_end]);
    repeated.extend_from_slice(&input[ref_offset + 1..]);
    assert!(f.deserialize::<CanonicalParent>(&repeated).is_ok());
    assert_eq!(
        schema
            .validate(
                &repeated,
                InputFormat::XlangMessage,
                &ParseLimits::default()
            )
            .unwrap()
            .root()
            .unwrap()
            .field_at(1)
            .unwrap()
            .field_at(0)
            .unwrap()
            .read_i32()
            .unwrap(),
        2
    );
    for end in 1..input.len() {
        assert!(
            schema
                .validate(
                    &input[..end],
                    InputFormat::XlangMessage,
                    &ParseLimits::default()
                )
                .is_err()
        );
    }
    let mut trailing = input.clone();
    trailing.push(0);
    assert!(matches!(
        schema.validate(
            &trailing,
            InputFormat::XlangMessage,
            &ParseLimits::default()
        ),
        Err(DataError::MalformedInput(_))
    ));
    let mut bad = input.clone();
    bad[meta_start + 4] ^= 1;
    assert!(matches!(
        schema.validate(&bad, InputFormat::XlangMessage, &ParseLimits::default()),
        Err(DataError::MalformedInput(_))
    ));
}

#[test]
fn same_schema_cross_group_names_rebuild_prefix_and_bind_nested_nodes() {
    let mut descriptor = named_count_descriptor();
    descriptor.encoding.mode = SchemaMode::SameSchema {
        version_hash: false,
    };
    let root_type = descriptor.root.clone();
    descriptor.root = ty(
        TypeKind::List(Box::new(ty(TypeKind::List(Box::new(root_type)), false))),
        false,
    );
    let schema = Schema::compile(descriptor, &SchemaLimits::default()).unwrap();
    let f = named_count_fory(false);
    let values = vec![
        vec![CanonicalParent {
            child: NormalCount { count: 1 },
            optional: None,
        }],
        vec![CanonicalParent {
            child: NormalCount { count: 2 },
            optional: None,
        }],
    ];
    let input = f.serialize(&values).unwrap();
    assert_eq!(
        f.deserialize::<Vec<Vec<CanonicalParent>>>(&input).unwrap(),
        values
    );
    let view = schema
        .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
        .unwrap();
    let root = view.root().unwrap();
    let last = root.element_at(1).unwrap();
    let retained = last
        .elements()
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .to_owned()
        .unwrap();
    assert_eq!(
        retained
            .as_view()
            .unwrap()
            .field_at(0)
            .unwrap()
            .field_at(0)
            .unwrap()
            .read_i32()
            .unwrap(),
        2
    );
    for node in root.elements().unwrap() {
        assert_eq!(node.unwrap().elements().unwrap().count(), 1);
    }
    assert_eq!(root.len().unwrap(), 2);
    drop(last);
    drop(root);
    let validated = view.validate().unwrap();
    let owned = validated
        .root()
        .unwrap()
        .element_at(1)
        .unwrap()
        .to_owned()
        .unwrap();
    drop(validated);
    drop(input);
    assert_eq!(
        owned
            .as_view()
            .unwrap()
            .element_at(0)
            .unwrap()
            .field_at(0)
            .unwrap()
            .field_at(0)
            .unwrap()
            .read_i32()
            .unwrap(),
        2
    );
    assert_eq!(
        retained
            .as_view()
            .unwrap()
            .field_at(0)
            .unwrap()
            .field_at(0)
            .unwrap()
            .read_i32()
            .unwrap(),
        2
    );
}

#[derive(Debug, PartialEq, fory::ForyStruct)]
struct NamedGroups {
    first: Vec<NormalCount>,
    second: Vec<AliasCount>,
}

#[derive(Debug, PartialEq, fory::ForyStruct)]
struct PrefixEmptyFields {
    empty: Empty,
    optional: Option<Empty>,
    tail: bool,
}

#[test]
fn single_field_prefix_preserves_nullable_zero_body_and_logical_order() {
    let user = ty(TypeKind::User(UserIdentity::Numeric(204)), false);
    let fields = vec![
        FieldDescriptor {
            id: None,
            name: "empty".into(),
            ty: user.clone(),
        },
        FieldDescriptor {
            id: None,
            name: "optional".into(),
            ty: TypeDescriptor {
                nullable: true,
                ..user
            },
        },
        FieldDescriptor {
            id: None,
            name: "tail".into(),
            ty: ty(TypeKind::Scalar(ScalarKind::Bool), false),
        },
    ];
    let order: Vec<usize> = <PrefixEmptyFields as fory::StructSerializer>::sorted_field_names()
        .iter()
        .map(|name| fields.iter().position(|field| field.name == *name).unwrap())
        .collect();
    let descriptor = SchemaDescriptor {
        encoding: RootEncoding {
            protocol_version: ProtocolVersion::Fory1_7,
            mode: SchemaMode::SameSchema {
                version_hash: false,
            },
            root_reference: RootReferenceMode::NullOnly,
        },
        key_semantics_version: KEY_SEMANTICS_VERSION,
        root: ty(TypeKind::User(UserIdentity::Numeric(205)), false),
        definitions: vec![
            UserDefinition {
                identity: UserIdentity::Numeric(204),
                kind: UserDefinitionKind::Struct {
                    fields: vec![],
                    physical_field_order: vec![],
                },
            },
            UserDefinition {
                identity: UserIdentity::Numeric(205),
                kind: UserDefinitionKind::Struct {
                    fields,
                    physical_field_order: order,
                },
            },
        ],
    };
    let mut f = fory::Fory::builder()
        .xlang(true)
        .compatible(false)
        .check_struct_version(false)
        .build();
    f.register::<Empty>(204).unwrap();
    f.register::<PrefixEmptyFields>(205).unwrap();
    for optional in [None, Some(Empty {})] {
        let value = PrefixEmptyFields {
            empty: Empty {},
            optional,
            tail: true,
        };
        let input = f.serialize(&value).unwrap();
        assert_eq!(f.deserialize::<PrefixEmptyFields>(&input).unwrap(), value);
        for reverse in [false, true] {
            let mut descriptor = descriptor.clone();
            let UserDefinitionKind::Struct {
                fields,
                physical_field_order,
            } = &mut descriptor.definitions[1].kind
            else {
                unreachable!()
            };
            if reverse {
                fields.reverse();
                for index in physical_field_order {
                    *index = 2 - *index;
                }
            }
            // SameSchema 的字段 tag 不上 wire，可用于核对 by_id 共用定位路径。
            for (index, field) in fields.iter_mut().enumerate() {
                field.id = Some(index as u32);
            }
            let schema = Schema::compile(descriptor, &SchemaLimits::default()).unwrap();
            let view = schema
                .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
                .unwrap();
            let root = view.root().unwrap();
            assert!(root.field_by_name("tail").unwrap().read_bool().unwrap());
            assert_eq!(
                root.field_by_name("optional").unwrap().is_null().unwrap(),
                value.optional.is_none()
            );
            assert_eq!(
                root.field_by_name("empty")
                    .unwrap()
                    .fields()
                    .unwrap()
                    .count(),
                0
            );
            for index in 0..3 {
                assert!(std::ptr::eq(
                    root.field_at(index).unwrap().node_type(),
                    root.field_by_id(index as u32).unwrap().node_type()
                ));
            }
            drop(root);
            let validated = view.validate().unwrap();
            let root = validated.root().unwrap();
            assert!(root.field_by_name("tail").unwrap().read_bool().unwrap());
            assert_eq!(
                root.field_by_name("optional").unwrap().is_null().unwrap(),
                value.optional.is_none()
            );
            assert_eq!(
                root.field_by_name("empty")
                    .unwrap()
                    .fields()
                    .unwrap()
                    .count(),
                0
            );
        }
    }
}

#[test]
fn single_field_lookup_does_not_read_later_named_group() {
    let mut descriptor = count_schema_descriptor();
    descriptor.encoding.mode = SchemaMode::SameSchema {
        version_hash: false,
    };
    let identity = UserIdentity::Name {
        namespace: "names".into(),
        type_name: "Count".into(),
    };
    descriptor.definitions[0].identity = identity.clone();
    let mut alias = descriptor.definitions[0].clone();
    let alias_identity = UserIdentity::Name {
        namespace: "names".into(),
        type_name: "Alias".into(),
    };
    alias.identity = alias_identity.clone();
    descriptor.definitions.push(alias);
    let parent = UserIdentity::Numeric(203);
    descriptor.root = ty(TypeKind::User(parent.clone()), false);
    descriptor.definitions.push(UserDefinition {
        identity: parent,
        kind: UserDefinitionKind::Struct {
            fields: vec![
                FieldDescriptor {
                    id: None,
                    name: "first".into(),
                    ty: ty(
                        TypeKind::List(Box::new(ty(TypeKind::User(identity), false))),
                        false,
                    ),
                },
                FieldDescriptor {
                    id: None,
                    name: "second".into(),
                    ty: ty(
                        TypeKind::List(Box::new(ty(TypeKind::User(alias_identity), false))),
                        false,
                    ),
                },
            ],
            physical_field_order: vec![0, 1],
        },
    });
    let mut f = fory::Fory::builder()
        .xlang(true)
        .compatible(false)
        .check_struct_version(false)
        .build();
    f.register::<NamedGroups>(203).unwrap();
    f.register_by_name::<NormalCount>("names.Count").unwrap();
    f.register_by_name::<AliasCount>("names.Alias").unwrap();
    let value = NamedGroups {
        first: vec![NormalCount { count: 11 }],
        second: vec![AliasCount { count: 22 }],
    };
    let mut input = f.serialize(&value).unwrap();
    assert_eq!(f.deserialize::<NamedGroups>(&input).unwrap(), value);
    let mut reader = fory::Reader::new(&input);
    reader.skip(3).unwrap();
    reader.read_var_u32().unwrap();
    assert_eq!(reader.read_var_u32().unwrap(), 1);
    assert_eq!(reader.read_u8().unwrap(), 8);
    assert_eq!(reader.read_u8().unwrap(), 29);
    for _ in 0..2 {
        let len = reader.read_var_u32().unwrap() >> 1;
        reader.skip(1 + len as usize).unwrap();
    }
    assert_eq!(reader.read_var_i32().unwrap(), 11);
    assert_eq!(reader.read_var_u32().unwrap(), 1);
    assert_eq!(reader.read_u8().unwrap(), 8);
    assert_eq!(reader.read_u8().unwrap(), 29);
    let bad_offset = reader.get_cursor();
    assert_eq!(reader.read_var_u32().unwrap(), 3);
    input[bad_offset] = 1;
    for reverse in [false, true] {
        let mut descriptor = descriptor.clone();
        if reverse {
            let UserDefinitionKind::Struct {
                fields,
                physical_field_order,
            } = &mut descriptor.definitions.last_mut().unwrap().kind
            else {
                unreachable!()
            };
            fields.swap(0, 1);
            *physical_field_order = vec![1, 0];
        }
        let schema = Schema::compile(descriptor, &SchemaLimits::default()).unwrap();
        let view = schema
            .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
            .unwrap();
        let root = view.root().unwrap();
        let first = root.field_at(usize::from(reverse)).unwrap();
        assert_eq!(
            first
                .element_at(0)
                .unwrap()
                .field_at(0)
                .unwrap()
                .read_i32()
                .unwrap(),
            11
        );
        assert_eq!(root.field_by_name("first").unwrap().len().unwrap(), 1);
        drop(first);
        drop(root);
        let retained = view.root().unwrap().to_owned().unwrap();
        let error = view.validate().err().unwrap();
        assert!(matches!(error, DataError::MalformedInput(_)));
        assert_eq!(
            format!("{error:?}"),
            format!("{:?}", retained.as_view().err().unwrap())
        );
    }
}

#[test]
fn same_schema_static_fields_keep_cross_type_and_root_name_events_in_wire_order() {
    let a = "abcdefghijklmnop";
    let b = "abcdefghijklmnopq";
    let first = UserIdentity::Name {
        namespace: a.into(),
        type_name: b.into(),
    };
    let second = UserIdentity::Name {
        namespace: b.into(),
        type_name: a.into(),
    };
    let parent = UserIdentity::Name {
        namespace: a.into(),
        type_name: a.into(),
    };
    let mut descriptor = count_schema_descriptor();
    descriptor.encoding.mode = SchemaMode::SameSchema {
        version_hash: false,
    };
    descriptor.definitions[0].identity = first.clone();
    let mut alias = descriptor.definitions[0].clone();
    alias.identity = second.clone();
    descriptor.definitions.push(alias);
    descriptor.root = ty(TypeKind::User(parent.clone()), false);
    descriptor.definitions.push(UserDefinition {
        identity: parent,
        kind: UserDefinitionKind::Struct {
            fields: vec![
                FieldDescriptor {
                    id: None,
                    name: "first".into(),
                    ty: ty(
                        TypeKind::List(Box::new(ty(TypeKind::User(first), false))),
                        false,
                    ),
                },
                FieldDescriptor {
                    id: None,
                    name: "second".into(),
                    ty: ty(
                        TypeKind::List(Box::new(ty(TypeKind::User(second), false))),
                        false,
                    ),
                },
            ],
            physical_field_order: vec![0, 1],
        },
    });
    let schema = Schema::compile(descriptor, &SchemaLimits::default()).unwrap();
    let mut f = fory::Fory::builder()
        .xlang(true)
        .compatible(false)
        .check_struct_version(false)
        .build();
    f.register_by_name::<NamedGroups>(&format!("{a}.{a}"))
        .unwrap();
    f.register_by_name::<NormalCount>(&format!("{a}.{b}"))
        .unwrap();
    f.register_by_name::<AliasCount>(&format!("{b}.{a}"))
        .unwrap();
    let value = NamedGroups {
        first: vec![NormalCount { count: 11 }],
        second: vec![AliasCount { count: 22 }],
    };
    let input = f.serialize(&value).unwrap();
    assert_eq!(f.deserialize::<NamedGroups>(&input).unwrap(), value);
    let view = schema
        .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
        .unwrap();
    let root = view.root().unwrap();
    let owned = root
        .field_at(1)
        .unwrap()
        .element_at(0)
        .unwrap()
        .to_owned()
        .unwrap();
    assert_eq!(
        owned
            .as_view()
            .unwrap()
            .field_at(0)
            .unwrap()
            .read_i32()
            .unwrap(),
        22
    );
    assert_eq!(
        root.field_at(0)
            .unwrap()
            .element_at(0)
            .unwrap()
            .field_at(0)
            .unwrap()
            .read_i32()
            .unwrap(),
        11
    );
    drop(root);
    view.validate().unwrap();
    drop(input);
    assert_eq!(
        owned
            .as_view()
            .unwrap()
            .field_at(0)
            .unwrap()
            .read_i32()
            .unwrap(),
        22
    );
}

#[test]
fn same_schema_named_map_chunks_and_nested_lists_preserve_dynamic_slots() {
    let mut descriptor = named_count_descriptor();
    descriptor.encoding.mode = SchemaMode::SameSchema {
        version_hash: false,
    };
    descriptor.root = ty(
        TypeKind::Map {
            key: Box::new(ty(TypeKind::Scalar(ScalarKind::String), false)),
            value: Box::new(ty(TypeKind::List(Box::new(descriptor.root)), false)),
        },
        false,
    );
    let schema = Schema::compile(descriptor, &SchemaLimits::default()).unwrap();
    let f = named_count_fory(false);
    let values: std::collections::HashMap<String, Vec<CanonicalParent>> = (0..520)
        .map(|i| {
            (
                format!("\u{1d}\u{3}fake_name_payload_{i}"),
                vec![CanonicalParent {
                    child: NormalCount { count: i },
                    optional: Some(NormalCount { count: -i }),
                }],
            )
        })
        .collect();
    let input = f.serialize(&values).unwrap();
    assert_eq!(
        f.deserialize::<std::collections::HashMap<String, Vec<CanonicalParent>>>(&input)
            .unwrap(),
        values
    );
    let view = schema
        .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
        .unwrap();
    let root = view.root().unwrap();
    let mut order = vec![];
    for pair in root.entries().unwrap() {
        let (key, value) = pair.unwrap();
        let key = key.read_string().unwrap().as_str().to_owned();
        assert_eq!(
            value
                .element_at(0)
                .unwrap()
                .field_at(0)
                .unwrap()
                .field_at(0)
                .unwrap()
                .read_i32()
                .unwrap(),
            values[&key][0].child.count
        );
        order.push(key);
    }
    drop(root);
    let validated = view.validate().unwrap();
    let root = validated.root().unwrap();
    let actual: Vec<_> = root
        .entries()
        .unwrap()
        .map(|pair| pair.unwrap().0.read_string().unwrap().as_str().to_owned())
        .collect();
    assert_eq!(actual, order);
}

#[test]
fn same_schema_group_refs_reject_future_wrong_target_and_preserve_first_failure() {
    let mut descriptor = count_schema_descriptor();
    let identity = UserIdentity::Name {
        namespace: "names".into(),
        type_name: "Count".into(),
    };
    descriptor.encoding.mode = SchemaMode::SameSchema {
        version_hash: false,
    };
    descriptor.definitions[0].identity = identity.clone();
    descriptor.root = ty(
        TypeKind::List(Box::new(ty(
            TypeKind::List(Box::new(ty(TypeKind::User(identity), false))),
            false,
        ))),
        false,
    );
    let schema = Schema::compile(descriptor, &SchemaLimits::default()).unwrap();
    let mut f = fory::Fory::builder()
        .xlang(true)
        .compatible(false)
        .check_struct_version(false)
        .build();
    f.register_by_name::<NormalCount>("names.Count").unwrap();
    let values = vec![
        vec![NormalCount { count: 1 }],
        vec![NormalCount { count: 2 }],
    ];
    let input = f.serialize(&values).unwrap();
    assert_eq!(
        f.deserialize::<Vec<Vec<NormalCount>>>(&input).unwrap(),
        values
    );
    let mut reader = fory::Reader::new(&input);
    reader.skip(6).unwrap();
    assert_eq!(reader.read_var_u32().unwrap(), 1);
    assert_eq!(reader.read_u8().unwrap(), 8);
    assert_eq!(reader.read_u8().unwrap(), 29);
    for _ in 0..2 {
        let n = reader.read_var_u32().unwrap() >> 1;
        reader
            .skip(if n > 16 {
                8
            } else if n > 0 {
                1
            } else {
                0
            })
            .unwrap();
        reader.skip(n as usize).unwrap();
    }
    assert_eq!(reader.read_var_i32().unwrap(), 1);
    assert_eq!(reader.read_var_u32().unwrap(), 1);
    assert_eq!(reader.read_u8().unwrap(), 8);
    assert_eq!(reader.read_u8().unwrap(), 29);
    let reference = reader.get_cursor();
    assert_eq!(reader.read_var_u32().unwrap(), 3);
    for token in [1, 7, 127, 5] {
        let mut bad = input.clone();
        bad[reference] = token;
        let view = schema
            .view(&bad, InputFormat::XlangMessage, &ParseLimits::default())
            .unwrap();
        let root = view.root().unwrap();
        let last = root.element_at(1).unwrap();
        assert!(matches!(last.read_i32(), Err(DataError::AccessError(_))));
        let error = last.len().unwrap_err();
        if token == 5 {
            assert!(matches!(error, DataError::SchemaMismatch(_)));
        } else {
            assert!(matches!(error, DataError::MalformedInput(_)));
        }
        assert_eq!(
            format!("{error:?}"),
            format!("{:?}", root.len().unwrap_err())
        );
        drop(last);
        drop(root);
        assert_eq!(
            format!("{error:?}"),
            format!("{:?}", view.validate().err().unwrap())
        );
    }
    for end in 1..input.len() {
        assert!(
            schema
                .validate(
                    &input[..end],
                    InputFormat::XlangMessage,
                    &ParseLimits::default()
                )
                .is_err()
        );
    }
    let mut trailing = input.clone();
    trailing.push(0);
    assert!(matches!(
        schema.validate(
            &trailing,
            InputFormat::XlangMessage,
            &ParseLimits::default()
        ),
        Err(DataError::MalformedInput(_))
    ));
}

#[test]
fn same_schema_named_user_map_crosses_multiple_typeinfo_chunks() {
    let mut descriptor = named_count_descriptor();
    descriptor.encoding.mode = SchemaMode::SameSchema {
        version_hash: false,
    };
    descriptor.root = ty(
        TypeKind::Map {
            key: Box::new(ty(TypeKind::Scalar(ScalarKind::String), false)),
            value: Box::new(descriptor.root),
        },
        false,
    );
    let schema = Schema::compile(descriptor, &SchemaLimits::default()).unwrap();
    let f = named_count_fory(false);
    let values: std::collections::HashMap<_, _> = (0..520)
        .map(|i| {
            (
                i.to_string(),
                CanonicalParent {
                    child: NormalCount { count: i },
                    optional: None,
                },
            )
        })
        .collect();
    let input = f.serialize(&values).unwrap();
    assert_eq!(
        f.deserialize::<std::collections::HashMap<String, CanonicalParent>>(&input)
            .unwrap(),
        values
    );
    let view = schema
        .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
        .unwrap();
    assert_eq!(
        view.root()
            .unwrap()
            .entries()
            .unwrap()
            .map(|pair| {
                pair.unwrap();
                1
            })
            .sum::<usize>(),
        520
    );
    let validated = view.validate().unwrap();
    assert_eq!(
        validated
            .root()
            .unwrap()
            .entries()
            .unwrap()
            .map(|pair| {
                pair.unwrap();
                1
            })
            .sum::<usize>(),
        520
    );
}

#[test]
fn same_schema_different_types_share_namespace_without_deduplicating_inline_events() {
    let mut descriptor = count_schema_descriptor();
    descriptor.encoding.mode = SchemaMode::SameSchema {
        version_hash: false,
    };
    let first = UserIdentity::Name {
        namespace: "shared".into(),
        type_name: "First".into(),
    };
    let second = UserIdentity::Name {
        namespace: "shared".into(),
        type_name: "Second".into(),
    };
    descriptor.definitions[0].identity = first.clone();
    let mut alias = descriptor.definitions[0].clone();
    alias.identity = second.clone();
    descriptor.definitions.push(alias);
    let parent = UserIdentity::Numeric(203);
    descriptor.root = ty(TypeKind::User(parent.clone()), false);
    descriptor.definitions.push(UserDefinition {
        identity: parent,
        kind: UserDefinitionKind::Struct {
            fields: vec![
                FieldDescriptor {
                    id: None,
                    name: "first".into(),
                    ty: ty(
                        TypeKind::List(Box::new(ty(TypeKind::User(first), false))),
                        false,
                    ),
                },
                FieldDescriptor {
                    id: None,
                    name: "second".into(),
                    ty: ty(
                        TypeKind::List(Box::new(ty(TypeKind::User(second), false))),
                        false,
                    ),
                },
            ],
            physical_field_order: vec![0, 1],
        },
    });
    let schema = Schema::compile(descriptor, &SchemaLimits::default()).unwrap();
    let mut f = fory::Fory::builder()
        .xlang(true)
        .compatible(false)
        .check_struct_version(false)
        .build();
    f.register::<NamedGroups>(203).unwrap();
    f.register_by_name::<NormalCount>("shared.First").unwrap();
    f.register_by_name::<AliasCount>("shared.Second").unwrap();
    let value = NamedGroups {
        first: vec![NormalCount { count: 3 }],
        second: vec![AliasCount { count: 4 }],
    };
    let input = f.serialize(&value).unwrap();
    assert_eq!(f.deserialize::<NamedGroups>(&input).unwrap(), value);
    let validated = schema
        .validate(&input, InputFormat::XlangMessage, &ParseLimits::default())
        .unwrap();
    assert_eq!(
        validated
            .root()
            .unwrap()
            .field_at(1)
            .unwrap()
            .element_at(0)
            .unwrap()
            .field_at(0)
            .unwrap()
            .read_i32()
            .unwrap(),
        4
    );
}

#[test]
fn compatible_named_multichunk_and_payload_decoys_do_not_create_name_slots() {
    let mut descriptor = named_count_descriptor();
    let root_type = descriptor.root.clone();
    descriptor.root = ty(
        TypeKind::Map {
            key: Box::new(ty(TypeKind::Scalar(ScalarKind::String), false)),
            value: Box::new(root_type),
        },
        false,
    );
    let schema = Schema::compile(descriptor, &SchemaLimits::default()).unwrap();
    let f = named_count_fory(true);
    let values: std::collections::HashMap<String, CanonicalParent> = (0..520)
        .map(|i| {
            (
                format!("\u{1e}\u{0}pretend_meta_name_\u{3}_{i}"),
                CanonicalParent {
                    child: NormalCount { count: i },
                    optional: Some(NormalCount { count: -i }),
                },
            )
        })
        .collect();
    let input = f.serialize(&values).unwrap();
    assert_eq!(
        f.deserialize::<std::collections::HashMap<String, CanonicalParent>>(&input)
            .unwrap(),
        values
    );
    let view = schema
        .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
        .unwrap();
    let root = view.root().unwrap();
    let mut order = Vec::new();
    for pair in root.entries().unwrap() {
        let (key, value) = pair.unwrap();
        let key = key.read_string().unwrap().as_str().to_owned();
        assert_eq!(
            value
                .field_at(1)
                .unwrap()
                .field_at(0)
                .unwrap()
                .read_i32()
                .unwrap(),
            values[&key].optional.as_ref().unwrap().count
        );
        order.push(key);
    }
    drop(root);
    let validated = view.validate().unwrap();
    let root = validated.root().unwrap();
    let actual: Vec<String> = root
        .entries()
        .unwrap()
        .map(|pair| pair.unwrap().0.read_string().unwrap().as_str().to_owned())
        .collect();
    assert_eq!(actual, order);
}

#[test]
fn compatible_named_nested_static_and_container_metadata_bind_exact_identity() {
    let descriptor = named_count_descriptor();
    let value = CanonicalParent {
        child: NormalCount { count: 17 },
        optional: Some(NormalCount { count: -23 }),
    };
    let f = named_count_fory(true);
    let schema = Schema::compile(descriptor.clone(), &SchemaLimits::default()).unwrap();
    let input = f.serialize(&value).unwrap();
    assert_eq!(f.deserialize::<CanonicalParent>(&input).unwrap(), value);
    let view = schema
        .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
        .unwrap();
    let root = view.root().unwrap();
    for _ in 0..3 {
        assert_eq!(
            root.field_at(1)
                .unwrap()
                .field_at(0)
                .unwrap()
                .read_i32()
                .unwrap(),
            -23
        );
    }
    let retained = root.field_at(0).unwrap().to_owned().unwrap();
    drop(root);
    let validated = view.validate().unwrap();
    let retained_validated = validated
        .root()
        .unwrap()
        .field_at(1)
        .unwrap()
        .to_owned()
        .unwrap();
    drop(validated);
    drop(input);
    assert_eq!(
        retained
            .as_view()
            .unwrap()
            .field_at(0)
            .unwrap()
            .read_i32()
            .unwrap(),
        17
    );
    assert_eq!(
        retained_validated
            .as_view()
            .unwrap()
            .field_at(0)
            .unwrap()
            .read_i32()
            .unwrap(),
        -23
    );
    let mut list_descriptor = descriptor.clone();
    list_descriptor.root = ty(TypeKind::List(Box::new(descriptor.root.clone())), false);
    let list_schema = Schema::compile(list_descriptor, &SchemaLimits::default()).unwrap();
    let input = f
        .serialize(&vec![
            value,
            CanonicalParent {
                child: NormalCount { count: 44 },
                optional: None,
            },
        ])
        .unwrap();
    let view = list_schema
        .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
        .unwrap();
    let root = view.root().unwrap();
    assert_eq!(
        root.element_at(1)
            .unwrap()
            .field_at(0)
            .unwrap()
            .field_at(0)
            .unwrap()
            .read_i32()
            .unwrap(),
        44
    );
    assert_eq!(root.elements().unwrap().count(), 2);
    drop(root);
    view.validate().unwrap();
    let mut map_descriptor = descriptor;
    map_descriptor.root = ty(
        TypeKind::Map {
            key: Box::new(ty(TypeKind::Scalar(ScalarKind::String), false)),
            value: Box::new(map_descriptor.root),
        },
        false,
    );
    let schema = Schema::compile(map_descriptor, &SchemaLimits::default()).unwrap();
    let value = std::collections::HashMap::from([(
        "entry".to_owned(),
        CanonicalParent {
            child: NormalCount { count: 9 },
            optional: Some(NormalCount { count: 10 }),
        },
    )]);
    let input = f.serialize(&value).unwrap();
    assert_eq!(
        f.deserialize::<std::collections::HashMap<String, CanonicalParent>>(&input)
            .unwrap(),
        value
    );
    let validated = schema
        .validate(&input, InputFormat::XlangMessage, &ParseLimits::default())
        .unwrap();
    assert_eq!(
        validated
            .root()
            .unwrap()
            .entries()
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .1
            .field_at(1)
            .unwrap()
            .field_at(0)
            .unwrap()
            .read_i32()
            .unwrap(),
        10
    );
}

#[test]
fn compatible_collection_fields_match_generic_contract_and_share_nested_metadata() {
    let mut descriptor = count_descriptor_for_nested();
    descriptor.root = ty(TypeKind::User(UserIdentity::Numeric(202)), false);
    descriptor.definitions.push(UserDefinition {
        identity: UserIdentity::Numeric(202),
        kind: UserDefinitionKind::Struct {
            fields: vec![
                FieldDescriptor {
                    id: None,
                    name: "children".into(),
                    ty: ty(
                        TypeKind::List(Box::new(ty(
                            TypeKind::User(UserIdentity::Numeric(201)),
                            true,
                        ))),
                        false,
                    ),
                },
                FieldDescriptor {
                    id: None,
                    name: "lookup".into(),
                    ty: ty(
                        TypeKind::Map {
                            key: Box::new(ty(TypeKind::Scalar(ScalarKind::String), false)),
                            value: Box::new(ty(TypeKind::User(UserIdentity::Numeric(200)), true)),
                        },
                        false,
                    ),
                },
            ],
            physical_field_order: vec![0, 1],
        },
    });
    let schema = Schema::compile(descriptor.clone(), &SchemaLimits::default()).unwrap();
    let mut f = fory::Fory::builder().xlang(true).compatible(true).build();
    f.register::<NormalCount>(200).unwrap();
    f.register::<CanonicalParent>(201).unwrap();
    f.register::<CompatibleCollections>(202).unwrap();
    let value = CompatibleCollections {
        children: vec![
            None,
            Some(CanonicalParent {
                child: NormalCount { count: 13 },
                optional: None,
            }),
        ],
        lookup: std::collections::HashMap::from([
            ("present".into(), Some(NormalCount { count: 21 })),
            ("absent".into(), None),
        ]),
    };
    let input = f.serialize(&value).unwrap();
    assert_eq!(
        f.deserialize::<CompatibleCollections>(&input).unwrap(),
        value
    );
    let view = schema
        .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
        .unwrap();
    let root = view.root().unwrap();
    // 后物理字段先访问，必须先扫描前字段中的新定义。
    let lookup = root.field_at(1).unwrap();
    for entry in lookup.entries().unwrap() {
        let (key, value) = entry.unwrap();
        if key.read_string().unwrap().as_str() == "present" {
            assert_eq!(value.field_at(0).unwrap().read_i32().unwrap(), 21);
        } else {
            assert!(value.is_null().unwrap());
        }
    }
    let children = root.field_at(0).unwrap();
    assert!(children.element_at(0).unwrap().is_null().unwrap());
    assert_eq!(
        children
            .element_at(1)
            .unwrap()
            .field_at(0)
            .unwrap()
            .field_at(0)
            .unwrap()
            .read_i32()
            .unwrap(),
        13
    );
    drop((lookup, children));
    drop(root);
    let validated = view.validate().unwrap();
    assert_eq!(
        validated
            .root()
            .unwrap()
            .field_at(0)
            .unwrap()
            .elements()
            .unwrap()
            .count(),
        2
    );
    let mut wrong = descriptor.clone();
    let UserDefinitionKind::Struct { fields, .. } = &mut wrong.definitions.last_mut().unwrap().kind
    else {
        unreachable!()
    };
    let TypeKind::List(element) = &mut fields[0].ty.kind else {
        unreachable!()
    };
    element.nullable = false;
    let wrong = Schema::compile(wrong, &SchemaLimits::default()).unwrap();
    assert!(matches!(
        wrong.view(&input, InputFormat::XlangMessage, &ParseLimits::default()),
        Err(DataError::SchemaMismatch(_))
    ));
    for end in 1..input.len() {
        assert!(
            schema
                .validate(
                    &input[..end],
                    InputFormat::XlangMessage,
                    &ParseLimits::default()
                )
                .is_err()
        );
    }
}

#[test]
fn compatible_numeric_nested_new_and_ref_are_reused_across_access_and_owned() {
    let mut descriptor = count_descriptor_for_nested();
    descriptor.encoding.mode = SchemaMode::Compatible;
    let s = Schema::compile(descriptor, &SchemaLimits::default()).unwrap();
    let mut f = fory::Fory::builder().xlang(true).compatible(true).build();
    f.register::<NormalCount>(200).unwrap();
    f.register::<CanonicalParent>(201).unwrap();
    for optional in [None, Some(NormalCount { count: 99 })] {
        let value = CanonicalParent {
            child: NormalCount { count: -7 },
            optional,
        };
        let input = f.serialize(&value).unwrap();
        assert_eq!(f.deserialize::<CanonicalParent>(&input).unwrap(), value);
        let view = s
            .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
            .unwrap();
        let root = view.root().unwrap();
        let retained = root
            .field_at(0)
            .unwrap()
            .field_at(0)
            .unwrap()
            .to_owned()
            .unwrap();
        for _ in 0..3 {
            assert_eq!(
                root.field_at(1).unwrap().is_null().unwrap(),
                value.optional.is_none()
            );
            if value.optional.is_some() {
                assert_eq!(
                    root.field_at(1)
                        .unwrap()
                        .field_at(0)
                        .unwrap()
                        .read_i32()
                        .unwrap(),
                    99
                );
            }
        }
        drop(root);
        let validated = view.validate().unwrap();
        let child = validated
            .root()
            .unwrap()
            .field_at(0)
            .unwrap()
            .to_owned()
            .unwrap();
        drop(validated);
        drop(input);
        assert_eq!(retained.as_view().unwrap().read_i32().unwrap(), -7);
        assert_eq!(
            child
                .as_view()
                .unwrap()
                .field_at(0)
                .unwrap()
                .read_i32()
                .unwrap(),
            -7
        );
    }
}

fn count_descriptor_for_nested() -> SchemaDescriptor {
    let mut descriptor = SchemaDescriptor {
        encoding: RootEncoding {
            protocol_version: ProtocolVersion::Fory1_7,
            mode: SchemaMode::Compatible,
            root_reference: RootReferenceMode::NullOnly,
        },
        key_semantics_version: KEY_SEMANTICS_VERSION,
        root: ty(TypeKind::User(UserIdentity::Numeric(201)), false),
        definitions: vec![],
    };
    descriptor.definitions.push(UserDefinition {
        identity: UserIdentity::Numeric(201),
        kind: UserDefinitionKind::Struct {
            fields: vec![
                FieldDescriptor {
                    id: None,
                    name: "child".into(),
                    ty: ty(TypeKind::User(UserIdentity::Numeric(200)), false),
                },
                FieldDescriptor {
                    id: None,
                    name: "optional".into(),
                    ty: ty(TypeKind::User(UserIdentity::Numeric(200)), true),
                },
            ],
            physical_field_order: vec![0, 1],
        },
    });
    descriptor
        .definitions
        .extend(count_schema_descriptor().definitions);
    descriptor
}

fn count_schema_descriptor() -> SchemaDescriptor {
    SchemaDescriptor {
        encoding: RootEncoding {
            protocol_version: ProtocolVersion::Fory1_7,
            mode: SchemaMode::Compatible,
            root_reference: RootReferenceMode::NullOnly,
        },
        key_semantics_version: KEY_SEMANTICS_VERSION,
        root: ty(TypeKind::User(UserIdentity::Numeric(200)), false),
        definitions: vec![UserDefinition {
            identity: UserIdentity::Numeric(200),
            kind: UserDefinitionKind::Struct {
                fields: vec![FieldDescriptor {
                    id: None,
                    name: "count".into(),
                    ty: TypeDescriptor {
                        integer_encoding: Some(IntegerEncoding::Varint),
                        ..ty(TypeKind::Scalar(ScalarKind::I32), false)
                    },
                }],
                physical_field_order: vec![0],
            },
        }],
    }
}

#[test]
fn compatible_numeric_container_roots_share_metadata_and_retain_children() {
    let mut f = fory::Fory::builder().xlang(true).compatible(true).build();
    f.register::<CanonicalParent>(201).unwrap();
    f.register::<NormalCount>(200).unwrap();
    let values = vec![
        CanonicalParent {
            child: NormalCount { count: 7 },
            optional: Some(NormalCount { count: 8 }),
        },
        CanonicalParent {
            child: NormalCount { count: 9 },
            optional: None,
        },
    ];
    let mut descriptor = count_descriptor_for_nested();
    let element = descriptor.root.clone();
    descriptor.root = ty(TypeKind::List(Box::new(element.clone())), false);
    let schema = Schema::compile(descriptor.clone(), &SchemaLimits::default()).unwrap();
    let input = f.serialize(&values).unwrap();
    assert_eq!(
        f.deserialize::<Vec<CanonicalParent>>(&input).unwrap(),
        values
    );
    let view = schema
        .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
        .unwrap();
    let root = view.root().unwrap();
    assert_eq!(root.len().unwrap(), 2);
    // 首次随机访问末项仍须按 wire 顺序建立前缀中的子定义槽。
    assert_eq!(
        root.element_at(1)
            .unwrap()
            .field_at(0)
            .unwrap()
            .field_at(0)
            .unwrap()
            .read_i32()
            .unwrap(),
        9
    );
    let retained = root
        .elements()
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .to_owned()
        .unwrap();
    drop(root);
    let validated = view.validate().unwrap();
    assert_eq!(validated.root().unwrap().elements().unwrap().count(), 2);
    let owned = validated
        .root()
        .unwrap()
        .element_at(1)
        .unwrap()
        .to_owned()
        .unwrap();
    drop(validated);
    drop(input);
    assert_eq!(
        retained
            .as_view()
            .unwrap()
            .field_at(0)
            .unwrap()
            .field_at(0)
            .unwrap()
            .read_i32()
            .unwrap(),
        7
    );
    assert_eq!(
        owned
            .as_view()
            .unwrap()
            .field_at(0)
            .unwrap()
            .field_at(0)
            .unwrap()
            .read_i32()
            .unwrap(),
        9
    );

    descriptor.root = ty(
        TypeKind::Map {
            key: Box::new(ty(TypeKind::Scalar(ScalarKind::String), false)),
            value: Box::new(element),
        },
        false,
    );
    let schema = Schema::compile(descriptor, &SchemaLimits::default()).unwrap();
    let values =
        std::collections::HashMap::from([("first".to_owned(), values.into_iter().next().unwrap())]);
    let input = f.serialize(&values).unwrap();
    assert_eq!(
        f.deserialize::<std::collections::HashMap<String, CanonicalParent>>(&input)
            .unwrap(),
        values
    );
    let view = schema
        .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
        .unwrap();
    let root = view.root().unwrap();
    let (key, value) = root.entries().unwrap().next().unwrap().unwrap();
    assert_eq!(key.read_string().unwrap().as_str(), "first");
    assert_eq!(
        value
            .field_at(1)
            .unwrap()
            .field_at(0)
            .unwrap()
            .read_i32()
            .unwrap(),
        8
    );
    drop((key, value));
    drop(root);
    let validated = view.validate().unwrap();
    assert_eq!(
        validated
            .root()
            .unwrap()
            .entries()
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .1
            .field_at(0)
            .unwrap()
            .field_at(0)
            .unwrap()
            .read_i32()
            .unwrap(),
        7
    );
}

#[test]
fn compatible_multichunk_map_reuses_user_metadata_without_losing_wire_order() {
    let mut descriptor = count_schema_descriptor();
    descriptor.root = ty(
        TypeKind::Map {
            key: Box::new(ty(TypeKind::Scalar(ScalarKind::String), false)),
            value: Box::new(ty(TypeKind::User(UserIdentity::Numeric(200)), false)),
        },
        false,
    );
    let schema = Schema::compile(descriptor, &SchemaLimits::default()).unwrap();
    let mut f = fory::Fory::builder().xlang(true).compatible(true).build();
    f.register::<NormalCount>(200).unwrap();
    let values: std::collections::HashMap<String, NormalCount> = (0..600)
        .map(|i| (format!("key_{i}"), NormalCount { count: i }))
        .collect();
    let input = f.serialize(&values).unwrap();
    assert_eq!(
        f.deserialize::<std::collections::HashMap<String, NormalCount>>(&input)
            .unwrap(),
        values
    );
    let view = schema
        .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
        .unwrap();
    let root = view.root().unwrap();
    let mut order = vec![];
    for pair in root.entries().unwrap() {
        let (key, value) = pair.unwrap();
        let key = key.read_string().unwrap().as_str().to_owned();
        assert_eq!(
            value.field_at(0).unwrap().read_i32().unwrap(),
            values[&key].count
        );
        order.push(key);
    }
    assert_eq!(order.len(), 600);
    drop(root);
    let validated = view.validate().unwrap();
    let root = validated.root().unwrap();
    let actual: Vec<String> = root
        .entries()
        .unwrap()
        .map(|pair| pair.unwrap().0.read_string().unwrap().as_str().to_owned())
        .collect();
    assert_eq!(actual, order);
}

fn skip_type_meta(reader: &mut fory::Reader<'_>) {
    let header = reader.read_i64().unwrap();
    let mut len = (header & 255) as usize;
    if len == 255 {
        len += reader.read_var_u32().unwrap() as usize;
    }
    reader.skip(len).unwrap();
}

#[test]
fn compatible_nested_refs_reject_forward_indices_and_repeated_new_consumes_slot() {
    let s = Schema::compile(count_descriptor_for_nested(), &SchemaLimits::default()).unwrap();
    let mut f = fory::Fory::builder().xlang(true).compatible(true).build();
    f.register::<NormalCount>(200).unwrap();
    f.register::<CanonicalParent>(201).unwrap();
    let input = f
        .serialize(&CanonicalParent {
            child: NormalCount { count: 1 },
            optional: Some(NormalCount { count: 2 }),
        })
        .unwrap();
    let mut reader = fory::Reader::new(&input);
    reader.skip(4).unwrap();
    skip_type_meta(&mut reader);
    assert_eq!(reader.read_u8().unwrap(), 28);
    let new_offset = reader.get_cursor();
    assert_eq!(reader.read_var_u32().unwrap(), 2);
    let meta_start = reader.get_cursor();
    skip_type_meta(&mut reader);
    let meta_end = reader.get_cursor();
    assert_eq!(reader.read_var_i32().unwrap(), 1);
    assert_eq!(reader.read_i8().unwrap(), -1);
    assert_eq!(reader.read_u8().unwrap(), 28);
    let ref_offset = reader.get_cursor();
    assert_eq!(reader.read_var_u32().unwrap(), 3);
    for (offset, token) in [(new_offset, 3u8), (ref_offset, 5), (ref_offset, 1)] {
        let mut bad = input.clone();
        bad[offset] = token;
        let view = s
            .view(&bad, InputFormat::XlangMessage, &ParseLimits::default())
            .unwrap();
        let root = view.root().unwrap();
        let error = root.field_at(1).err().unwrap();
        assert!(matches!(
            error,
            DataError::MalformedInput(_) | DataError::SchemaMismatch(_)
        ));
        assert_eq!(
            format!("{error:?}"),
            format!("{:?}", root.field_at(0).err().unwrap())
        );
        drop(root);
        assert_eq!(
            format!("{error:?}"),
            format!("{:?}", view.validate().err().unwrap())
        );
    }
    let mut repeated = input[..ref_offset].to_vec();
    repeated.push(4);
    repeated.extend_from_slice(&input[meta_start..meta_end]);
    repeated.extend_from_slice(&input[ref_offset + 1..]);
    let validated = s
        .validate(
            &repeated,
            InputFormat::XlangMessage,
            &ParseLimits::default(),
        )
        .unwrap();
    assert_eq!(
        validated
            .root()
            .unwrap()
            .field_at(1)
            .unwrap()
            .field_at(0)
            .unwrap()
            .read_i32()
            .unwrap(),
        2
    );
    for end in 4..input.len() {
        assert!(
            s.validate(
                &input[..end],
                InputFormat::XlangMessage,
                &ParseLimits::default()
            )
            .is_err()
        );
    }
    let mut trailing = input.clone();
    trailing.push(0);
    assert!(matches!(
        s.validate(
            &trailing,
            InputFormat::XlangMessage,
            &ParseLimits::default()
        ),
        Err(DataError::MalformedInput(_))
    ));
    let mut bad = input.clone();
    bad[meta_start + 4] ^= 1;
    assert!(matches!(
        s.validate(&bad, InputFormat::XlangMessage, &ParseLimits::default()),
        Err(DataError::MalformedInput(_))
    ));
}

#[test]
fn canonical_static_user_hashes_and_unknown_container_fingerprints_have_explicit_boundaries() {
    let mut descriptor = SchemaDescriptor {
        encoding: RootEncoding {
            protocol_version: ProtocolVersion::Fory1_7,
            mode: SchemaMode::SameSchema { version_hash: true },
            root_reference: RootReferenceMode::NullOnly,
        },
        key_semantics_version: KEY_SEMANTICS_VERSION,
        root: ty(TypeKind::User(UserIdentity::Numeric(201)), false),
        definitions: vec![
            UserDefinition {
                identity: UserIdentity::Numeric(201),
                kind: UserDefinitionKind::Struct {
                    fields: vec![
                        FieldDescriptor {
                            id: None,
                            name: "child".into(),
                            ty: ty(TypeKind::User(UserIdentity::Numeric(200)), false),
                        },
                        FieldDescriptor {
                            id: None,
                            name: "optional".into(),
                            ty: ty(TypeKind::User(UserIdentity::Numeric(200)), true),
                        },
                    ],
                    physical_field_order: vec![0, 1],
                },
            },
            UserDefinition {
                identity: UserIdentity::Numeric(200),
                kind: UserDefinitionKind::Struct {
                    fields: vec![FieldDescriptor {
                        id: None,
                        name: "count".into(),
                        ty: TypeDescriptor {
                            integer_encoding: Some(IntegerEncoding::Varint),
                            ..ty(TypeKind::Scalar(ScalarKind::I32), false)
                        },
                    }],
                    physical_field_order: vec![0],
                },
            },
        ],
    };
    let s = Schema::compile(descriptor.clone(), &SchemaLimits::default()).unwrap();
    let mut f = fory::Fory::builder()
        .xlang(true)
        .compatible(false)
        .check_struct_version(true)
        .build();
    f.register::<NormalCount>(200).unwrap();
    f.register::<CanonicalParent>(201).unwrap();
    for optional in [None, Some(NormalCount { count: 99 })] {
        let value = CanonicalParent {
            child: NormalCount { count: -7 },
            optional,
        };
        let input = f.serialize(&value).unwrap();
        assert_eq!(f.deserialize::<CanonicalParent>(&input).unwrap(), value);
        let view = s
            .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
            .unwrap();
        assert_eq!(
            view.root()
                .unwrap()
                .field_at(0)
                .unwrap()
                .field_at(0)
                .unwrap()
                .read_i32()
                .unwrap(),
            -7
        );
        let validated = view.validate().unwrap();
        assert_eq!(
            validated
                .root()
                .unwrap()
                .field_at(1)
                .unwrap()
                .is_null()
                .unwrap(),
            value.optional.is_none()
        );
        let retained = validated
            .root()
            .unwrap()
            .field_at(0)
            .unwrap()
            .field_at(0)
            .unwrap()
            .to_owned()
            .unwrap();
        drop(validated);
        drop(input);
        assert_eq!(retained.as_view().unwrap().read_i32().unwrap(), -7);
    }
    let UserDefinitionKind::Struct { fields, .. } = &mut descriptor.definitions[0].kind else {
        unreachable!()
    };
    fields[0].ty = ty(
        TypeKind::List(Box::new(ty(
            TypeKind::User(UserIdentity::Numeric(200)),
            false,
        ))),
        false,
    );
    let s = Schema::compile(descriptor, &SchemaLimits::default()).unwrap();
    assert!(matches!(
        s.view(&[], InputFormat::RootValue, &ParseLimits::default()),
        Err(DataError::MalformedInput(_))
    ));
    #[derive(Debug, PartialEq, fory::ForyStruct)]
    struct ContainerParent {
        child: Vec<NormalCount>,
        optional: Option<NormalCount>,
    }
    let mut f = fory::Fory::builder()
        .xlang(true)
        .compatible(false)
        .check_struct_version(true)
        .build();
    f.register::<NormalCount>(200).unwrap();
    f.register::<ContainerParent>(201).unwrap();
    let input = f
        .serialize(&ContainerParent {
            child: vec![NormalCount { count: -123 }],
            optional: None,
        })
        .unwrap();
    let view = s
        .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
        .unwrap();
    assert_eq!(
        view.root()
            .unwrap()
            .field_at(0)
            .unwrap()
            .element_at(0)
            .unwrap()
            .field_at(0)
            .unwrap()
            .read_i32()
            .unwrap(),
        -123
    );
    let validated = view.validate().unwrap();
    let owned = validated
        .root()
        .unwrap()
        .field_at(0)
        .unwrap()
        .element_at(0)
        .unwrap()
        .to_owned()
        .unwrap();
    drop(validated);
    drop(input);
    assert_eq!(
        owned
            .as_view()
            .unwrap()
            .field_at(0)
            .unwrap()
            .read_i32()
            .unwrap(),
        -123
    );
}

fn count_schema(mode: SchemaMode) -> Schema {
    Schema::compile(
        SchemaDescriptor {
            encoding: RootEncoding {
                protocol_version: ProtocolVersion::Fory1_7,
                mode,
                root_reference: RootReferenceMode::NullOnly,
            },
            key_semantics_version: KEY_SEMANTICS_VERSION,
            root: ty(TypeKind::User(UserIdentity::Numeric(200)), false),
            definitions: vec![UserDefinition {
                identity: UserIdentity::Numeric(200),
                kind: UserDefinitionKind::Struct {
                    fields: vec![FieldDescriptor {
                        id: None,
                        name: "count".into(),
                        ty: TypeDescriptor {
                            integer_encoding: Some(IntegerEncoding::Varint),
                            ..ty(TypeKind::Scalar(ScalarKind::I32), false)
                        },
                    }],
                    physical_field_order: vec![0],
                },
            }],
        },
        &SchemaLimits::default(),
    )
    .unwrap()
}

#[test]
fn alias_has_same_identity_body_but_different_version_fingerprint() {
    let mut normal = fory::Fory::builder()
        .xlang(true)
        .compatible(false)
        .check_struct_version(true)
        .build();
    normal.register::<NormalCount>(200).unwrap();
    let mut alias = fory::Fory::builder()
        .xlang(true)
        .compatible(false)
        .check_struct_version(true)
        .build();
    alias.register::<AliasCount>(200).unwrap();
    let a = normal.serialize(&NormalCount { count: -42 }).unwrap();
    let b = alias.serialize(&AliasCount { count: -42 }).unwrap();
    assert_eq!(normal.deserialize::<NormalCount>(&a).unwrap().count, -42);
    assert_eq!(alias.deserialize::<AliasCount>(&b).unwrap().count, -42);
    assert_eq!(&a[..5], &b[..5]);
    assert_eq!(&a[9..], &b[9..]);
    assert_ne!(&a[5..9], &b[5..9]);
    for (input, fingerprint) in [(&a, "count,5,0,0;"), (&b, "count,0,0,0;")] {
        let hash = fory::__private::util::murmurhash3_x64_128(fingerprint.as_bytes(), 47).0 as u32;
        assert_eq!(&input[5..9], &hash.to_le_bytes());
        let s = count_schema(SchemaMode::SameSchema { version_hash: true });
        if fingerprint == "count,5,0,0;" {
            assert_eq!(
                s.validate(input, InputFormat::XlangMessage, &ParseLimits::default())
                    .unwrap()
                    .root()
                    .unwrap()
                    .field_at(0)
                    .unwrap()
                    .read_i32()
                    .unwrap(),
                -42
            );
        } else {
            assert!(matches!(
                s.validate(input, InputFormat::XlangMessage, &ParseLimits::default()),
                Err(DataError::SchemaMismatch(_))
            ));
        }
    }
}

fn metadata_message(body: &[u8], named: bool) -> Vec<u8> {
    let len = body.len();
    let low = len.min(255) as u64;
    let mut hashed = body.to_vec();
    hashed.extend_from_slice(&(low as u16).to_le_bytes());
    let hash = (fory::__private::util::murmurhash3_x64_128(&hashed, 47).0 as i64)
        .wrapping_shl(12)
        .wrapping_abs() as u64;
    let mut input = vec![1, 255, if named { 30 } else { 28 }, 0];
    let mut writer = fory::Writer::from_buffer(&mut input);
    writer.write_i64(((hash & !0xfff) | low) as i64);
    if len >= 255 {
        writer.write_var_u32((len - 255) as u32);
    }
    writer.write_bytes(body);
    writer.write_u8(1);
    input
}

#[test]
fn compatible_field_tag_above_wire_max_is_malformed_even_when_schema_matches() {
    let tag = 1u32 << 29;
    let s = Schema::compile(
        SchemaDescriptor {
            encoding: RootEncoding {
                protocol_version: ProtocolVersion::Fory1_7,
                mode: SchemaMode::Compatible,
                root_reference: RootReferenceMode::NullOnly,
            },
            key_semantics_version: KEY_SEMANTICS_VERSION,
            root: ty(TypeKind::User(UserIdentity::Numeric(100)), false),
            definitions: vec![UserDefinition {
                identity: UserIdentity::Numeric(100),
                kind: UserDefinitionKind::Struct {
                    fields: vec![FieldDescriptor {
                        id: Some(tag),
                        name: "flag".into(),
                        ty: ty(TypeKind::Scalar(ScalarKind::Bool), false),
                    }],
                    physical_field_order: vec![0],
                },
            }],
        },
        &SchemaLimits::default(),
    )
    .unwrap();
    let mut body = vec![0xc1, 100, 0xfc];
    let mut writer = fory::Writer::from_buffer(&mut body);
    writer.write_var_u32(tag - 15);
    writer.write_u8(1);
    let mut upstream = fory::Reader::new(&body[2..]);
    assert!(fory::__private::meta::FieldInfo::from_bytes(&mut upstream).is_err());
    let input = metadata_message(&body, false);
    assert!(matches!(
        s.validate(&input, InputFormat::XlangMessage, &ParseLimits::default()),
        Err(DataError::MalformedInput(_))
    ));
}

#[test]
fn compatible_named_length_overflow_is_malformed_even_with_valid_hash_and_payload() {
    for namespace_target in [true, false] {
        let long = "a".repeat(63);
        let (namespace, name) = if namespace_target {
            (long.as_str(), "N")
        } else {
            ("n", long.as_str())
        };
        let mut b = SchemaRegistryBuilder::new();
        b.register_by_name::<Empty>(namespace, name).unwrap();
        let r = b.build().unwrap();
        let encoding = RootEncoding {
            protocol_version: ProtocolVersion::Fory1_7,
            mode: SchemaMode::Compatible,
            root_reference: RootReferenceMode::NullOnly,
        };
        let s = Schema::compile(
            r.describe::<Empty>(&encoding).unwrap(),
            &SchemaLimits::default(),
        )
        .unwrap();
        for extension in [
            vec![128, 128, 128, 128, 16],
            vec![128, 128, 128, 128, 32],
            vec![128, 128, 128, 128, 128],
            vec![128],
        ] {
            let mut body = vec![0xe0];
            for (target, text) in [(namespace_target, namespace), (!namespace_target, name)] {
                if target {
                    body.push(252);
                    body.extend_from_slice(&extension);
                    body.extend_from_slice(text.as_bytes());
                } else {
                    body.push((text.len() as u8) << 2);
                    body.extend_from_slice(text.as_bytes());
                }
            }
            let mut input = metadata_message(&body, true);
            input.pop(); // Empty struct has no payload; do not hide acceptance behind trailing bytes.
            assert!(matches!(
                s.validate(&input, InputFormat::XlangMessage, &ParseLimits::default()),
                Err(DataError::MalformedInput(_))
            ));
        }
    }
}

#[test]
fn compatible_checked_names_keep_encoding_and_sixty_three_length_boundary() {
    for len in [62usize, 63, 64] {
        for named_part in [true, false] {
            let long = "a".repeat(len);
            let (namespace, name) = if named_part {
                (long.as_str(), "N")
            } else {
                ("n", long.as_str())
            };
            let mut b = SchemaRegistryBuilder::new();
            b.register_by_name::<Empty>(namespace, name).unwrap();
            let r = b.build().unwrap();
            let s = Schema::compile(
                r.describe::<Empty>(&RootEncoding {
                    protocol_version: ProtocolVersion::Fory1_7,
                    mode: SchemaMode::Compatible,
                    root_reference: RootReferenceMode::NullOnly,
                })
                .unwrap(),
                &SchemaLimits::default(),
            )
            .unwrap();
            let mut body = vec![0xe0];
            for text in [namespace, name] {
                body.push((text.len().min(63) as u8) << 2);
                if text.len() >= 63 {
                    body.push((text.len() - 63) as u8);
                }
                body.extend_from_slice(text.as_bytes());
            }
            let mut input = metadata_message(&body, true);
            input.pop();
            assert_eq!(
                s.validate(&input, InputFormat::XlangMessage, &ParseLimits::default())
                    .unwrap()
                    .root()
                    .unwrap()
                    .fields()
                    .unwrap()
                    .count(),
                0
            );
            let mut f = fory::Fory::builder().xlang(true).compatible(true).build();
            r.apply_to_fory(&mut f).unwrap();
            assert_eq!(f.deserialize::<Empty>(&input).unwrap(), Empty {});
            // Both a truly truncated extension and an invalid namespace encoding are malformed.
            for body in [vec![0xe0, 252, 128], vec![0xe0, 3]] {
                let mut bad = metadata_message(&body, true);
                bad.pop();
                assert!(matches!(
                    s.validate(&bad, InputFormat::XlangMessage, &ParseLimits::default()),
                    Err(DataError::MalformedInput(_))
                ));
            }
        }
    }
}

#[derive(Debug, PartialEq, fory::ForyStruct)]
struct DenseBools {
    #[fory(array)]
    values: Vec<bool>,
}
#[derive(Debug, PartialEq, fory::ForyStruct)]
struct DenseShorts {
    #[fory(array)]
    values: Vec<i16>,
}
#[derive(Debug, PartialEq, fory::ForyStruct)]
struct GenericBools {
    values: std::collections::VecDeque<bool>,
}
#[derive(Debug, PartialEq, fory::ForyStruct)]
struct GenericShorts {
    values: std::collections::VecDeque<i16>,
}
#[derive(Debug, PartialEq, fory::ForyStruct)]
struct GenericStrings {
    values: Vec<String>,
}
#[derive(Debug, PartialEq, fory::ForyStruct)]
struct PlainBools {
    values: Vec<bool>,
}
#[derive(Debug, PartialEq, fory::ForyStruct)]
struct PlainShorts {
    values: Vec<i16>,
}

#[test]
fn unique_static_string_lists_remain_readable() {
    let mut f = fory::Fory::builder()
        .xlang(true)
        .compatible(false)
        .check_struct_version(false)
        .build();
    f.register::<GenericStrings>(200).unwrap();
    let value = GenericStrings {
        values: vec!["one".into(), "two".into()],
    };
    let input = f.serialize(&value).unwrap();
    assert_eq!(f.deserialize::<GenericStrings>(&input).unwrap(), value);
    let s = list_field_schema(ScalarKind::String);
    let view = s
        .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
        .unwrap();
    let root = view.root().unwrap();
    assert_eq!(
        root.field_at(0)
            .unwrap()
            .element_at(1)
            .unwrap()
            .read_string()
            .unwrap()
            .as_str(),
        "two"
    );
    let validated = view.validate().unwrap();
    let root = validated.root().unwrap();
    assert_eq!(
        root.field_at(0)
            .unwrap()
            .elements()
            .unwrap()
            .map(|n| n.unwrap().read_string().unwrap().as_str().to_owned())
            .collect::<Vec<_>>(),
        value.values
    );
}

fn list_field_schema(kind: ScalarKind) -> Schema {
    Schema::compile(
        SchemaDescriptor {
            encoding: RootEncoding {
                protocol_version: ProtocolVersion::Fory1_7,
                mode: SchemaMode::SameSchema {
                    version_hash: false,
                },
                root_reference: RootReferenceMode::NullOnly,
            },
            key_semantics_version: KEY_SEMANTICS_VERSION,
            root: ty(TypeKind::User(UserIdentity::Numeric(200)), false),
            definitions: vec![UserDefinition {
                identity: UserIdentity::Numeric(200),
                kind: UserDefinitionKind::Struct {
                    fields: vec![FieldDescriptor {
                        id: None,
                        name: "values".into(),
                        ty: ty(
                            TypeKind::List(Box::new(ty(TypeKind::Scalar(kind), false))),
                            false,
                        ),
                    }],
                    physical_field_order: vec![0],
                },
            }],
        },
        &SchemaLimits::default(),
    )
    .unwrap()
}

fn named_fixture(namespace: &str, name: &str) -> (Schema, fory::Fory) {
    let mut b = SchemaRegistryBuilder::new();
    b.register_by_name::<Record>(namespace, name).unwrap();
    let r = b.build().unwrap();
    let s = Schema::compile(
        r.describe::<Record>(&RootEncoding {
            protocol_version: ProtocolVersion::Fory1_7,
            mode: SchemaMode::SameSchema {
                version_hash: false,
            },
            root_reference: RootReferenceMode::NullOnly,
        })
        .unwrap(),
        &SchemaLimits::default(),
    )
    .unwrap();
    let mut f = fory::Fory::builder()
        .xlang(true)
        .compatible(false)
        .check_struct_version(false)
        .build();
    r.apply_to_fory(&mut f).unwrap();
    (s, f)
}

#[test]
fn inline_name_hashes_check_encoded_lengths_sixteen_and_seventeen() {
    for (namespace, name, length) in [
        ("a".repeat(25), "b".repeat(25), 16usize),
        ("a".repeat(26), "b".repeat(26), 17usize),
    ] {
        let (s, f) = named_fixture(&namespace, &name);
        let value = Record {
            text: None,
            flag: true,
            count: 42,
        };
        let input = f.serialize(&value).unwrap();
        let (_, oracle) = named_fixture(&namespace, &name);
        assert_eq!(oracle.deserialize::<Record>(&input).unwrap(), value);
        assert_eq!(
            s.validate(&input, InputFormat::XlangMessage, &ParseLimits::default())
                .unwrap()
                .root()
                .unwrap()
                .field_at(2)
                .unwrap()
                .read_i32()
                .unwrap(),
            42
        );
        let mut reader = fory::Reader::new(&input);
        reader.skip(3).unwrap();
        for _ in 0..2 {
            let header = reader.read_var_u32().unwrap();
            assert_eq!(header & 1, 0);
            assert_eq!(header >> 1, length as u32);
            let hash_offset = reader.get_cursor();
            if length > 16 {
                reader.read_i64().unwrap();
            } else {
                assert!(matches!(reader.read_u8().unwrap(), 1 | 4));
            }
            reader.skip(length).unwrap();
            if length > 16 {
                let mut bad = input.clone();
                bad[hash_offset + 3] ^= 0x40;
                let (_, oracle) = named_fixture(&namespace, &name);
                assert!(oracle.deserialize::<Record>(&bad).is_err());
                assert!(matches!(
                    s.validate(&bad, InputFormat::XlangMessage, &ParseLimits::default()),
                    Err(DataError::MalformedInput(_))
                ));
            }
        }
    }
}

#[test]
fn corrupt_long_group_name_hash_preserves_first_failure() {
    let namespace = "a".repeat(37);
    let name = "b".repeat(37);
    let mut b = SchemaRegistryBuilder::new();
    b.register_by_name::<Record>(&namespace, &name).unwrap();
    let r = b.build().unwrap();
    let mut d = r
        .describe::<Record>(&RootEncoding {
            protocol_version: ProtocolVersion::Fory1_7,
            mode: SchemaMode::SameSchema {
                version_hash: false,
            },
            root_reference: RootReferenceMode::NullOnly,
        })
        .unwrap();
    d.root = ty(TypeKind::List(Box::new(d.root)), false);
    let s = Schema::compile(d, &SchemaLimits::default()).unwrap();
    let (_, f) = named_fixture(&namespace, &name);
    let mut input = f
        .serialize(&vec![Record {
            text: None,
            flag: true,
            count: 1,
        }])
        .unwrap();
    let mut reader = fory::Reader::new(&input);
    reader.skip(6).unwrap(); // Message, root ref/type/count, collection header, group type.
    assert_eq!(reader.read_var_u32().unwrap(), 48);
    let hash_offset = reader.get_cursor();
    input[hash_offset + 2] ^= 0x20;
    let (_, oracle) = named_fixture(&namespace, &name);
    assert!(oracle.deserialize::<Vec<Record>>(&input).is_err());
    let view = s
        .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
        .unwrap();
    let root = view.root().unwrap();
    let error = root.len().err().unwrap();
    assert!(matches!(error, DataError::MalformedInput(_)));
    assert_eq!(
        format!("{error:?}"),
        format!("{:?}", root.elements().err().unwrap())
    );
    drop(root);
    assert_eq!(
        format!("{error:?}"),
        format!("{:?}", view.validate().err().unwrap())
    );
}

#[test]
fn ambiguous_static_primitive_lists_are_not_guessed_or_reported_corrupt() {
    let mut f = fory::Fory::builder()
        .xlang(true)
        .compatible(false)
        .check_struct_version(false)
        .build();
    f.register::<DenseBools>(200).unwrap();
    let value = DenseBools {
        values: vec![true, false],
    };
    let dense_bool = f.serialize(&value).unwrap();
    assert_eq!(f.deserialize::<DenseBools>(&dense_bool).unwrap(), value);
    let mut f = fory::Fory::builder()
        .xlang(true)
        .compatible(false)
        .check_struct_version(false)
        .build();
    f.register::<GenericBools>(200).unwrap();
    let value = GenericBools {
        values: std::collections::VecDeque::from([true, false]),
    };
    let generic_bool = f.serialize(&value).unwrap();
    assert_eq!(f.deserialize::<GenericBools>(&generic_bool).unwrap(), value);
    let mut f = fory::Fory::builder()
        .xlang(true)
        .compatible(false)
        .check_struct_version(false)
        .build();
    f.register::<DenseShorts>(200).unwrap();
    let value = DenseShorts {
        values: vec![7, -9],
    };
    let dense_short = f.serialize(&value).unwrap();
    assert_eq!(f.deserialize::<DenseShorts>(&dense_short).unwrap(), value);
    let mut f = fory::Fory::builder()
        .xlang(true)
        .compatible(false)
        .check_struct_version(false)
        .build();
    f.register::<GenericShorts>(200).unwrap();
    let value = GenericShorts {
        values: std::collections::VecDeque::from([7, -9]),
    };
    let generic_short = f.serialize(&value).unwrap();
    assert_eq!(
        f.deserialize::<GenericShorts>(&generic_short).unwrap(),
        value
    );
    assert_ne!(dense_bool, generic_bool);
    assert_ne!(dense_short, generic_short);
    let mut plain = fory::Fory::builder()
        .xlang(true)
        .compatible(false)
        .check_struct_version(false)
        .build();
    plain.register::<PlainBools>(200).unwrap();
    let value = PlainBools {
        values: vec![true, false],
    };
    let input = plain.serialize(&value).unwrap();
    assert_eq!(plain.deserialize::<PlainBools>(&input).unwrap(), value);
    assert_eq!(input, generic_bool);
    let mut plain = fory::Fory::builder()
        .xlang(true)
        .compatible(false)
        .check_struct_version(false)
        .build();
    plain.register::<PlainShorts>(200).unwrap();
    let value = PlainShorts {
        values: vec![7, -9],
    };
    let input = plain.serialize(&value).unwrap();
    assert_eq!(plain.deserialize::<PlainShorts>(&input).unwrap(), value);
    assert_eq!(input, generic_short);
    for (kind, inputs) in [
        (ScalarKind::Bool, [dense_bool, generic_bool]),
        (ScalarKind::I16, [dense_short, generic_short]),
    ] {
        let s = list_field_schema(kind);
        for input in inputs {
            let view = s
                .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
                .unwrap();
            let root = view.root().unwrap();
            assert!(matches!(
                root.field_at(0),
                Err(DataError::NotImplemented(_))
            ));
            drop(root);
            assert!(matches!(view.validate(), Err(DataError::NotImplemented(_))));
        }
    }
}

fn ty(kind: TypeKind, nullable: bool) -> TypeDescriptor {
    TypeDescriptor {
        kind,
        nullable,
        track_ref: false,
        integer_encoding: None,
    }
}

#[derive(Debug, PartialEq, fory::ForyStruct)]
struct Record {
    text: Option<String>,
    #[fory(id = 7)]
    flag: bool,
    count: i32,
}

impl DescribeSchema for Record {
    fn describe_type(_: &RootEncoding, r: &SchemaRegistry) -> Result<TypeDescriptor, SchemaError> {
        Ok(ty(TypeKind::User(r.identity_of::<Self>()?.clone()), false))
    }
    fn describe_definition(
        _: &RootEncoding,
        r: &SchemaRegistry,
    ) -> Result<Option<UserDefinition>, SchemaError> {
        let mut count = ty(TypeKind::Scalar(ScalarKind::I32), false);
        count.integer_encoding = Some(IntegerEncoding::Varint);
        let fields = vec![
            FieldDescriptor {
                id: None,
                name: "text".into(),
                ty: ty(TypeKind::Scalar(ScalarKind::String), true),
            },
            FieldDescriptor {
                id: Some(7),
                name: "flag".into(),
                ty: ty(TypeKind::Scalar(ScalarKind::Bool), false),
            },
            FieldDescriptor {
                id: None,
                name: "count".into(),
                ty: count,
            },
        ];
        let physical_field_order = <Self as fory::StructSerializer>::sorted_field_names()
            .iter()
            .map(|name| fields.iter().position(|f| f.name == *name).unwrap())
            .collect();
        Ok(Some(UserDefinition {
            identity: r.identity_of::<Self>()?.clone(),
            kind: UserDefinitionKind::Struct {
                fields,
                physical_field_order,
            },
        }))
    }
}

fn setup(mode: SchemaMode, named: bool) -> (Schema, fory::Fory) {
    let mut builder = SchemaRegistryBuilder::new();
    if named {
        builder
            .register_by_name::<Record>("test", "Record")
            .unwrap();
    } else {
        builder.register_by_id::<Record>(100).unwrap();
    }
    let registry = builder.build().unwrap();
    let encoding = RootEncoding {
        protocol_version: ProtocolVersion::Fory1_7,
        mode,
        root_reference: RootReferenceMode::NullOnly,
    };
    let schema = Schema::compile(
        registry.describe::<Record>(&encoding).unwrap(),
        &SchemaLimits::default(),
    )
    .unwrap();
    let mut fory = fory::Fory::builder()
        .xlang(true)
        .compatible(matches!(mode, SchemaMode::Compatible))
        .check_struct_version(matches!(
            mode,
            SchemaMode::SameSchema { version_hash: true }
        ))
        .build();
    registry.apply_to_fory(&mut fory).unwrap();
    (schema, fory)
}

#[test]
fn numeric_struct_fields_use_logical_order_and_retain_root() {
    let (schema, fory) = setup(
        SchemaMode::SameSchema {
            version_hash: false,
        },
        false,
    );
    let record = Record {
        text: Some("hello".into()),
        flag: true,
        count: -123,
    };
    let input = fory.serialize(&record).unwrap();
    assert_eq!(fory.deserialize::<Record>(&input).unwrap(), record);
    let view = schema
        .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
        .unwrap();
    let root = view.root().unwrap();
    assert!(matches!(root.field_at(3), Err(DataError::AccessError(_))));
    assert!(matches!(
        root.field_by_name("missing"),
        Err(DataError::AccessError(_))
    ));
    assert!(matches!(
        root.field_by_id(0),
        Err(DataError::AccessError(_))
    ));
    assert!(root.field_by_id(7).unwrap().read_bool().unwrap());
    assert_eq!(
        root.field_by_name("count").unwrap().read_i32().unwrap(),
        -123
    );
    assert_eq!(
        root.fields()
            .unwrap()
            .map(|p| p.unwrap().0.name.clone())
            .collect::<Vec<_>>(),
        vec!["text", "flag", "count"]
    );
    let owned = root.field_at(0).unwrap().to_owned().unwrap();
    drop(root);
    drop(view);
    assert_eq!(
        owned.as_view().unwrap().read_string().unwrap().as_str(),
        "hello"
    );
    let validated = schema
        .validate(&input, InputFormat::XlangMessage, &ParseLimits::default())
        .unwrap();
    let root = validated.root().unwrap();
    assert!(root.field_by_id(7).unwrap().read_bool().unwrap());
    assert_eq!(root.field_at(2).unwrap().read_i32().unwrap(), -123);
    assert_eq!(root.fields().unwrap().count(), 3);
}

#[test]
fn named_identity_is_exact_and_unsupported_modes_are_not_accepted() {
    let value = Record {
        text: None,
        flag: false,
        count: 42,
    };
    let mode = SchemaMode::SameSchema {
        version_hash: false,
    };
    let (s, f) = setup(mode, true);
    let input = f.serialize(&value).unwrap();
    assert_eq!(f.deserialize::<Record>(&input).unwrap(), value);
    let validated = s
        .validate(&input, InputFormat::XlangMessage, &ParseLimits::default())
        .unwrap();
    assert!(
        validated
            .root()
            .unwrap()
            .field_at(0)
            .unwrap()
            .is_null()
            .unwrap()
    );
    for mode in [
        SchemaMode::Compatible,
        SchemaMode::SameSchema { version_hash: true },
    ] {
        for named in [false, true] {
            let (s, f) = setup(mode, named);
            let input = f.serialize(&value).unwrap();
            assert_eq!(
                s.validate(&input, InputFormat::XlangMessage, &ParseLimits::default())
                    .unwrap()
                    .root()
                    .unwrap()
                    .field_at(2)
                    .unwrap()
                    .read_i32()
                    .unwrap(),
                42
            );
        }
    }
}

#[test]
fn canonical_version_hash_matches_and_mutations_are_schema_mismatches() {
    for named in [false, true] {
        let (s, f) = setup(SchemaMode::SameSchema { version_hash: true }, named);
        let value = Record {
            text: Some("hash".into()),
            flag: true,
            count: -42,
        };
        let input = f.serialize(&value).unwrap();
        assert_eq!(f.deserialize::<Record>(&input).unwrap(), value);
        let view = s
            .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
            .unwrap();
        assert_eq!(
            view.root()
                .unwrap()
                .field_at(2)
                .unwrap()
                .read_i32()
                .unwrap(),
            -42
        );
        let validated = view.validate().unwrap();
        assert!(
            validated
                .root()
                .unwrap()
                .field_by_id(7)
                .unwrap()
                .read_bool()
                .unwrap()
        );
        let mut reader = fory::Reader::new(&input);
        reader.skip(3).unwrap();
        if named {
            for _ in 0..2 {
                let n = reader.read_var_u32().unwrap() >> 1;
                reader
                    .skip(if n > 16 {
                        8
                    } else if n > 0 {
                        1
                    } else {
                        0
                    })
                    .unwrap();
                reader.skip(n as usize).unwrap();
            }
        } else {
            reader.read_var_u32().unwrap();
        }
        let offset = reader.get_cursor();
        let mut bad = input.clone();
        bad[offset] ^= 1;
        assert!(f.deserialize::<Record>(&bad).is_err());
        assert!(matches!(
            s.validate(&bad, InputFormat::XlangMessage, &ParseLimits::default()),
            Err(DataError::SchemaMismatch(_))
        ));
        for end in offset..offset + 4 {
            assert!(
                s.validate(
                    &input[..end],
                    InputFormat::XlangMessage,
                    &ParseLimits::default()
                )
                .is_err()
            );
        }
    }
}

#[test]
fn compatible_scalar_struct_metadata_is_strict_and_owned() {
    for named in [false, true] {
        let (s, f) = setup(SchemaMode::Compatible, named);
        let value = Record {
            text: Some("meta".into()),
            flag: true,
            count: 12,
        };
        let input = f.serialize(&value).unwrap();
        assert_eq!(f.deserialize::<Record>(&input).unwrap(), value);
        let view = s
            .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
            .unwrap();
        let root = view.root().unwrap();
        assert_eq!(root.field_at(2).unwrap().read_i32().unwrap(), 12);
        let retained = root.field_at(0).unwrap().to_owned().unwrap();
        drop(root);
        drop(view);
        let validated = s
            .validate(&input, InputFormat::XlangMessage, &ParseLimits::default())
            .unwrap();
        let retained_validated = validated
            .root()
            .unwrap()
            .field_at(0)
            .unwrap()
            .to_owned()
            .unwrap();
        drop(validated);
        drop(input);
        assert_eq!(
            retained.as_view().unwrap().read_string().unwrap().as_str(),
            "meta"
        );
        assert_eq!(
            retained_validated
                .as_view()
                .unwrap()
                .read_string()
                .unwrap()
                .as_str(),
            "meta"
        );
    }
}

#[test]
fn compatible_metadata_mutations_are_not_accepted_by_hash_alone() {
    let (s, f) = setup(SchemaMode::Compatible, false);
    let input = f
        .serialize(&Record {
            text: None,
            flag: true,
            count: 12,
        })
        .unwrap();
    let mut reader = fory::Reader::new(&input);
    reader.skip(4).unwrap(); // message/ref/type/new slot
    let header_offset = reader.get_cursor();
    let header = reader.read_i64().unwrap();
    let len = (header & 255) as usize;
    assert!(len < 255);
    let body_offset = reader.get_cursor();
    let body_end = body_offset + len;
    let mut meta = fory::Reader::new(&input[body_offset..body_end]);
    meta.read_u8().unwrap();
    meta.read_var_u32().unwrap();
    let mut ranges = vec![];
    while meta.get_cursor() < len {
        let start = meta.get_cursor();
        fory::__private::meta::FieldInfo::from_bytes(&mut meta).unwrap();
        ranges.push(start..meta.get_cursor());
    }
    assert_eq!(ranges.len(), 3);
    let rehash = |bytes: &mut Vec<u8>| {
        let mut hash_input = bytes[body_offset..body_end].to_vec();
        hash_input.extend_from_slice(&((header as u64 & 0xfff) as u16).to_le_bytes());
        let hash = (fory::__private::util::murmurhash3_x64_128(&hash_input, 47).0 as i64)
            .wrapping_shl(12)
            .wrapping_abs() as u64;
        let updated = (hash & !0xfff) | (header as u64 & 0xfff);
        bytes[header_offset..header_offset + 8].copy_from_slice(&updated.to_le_bytes());
    };
    let mut mutations = vec![];
    let mut bad = input.clone();
    bad[body_offset] ^= 1;
    mutations.push(bad); // count
    let mut bad = input.clone();
    bad[body_offset + ranges[0].start] ^= 2;
    mutations.push(bad); // nullable
    let mut bad = input.clone();
    bad[body_offset + ranges[0].start] ^= 1;
    mutations.push(bad); // ref
    let mut bad = input.clone();
    bad[body_offset + ranges[0].start + 1] = 2;
    mutations.push(bad); // type
    let mut bad = input.clone();
    bad[body_offset + ranges[0].end - 1] ^= 4;
    mutations.push(bad); // name/tag
    let mut bad = input.clone();
    let mut swapped = input[body_offset + ranges[1].start..body_offset + ranges[1].end].to_vec();
    swapped.extend_from_slice(&input[body_offset + ranges[0].start..body_offset + ranges[0].end]);
    bad[body_offset + ranges[0].start..body_offset + ranges[1].end].copy_from_slice(&swapped);
    mutations.push(bad);
    for mut bad in mutations {
        rehash(&mut bad);
        assert!(matches!(
            s.validate(&bad, InputFormat::XlangMessage, &ParseLimits::default()),
            Err(DataError::SchemaMismatch(_)) | Err(DataError::MalformedInput(_))
        ));
    }
    let mut bad = input.clone();
    bad[header_offset + 4] ^= 1;
    assert!(f.deserialize::<Record>(&bad).is_err());
    assert!(matches!(
        s.validate(&bad, InputFormat::XlangMessage, &ParseLimits::default()),
        Err(DataError::MalformedInput(_))
    ));
    for end in 0..input.len() {
        assert!(
            s.validate(
                &input[..end],
                InputFormat::XlangMessage,
                &ParseLimits::default()
            )
            .is_err()
        );
    }
    let mut trailing = input.clone();
    trailing.push(0);
    let view = s
        .view(
            &trailing,
            InputFormat::XlangMessage,
            &ParseLimits::default(),
        )
        .unwrap();
    let retained = view.root().unwrap().to_owned().unwrap();
    let error = view.validate().err().unwrap();
    assert_eq!(
        format!("{error:?}"),
        format!("{:?}", retained.as_view().err().unwrap())
    );
}

#[test]
fn versioned_container_roots_validate_canonical_hash_per_struct() {
    let mode = SchemaMode::SameSchema { version_hash: true };
    let mut b = SchemaRegistryBuilder::new();
    b.register_by_id::<Record>(100).unwrap();
    let r = b.build().unwrap();
    let encoding = RootEncoding {
        protocol_version: ProtocolVersion::Fory1_7,
        mode,
        root_reference: RootReferenceMode::NullOnly,
    };
    let mut descriptor = r.describe::<Record>(&encoding).unwrap();
    descriptor.root = ty(TypeKind::List(Box::new(descriptor.root)), false);
    let s = Schema::compile(descriptor, &SchemaLimits::default()).unwrap();
    let (_, f) = setup(mode, false);
    let input = f
        .serialize(&vec![
            Record {
                text: None,
                flag: true,
                count: 1,
            },
            Record {
                text: Some("two".into()),
                flag: false,
                count: 2,
            },
        ])
        .unwrap();
    assert_eq!(f.deserialize::<Vec<Record>>(&input).unwrap().len(), 2);
    let validated = s
        .validate(&input, InputFormat::XlangMessage, &ParseLimits::default())
        .unwrap();
    assert_eq!(
        validated
            .root()
            .unwrap()
            .element_at(1)
            .unwrap()
            .field_at(2)
            .unwrap()
            .read_i32()
            .unwrap(),
        2
    );
    let mut bad = input.clone();
    bad[7] ^= 1;
    assert!(matches!(
        s.validate(&bad, InputFormat::XlangMessage, &ParseLimits::default()),
        Err(DataError::SchemaMismatch(_))
    ));
}

#[derive(Debug, PartialEq, fory::ForyStruct)]
struct Encodings {
    #[fory(encoding = "fixed")]
    fixed: i32,
    #[fory(encoding = "tagged")]
    tagged: i64,
    optional: Option<u32>,
}

#[test]
fn metadata_and_version_hash_respect_explicit_integer_codecs() {
    for mode in [
        SchemaMode::Compatible,
        SchemaMode::SameSchema { version_hash: true },
    ] {
        let fields = vec![
            FieldDescriptor {
                id: None,
                name: "fixed".into(),
                ty: TypeDescriptor {
                    integer_encoding: Some(IntegerEncoding::Fixed),
                    ..ty(TypeKind::Scalar(ScalarKind::I32), false)
                },
            },
            FieldDescriptor {
                id: None,
                name: "tagged".into(),
                ty: TypeDescriptor {
                    integer_encoding: Some(IntegerEncoding::Tagged),
                    ..ty(TypeKind::Scalar(ScalarKind::I64), false)
                },
            },
            FieldDescriptor {
                id: None,
                name: "optional".into(),
                ty: TypeDescriptor {
                    integer_encoding: Some(IntegerEncoding::Varint),
                    ..ty(TypeKind::Scalar(ScalarKind::U32), true)
                },
            },
        ];
        let order = <Encodings as fory::StructSerializer>::sorted_field_names()
            .iter()
            .map(|name| fields.iter().position(|f| f.name == *name).unwrap())
            .collect();
        let s = Schema::compile(
            SchemaDescriptor {
                encoding: RootEncoding {
                    protocol_version: ProtocolVersion::Fory1_7,
                    mode,
                    root_reference: RootReferenceMode::NullOnly,
                },
                key_semantics_version: KEY_SEMANTICS_VERSION,
                root: ty(TypeKind::User(UserIdentity::Numeric(222)), false),
                definitions: vec![UserDefinition {
                    identity: UserIdentity::Numeric(222),
                    kind: UserDefinitionKind::Struct {
                        fields,
                        physical_field_order: order,
                    },
                }],
            },
            &SchemaLimits::default(),
        )
        .unwrap();
        let mut f = fory::Fory::builder()
            .xlang(true)
            .compatible(matches!(mode, SchemaMode::Compatible))
            .check_struct_version(matches!(
                mode,
                SchemaMode::SameSchema { version_hash: true }
            ))
            .build();
        f.register::<Encodings>(222).unwrap();
        let value = Encodings {
            fixed: i32::MIN,
            tagged: i64::MAX,
            optional: Some(u32::MAX),
        };
        let input = f.serialize(&value).unwrap();
        assert_eq!(f.deserialize::<Encodings>(&input).unwrap(), value);
        let validated = s
            .validate(&input, InputFormat::XlangMessage, &ParseLimits::default())
            .unwrap();
        let root = validated.root().unwrap();
        assert_eq!(root.field_at(0).unwrap().read_i32().unwrap(), i32::MIN);
        assert_eq!(root.field_at(1).unwrap().read_i64().unwrap(), i64::MAX);
        assert_eq!(root.field_at(2).unwrap().read_u32().unwrap(), u32::MAX);
    }
}

#[derive(Debug, PartialEq, fory::ForyStruct)]
struct Empty {}
impl DescribeSchema for Empty {
    fn describe_type(_: &RootEncoding, r: &SchemaRegistry) -> Result<TypeDescriptor, SchemaError> {
        Ok(ty(TypeKind::User(r.identity_of::<Self>()?.clone()), false))
    }
    fn describe_definition(
        _: &RootEncoding,
        r: &SchemaRegistry,
    ) -> Result<Option<UserDefinition>, SchemaError> {
        Ok(Some(UserDefinition {
            identity: r.identity_of::<Self>()?.clone(),
            kind: UserDefinitionKind::Struct {
                fields: vec![],
                physical_field_order: vec![],
            },
        }))
    }
}

#[derive(Debug, PartialEq, fory::ForyStruct)]
struct Nested {
    child: Option<Record>,
    items: Vec<Record>,
    mapping: std::collections::BTreeMap<String, Record>,
    day: fory::Date,
    empty: Empty,
}
impl DescribeSchema for Nested {
    fn describe_type(_: &RootEncoding, r: &SchemaRegistry) -> Result<TypeDescriptor, SchemaError> {
        Ok(ty(TypeKind::User(r.identity_of::<Self>()?.clone()), false))
    }
    fn describe_definition(
        _: &RootEncoding,
        r: &SchemaRegistry,
    ) -> Result<Option<UserDefinition>, SchemaError> {
        let record = || {
            ty(
                TypeKind::User(r.identity_of::<Record>().unwrap().clone()),
                false,
            )
        };
        let fields = vec![
            FieldDescriptor {
                id: None,
                name: "child".into(),
                ty: ty(record().kind, true),
            },
            FieldDescriptor {
                id: None,
                name: "items".into(),
                ty: ty(TypeKind::List(Box::new(record())), false),
            },
            FieldDescriptor {
                id: None,
                name: "mapping".into(),
                ty: ty(
                    TypeKind::Map {
                        key: Box::new(ty(TypeKind::Scalar(ScalarKind::String), false)),
                        value: Box::new(record()),
                    },
                    false,
                ),
            },
            FieldDescriptor {
                id: None,
                name: "day".into(),
                ty: ty(TypeKind::Scalar(ScalarKind::Date), false),
            },
            FieldDescriptor {
                id: None,
                name: "empty".into(),
                ty: ty(TypeKind::User(r.identity_of::<Empty>()?.clone()), false),
            },
        ];
        let physical_field_order = <Self as fory::StructSerializer>::sorted_field_names()
            .iter()
            .map(|name| fields.iter().position(|f| f.name == *name).unwrap())
            .collect();
        Ok(Some(UserDefinition {
            identity: r.identity_of::<Self>()?.clone(),
            kind: UserDefinitionKind::Struct {
                fields,
                physical_field_order,
            },
        }))
    }
}

#[test]
fn nested_structs_in_fields_lists_maps_and_zero_byte_empty_bodies() {
    for named in [false, true] {
        let mut b = SchemaRegistryBuilder::new();
        b.register_by_id::<Record>(100).unwrap();
        b.register_by_id::<Empty>(101).unwrap();
        if named {
            b.register_by_name::<Nested>("sample", "Nested").unwrap();
        } else {
            b.register_by_id::<Nested>(102).unwrap();
        }
        let r = b.build().unwrap();
        let encoding = RootEncoding {
            protocol_version: ProtocolVersion::Fory1_7,
            mode: SchemaMode::SameSchema {
                version_hash: false,
            },
            root_reference: RootReferenceMode::NullOnly,
        };
        let s = Schema::compile(
            r.describe::<Nested>(&encoding).unwrap(),
            &SchemaLimits::default(),
        )
        .unwrap();
        let mut f = fory::Fory::builder()
            .xlang(true)
            .compatible(false)
            .check_struct_version(false)
            .build();
        r.apply_to_fory(&mut f).unwrap();
        let record = || Record {
            text: Some("nested".into()),
            flag: true,
            count: 123,
        };
        for child in [None, Some(record())] {
            let value = Nested {
                child,
                items: vec![record(), record()],
                mapping: std::collections::BTreeMap::from([("key".into(), record())]),
                day: fory::Date::from_epoch_days(-1),
                empty: Empty {},
            };
            let input = f.serialize(&value).unwrap();
            assert_eq!(f.deserialize::<Nested>(&input).unwrap(), value);
            let view = s
                .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
                .unwrap();
            let root = view.root().unwrap();
            assert_eq!(
                root.field_at(0).unwrap().is_null().unwrap(),
                value.child.is_none()
            );
            assert_eq!(
                root.field_by_name("day")
                    .unwrap()
                    .read_date()
                    .unwrap()
                    .epoch_days(),
                -1
            );
            let list = root.field_at(1).unwrap();
            assert_eq!(
                list.element_at(1)
                    .unwrap()
                    .field_at(2)
                    .unwrap()
                    .read_i32()
                    .unwrap(),
                123
            );
            let retained = list
                .elements()
                .unwrap()
                .next()
                .unwrap()
                .unwrap()
                .field_at(0)
                .unwrap()
                .to_owned()
                .unwrap();
            let map = root.field_at(2).unwrap();
            let (key, value) = map.entries().unwrap().next().unwrap().unwrap();
            assert_eq!(key.read_string().unwrap().as_str(), "key");
            assert!(value.field_by_id(7).unwrap().read_bool().unwrap());
            assert_eq!(root.field_at(4).unwrap().fields().unwrap().count(), 0);
            drop(value);
            drop(key);
            drop(map);
            drop(list);
            drop(root);
            drop(view);
            let validated = s
                .validate(&input, InputFormat::XlangMessage, &ParseLimits::default())
                .unwrap();
            let retained_validated = validated
                .root()
                .unwrap()
                .field_at(1)
                .unwrap()
                .element_at(0)
                .unwrap()
                .field_at(0)
                .unwrap()
                .to_owned()
                .unwrap();
            drop(validated);
            drop(input);
            assert_eq!(
                retained.as_view().unwrap().read_string().unwrap().as_str(),
                "nested"
            );
            assert_eq!(
                retained_validated
                    .as_view()
                    .unwrap()
                    .read_string()
                    .unwrap()
                    .as_str(),
                "nested"
            );
        }
        let s = Schema::compile(
            r.describe::<Empty>(&encoding).unwrap(),
            &SchemaLimits::default(),
        )
        .unwrap();
        let input = f.serialize(&Empty {}).unwrap();
        assert_eq!(
            s.validate(&input, InputFormat::XlangMessage, &ParseLimits::default())
                .unwrap()
                .root()
                .unwrap()
                .fields()
                .unwrap()
                .count(),
            0
        );
        let mut descriptor = r.describe::<Empty>(&encoding).unwrap();
        descriptor.root = ty(TypeKind::List(Box::new(descriptor.root)), false);
        let s = Schema::compile(descriptor, &SchemaLimits::default()).unwrap();
        let input = f.serialize(&vec![Empty {}, Empty {}, Empty {}]).unwrap();
        assert_eq!(
            s.validate(&input, InputFormat::XlangMessage, &ParseLimits::default())
                .unwrap()
                .root()
                .unwrap()
                .elements()
                .unwrap()
                .map(|n| n.unwrap().fields().unwrap().count())
                .collect::<Vec<_>>(),
            vec![0, 0, 0]
        );
    }
}

#[test]
fn malformed_structs_identity_nulls_limits_and_failed_aliases() {
    let mode = SchemaMode::SameSchema {
        version_hash: false,
    };
    for named in [false, true] {
        let (s, f) = setup(mode, named);
        let input = f
            .serialize(&Record {
                text: Some("abc".into()),
                flag: true,
                count: 42,
            })
            .unwrap();
        for format in [InputFormat::XlangMessage, InputFormat::RootValue] {
            let bytes = if matches!(format, InputFormat::RootValue) {
                &input[1..]
            } else {
                &input[..]
            };
            s.validate(
                bytes,
                format,
                &ParseLimits {
                    max_input_bytes: bytes.len(),
                },
            )
            .unwrap();
            assert!(matches!(
                s.view(
                    bytes,
                    format,
                    &ParseLimits {
                        max_input_bytes: bytes.len() - 1
                    }
                ),
                Err(DataError::ResourceLimit(_))
            ));
            for end in 0..bytes.len() {
                assert!(
                    s.validate(&bytes[..end], format, &ParseLimits::default())
                        .is_err()
                );
            }
            let mut trailing = bytes.to_vec();
            trailing.push(0);
            let view = s.view(&trailing, format, &ParseLimits::default()).unwrap();
            let owned = view.root().unwrap().to_owned().unwrap();
            let error = view.validate().err().unwrap();
            assert!(matches!(error, DataError::MalformedInput(_)));
            assert_eq!(
                format!("{error:?}"),
                format!("{:?}", owned.as_view().err().unwrap())
            );
        }
        let mut wrong = input.clone();
        if named {
            // Build another exact Name identity against the same bytes.
            let mut b = SchemaRegistryBuilder::new();
            b.register_by_name::<Record>("other", "Record").unwrap();
            let r = b.build().unwrap();
            let s = Schema::compile(
                r.describe::<Record>(&RootEncoding {
                    protocol_version: ProtocolVersion::Fory1_7,
                    mode,
                    root_reference: RootReferenceMode::NullOnly,
                })
                .unwrap(),
                &SchemaLimits::default(),
            )
            .unwrap();
            assert!(matches!(
                s.view(&input, InputFormat::XlangMessage, &ParseLimits::default()),
                Err(DataError::SchemaMismatch(_))
            ));
        } else {
            wrong[3] = 99;
            assert!(matches!(
                s.view(&wrong, InputFormat::XlangMessage, &ParseLimits::default()),
                Err(DataError::SchemaMismatch(_))
            ));
            wrong = input.clone();
            wrong[2] = 25;
            assert!(matches!(
                s.view(&wrong, InputFormat::XlangMessage, &ParseLimits::default()),
                Err(DataError::SchemaMismatch(_))
            ));
            let mut body = vec![];
            let mut writer = fory::Writer::from_buffer(&mut body);
            writer.write_i8(-1);
            writer.write_var_u32(27);
            writer.write_var_u32(100);
            for name in <Record as fory::StructSerializer>::sorted_field_names() {
                match *name {
                    "count" => writer.write_var_i32(42),
                    "flag" => writer.write_u8(2),
                    "text" => writer.write_i8(-3),
                    _ => unreachable!(),
                }
            }
            let view = s
                .view(&body, InputFormat::RootValue, &ParseLimits::default())
                .unwrap();
            let root = view.root().unwrap();
            let error = root.field_by_name("flag").unwrap().read_bool().unwrap_err();
            assert!(matches!(error, DataError::MalformedInput(_)));
            assert_eq!(
                format!("{error:?}"),
                format!("{:?}", root.field_by_name("missing").err().unwrap())
            );
        }
        let mut b = SchemaRegistryBuilder::new();
        if named {
            b.register_by_name::<Record>("test", "Record").unwrap();
        } else {
            b.register_by_id::<Record>(100).unwrap();
        }
        let r = b.build().unwrap();
        let mut descriptor = r
            .describe::<Record>(&RootEncoding {
                protocol_version: ProtocolVersion::Fory1_7,
                mode,
                root_reference: RootReferenceMode::NullOnly,
            })
            .unwrap();
        descriptor.root.nullable = true;
        let s = Schema::compile(descriptor, &SchemaLimits::default()).unwrap();
        let input = f.serialize(&None::<Record>).unwrap();
        let view = s
            .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
            .unwrap();
        let root = view.root().unwrap();
        assert!(root.is_null().unwrap());
        assert!(matches!(root.field_at(0), Err(DataError::AccessError(_))));
        drop(root);
        assert!(view.validate().unwrap().root().unwrap().is_null().unwrap());
    }
}

#[test]
fn deep_schema_struct_chain_scans_without_recursion_and_rejects_no_progress_cycle() {
    let depth = 2048u32;
    let encoding = RootEncoding {
        protocol_version: ProtocolVersion::Fory1_7,
        mode: SchemaMode::SameSchema {
            version_hash: false,
        },
        root_reference: RootReferenceMode::NullOnly,
    };
    let definitions = (0..depth)
        .map(|i| UserDefinition {
            identity: UserIdentity::Numeric(i),
            kind: UserDefinitionKind::Struct {
                fields: vec![FieldDescriptor {
                    id: None,
                    name: "next".into(),
                    ty: if i + 1 == depth {
                        ty(TypeKind::Scalar(ScalarKind::Bool), false)
                    } else {
                        ty(TypeKind::User(UserIdentity::Numeric(i + 1)), false)
                    },
                }],
                physical_field_order: vec![0],
            },
        })
        .collect();
    let limits = SchemaLimits {
        max_definitions: depth as usize,
        max_type_nodes: depth as usize + 1,
        max_members: depth as usize,
        ..SchemaLimits::default()
    };
    let s = Schema::compile(
        SchemaDescriptor {
            encoding: encoding.clone(),
            key_semantics_version: KEY_SEMANTICS_VERSION,
            root: ty(TypeKind::User(UserIdentity::Numeric(0)), false),
            definitions,
        },
        &limits,
    )
    .unwrap();
    let input = [255, 27, 0, 1];
    let validated = s
        .validate(&input, InputFormat::RootValue, &ParseLimits::default())
        .unwrap();
    let field = validated
        .root()
        .unwrap()
        .field_at(0)
        .unwrap()
        .to_owned()
        .unwrap();
    assert!(matches!(
        field.as_view().unwrap().node_type().kind,
        TypeKind::User(UserIdentity::Numeric(1))
    ));
    let root = ty(TypeKind::User(UserIdentity::Numeric(1)), false);
    let s = Schema::compile(
        SchemaDescriptor {
            encoding,
            key_semantics_version: KEY_SEMANTICS_VERSION,
            root: root.clone(),
            definitions: vec![UserDefinition {
                identity: UserIdentity::Numeric(1),
                kind: UserDefinitionKind::Struct {
                    fields: vec![FieldDescriptor {
                        id: None,
                        name: "again".into(),
                        ty: root,
                    }],
                    physical_field_order: vec![0],
                },
            }],
        },
        &SchemaLimits::default(),
    )
    .unwrap();
    assert!(matches!(
        s.validate(
            &[255, 27, 1],
            InputFormat::RootValue,
            &ParseLimits::default()
        ),
        Err(DataError::MalformedInput(_))
    ));
}
