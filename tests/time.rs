use pi_fory_schema::*;

#[test]
fn static_temporal_arrays_use_list_bodies() {
    macro_rules! check {
        ($t:ty, $kind:expr, $value:expr, $read:ident) => {{
            #[derive(Debug, PartialEq, fory::ForyStruct)]
            struct TemporalArray {
                values: [$t; 2],
            }
            for compatible in [false, true] {
                let value = $value;
                let ty = TypeDescriptor {
                    kind: TypeKind::Array {
                        element: Box::new(TypeDescriptor {
                            kind: TypeKind::Scalar($kind),
                            nullable: false,
                            track_ref: false,
                            integer_encoding: None,
                        }),
                        length: Some(2),
                    },
                    nullable: false,
                    track_ref: false,
                    integer_encoding: None,
                };
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
                        root: TypeDescriptor {
                            kind: TypeKind::User(UserIdentity::Numeric(731)),
                            nullable: false,
                            track_ref: false,
                            integer_encoding: None,
                        },
                        definitions: vec![UserDefinition {
                            identity: UserIdentity::Numeric(731),
                            kind: UserDefinitionKind::Struct {
                                fields: vec![FieldDescriptor {
                                    id: None,
                                    name: "values".into(),
                                    ty,
                                }],
                                physical_field_order: vec![0],
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
                f.register::<TemporalArray>(731).unwrap();
                let expected = TemporalArray {
                    values: [value, value],
                };
                let input = f.serialize(&expected).unwrap();
                assert_eq!(f.deserialize::<TemporalArray>(&input).unwrap(), expected);
                let view = schema
                    .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
                    .unwrap();
                assert_eq!(
                    view.root()
                        .unwrap()
                        .field_at(0)
                        .unwrap()
                        .element_at(1)
                        .unwrap()
                        .$read()
                        .unwrap(),
                    value
                );
                let validated = view.validate().unwrap();
                let root = validated.root().unwrap();
                let values = root.field_at(0).unwrap();
                assert_eq!(
                    values
                        .elements()
                        .unwrap()
                        .map(|item| item.unwrap().$read().unwrap())
                        .collect::<Vec<_>>(),
                    expected.values
                );
                let owned = values.element_at(1).unwrap().to_owned().unwrap();
                drop(values);
                drop(root);
                drop(validated);
                drop(input);
                drop(schema);
                assert_eq!(owned.as_view().unwrap().$read().unwrap(), value);
            }
        }};
    }
    check!(
        fory::Date,
        ScalarKind::Date,
        fory::Date::from_epoch_days(-719_528),
        read_date
    );
    check!(
        fory::Timestamp,
        ScalarKind::Timestamp,
        fory::Timestamp::new(-1, 999_999_999).unwrap(),
        read_timestamp
    );
    check!(
        fory::Duration,
        ScalarKind::Duration,
        fory::Duration::new(-2, -1).unwrap(),
        read_duration
    );
}

#[test]
fn tracked_temporal_carriers_preserve_values_markers_and_ownership() {
    for compatible in [false, true] {
        for named in [false, true] {
            macro_rules! check {
                ($t:ty, $kind:expr, $value:expr, $read:ident) => {{
                    let value: $t = $value;
                    let f = fory::Fory::builder()
                        .xlang(true)
                        .compatible(compatible)
                        .check_struct_version(false)
                        .build();
                    let shared = std::rc::Rc::new(value);
                    let input = f
                        .serialize(&vec![std::rc::Rc::clone(&shared), shared])
                        .unwrap();
                    let decoded = f.deserialize::<Vec<std::rc::Rc<$t>>>(&input).unwrap();
                    assert!(std::rc::Rc::ptr_eq(&decoded[0], &decoded[1]));
                    let child = TypeDescriptor {
                        kind: TypeKind::Scalar($kind),
                        nullable: false,
                        track_ref: true,
                        integer_encoding: None,
                    };
                    let mut d = SchemaDescriptor {
                        encoding: RootEncoding {
                            protocol_version: ProtocolVersion::Fory1_7,
                            mode: if compatible {
                                SchemaMode::Compatible
                            } else {
                                SchemaMode::SameSchema {
                                    version_hash: false,
                                }
                            },
                            root_reference: RootReferenceMode::Tracking,
                        },
                        key_semantics_version: KEY_SEMANTICS_VERSION,
                        root: TypeDescriptor {
                            kind: TypeKind::List(Box::new(child.clone())),
                            nullable: false,
                            track_ref: false,
                            integer_encoding: None,
                        },
                        definitions: vec![],
                    };
                    let schema = Schema::compile(d.clone(), &SchemaLimits::default()).unwrap();
                    let view = schema
                        .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
                        .unwrap();
                    let root = view.root().unwrap();
                    let edge = root.element_at(1).unwrap();
                    let id = edge.reference_id().unwrap().unwrap();
                    assert!(matches!(edge.$read(), Err(DataError::AccessError(_))));
                    assert_eq!(
                        root.element_at(0).unwrap().object_id().unwrap().unwrap(),
                        id
                    );
                    assert_eq!(edge.resolve_reference(&id).unwrap().$read().unwrap(), value);
                    let owned = edge.to_owned().unwrap();
                    drop(edge);
                    drop(root);
                    let validated = view.validate().unwrap();
                    let root = validated.root().unwrap();
                    let edge = root.element_at(1).unwrap();
                    assert!(matches!(edge.$read(), Err(DataError::AccessError(_))));
                    assert_eq!(edge.resolve_reference(&id).unwrap().$read().unwrap(), value);
                    drop(edge);
                    drop(root);
                    drop(validated);
                    drop(input);
                    assert_eq!(
                        owned
                            .as_view()
                            .unwrap()
                            .resolve_reference(&id)
                            .unwrap()
                            .$read()
                            .unwrap(),
                        value
                    );
                    #[derive(Debug, PartialEq, fory::ForyStruct)]
                    struct TemporalFields {
                        first: std::sync::Arc<$t>,
                        second: std::sync::Arc<$t>,
                    }
                    let mut f = fory::Fory::builder()
                        .xlang(true)
                        .compatible(compatible)
                        .check_struct_version(false)
                        .build();
                    let identity = if named {
                        f.register_by_name::<TemporalFields>("time.Shared").unwrap();
                        UserIdentity::Name {
                            namespace: "time".into(),
                            type_name: "Shared".into(),
                        }
                    } else {
                        f.register::<TemporalFields>(510).unwrap();
                        UserIdentity::Numeric(510)
                    };
                    let shared = std::sync::Arc::new(value);
                    let input = f
                        .serialize(&TemporalFields {
                            first: std::sync::Arc::clone(&shared),
                            second: shared,
                        })
                        .unwrap();
                    let decoded = f.deserialize::<TemporalFields>(&input).unwrap();
                    assert!(std::sync::Arc::ptr_eq(&decoded.first, &decoded.second));
                    d.root.kind = TypeKind::User(identity.clone());
                    d.definitions = vec![UserDefinition {
                        identity,
                        kind: UserDefinitionKind::Struct {
                            fields: vec![
                                FieldDescriptor {
                                    id: None,
                                    name: "first".into(),
                                    ty: child.clone(),
                                },
                                FieldDescriptor {
                                    id: None,
                                    name: "second".into(),
                                    ty: child.clone(),
                                },
                            ],
                            physical_field_order: vec![0, 1],
                        },
                    }];
                    let schema = Schema::compile(d.clone(), &SchemaLimits::default()).unwrap();
                    let view = schema
                        .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
                        .unwrap();
                    let root = view.root().unwrap();
                    let edge = root.field_at(1).unwrap();
                    let id = edge.reference_id().unwrap().unwrap();
                    assert!(matches!(edge.$read(), Err(DataError::AccessError(_))));
                    assert_eq!(edge.resolve_reference(&id).unwrap().$read().unwrap(), value);
                    drop(edge);
                    drop(root);
                    let validated = view.validate().unwrap();
                    assert_eq!(
                        validated
                            .root()
                            .unwrap()
                            .resolve_reference(&id)
                            .unwrap()
                            .$read()
                            .unwrap(),
                        value
                    );
                    d.root = child;
                    d.definitions.clear();
                    let schema = Schema::compile(d, &SchemaLimits::default()).unwrap();
                    let mut input = f.serialize(&std::rc::Rc::new(value)).unwrap();
                    assert_eq!(input[1], 255);
                    let view = schema
                        .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
                        .unwrap();
                    assert!(view.root().unwrap().object_id().unwrap().is_none());
                    input[1] = 0;
                    let validated = schema
                        .validate(&input, InputFormat::XlangMessage, &ParseLimits::default())
                        .unwrap();
                    assert!(validated.root().unwrap().object_id().unwrap().is_some());
                    assert_eq!(validated.root().unwrap().$read().unwrap(), value);
                }};
            }
            check!(
                fory::Date,
                ScalarKind::Date,
                fory::Date::from_epoch_days(-719_528),
                read_date
            );
            check!(
                fory::Timestamp,
                ScalarKind::Timestamp,
                fory::Timestamp::new(-1, 999_999_999).unwrap(),
                read_timestamp
            );
            check!(
                fory::Duration,
                ScalarKind::Duration,
                fory::Duration::new(-2, -1).unwrap(),
                read_duration
            );
        }
    }
}

fn wire(format: InputFormat, id: fory::TypeId, seconds: i64, nanos: i32) -> Vec<u8> {
    let mut input = vec![];
    let mut writer = fory::Writer::from_buffer(&mut input);
    if matches!(format, InputFormat::XlangMessage) {
        producer(SchemaMode::Compatible, false).write_head::<fory::Date>(&mut writer);
    }
    writer.write_i8(fory::RefFlag::NotNullValue as i8);
    writer.write_var_u32(id as u32);
    if matches!(id, fory::TypeId::TIMESTAMP) {
        writer.write_i64(seconds);
    } else {
        writer.write_var_i64(seconds);
    }
    if !matches!(id, fory::TypeId::DATE) {
        writer.write_i32(nanos);
    }
    input
}

#[test]
fn writer_temporal_roots_null_and_failures() {
    for mode in modes() {
        for tracking in [false, true] {
            for format in [InputFormat::RootValue, InputFormat::XlangMessage] {
                for (kind, id) in [
                    (ScalarKind::Date, fory::TypeId::DATE),
                    (ScalarKind::Timestamp, fory::TypeId::TIMESTAMP),
                    (ScalarKind::Duration, fory::TypeId::DURATION),
                ] {
                    let schema = schema(kind, mode, tracking, false);
                    for seconds in [i64::MIN, -1, 0, 1, i64::MAX] {
                        let input = wire(format, id, seconds, 999_999_999);
                        let view = schema
                            .view(&input, format, &ParseLimits::default())
                            .unwrap();
                        let node = view.root().unwrap();
                        assert!(matches!(node.read_bool(), Err(DataError::AccessError(_))));
                        match kind {
                            ScalarKind::Date => assert_eq!(
                                node.read_date().unwrap(),
                                fory::Date::from_epoch_days(seconds)
                            ),
                            ScalarKind::Timestamp => assert_eq!(
                                node.read_timestamp().unwrap(),
                                fory::Timestamp::new(seconds, 999_999_999).unwrap()
                            ),
                            _ => assert_eq!(
                                node.read_duration().unwrap(),
                                fory::Duration::new(seconds, 999_999_999).unwrap()
                            ),
                        }
                        let owned = view.validate().unwrap().root().unwrap().to_owned().unwrap();
                        drop(input);
                        let node = owned.as_view().unwrap();
                        match kind {
                            ScalarKind::Date => {
                                assert_eq!(node.read_date().unwrap().epoch_days(), seconds)
                            }
                            ScalarKind::Timestamp => {
                                assert_eq!(node.read_timestamp().unwrap().seconds(), seconds)
                            }
                            _ => assert_eq!(node.read_duration().unwrap().seconds(), seconds),
                        }
                    }
                    let input = wire(format, id, 1, 0);
                    let body = if matches!(format, InputFormat::XlangMessage) {
                        3
                    } else {
                        2
                    };
                    for end in body..input.len() {
                        assert!(matches!(
                            schema.validate(&input[..end], format, &ParseLimits::default()),
                            Err(DataError::MalformedInput(_))
                        ));
                    }
                    let mut trailing = input.clone();
                    trailing.push(0);
                    let owner = schema
                        .view_owned(
                            bytes::Bytes::from(trailing),
                            format,
                            &ParseLimits::default(),
                        )
                        .unwrap();
                    let survivor = owner.as_view().unwrap().root().unwrap().to_owned().unwrap();
                    let error = owner.validate().err().unwrap();
                    assert!(
                        matches!(&error,DataError::MalformedInput(location) if location.byte_offset == Some(input.len()))
                    );
                    assert_eq!(
                        format!("{error:?}"),
                        format!("{:?}", survivor.as_view().err().unwrap())
                    );
                    let mut wrong = input.clone();
                    wrong[body - 1] = 1;
                    assert!(matches!(
                        schema.view(&wrong, format, &ParseLimits::default()),
                        Err(DataError::SchemaMismatch(_))
                    ));
                    let mut null = input[..body - 1].to_vec();
                    null[body - 2] = 253;
                    assert!(matches!(
                        schema.validate(&null, format, &ParseLimits::default()),
                        Err(DataError::SchemaMismatch(_))
                    ));
                    let nullable = self::schema(kind, mode, tracking, true);
                    let view = nullable
                        .view(&null, format, &ParseLimits::default())
                        .unwrap();
                    let node = view.root().unwrap();
                    match kind {
                        ScalarKind::Date => {
                            assert!(matches!(node.read_date(), Err(DataError::AccessError(_))))
                        }
                        ScalarKind::Timestamp => assert!(matches!(
                            node.read_timestamp(),
                            Err(DataError::AccessError(_))
                        )),
                        _ => assert!(matches!(
                            node.read_duration(),
                            Err(DataError::AccessError(_))
                        )),
                    }
                    assert!(node.is_null().unwrap());
                    drop(node);
                    let validated = view.validate().unwrap();
                    let node = validated.root().unwrap();
                    assert!(node.is_null().unwrap());
                    match kind {
                        ScalarKind::Date => {
                            assert!(matches!(node.read_date(), Err(DataError::AccessError(_))))
                        }
                        ScalarKind::Timestamp => assert!(matches!(
                            node.read_timestamp(),
                            Err(DataError::AccessError(_))
                        )),
                        _ => assert!(matches!(
                            node.read_duration(),
                            Err(DataError::AccessError(_))
                        )),
                    }
                }
            }
        }
    }
}

#[test]
fn native_nanos_rules_and_first_failure() {
    for format in [InputFormat::RootValue, InputFormat::XlangMessage] {
        for (kind, id, seconds, nanos) in [
            (
                ScalarKind::Timestamp,
                fory::TypeId::TIMESTAMP,
                0,
                1_000_000_000,
            ),
            (ScalarKind::Timestamp, fory::TypeId::TIMESTAMP, 0, -1),
            (
                ScalarKind::Duration,
                fory::TypeId::DURATION,
                0,
                1_000_000_000,
            ),
            (
                ScalarKind::Duration,
                fory::TypeId::DURATION,
                0,
                -1_000_000_000,
            ),
            (ScalarKind::Duration, fory::TypeId::DURATION, i64::MIN, -1),
        ] {
            let input = wire(format, id, seconds, nanos);
            let schema = schema(kind, SchemaMode::Compatible, false, false);
            let view = schema
                .view(&input, format, &ParseLimits::default())
                .unwrap();
            let node = view.root().unwrap();
            let error = if matches!(kind, ScalarKind::Timestamp) {
                node.read_timestamp().err().unwrap()
            } else {
                node.read_duration().err().unwrap()
            };
            assert!(matches!(&error, DataError::MalformedInput(_)));
            assert_eq!(
                format!("{error:?}"),
                format!("{:?}", node.read_bool().err().unwrap())
            );
            drop(node);
            assert_eq!(
                format!("{error:?}"),
                format!("{:?}", view.validate().err().unwrap())
            );
            let independent = schema
                .validate(&input, format, &ParseLimits::default())
                .err()
                .unwrap();
            assert_eq!(format!("{error:?}"), format!("{independent:?}"));
            if matches!(format, InputFormat::XlangMessage) {
                let fory = producer(SchemaMode::Compatible, false);
                if matches!(kind, ScalarKind::Timestamp) {
                    assert!(fory.deserialize::<fory::Timestamp>(&input).is_err());
                } else {
                    assert!(fory.deserialize::<fory::Duration>(&input).is_err());
                }
            }
        }
        for nanos in [-999_999_999, -1, 0, 999_999_999] {
            let input = wire(format, fory::TypeId::DURATION, 0, nanos);
            let expected = fory::Duration::new(0, nanos).unwrap();
            let schema = schema(ScalarKind::Duration, SchemaMode::Compatible, false, false);
            assert_eq!(
                schema
                    .validate(&input, format, &ParseLimits::default())
                    .unwrap()
                    .root()
                    .unwrap()
                    .read_duration()
                    .unwrap(),
                expected
            );
            if matches!(format, InputFormat::XlangMessage) {
                assert_eq!(
                    producer(SchemaMode::Compatible, false)
                        .deserialize::<fory::Duration>(&input)
                        .unwrap(),
                    expected
                );
            }
        }
    }
}

fn modes() -> [SchemaMode; 3] {
    [
        SchemaMode::Compatible,
        SchemaMode::SameSchema {
            version_hash: false,
        },
        SchemaMode::SameSchema { version_hash: true },
    ]
}
fn producer(mode: SchemaMode, tracking: bool) -> fory::Fory {
    fory::Fory::builder()
        .xlang(true)
        .compatible(matches!(mode, SchemaMode::Compatible))
        .check_struct_version(matches!(
            mode,
            SchemaMode::SameSchema { version_hash: true }
        ))
        .track_ref(tracking)
        .build()
}
fn schema(
    kind: ScalarKind,
    mode: SchemaMode,
    tracking: bool,
    nullable: bool,
) -> std::sync::Arc<Schema> {
    std::sync::Arc::new(
        Schema::compile(
            SchemaDescriptor {
                encoding: RootEncoding {
                    protocol_version: ProtocolVersion::Fory1_7,
                    mode,
                    root_reference: if tracking {
                        RootReferenceMode::Tracking
                    } else {
                        RootReferenceMode::NullOnly
                    },
                },
                key_semantics_version: KEY_SEMANTICS_VERSION,
                root: TypeDescriptor {
                    nullable,
                    track_ref: false,
                    integer_encoding: None,
                    kind: TypeKind::Scalar(kind),
                },
                definitions: vec![],
            },
            &SchemaLimits::default(),
        )
        .unwrap(),
    )
}

#[test]
fn actual_fory_temporal_carriers() {
    for mode in modes() {
        for tracking in [false, true] {
            let fory = producer(mode, tracking);
            macro_rules! check {
                ($kind:ident, $ty:ty, $read:ident, $values:expr) => {
                    let schema = schema(ScalarKind::$kind, mode, tracking, false);
                    let null = fory.serialize(&None::<$ty>).unwrap();
                    let nullable = self::schema(ScalarKind::$kind, mode, tracking, true);
                    assert!(
                        nullable
                            .validate(&null, InputFormat::XlangMessage, &ParseLimits::default())
                            .unwrap()
                            .root()
                            .unwrap()
                            .is_null()
                            .unwrap()
                    );
                    assert!(matches!(
                        schema.validate(&null, InputFormat::XlangMessage, &ParseLimits::default()),
                        Err(DataError::SchemaMismatch(_))
                    ));
                    for requirement in [
                        KeyRequirement::Hash,
                        KeyRequirement::Ordered,
                        KeyRequirement::HashAndOrdered,
                    ] {
                        assert!(matches!(
                            schema.check_key_support(requirement),
                            Err(SchemaError::UnsupportedKeyCapability(_))
                        ));
                    }
                    for value in $values {
                        let input = fory.serialize(&value).unwrap();
                        assert_eq!(fory.deserialize::<$ty>(&input).unwrap(), value);
                        let view = schema
                            .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
                            .unwrap();
                        let node = view.root().unwrap();
                        assert!(matches!(node.read_bool(), Err(DataError::AccessError(_))));
                        assert_eq!(node.$read().unwrap(), value);
                        let owned = node.to_owned().unwrap();
                        assert_eq!(owned.as_view().unwrap().$read().unwrap(), value);
                        assert_eq!(
                            view.validate().unwrap().root().unwrap().$read().unwrap(),
                            value
                        );
                        let validated = schema
                            .validate(&input, InputFormat::XlangMessage, &ParseLimits::default())
                            .unwrap();
                        let owned = validated.root().unwrap().to_owned().unwrap();
                        assert_eq!(owned.as_view().unwrap().$read().unwrap(), value);
                        let owned = schema
                            .validate_owned(
                                bytes::Bytes::from(input.clone()),
                                InputFormat::XlangMessage,
                                &ParseLimits {
                                    max_input_bytes: input.len(),
                                },
                            )
                            .unwrap();
                        assert_eq!(
                            owned.as_view().unwrap().root().unwrap().$read().unwrap(),
                            value
                        );
                        for end in 3..input.len() {
                            assert!(matches!(
                                schema.validate(
                                    &input[..end],
                                    InputFormat::XlangMessage,
                                    &ParseLimits::default()
                                ),
                                Err(DataError::MalformedInput(_))
                            ));
                        }
                        assert!(matches!(
                            schema.validate(
                                &input,
                                InputFormat::XlangMessage,
                                &ParseLimits {
                                    max_input_bytes: input.len() - 1
                                }
                            ),
                            Err(DataError::ResourceLimit(_))
                        ));
                    }
                };
            }
            check!(
                Date,
                fory::Date,
                read_date,
                [i64::MIN, -1, 0, 1, i64::MAX].map(fory::Date::from_epoch_days)
            );
            check!(
                Timestamp,
                fory::Timestamp,
                read_timestamp,
                [
                    (i64::MIN, 0),
                    (-1, 999_999_999),
                    (0, 0),
                    (1, 1),
                    (i64::MAX, 999_999_999)
                ]
                .map(|(seconds, nanos)| fory::Timestamp::new(seconds, nanos).unwrap())
            );
            check!(
                Duration,
                fory::Duration,
                read_duration,
                [
                    (i64::MIN, 0),
                    (-1, 999_999_999),
                    (0, -1),
                    (0, 0),
                    (1, -999_999_999),
                    (i64::MAX, 999_999_999)
                ]
                .map(|(seconds, nanos)| fory::Duration::new(seconds, nanos).unwrap())
            );
        }
    }
}
