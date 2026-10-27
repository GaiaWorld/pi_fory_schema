use pi_fory_schema::*;

#[test]
fn validation_failures_null_limits_and_alias() {
    for mode in modes() {
        for format in [InputFormat::RootValue, InputFormat::XlangMessage] {
            for tracking in [false, true] {
                let descriptor = SchemaDescriptor {
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
                        nullable: true,
                        track_ref: false,
                        integer_encoding: None,
                        kind: TypeKind::Scalar(ScalarKind::Bool),
                    },
                    definitions: vec![],
                };
                let schema = std::sync::Arc::new(
                    Schema::compile(descriptor, &SchemaLimits::default()).unwrap(),
                );
                let mut null = vec![];
                let mut writer = fory::Writer::from_buffer(&mut null);
                if matches!(format, InputFormat::XlangMessage) {
                    producer(mode, tracking).write_head::<bool>(&mut writer);
                }
                writer.write_i8(fory::RefFlag::Null as i8);
                let validated = schema
                    .validate(
                        &null,
                        format,
                        &ParseLimits {
                            max_input_bytes: null.len(),
                        },
                    )
                    .unwrap();
                let node = validated.root().unwrap();
                assert!(node.is_null().unwrap());
                assert!(matches!(node.read_bool(), Err(DataError::AccessError(_))));
                assert!(
                    node.to_owned()
                        .unwrap()
                        .as_view()
                        .unwrap()
                        .is_null()
                        .unwrap()
                );
                assert!(matches!(
                    schema.validate(
                        &null,
                        format,
                        &ParseLimits {
                            max_input_bytes: null.len() - 1
                        }
                    ),
                    Err(DataError::ResourceLimit(_))
                ));
                let owned = schema
                    .validate_bytes(
                        bytes::Bytes::from(null.clone()),
                        format,
                        &ParseLimits::default(),
                    )
                    .unwrap();
                assert!(owned.as_view().unwrap().root().unwrap().is_null().unwrap());
                let end = null.len();
                null.push(0);
                let view = schema.view(&null, format, &ParseLimits::default()).unwrap();
                let survivor = view.root().unwrap().to_owned().unwrap();
                let error = view.validate().err().unwrap();
                assert!(
                    matches!(&error, DataError::MalformedInput(location) if location.byte_offset == Some(end))
                );
                assert_eq!(
                    format!("{error:?}"),
                    format!("{:?}", survivor.as_view().err().unwrap())
                );
                let mut input = vec![];
                let mut writer = fory::Writer::from_buffer(&mut input);
                if matches!(format, InputFormat::XlangMessage) {
                    producer(mode, tracking).write_head::<bool>(&mut writer);
                }
                writer.write_i8(-1);
                writer.write_var_u32(fory::TypeId::BOOL as u32);
                writer.write_bool(true);
                let validated = schema
                    .validate(&input, format, &ParseLimits::default())
                    .unwrap();
                let node = validated.root().unwrap();
                assert!(matches!(node.read_i32(), Err(DataError::AccessError(_))));
                assert!(node.read_bool().unwrap());
                let key = validated.into_key(KeyRequirement::Hash).unwrap();
                assert!(key.try_eq(&key).unwrap());
                let body = input.len() - 1;
                input[body] = 2;
                let error = schema
                    .validate(&input, format, &ParseLimits::default())
                    .err()
                    .unwrap();
                assert!(
                    matches!(error, DataError::MalformedInput(location) if location.byte_offset == Some(body))
                );
                input[body] = 1;
                assert!(matches!(
                    schema.validate(&input[..body], format, &ParseLimits::default()),
                    Err(DataError::MalformedInput(_))
                ));
                input[body - 1] = fory::TypeId::INT8 as u8;
                let error = schema
                    .validate(&input, format, &ParseLimits::default())
                    .err()
                    .unwrap();
                assert!(
                    matches!(error, DataError::SchemaMismatch(location) if location.byte_offset == Some(body-1))
                );
            }
        }
    }
    let mode = SchemaMode::Compatible;
    let schema = schema(ScalarKind::U32, Some(IntegerEncoding::Varint), mode);
    assert!(matches!(
        schema.validate(
            &[1, 255, 12, 255, 255, 255, 255, 16],
            InputFormat::XlangMessage,
            &ParseLimits::default()
        ),
        Err(DataError::MalformedInput(_))
    ));
    for kind in [
        ScalarKind::Date,
        ScalarKind::Timestamp,
        ScalarKind::Duration,
    ] {
        assert!(matches!(
            self::schema(kind, None, mode).validate(
                &[1, 255, 21],
                InputFormat::XlangMessage,
                &ParseLimits::default()
            ),
            Err(DataError::SchemaMismatch(_))
        ));
    }
}

#[test]
fn scalar_validation_public_chain() {
    for mode in modes() {
        let input = producer(mode, false).serialize(&true).unwrap();
        let schema = std::sync::Arc::new(schema(ScalarKind::Bool, None, mode));
        let validated = schema
            .validate(&input, InputFormat::XlangMessage, &ParseLimits::default())
            .unwrap();
        assert!(validated.root().unwrap().read_bool().unwrap());
        let owned_node = validated.root().unwrap().to_owned().unwrap();
        assert!(owned_node.as_view().unwrap().read_bool().unwrap());
        let ordinary = schema
            .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
            .unwrap();
        let validated = ordinary.validate().unwrap();
        assert!(validated.root().unwrap().read_bool().unwrap());
        let owned = schema
            .validate_owned(
                bytes::Bytes::from(input.clone()),
                InputFormat::XlangMessage,
                &ParseLimits::default(),
            )
            .unwrap();
        assert!(
            owned
                .as_view()
                .unwrap()
                .root()
                .unwrap()
                .read_bool()
                .unwrap()
        );
        let owned = schema
            .view_owned(
                bytes::Bytes::from(input),
                InputFormat::XlangMessage,
                &ParseLimits::default(),
            )
            .unwrap()
            .validate()
            .unwrap();
        let node = owned.as_view().unwrap().root().unwrap().to_owned().unwrap();
        drop(owned);
        assert!(node.as_view().unwrap().read_bool().unwrap());
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

#[test]
fn root_value_small_scalars_use_verified_writer_envelope() {
    for mode in modes() {
        macro_rules! check {
            ($kind:ident, $id:ident, $write:ident, $read:ident, $value:expr) => {
                let mut input = vec![];
                let mut writer = fory::Writer::from_buffer(&mut input);
                writer.write_i8(fory::RefFlag::NotNullValue as i8);
                writer.write_var_u32(fory::TypeId::$id as u32);
                writer.$write($value);
                let schema = schema(ScalarKind::$kind, None, mode);
                let view = schema
                    .view(&input, InputFormat::RootValue, &ParseLimits::default())
                    .unwrap();
                assert_eq!(view.root().unwrap().$read().unwrap(), $value);
                let validated = schema
                    .validate(&input, InputFormat::RootValue, &ParseLimits::default())
                    .unwrap();
                assert_eq!(validated.root().unwrap().$read().unwrap(), $value);
                assert_eq!(
                    validated
                        .root()
                        .unwrap()
                        .to_owned()
                        .unwrap()
                        .as_view()
                        .unwrap()
                        .$read()
                        .unwrap(),
                    $value
                );
                let view = schema
                    .view(
                        &input[..input.len() - 1],
                        InputFormat::RootValue,
                        &ParseLimits::default(),
                    )
                    .unwrap();
                assert!(matches!(
                    view.root().unwrap().$read(),
                    Err(DataError::MalformedInput(_))
                ));
            };
        }
        check!(Bool, BOOL, write_bool, read_bool, false);
        check!(I8, INT8, write_i8, read_i8, -128i8);
        check!(I16, INT16, write_i16, read_i16, -32768i16);
        check!(U8, UINT8, write_u8, read_u8, 255u8);
        check!(U16, UINT16, write_u16, read_u16, 65535u16);
        check!(F32, FLOAT32, write_f32, read_f32, -1.25f32);
        check!(F64, FLOAT64, write_f64, read_f64, -1.25f64);
    }
}

#[test]
fn overflow_headers_unimplemented_roots_and_owned_failures() {
    let mode = SchemaMode::SameSchema { version_hash: true };
    let schema32 = schema(ScalarKind::U32, Some(IntegerEncoding::Varint), mode);
    let malformed = [1u8, 255, 12, 255, 255, 255, 255, 16];
    let view = schema32
        .view(
            &malformed,
            InputFormat::XlangMessage,
            &ParseLimits::default(),
        )
        .unwrap();
    assert!(matches!(
        view.root().unwrap().read_u32(),
        Err(DataError::MalformedInput(_))
    ));
    let bad_header = [1u8, 255, 255, 255, 255, 255, 16];
    assert!(matches!(
        schema32.view(
            &bad_header,
            InputFormat::XlangMessage,
            &ParseLimits::default()
        ),
        Err(DataError::MalformedInput(_))
    ));
    let bool_schema = std::sync::Arc::new(schema(ScalarKind::Bool, None, mode));
    let input = bytes::Bytes::from(producer(mode, false).serialize(&false).unwrap());
    assert!(matches!(
        bool_schema.view_owned(
            input.clone(),
            InputFormat::XlangMessage,
            &ParseLimits {
                max_input_bytes: input.len() - 1
            }
        ),
        Err(DataError::ResourceLimit(_))
    ));
    let owned = bool_schema
        .view_owned(
            input.clone(),
            InputFormat::XlangMessage,
            &ParseLimits {
                max_input_bytes: input.len(),
            },
        )
        .unwrap();
    let promoted = owned.as_view().unwrap().root().unwrap().to_owned().unwrap();
    drop(owned);
    drop(input);
    drop(bool_schema);
    assert!(!promoted.as_view().unwrap().read_bool().unwrap());
    let bool_schema = std::sync::Arc::new(schema(ScalarKind::Bool, None, mode));
    let owned = bool_schema
        .view_owned(
            bytes::Bytes::from_static(&[1, 255, 1, 3]),
            InputFormat::XlangMessage,
            &ParseLimits::default(),
        )
        .unwrap();
    let view = owned.as_view().unwrap();
    let node = view.root().unwrap();
    let first = format!("{:?}", node.read_bool().unwrap_err());
    assert_eq!(first, format!("{:?}", node.read_string().err().unwrap()));
    assert_eq!(first, format!("{:?}", node.to_owned().err().unwrap()));
    drop(node);
    drop(view);
    assert_eq!(first, format!("{:?}", owned.as_view().err().unwrap()));
    assert_eq!(first, format!("{:?}", owned.validate().err().unwrap()));
    for kind in [
        ScalarKind::Date,
        ScalarKind::Timestamp,
        ScalarKind::Duration,
    ] {
        let schema = schema(kind, None, mode);
        assert!(matches!(
            schema.view(
                &[1, 255, 21],
                InputFormat::XlangMessage,
                &ParseLimits::default()
            ),
            Err(DataError::SchemaMismatch(_))
        ));
    }
}

#[test]
fn explicit_integer_writer_roots() {
    // Writer emits the verified complete scalar envelope, never an arbitrary stripped body.
    for mode in modes() {
        for format in [InputFormat::RootValue, InputFormat::XlangMessage] {
            for encoding in [
                IntegerEncoding::Fixed,
                IntegerEncoding::Varint,
                IntegerEncoding::Tagged,
            ] {
                for signed in [false, true] {
                    for value in [0u64, 1, 0x3fffffff, 0x40000000, u64::MAX] {
                        let mut input = vec![];
                        let mut writer = fory::Writer::from_buffer(&mut input);
                        if matches!(format, InputFormat::XlangMessage) {
                            producer(mode, false).write_head::<u64>(&mut writer);
                        }
                        writer.write_i8(fory::RefFlag::NotNullValue as i8);
                        let id = match (signed, encoding) {
                            (true, IntegerEncoding::Fixed) => fory::TypeId::INT64,
                            (true, IntegerEncoding::Varint) => fory::TypeId::VARINT64,
                            (true, IntegerEncoding::Tagged) => fory::TypeId::TAGGED_INT64,
                            (false, IntegerEncoding::Fixed) => fory::TypeId::UINT64,
                            (false, IntegerEncoding::Varint) => fory::TypeId::VAR_UINT64,
                            (false, IntegerEncoding::Tagged) => fory::TypeId::TAGGED_UINT64,
                        };
                        writer.write_var_u32(id as u32);
                        match (signed, encoding) {
                            (true, IntegerEncoding::Fixed) => writer.write_i64(value as i64),
                            (true, IntegerEncoding::Varint) => writer.write_var_i64(value as i64),
                            (true, IntegerEncoding::Tagged) => {
                                writer.write_tagged_i64(value as i64)
                            }
                            (false, IntegerEncoding::Fixed) => writer.write_u64(value),
                            (false, IntegerEncoding::Varint) => writer.write_var_u64(value),
                            (false, IntegerEncoding::Tagged) => writer.write_tagged_u64(value),
                        }
                        let schema = schema(
                            if signed {
                                ScalarKind::I64
                            } else {
                                ScalarKind::U64
                            },
                            Some(encoding),
                            mode,
                        );
                        let view = schema
                            .view(&input, format, &ParseLimits::default())
                            .unwrap();
                        let node = view.root().unwrap();
                        if signed {
                            assert_eq!(node.read_i64().unwrap(), value as i64);
                        } else {
                            assert_eq!(node.read_u64().unwrap(), value);
                        }
                        let validated = schema
                            .validate(&input, format, &ParseLimits::default())
                            .unwrap();
                        if signed {
                            assert_eq!(validated.root().unwrap().read_i64().unwrap(), value as i64);
                        } else {
                            assert_eq!(validated.root().unwrap().read_u64().unwrap(), value);
                        }
                        assert!(matches!(
                            schema.validate(
                                &input[..input.len() - 1],
                                format,
                                &ParseLimits::default()
                            ),
                            Err(DataError::MalformedInput(_))
                        ));
                        let view = schema
                            .view(&input[..input.len() - 1], format, &ParseLimits::default())
                            .unwrap();
                        let node = view.root().unwrap();
                        if signed {
                            assert!(matches!(node.read_i64(), Err(DataError::MalformedInput(_))));
                        } else {
                            assert!(matches!(node.read_u64(), Err(DataError::MalformedInput(_))));
                        }
                    }
                }
            }
            for signed in [false, true] {
                for encoding in [IntegerEncoding::Fixed, IntegerEncoding::Varint] {
                    for value in [0u32, 1, 128, u32::MAX] {
                        let mut input = vec![];
                        let mut writer = fory::Writer::from_buffer(&mut input);
                        if matches!(format, InputFormat::XlangMessage) {
                            producer(mode, false).write_head::<u32>(&mut writer);
                        }
                        writer.write_i8(-1);
                        writer.write_var_u32(match (signed, encoding) {
                            (true, IntegerEncoding::Fixed) => 4,
                            (true, _) => 5,
                            (false, IntegerEncoding::Fixed) => 11,
                            (false, _) => 12,
                        });
                        match (signed, encoding) {
                            (true, IntegerEncoding::Fixed) => writer.write_i32(value as i32),
                            (true, _) => writer.write_var_i32(value as i32),
                            (false, IntegerEncoding::Fixed) => writer.write_u32(value),
                            (false, _) => writer.write_var_u32(value),
                        }
                        let schema = schema(
                            if signed {
                                ScalarKind::I32
                            } else {
                                ScalarKind::U32
                            },
                            Some(encoding),
                            mode,
                        );
                        let view = schema
                            .view(&input, format, &ParseLimits::default())
                            .unwrap();
                        if signed {
                            assert_eq!(view.root().unwrap().read_i32().unwrap(), value as i32);
                        } else {
                            assert_eq!(view.root().unwrap().read_u32().unwrap(), value);
                        }
                        let validated = schema
                            .validate(&input, format, &ParseLimits::default())
                            .unwrap();
                        if signed {
                            assert_eq!(validated.root().unwrap().read_i32().unwrap(), value as i32);
                        } else {
                            assert_eq!(validated.root().unwrap().read_u32().unwrap(), value);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn limits_headers_access_recovery_and_first_failure() {
    let schema = schema(ScalarKind::Bool, None, SchemaMode::Compatible);
    let input = producer(SchemaMode::Compatible, false)
        .serialize(&true)
        .unwrap();
    let mut limits = ParseLimits {
        max_input_bytes: input.len(),
    };
    let view = schema
        .view(&input, InputFormat::XlangMessage, &limits)
        .unwrap();
    limits.max_input_bytes = 0;
    let node = view.root().unwrap();
    assert!(matches!(node.read_i32(), Err(DataError::AccessError(_))));
    assert!(node.read_bool().unwrap());
    assert!(matches!(
        schema.view(&input, InputFormat::XlangMessage, &limits),
        Err(DataError::ResourceLimit(_))
    ));
    assert!(matches!(
        schema.view(
            &input,
            InputFormat::XlangMessage,
            &ParseLimits {
                max_input_bytes: input.len() - 1
            }
        ),
        Err(DataError::ResourceLimit(_))
    ));
    for prefix in 0..3 {
        assert!(matches!(
            schema.view(
                &input[..prefix],
                InputFormat::XlangMessage,
                &ParseLimits::default()
            ),
            Err(DataError::MalformedInput(_))
        ));
    }
    for header in [0u8, 2, 3, 255] {
        let mut bad = input.clone();
        bad[0] = header;
        assert!(matches!(
            schema.view(&bad, InputFormat::XlangMessage, &ParseLimits::default()),
            Err(DataError::MalformedInput(_))
        ));
    }
    for flag in [0u8, 254, 1] {
        let mut bad = input.clone();
        bad[1] = flag;
        assert!(matches!(
            schema.view(&bad, InputFormat::XlangMessage, &ParseLimits::default()),
            Err(DataError::MalformedInput(_))
        ));
    }
    let mut bad = input.clone();
    bad[2] = 2;
    assert!(matches!(
        schema.view(&bad, InputFormat::XlangMessage, &ParseLimits::default()),
        Err(DataError::SchemaMismatch(_))
    ));
    let mut bad = input.clone();
    bad[3] = 2;
    let view = schema
        .view(&bad, InputFormat::XlangMessage, &ParseLimits::default())
        .unwrap();
    let node = view.root().unwrap();
    let first = format!("{:?}", node.read_bool().unwrap_err());
    assert_eq!(first, format!("{:?}", node.read_i8().unwrap_err()));
    assert_eq!(first, format!("{:?}", node.is_null().unwrap_err()));
    assert!(matches!(
        node.node_type().kind,
        TypeKind::Scalar(ScalarKind::Bool)
    ));
    assert_eq!(first, format!("{:?}", view.root().err().unwrap()));
    drop(node);
    assert_eq!(first, format!("{:?}", view.validate().err().unwrap()));
    let mut trailing = input.clone();
    trailing.push(255);
    let view = schema
        .view(
            &trailing,
            InputFormat::XlangMessage,
            &ParseLimits::default(),
        )
        .unwrap();
    assert!(view.root().unwrap().read_bool().unwrap());
    assert!(matches!(view.validate(), Err(DataError::MalformedInput(_))));
}

#[test]
fn actual_fory_null_and_owned_survive_sources() {
    let mode = SchemaMode::Compatible;
    let input = producer(mode, false).serialize(&None::<bool>).unwrap();
    let descriptor = SchemaDescriptor {
        encoding: RootEncoding {
            protocol_version: ProtocolVersion::Fory1_7,
            mode,
            root_reference: RootReferenceMode::NullOnly,
        },
        key_semantics_version: KEY_SEMANTICS_VERSION,
        root: TypeDescriptor {
            nullable: true,
            track_ref: false,
            integer_encoding: None,
            kind: TypeKind::Scalar(ScalarKind::Bool),
        },
        definitions: vec![],
    };
    let nullable = Schema::compile(descriptor, &SchemaLimits::default()).unwrap();
    let view = nullable
        .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
        .unwrap();
    let node = view.root().unwrap();
    assert!(node.is_null().unwrap());
    assert!(matches!(node.read_bool(), Err(DataError::AccessError(_))));
    assert!(node.is_null().unwrap());
    assert!(matches!(
        schema(ScalarKind::Bool, None, mode).view(
            &input,
            InputFormat::XlangMessage,
            &ParseLimits::default()
        ),
        Err(DataError::SchemaMismatch(_))
    ));
    let owned = {
        let input = producer(mode, false).serialize(&false).unwrap();
        let schema = schema(ScalarKind::Bool, None, mode);
        let view = schema
            .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
            .unwrap();
        view.root().unwrap().to_owned().unwrap()
    };
    assert!(!owned.as_view().unwrap().read_bool().unwrap());
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

#[test]
fn actual_fory_numeric_scalars_and_truncations() {
    for mode in modes() {
        for tracking in [false, true] {
            let fory = producer(mode, tracking);
            macro_rules! check {
                ($kind:ident, $encoding:expr, $read:ident, $values:expr) => {
                    for value in $values {
                        let mut descriptor = schema(ScalarKind::$kind, $encoding, mode);
                        if tracking {
                            descriptor = Schema::compile(
                                SchemaDescriptor {
                                    encoding: RootEncoding {
                                        protocol_version: ProtocolVersion::Fory1_7,
                                        mode,
                                        root_reference: RootReferenceMode::Tracking,
                                    },
                                    key_semantics_version: KEY_SEMANTICS_VERSION,
                                    root: TypeDescriptor {
                                        nullable: false,
                                        track_ref: false,
                                        integer_encoding: $encoding,
                                        kind: TypeKind::Scalar(ScalarKind::$kind),
                                    },
                                    definitions: vec![],
                                },
                                &SchemaLimits::default(),
                            )
                            .unwrap();
                        }
                        let input = fory.serialize(&value).unwrap();
                        let view = descriptor
                            .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
                            .unwrap();
                        assert_eq!(view.root().unwrap().$read().unwrap(), value);
                        let validated = descriptor
                            .validate(&input, InputFormat::XlangMessage, &ParseLimits::default())
                            .unwrap();
                        assert_eq!(validated.root().unwrap().$read().unwrap(), value);
                        assert_eq!(
                            validated
                                .root()
                                .unwrap()
                                .to_owned()
                                .unwrap()
                                .as_view()
                                .unwrap()
                                .$read()
                                .unwrap(),
                            value
                        );
                        for end in 3..input.len() {
                            assert!(matches!(
                                descriptor.validate(
                                    &input[..end],
                                    InputFormat::XlangMessage,
                                    &ParseLimits::default()
                                ),
                                Err(DataError::MalformedInput(_))
                            ));
                            let view = descriptor
                                .view(
                                    &input[..end],
                                    InputFormat::XlangMessage,
                                    &ParseLimits::default(),
                                )
                                .unwrap();
                            assert!(matches!(
                                view.root().unwrap().$read(),
                                Err(DataError::MalformedInput(_))
                            ));
                            assert!(matches!(view.root(), Err(DataError::MalformedInput(_))));
                        }
                    }
                };
            }
            check!(I8, None, read_i8, [i8::MIN, 0, i8::MAX]);
            check!(I16, None, read_i16, [i16::MIN, 0, i16::MAX]);
            check!(
                I32,
                Some(IntegerEncoding::Varint),
                read_i32,
                [i32::MIN, -1, 0, 128, i32::MAX]
            );
            check!(
                I64,
                Some(IntegerEncoding::Varint),
                read_i64,
                [i64::MIN, -1, 0, 128, i64::MAX]
            );
            check!(U8, None, read_u8, [0u8, u8::MAX]);
            check!(U16, None, read_u16, [0u16, u16::MAX]);
            check!(
                U32,
                Some(IntegerEncoding::Varint),
                read_u32,
                [0u32, 128, u32::MAX]
            );
            check!(
                U64,
                Some(IntegerEncoding::Varint),
                read_u64,
                [0u64, 128, u64::MAX]
            );
        }
    }
}

#[test]
fn float_bits_are_preserved() {
    for mode in modes() {
        let fory = producer(mode, false);
        for bits in [0u32, 0x80000000, 0x7f800000, 0xff800000, 0x7fc12345, 1] {
            let input = fory.serialize(&f32::from_bits(bits)).unwrap();
            let schema = schema(ScalarKind::F32, None, mode);
            let view = schema
                .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
                .unwrap();
            assert_eq!(view.root().unwrap().read_f32().unwrap().to_bits(), bits);
            let validated = schema
                .validate(&input, InputFormat::XlangMessage, &ParseLimits::default())
                .unwrap();
            assert_eq!(
                validated.root().unwrap().read_f32().unwrap().to_bits(),
                bits
            );
            assert_eq!(
                validated
                    .root()
                    .unwrap()
                    .to_owned()
                    .unwrap()
                    .as_view()
                    .unwrap()
                    .read_f32()
                    .unwrap()
                    .to_bits(),
                bits
            );
            let view = schema
                .view(
                    &input[..input.len() - 1],
                    InputFormat::XlangMessage,
                    &ParseLimits::default(),
                )
                .unwrap();
            assert!(matches!(
                view.root().unwrap().read_f32(),
                Err(DataError::MalformedInput(_))
            ));
        }
        for bits in [
            0u64,
            0x8000000000000000,
            0x7ff0000000000000,
            0xfff0000000000000,
            0x7ff8123456789abc,
            1,
        ] {
            let input = fory.serialize(&f64::from_bits(bits)).unwrap();
            let schema = schema(ScalarKind::F64, None, mode);
            let view = schema
                .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
                .unwrap();
            assert_eq!(view.root().unwrap().read_f64().unwrap().to_bits(), bits);
            let validated = schema
                .validate(&input, InputFormat::XlangMessage, &ParseLimits::default())
                .unwrap();
            assert_eq!(
                validated.root().unwrap().read_f64().unwrap().to_bits(),
                bits
            );
            assert_eq!(
                validated
                    .root()
                    .unwrap()
                    .to_owned()
                    .unwrap()
                    .as_view()
                    .unwrap()
                    .read_f64()
                    .unwrap()
                    .to_bits(),
                bits
            );
            let view = schema
                .view(
                    &input[..input.len() - 1],
                    InputFormat::XlangMessage,
                    &ParseLimits::default(),
                )
                .unwrap();
            assert!(matches!(
                view.root().unwrap().read_f64(),
                Err(DataError::MalformedInput(_))
            ));
        }
    }
}

fn schema(kind: ScalarKind, encoding: Option<IntegerEncoding>, mode: SchemaMode) -> Schema {
    Schema::compile(
        SchemaDescriptor {
            encoding: RootEncoding {
                protocol_version: ProtocolVersion::Fory1_7,
                mode,
                root_reference: RootReferenceMode::NullOnly,
            },
            key_semantics_version: KEY_SEMANTICS_VERSION,
            root: TypeDescriptor {
                nullable: false,
                track_ref: false,
                integer_encoding: encoding,
                kind: TypeKind::Scalar(kind),
            },
            definitions: vec![],
        },
        &SchemaLimits::default(),
    )
    .unwrap()
}

#[test]
fn actual_fory_bool_root_and_owned() {
    for mode in [
        SchemaMode::Compatible,
        SchemaMode::SameSchema { version_hash: true },
        SchemaMode::SameSchema {
            version_hash: false,
        },
    ] {
        let fory = fory::Fory::builder()
            .xlang(true)
            .compatible(matches!(mode, SchemaMode::Compatible))
            .check_struct_version(matches!(
                mode,
                SchemaMode::SameSchema { version_hash: true }
            ))
            .build();
        let input = fory.serialize(&true).unwrap();
        let schema = schema(ScalarKind::Bool, None, mode);
        let view = schema
            .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
            .unwrap_or_else(|e| panic!("root opening must work: {e:?}"));
        let node = view.root().unwrap();
        assert!(!node.is_null().unwrap());
        assert!(node.read_bool().unwrap());
        let owned = node.to_owned().unwrap();
        assert!(owned.as_view().unwrap().read_bool().unwrap());
        let schema = std::sync::Arc::new(schema);
        let owned = schema
            .view_owned(
                bytes::Bytes::from(input),
                InputFormat::XlangMessage,
                &ParseLimits::default(),
            )
            .unwrap();
        assert!(
            owned
                .as_view()
                .unwrap()
                .root()
                .unwrap()
                .read_bool()
                .unwrap()
        );
    }
}
