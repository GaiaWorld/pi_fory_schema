use pi_fory_schema::*;

fn malformed_offset(error: &DataError) -> usize {
    match error {
        DataError::MalformedInput(location) => location.byte_offset.unwrap(),
        _ => panic!("expected malformed input: {error:?}"),
    }
}

#[test]
fn borrowed_string_payload_pointers_and_header_boundaries() {
    for format in [InputFormat::RootValue, InputFormat::XlangMessage] {
        let schema = schema(ScalarKind::String, SchemaMode::Compatible, false, false);
        for (tag, payload) in [
            (0, b"ascii".to_vec()),
            (2, b"ascii".to_vec()),
            (2, "\u{4e2d}\u{1f600}".as_bytes().to_vec()),
        ] {
            let input = wire(format, true, tag, &payload);
            let pointer = input[input.len() - payload.len()..].as_ptr();
            let view = schema
                .view(&input, format, &ParseLimits::default())
                .unwrap();
            let node = view.root().unwrap();
            assert_eq!(node.read_string().unwrap().as_str().as_ptr(), pointer);
            drop(node);
            let validated = view.validate().unwrap();
            assert_eq!(
                validated
                    .root()
                    .unwrap()
                    .read_string()
                    .unwrap()
                    .as_str()
                    .as_ptr(),
                pointer
            );
        }
        let payload = vec![b'A'; 4096];
        let input = wire(format, true, 2, &payload);
        let body = if matches!(format, InputFormat::XlangMessage) {
            3
        } else {
            2
        };
        let header_len = input.len() - body - payload.len();
        assert!(header_len > 1);
        for end in body..body + header_len {
            let view = schema
                .view(&input[..end], format, &ParseLimits::default())
                .unwrap();
            let error = view.root().unwrap().read_string().err().unwrap();
            assert_eq!(malformed_offset(&error), body);
            let validation = schema
                .validate(&input[..end], format, &ParseLimits::default())
                .err()
                .unwrap();
            assert_eq!(malformed_offset(&validation), body);
        }
        let mut overflow = input[..body].to_vec();
        overflow.extend_from_slice(&[255, 255, 255, 255, 255, 2, 0, 0]);
        assert!(overflow.len() - body >= 8);
        let view = schema
            .view(&overflow, format, &ParseLimits::default())
            .unwrap();
        let error = view.root().unwrap().read_string().err().unwrap();
        assert_eq!(malformed_offset(&error), body);
        assert_eq!(
            malformed_offset(
                &schema
                    .validate(&overflow, format, &ParseLimits::default())
                    .err()
                    .unwrap()
            ),
            body
        );
    }
}

#[test]
fn owned_aliases_keep_first_content_failure() {
    for format in [InputFormat::RootValue, InputFormat::XlangMessage] {
        let input = wire(format, true, 2, &[65, 66, 255]);
        let expected = input.len() - 1;
        let schema = schema(ScalarKind::String, SchemaMode::Compatible, false, false);
        let owner = schema
            .view_owned(bytes::Bytes::from(input), format, &ParseLimits::default())
            .unwrap();
        let first_view = owner.as_view().unwrap();
        let second_view = owner.as_view().unwrap();
        let node = first_view.root().unwrap();
        let owned_node = node.to_owned().unwrap();
        let error = node.read_string().err().unwrap();
        assert_eq!(malformed_offset(&error), expected);
        assert_eq!(
            format!("{error:?}"),
            format!("{:?}", second_view.root().err().unwrap())
        );
        assert_eq!(
            format!("{error:?}"),
            format!("{:?}", owned_node.as_view().err().unwrap())
        );
        assert_eq!(
            format!("{error:?}"),
            format!("{:?}", second_view.validate().err().unwrap())
        );
        drop(node);
        drop(first_view);
        assert_eq!(
            format!("{error:?}"),
            format!("{:?}", owner.validate().err().unwrap())
        );
    }
}

#[test]
fn decoded_access_and_validation_share_precise_offsets() {
    for format in [InputFormat::RootValue, InputFormat::XlangMessage] {
        for (tag, payload, bad_byte) in [
            (2, vec![65, 66, 0xff], 2),
            (1, vec![65, 0, 66, 0, 1], 4),
            (1, vec![65, 0, 0, 0xdc], 2),
            (1, vec![65, 0, 0, 0xd8], 2),
            (1, vec![65, 0, 0, 0xd8, 66, 0], 2),
            (1, vec![0x3d, 0xd8, 0, 0xde, 0, 0xdc], 4),
        ] {
            let input = wire(format, true, tag, &payload);
            let expected = input.len() - payload.len() + bad_byte;
            let schema = schema(ScalarKind::String, SchemaMode::Compatible, false, false);
            let view = schema
                .view(&input, format, &ParseLimits::default())
                .unwrap();
            let error = view.root().unwrap().read_string().err().unwrap();
            let independently_validated = schema
                .validate(&input, format, &ParseLimits::default())
                .err()
                .unwrap();
            assert_eq!(
                malformed_offset(&error),
                malformed_offset(&independently_validated)
            );
            assert_eq!(malformed_offset(&error), expected);
            assert_eq!(
                format!("{error:?}"),
                format!("{:?}", view.validate().err().unwrap())
            );
        }
    }
}

#[test]
fn nullable_content_and_live_holders() {
    for mode in modes() {
        for tracking in [false, true] {
            let fory = producer(mode, tracking);
            for string in [false, true] {
                let input = if string {
                    fory.serialize(&None::<String>).unwrap()
                } else {
                    fory.serialize(&None::<Vec<u8>>).unwrap()
                };
                let kind = if string {
                    ScalarKind::String
                } else {
                    ScalarKind::Binary
                };
                let schema = schema(kind, mode, tracking, true);
                let view = schema
                    .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
                    .unwrap();
                let node = view.root().unwrap();
                assert!(node.is_null().unwrap());
                if string {
                    assert!(matches!(node.read_string(), Err(DataError::AccessError(_))));
                } else {
                    assert!(matches!(node.read_binary(), Err(DataError::AccessError(_))));
                }
                drop(node);
                let validated = view.validate().unwrap();
                let node = validated.root().unwrap();
                assert!(node.is_null().unwrap());
                if string {
                    assert!(matches!(node.read_string(), Err(DataError::AccessError(_))));
                } else {
                    assert!(matches!(node.read_binary(), Err(DataError::AccessError(_))));
                }
                assert!(matches!(
                    self::schema(kind, mode, tracking, false).validate(
                        &input,
                        InputFormat::XlangMessage,
                        &ParseLimits::default()
                    ),
                    Err(DataError::SchemaMismatch(_))
                ));
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
                let mut root_null = vec![];
                fory::Writer::from_buffer(&mut root_null).write_i8(fory::RefFlag::Null as i8);
                assert!(
                    schema
                        .validate(&root_null, InputFormat::RootValue, &ParseLimits::default())
                        .unwrap()
                        .root()
                        .unwrap()
                        .is_null()
                        .unwrap()
                );
            }
        }
    }
    for (tag, bytes) in [
        (0, vec![255]),
        (1, vec![0x3d, 0xd8, 0, 0xde]),
        (2, b"safe".to_vec()),
    ] {
        let mut input = wire(InputFormat::RootValue, true, tag, &bytes);
        input.push(0);
        let schema = schema(ScalarKind::String, SchemaMode::Compatible, false, false);
        let view = schema
            .view(&input, InputFormat::RootValue, &ParseLimits::default())
            .unwrap();
        let node = view.root().unwrap();
        let content = node.read_string().unwrap();
        let expected = content.as_str().to_owned();
        let shared = node.to_owned().unwrap();
        assert!(matches!(
            shared.as_view().unwrap().read_bool(),
            Err(DataError::AccessError(_))
        ));
        assert!(matches!(
            shared.as_view().unwrap().node_type().kind,
            TypeKind::Scalar(ScalarKind::String)
        ));
        assert!(matches!(
            schema.validate(&input, InputFormat::RootValue, &ParseLimits::default()),
            Err(DataError::MalformedInput(_))
        ));
        assert_eq!(content.as_str(), expected);
    }
}

fn wire(format: InputFormat, string: bool, tag: u64, payload: &[u8]) -> Vec<u8> {
    let mut input = vec![];
    let mut writer = fory::Writer::from_buffer(&mut input);
    if matches!(format, InputFormat::XlangMessage) {
        producer(SchemaMode::Compatible, false).write_head::<String>(&mut writer);
    }
    writer.write_i8(fory::RefFlag::NotNullValue as i8);
    writer.write_var_u32(if string {
        fory::TypeId::STRING as u32
    } else {
        fory::TypeId::BINARY as u32
    });
    if string {
        writer.write_var_u36_small(((payload.len() as u64) << 2) | tag);
    } else {
        writer.write_var_u32(payload.len() as u32);
    }
    writer.write_bytes(payload);
    input
}

#[test]
fn writer_string_encodings_and_binary_ownership() {
    for mode in modes() {
        for format in [InputFormat::XlangMessage, InputFormat::RootValue] {
            for tracking in [false, true] {
                let schema = schema(ScalarKind::String, mode, tracking, false);
                for (tag, payload, expected) in [
                    (0, vec![], ""),
                    (0, vec![0, 65, 127, 128, 255], "\0A\u{7f}\u{80}\u{ff}"),
                    (
                        1,
                        vec![0x2d, 0x4e, 0x3d, 0xd8, 0x00, 0xde],
                        "\u{4e2d}\u{1f600}",
                    ),
                    (
                        2,
                        "e\u{301}\u{1f600}".as_bytes().to_vec(),
                        "e\u{301}\u{1f600}",
                    ),
                ] {
                    let input = wire(format, true, tag, &payload);
                    let view = schema
                        .view(
                            &input,
                            format,
                            &ParseLimits {
                                max_input_bytes: input.len(),
                            },
                        )
                        .unwrap();
                    let node = view.root().unwrap();
                    assert_eq!(node.read_string().unwrap().as_str(), expected);
                    let retained = node.to_owned().unwrap();
                    let validated = view.validate().unwrap();
                    assert_eq!(
                        validated.root().unwrap().read_string().unwrap().as_str(),
                        expected
                    );
                    let validated_retained = validated.root().unwrap().to_owned().unwrap();
                    drop(validated);
                    drop(input);
                    assert_eq!(
                        retained.as_view().unwrap().read_string().unwrap().as_str(),
                        expected
                    );
                    assert_eq!(
                        validated_retained
                            .as_view()
                            .unwrap()
                            .read_string()
                            .unwrap()
                            .as_str(),
                        expected
                    );
                }
                let schema = self::schema(ScalarKind::Binary, mode, tracking, false);
                for payload in [vec![], (0..=255u8).collect::<Vec<_>>()] {
                    let input = wire(format, false, 0, &payload);
                    let view = schema
                        .view(&input, format, &ParseLimits::default())
                        .unwrap();
                    let node = view.root().unwrap();
                    let content = node.read_binary().unwrap();
                    assert_eq!(content.as_bytes(), payload);
                    if !payload.is_empty() {
                        assert_eq!(
                            content.as_bytes().as_ptr(),
                            input[input.len() - payload.len()..].as_ptr()
                        );
                    }
                    drop(content);
                    drop(node);
                    let owned = view.validate().unwrap().root().unwrap().to_owned().unwrap();
                    drop(input);
                    assert_eq!(
                        owned.as_view().unwrap().read_binary().unwrap().as_bytes(),
                        payload
                    );
                }
            }
        }
    }
}

#[test]
fn malformed_content_is_terminal_and_trailing_is_not_validated() {
    for format in [InputFormat::XlangMessage, InputFormat::RootValue] {
        let schema = schema(ScalarKind::String, SchemaMode::Compatible, false, false);
        for (tag, payload) in [
            (3, vec![]),
            (2, vec![0xff]),
            (2, vec![0xc0, 0x80]),
            (1, vec![1]),
            (1, vec![0, 0xd8]),
            (1, vec![0, 0xdc]),
            (1, vec![0, 0xd8, 65, 0]),
        ] {
            let input = wire(format, true, tag, &payload);
            let view = schema
                .view(&input, format, &ParseLimits::default())
                .unwrap();
            let node = view.root().unwrap();
            let first = node.read_string().err().unwrap();
            assert!(matches!(&first, DataError::MalformedInput(_)));
            assert_eq!(
                format!("{first:?}"),
                format!("{:?}", node.read_bool().err().unwrap())
            );
            assert!(matches!(
                node.node_type().kind,
                TypeKind::Scalar(ScalarKind::String)
            ));
            drop(node);
            assert_eq!(
                format!("{first:?}"),
                format!("{:?}", view.validate().err().unwrap())
            );
            assert!(matches!(
                schema.validate(&input, format, &ParseLimits::default()),
                Err(DataError::MalformedInput(_))
            ));
        }
        for string in [false, true] {
            let schema = self::schema(
                if string {
                    ScalarKind::String
                } else {
                    ScalarKind::Binary
                },
                SchemaMode::Compatible,
                false,
                false,
            );
            let input = wire(format, string, 2, b"abc");
            let body = if matches!(format, InputFormat::XlangMessage) {
                3
            } else {
                2
            };
            for end in body..input.len() {
                let view = schema
                    .view(&input[..end], format, &ParseLimits::default())
                    .unwrap();
                let node = view.root().unwrap();
                if string {
                    assert!(matches!(
                        node.read_string(),
                        Err(DataError::MalformedInput(_))
                    ));
                } else {
                    assert!(matches!(
                        node.read_binary(),
                        Err(DataError::MalformedInput(_))
                    ));
                }
                assert!(matches!(
                    schema.validate(&input[..end], format, &ParseLimits::default()),
                    Err(DataError::MalformedInput(_))
                ));
            }
            let mut trailing = input.clone();
            trailing.push(0);
            let view = schema
                .view(&trailing, format, &ParseLimits::default())
                .unwrap();
            let error = view.validate().err().unwrap();
            assert!(
                matches!(error, DataError::MalformedInput(location) if location.byte_offset == Some(input.len()))
            );
            assert!(matches!(
                schema.validate(
                    &input,
                    format,
                    &ParseLimits {
                        max_input_bytes: input.len() - 1
                    }
                ),
                Err(DataError::ResourceLimit(_))
            ));
            let mut bad = input.clone();
            bad[body - 1] = if string { 41 } else { 21 };
            assert!(matches!(
                schema.view(&bad, format, &ParseLimits::default()),
                Err(DataError::SchemaMismatch(_))
            ));
            if !string {
                bad[body - 1] = fory::TypeId::UINT8_ARRAY as u8;
                assert!(matches!(
                    schema.view(&bad, format, &ParseLimits::default()),
                    Err(DataError::SchemaMismatch(_))
                ));
            }
            let mut huge = input[..body].to_vec();
            let mut writer = fory::Writer::from_buffer(&mut huge);
            if string {
                writer.write_var_u36_small(((1u64 << 34) - 1) << 2 | 2);
            } else {
                writer.write_var_u32(u32::MAX);
            }
            assert!(matches!(
                schema.validate(&huge, format, &ParseLimits::default()),
                Err(DataError::MalformedInput(_))
            ));
            let mut overflow = input[..body].to_vec();
            overflow.extend_from_slice(if string {
                &[255, 255, 255, 255, 255, 2]
            } else {
                &[255, 255, 255, 255, 16]
            });
            assert!(matches!(
                schema.validate(&overflow, format, &ParseLimits::default()),
                Err(DataError::MalformedInput(_))
            ));
        }
    }
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

#[test]
fn actual_fory_content_roots() {
    for mode in modes() {
        for tracking in [false, true] {
            let fory = producer(mode, tracking);
            for text in [
                "",
                "ASCII\0",
                "caf\u{e9}",
                "\u{4e2d}\u{6587}",
                "\u{1f600}",
                "e\u{301}",
            ] {
                let input = fory.serialize(&text.to_owned()).unwrap();
                assert_eq!(input[2], fory::TypeId::STRING as u8);
                let schema = schema(ScalarKind::String, mode, tracking, false);
                let view = schema
                    .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
                    .unwrap();
                let node = view.root().unwrap();
                assert_eq!(node.read_string().unwrap().as_str(), text);
                assert!(matches!(node.read_binary(), Err(DataError::AccessError(_))));
                let owned = node.to_owned().unwrap();
                assert_eq!(
                    owned.as_view().unwrap().read_string().unwrap().as_str(),
                    text
                );
                let validated = view.validate().unwrap();
                assert_eq!(
                    validated.root().unwrap().read_string().unwrap().as_str(),
                    text
                );
                let owned = validated.root().unwrap().to_owned().unwrap();
                assert_eq!(
                    owned.as_view().unwrap().read_string().unwrap().as_str(),
                    text
                );
                let owned = schema
                    .validate_owned(
                        bytes::Bytes::from(input),
                        InputFormat::XlangMessage,
                        &ParseLimits::default(),
                    )
                    .unwrap();
                assert_eq!(
                    owned
                        .as_view()
                        .unwrap()
                        .root()
                        .unwrap()
                        .read_string()
                        .unwrap()
                        .as_str(),
                    text
                );
            }
            for value in [vec![], vec![0, 1, 127, 128, 255], (0..=255u8).collect()] {
                let input = fory.serialize(&value).unwrap();
                assert_eq!(input[2], fory::TypeId::BINARY as u8);
                let schema = schema(ScalarKind::Binary, mode, tracking, false);
                let view = schema
                    .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
                    .unwrap();
                let node = view.root().unwrap();
                assert_eq!(node.read_binary().unwrap().as_bytes(), value);
                assert!(matches!(node.read_string(), Err(DataError::AccessError(_))));
                let owned = node.to_owned().unwrap();
                assert_eq!(
                    owned.as_view().unwrap().read_binary().unwrap().as_bytes(),
                    value
                );
                let validated = view.validate().unwrap();
                assert_eq!(
                    validated.root().unwrap().read_binary().unwrap().as_bytes(),
                    value
                );
                let owned = validated.root().unwrap().to_owned().unwrap();
                assert_eq!(
                    owned.as_view().unwrap().read_binary().unwrap().as_bytes(),
                    value
                );
                let owned = schema
                    .validate_owned(
                        bytes::Bytes::from(input),
                        InputFormat::XlangMessage,
                        &ParseLimits::default(),
                    )
                    .unwrap();
                assert_eq!(
                    owned
                        .as_view()
                        .unwrap()
                        .root()
                        .unwrap()
                        .read_binary()
                        .unwrap()
                        .as_bytes(),
                    value
                );
            }
        }
    }
}
