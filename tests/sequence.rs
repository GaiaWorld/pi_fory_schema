use pi_fory_schema::*;

#[test]
fn dense_actual_fixed_preserves_logical_integer_encoding() {
    for mode in modes() {
        let f = producer(mode);
        macro_rules! check {
            ($kind:ident, $enc:ident, $read:ident, $values:expr, $id:expr, $fixed:expr) => {{
                let values = $values;
                for input in [
                    f.serialize(&values.to_vec()).unwrap(),
                    f.serialize(&values).unwrap(),
                ] {
                    for length in [None, Some(Some(values.len() as u64))] {
                        let s = schema(
                            scalar(ScalarKind::$kind, Some(IntegerEncoding::$enc), false),
                            length,
                            mode,
                        );
                        let view = s
                            .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
                            .unwrap();
                        let root = view.root().unwrap();
                        assert_eq!(root.len().unwrap(), values.len());
                        for i in (0..values.len()).rev() {
                            let child = root.element_at(i).unwrap();
                            assert_eq!(
                                child.node_type().integer_encoding,
                                Some(IntegerEncoding::$enc)
                            );
                            assert!(child.object_id().unwrap().is_none());
                            assert_eq!(child.$read().unwrap(), values[i]);
                        }
                        assert_eq!(
                            root.elements()
                                .unwrap()
                                .map(|n| n.unwrap().$read().unwrap())
                                .collect::<Vec<_>>(),
                            values
                        );
                        let owned = root.element_at(1).unwrap().to_owned().unwrap();
                        drop(root);
                        let validated = view.validate().unwrap();
                        let root = validated.root().unwrap();
                        assert_eq!(
                            root.elements()
                                .unwrap()
                                .map(|n| n.unwrap().$read().unwrap())
                                .collect::<Vec<_>>(),
                            values
                        );
                        assert_eq!(
                            root.element_at(values.len() - 1).unwrap().$read().unwrap(),
                            values[values.len() - 1]
                        );
                        let retained = root.element_at(0).unwrap().to_owned().unwrap();
                        drop(root);
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
                        let view = s
                            .view(
                                &trailing,
                                InputFormat::XlangMessage,
                                &ParseLimits::default(),
                            )
                            .unwrap();
                        let survivor = view.root().unwrap().to_owned().unwrap();
                        let error = view.validate().err().unwrap();
                        assert!(matches!(error, DataError::MalformedInput(_)));
                        assert_eq!(
                            format!("{error:?}"),
                            format!("{:?}", survivor.as_view().err().unwrap())
                        );
                        assert_eq!(owned.as_view().unwrap().$read().unwrap(), values[1]);
                        assert_eq!(retained.as_view().unwrap().$read().unwrap(), values[0]);
                    }
                    let wrong = schema(
                        scalar(ScalarKind::$kind, Some(IntegerEncoding::$enc), false),
                        Some(Some(999)),
                        mode,
                    );
                    assert!(matches!(
                        wrong.validate(&input, InputFormat::XlangMessage, &ParseLimits::default()),
                        Err(DataError::SchemaMismatch(_))
                    ));
                }
                let s = schema(
                    scalar(ScalarKind::$kind, Some(IntegerEncoding::$enc), false),
                    None,
                    mode,
                );
                let empty = f.serialize(&values[..0].to_vec()).unwrap();
                assert_eq!(
                    s.validate(&empty, InputFormat::XlangMessage, &ParseLimits::default())
                        .unwrap()
                        .root()
                        .unwrap()
                        .len()
                        .unwrap(),
                    0
                );
                let large = f.serialize(&vec![values[1]; 4096]).unwrap();
                let validated = s
                    .validate(&large, InputFormat::XlangMessage, &ParseLimits::default())
                    .unwrap();
                assert_eq!(
                    validated
                        .root()
                        .unwrap()
                        .element_at(4095)
                        .unwrap()
                        .$read()
                        .unwrap(),
                    values[1]
                );
                let malformed = [255, $id, 1, 0];
                assert!(matches!(
                    s.validate(&malformed, InputFormat::RootValue, &ParseLimits::default()),
                    Err(DataError::MalformedInput(_))
                ));
                let wrong_group = list_wire(InputFormat::RootValue, 8, Some($fixed), 1, &[0; 8]);
                assert!(matches!(
                    s.validate(
                        &wrong_group,
                        InputFormat::RootValue,
                        &ParseLimits::default()
                    ),
                    Err(DataError::SchemaMismatch(_))
                ));
                let scalar_s = Schema::compile(
                    SchemaDescriptor {
                        encoding: RootEncoding {
                            protocol_version: ProtocolVersion::Fory1_7,
                            mode,
                            root_reference: RootReferenceMode::NullOnly,
                        },
                        key_semantics_version: KEY_SEMANTICS_VERSION,
                        root: scalar(ScalarKind::$kind, Some(IntegerEncoding::$enc), false),
                        definitions: vec![],
                    },
                    &SchemaLimits::default(),
                )
                .unwrap();
                assert!(matches!(
                    scalar_s.view(
                        &[255, $fixed, 0, 0, 0, 0, 0, 0, 0, 0],
                        InputFormat::RootValue,
                        &ParseLimits::default()
                    ),
                    Err(DataError::SchemaMismatch(_))
                ));
            }};
        }
        check!(
            I32,
            Varint,
            read_i32,
            [i32::MIN, -129, 0, 128, i32::MAX],
            46,
            4
        );
        check!(U32, Varint, read_u32, [0u32, 128, u32::MAX], 50, 11);
        check!(
            I64,
            Varint,
            read_i64,
            [i64::MIN, -129, 0, 128, i64::MAX],
            47,
            6
        );
        check!(
            I64,
            Tagged,
            read_i64,
            [i64::MIN, -129, 0, 128, i64::MAX],
            47,
            6
        );
        check!(U64, Varint, read_u64, [0u64, 128, u64::MAX], 51, 13);
        check!(U64, Tagged, read_u64, [0u64, 128, u64::MAX], 51, 13);
    }
}

#[test]
fn iterator_scans_current_body_and_stops_after_first_error() {
    let mode = SchemaMode::Compatible;
    let schema = schema(scalar(ScalarKind::Bool, None, false), None, mode);
    let input = list_wire(InputFormat::RootValue, 8, Some(1), 3, &[1, 2, 0]);
    let view = schema
        .view(&input, InputFormat::RootValue, &ParseLimits::default())
        .unwrap();
    let root = view.root().unwrap();
    // Random access still returns the selected node before reading its body.
    assert!(root.element_at(1).is_ok());
    let mut iter = root.elements().unwrap();
    let first = iter.next().unwrap().unwrap();
    assert!(first.read_bool().unwrap());
    let error = iter.next().unwrap().err().unwrap();
    assert!(
        matches!(&error,DataError::MalformedInput(location) if location.byte_offset==Some(input.len()-2))
    );
    assert!(iter.next().is_none());
    assert!(iter.next().is_none());
    assert_eq!(
        format!("{error:?}"),
        format!("{:?}", root.len().err().unwrap())
    );
    assert_eq!(
        format!("{error:?}"),
        format!("{:?}", first.read_bool().err().unwrap())
    );
    drop(first);
    drop(iter);
    drop(root);
    assert_eq!(
        format!("{error:?}"),
        format!("{:?}", view.validate().err().unwrap())
    );
    let input = list_wire(InputFormat::RootValue, 8, Some(1), 2, &[1]);
    let view = schema
        .view(&input, InputFormat::RootValue, &ParseLimits::default())
        .unwrap();
    let root = view.root().unwrap();
    let mut iter = root.elements().unwrap();
    assert!(iter.next().unwrap().unwrap().read_bool().unwrap());
    assert!(matches!(
        iter.next().unwrap(),
        Err(DataError::MalformedInput(_))
    ));
    assert!(iter.next().is_none());
    let values = vec![true; 16384];
    let input = producer(mode).serialize(&values).unwrap();
    let validated = schema
        .validate(&input, InputFormat::XlangMessage, &ParseLimits::default())
        .unwrap();
    let root = validated.root().unwrap();
    let mut iter = root.elements().unwrap();
    let retained = iter.next().unwrap().unwrap().to_owned().unwrap();
    assert_eq!(
        iter.map(|node| {
            assert!(node.unwrap().read_bool().unwrap());
            1usize
        })
        .sum::<usize>(),
        values.len() - 1
    );
    drop(root);
    drop(validated);
    drop(input);
    assert!(retained.as_view().unwrap().read_bool().unwrap());
}

#[test]
fn explicit_generic_integer_codecs_and_tracking_root() {
    for mode in modes() {
        for format in [InputFormat::RootValue, InputFormat::XlangMessage] {
            for (encoding, id) in [
                (IntegerEncoding::Fixed, 6),
                (IntegerEncoding::Varint, 7),
                (IntegerEncoding::Tagged, 8),
            ] {
                let mut body = vec![];
                let mut writer = fory::Writer::from_buffer(&mut body);
                for value in [i64::MIN, -1, 0, i64::MAX] {
                    match encoding {
                        IntegerEncoding::Fixed => writer.write_i64(value),
                        IntegerEncoding::Varint => writer.write_var_i64(value),
                        IntegerEncoding::Tagged => writer.write_tagged_i64(value),
                    }
                }
                let input = list_wire(format, 8, Some(id), 4, &body);
                let schema = schema(scalar(ScalarKind::I64, Some(encoding), false), None, mode);
                let validated = schema
                    .validate(&input, format, &ParseLimits::default())
                    .unwrap();
                let root = validated.root().unwrap();
                let values: Vec<_> = root
                    .elements()
                    .unwrap()
                    .map(|node| node.unwrap().read_i64().unwrap())
                    .collect();
                assert_eq!(values, vec![i64::MIN, -1, 0, i64::MAX]);
            }
        }
        let fory = fory::Fory::builder()
            .xlang(true)
            .compatible(matches!(mode, SchemaMode::Compatible))
            .track_ref(true)
            .build();
        let input = fory
            .serialize(&vec!["a".to_owned(), "b".to_owned()])
            .unwrap();
        let schema = Schema::compile(
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
                    integer_encoding: None,
                    kind: TypeKind::List(Box::new(scalar(ScalarKind::String, None, false))),
                },
                definitions: vec![],
            },
            &SchemaLimits::default(),
        )
        .unwrap();
        let validated = schema
            .validate(&input, InputFormat::XlangMessage, &ParseLimits::default())
            .unwrap();
        assert_eq!(validated.root().unwrap().len().unwrap(), 2);
    }
}

fn list_wire(
    format: InputFormat,
    header: u8,
    group: Option<u32>,
    len: u32,
    body: &[u8],
) -> Vec<u8> {
    let mut input = vec![];
    let mut writer = fory::Writer::from_buffer(&mut input);
    if matches!(format, InputFormat::XlangMessage) {
        producer(SchemaMode::Compatible).write_head::<Vec<String>>(&mut writer);
    }
    writer.write_i8(-1);
    writer.write_var_u32(fory::TypeId::LIST as u32);
    writer.write_var_u32(len);
    if len > 0 {
        writer.write_u8(header);
        if let Some(group) = group {
            writer.write_var_u32(group);
        }
    }
    writer.write_bytes(body);
    input
}

#[test]
fn writer_groups_formats_errors_and_ownership() {
    for mode in modes() {
        for format in [InputFormat::XlangMessage, InputFormat::RootValue] {
            for declared in [false, true] {
                let mut payload = vec![];
                let mut writer = fory::Writer::from_buffer(&mut payload);
                writer.write_var_i32(-123);
                writer.write_var_i32(i32::MAX);
                let input = list_wire(
                    format,
                    if declared { 12 } else { 8 },
                    if declared { None } else { Some(5) },
                    2,
                    &payload,
                );
                let schema = schema(
                    scalar(ScalarKind::I32, Some(IntegerEncoding::Varint), false),
                    Some(Some(2)),
                    mode,
                );
                let owner = schema
                    .view_owned(
                        bytes::Bytes::from(input.clone()),
                        format,
                        &ParseLimits {
                            max_input_bytes: input.len(),
                        },
                    )
                    .unwrap();
                let view = owner.as_view().unwrap();
                let root = view.root().unwrap();
                assert!(matches!(root.element_at(2), Err(DataError::AccessError(_))));
                assert_eq!(root.element_at(1).unwrap().read_i32().unwrap(), i32::MAX);
                let mut iter = root.elements().unwrap();
                let child = iter.next().unwrap().unwrap().to_owned().unwrap();
                drop(iter);
                drop(root);
                drop(view);
                drop(owner);
                assert_eq!(child.as_view().unwrap().read_i32().unwrap(), -123);
                let validated = schema
                    .validate_owned(
                        bytes::Bytes::from(input.clone()),
                        format,
                        &ParseLimits::default(),
                    )
                    .unwrap();
                let child = validated
                    .as_view()
                    .unwrap()
                    .root()
                    .unwrap()
                    .element_at(1)
                    .unwrap()
                    .to_owned()
                    .unwrap();
                drop(validated);
                assert_eq!(child.as_view().unwrap().read_i32().unwrap(), i32::MAX);
                for end in 3..input.len() {
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
                let survivor = view.root().unwrap().to_owned().unwrap();
                let error = view.validate().err().unwrap();
                assert!(
                    matches!(&error,DataError::MalformedInput(location) if location.byte_offset==Some(input.len()))
                );
                assert_eq!(
                    format!("{error:?}"),
                    format!("{:?}", survivor.as_view().err().unwrap())
                );
                assert!(matches!(
                    schema.view(
                        &input,
                        format,
                        &ParseLimits {
                            max_input_bytes: input.len() - 1
                        }
                    ),
                    Err(DataError::ResourceLimit(_))
                ));
            }
            let schema = schema(scalar(ScalarKind::Bool, None, false), None, mode);
            for (header, group, body) in [
                (128, Some(1), vec![1]),
                (0, Some(1), vec![1]),
                (8, Some(2), vec![1]),
                (10, Some(1), vec![253]),
                (8, Some(1), vec![2]),
            ] {
                let input = list_wire(format, header, group, 1, &body);
                let view = schema
                    .view(&input, format, &ParseLimits::default())
                    .unwrap();
                let root = view.root().unwrap();
                let error = if header == 10 || body == vec![2] {
                    root.element_at(0).unwrap().read_bool().err().unwrap()
                } else {
                    root.len().err().unwrap()
                };
                assert!(matches!(
                    &error,
                    DataError::MalformedInput(_) | DataError::SchemaMismatch(_)
                ));
                drop(root);
                assert_eq!(
                    format!("{error:?}"),
                    format!("{:?}", view.validate().err().unwrap())
                );
            }
            let tracked = list_wire(format, 9, Some(1), 1, &[0, 1]);
            assert!(matches!(
                schema.validate(&tracked, format, &ParseLimits::default()),
                Err(DataError::NotImplemented(_))
            ));
            let mut dense = vec![];
            let mut writer = fory::Writer::from_buffer(&mut dense);
            if matches!(format, InputFormat::XlangMessage) {
                producer(mode).write_head::<Vec<i32>>(&mut writer);
            }
            writer.write_i8(-1);
            writer.write_var_u32(46);
            writer.write_var_u32(3);
            writer.write_bytes(&[0, 0, 0]);
            let fixed = self::schema(
                scalar(ScalarKind::I32, Some(IntegerEncoding::Fixed), false),
                None,
                mode,
            );
            assert!(matches!(
                fixed.validate(&dense, format, &ParseLimits::default()),
                Err(DataError::MalformedInput(_))
            ));
            let var = self::schema(
                scalar(ScalarKind::I32, Some(IntegerEncoding::Varint), false),
                None,
                mode,
            );
            assert!(matches!(
                var.validate(&dense, format, &ParseLimits::default()),
                Err(DataError::MalformedInput(_))
            ));
            let huge = list_wire(format, 12, None, u32::MAX, &[]);
            assert!(matches!(
                schema.validate(&huge, format, &ParseLimits::default()),
                Err(DataError::MalformedInput(_))
            ));
        }
    }
}

#[test]
fn nullable_roots_nested_boundaries_and_empty_arrays() {
    for mode in modes() {
        let fory = producer(mode);
        let input = fory.serialize(&[0i16; 0]).unwrap();
        let schema = schema(scalar(ScalarKind::I16, None, false), Some(Some(0)), mode);
        assert_eq!(
            schema
                .validate(&input, InputFormat::XlangMessage, &ParseLimits::default())
                .unwrap()
                .root()
                .unwrap()
                .len()
                .unwrap(),
            0
        );
        let null = fory.serialize(&None::<Vec<String>>).unwrap();
        let nullable = Schema::compile(
            SchemaDescriptor {
                encoding: RootEncoding {
                    protocol_version: ProtocolVersion::Fory1_7,
                    mode,
                    root_reference: RootReferenceMode::Tracking,
                },
                key_semantics_version: KEY_SEMANTICS_VERSION,
                root: TypeDescriptor {
                    nullable: true,
                    track_ref: false,
                    integer_encoding: None,
                    kind: TypeKind::List(Box::new(scalar(ScalarKind::String, None, false))),
                },
                definitions: vec![],
            },
            &SchemaLimits::default(),
        )
        .unwrap();
        let view = nullable
            .view(&null, InputFormat::XlangMessage, &ParseLimits::default())
            .unwrap();
        let root = view.root().unwrap();
        assert!(root.is_null().unwrap());
        assert!(matches!(root.len(), Err(DataError::AccessError(_))));
        drop(root);
        assert!(view.validate().unwrap().root().unwrap().is_null().unwrap());
        let nested = TypeDescriptor {
            nullable: false,
            track_ref: false,
            integer_encoding: None,
            kind: TypeKind::List(Box::new(scalar(ScalarKind::Bool, None, false))),
        };
        let schema = self::schema(nested, None, mode);
        let input = fory.serialize(&vec![vec![true]]).unwrap();
        let validated = schema
            .validate(&input, InputFormat::XlangMessage, &ParseLimits::default())
            .unwrap();
        assert!(
            validated
                .root()
                .unwrap()
                .element_at(0)
                .unwrap()
                .element_at(0)
                .unwrap()
                .read_bool()
                .unwrap()
        );
    }
}

#[test]
fn actual_fory_all_scalar_sequence_elements_and_arrays() {
    for mode in modes() {
        let fory = producer(mode);
        macro_rules! check {
            ($kind:ident,$encoding:expr,$read:ident,$values:expr) => {
                let values = $values;
                for input in [
                    fory.serialize(&values.to_vec()).unwrap(),
                    fory.serialize(&values).unwrap(),
                ] {
                    for length in [None, Some(None), Some(Some(values.len() as u64))] {
                        let schema =
                            schema(scalar(ScalarKind::$kind, $encoding, false), length, mode);
                        let view = schema
                            .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
                            .unwrap();
                        let node = view.root().unwrap();
                        assert_eq!(node.len().unwrap(), values.len());
                        assert!(matches!(
                            node.element_at(values.len()),
                            Err(DataError::AccessError(_))
                        ));
                        let actual: Vec<_> = node
                            .elements()
                            .unwrap()
                            .map(|node| node.unwrap().$read().unwrap())
                            .collect();
                        assert_eq!(actual, values.to_vec());
                        let child = node.element_at(0).unwrap().to_owned().unwrap();
                        assert_eq!(child.as_view().unwrap().$read().unwrap(), values[0]);
                        let validated = view.validate().unwrap();
                        let root = validated.root().unwrap();
                        assert!(matches!(
                            root.element_at(values.len()),
                            Err(DataError::AccessError(_))
                        ));
                        let actual: Vec<_> = root
                            .elements()
                            .unwrap()
                            .map(|node| node.unwrap().$read().unwrap())
                            .collect();
                        assert_eq!(actual, values.to_vec());
                        let owned = root.element_at(0).unwrap().to_owned().unwrap();
                        assert_eq!(owned.as_view().unwrap().$read().unwrap(), values[0]);
                    }
                }
                let input = fory.serialize(&Vec::from(values)).unwrap();
                let wrong = schema(
                    scalar(ScalarKind::$kind, $encoding, false),
                    Some(Some(999)),
                    mode,
                );
                assert!(matches!(
                    wrong.validate(&input, InputFormat::XlangMessage, &ParseLimits::default()),
                    Err(DataError::SchemaMismatch(_))
                ));
                let empty = fory.serialize(&Vec::<_>::from(&values[..0])).unwrap();
                let schema = schema(scalar(ScalarKind::$kind, $encoding, false), None, mode);
                let validated = schema
                    .validate(&empty, InputFormat::XlangMessage, &ParseLimits::default())
                    .unwrap();
                assert_eq!(validated.root().unwrap().len().unwrap(), 0);
                assert_eq!(validated.root().unwrap().elements().unwrap().count(), 0);
            };
        }
        check!(Bool, None, read_bool, [false, true, false]);
        check!(I8, None, read_i8, [i8::MIN, 0, i8::MAX]);
        check!(I16, None, read_i16, [i16::MIN, 0, i16::MAX]);
        check!(
            I32,
            Some(IntegerEncoding::Fixed),
            read_i32,
            [i32::MIN, 0, i32::MAX]
        );
        check!(
            I64,
            Some(IntegerEncoding::Fixed),
            read_i64,
            [i64::MIN, 0, i64::MAX]
        );
        check!(U8, None, read_u8, [0u8, 128, 255]);
        check!(U16, None, read_u16, [0u16, 128, u16::MAX]);
        check!(
            U32,
            Some(IntegerEncoding::Fixed),
            read_u32,
            [0u32, 128, u32::MAX]
        );
        check!(
            U64,
            Some(IntegerEncoding::Fixed),
            read_u64,
            [0u64, 128, u64::MAX]
        );
        check!(F32, None, read_f32, [-1.5f32, 0., f32::INFINITY]);
        check!(F64, None, read_f64, [-1.5f64, 0., f64::INFINITY]);
        check!(
            Date,
            None,
            read_date,
            [
                fory::Date::from_epoch_days(-1),
                fory::Date::UNIX_EPOCH,
                fory::Date::from_epoch_days(i64::MAX)
            ]
        );
        check!(
            Timestamp,
            None,
            read_timestamp,
            [
                fory::Timestamp::new(-1, 999_999_999).unwrap(),
                fory::Timestamp::UNIX_EPOCH,
                fory::Timestamp::new(i64::MAX, 1).unwrap()
            ]
        );
        check!(
            Duration,
            None,
            read_duration,
            [
                fory::Duration::new(0, -1).unwrap(),
                fory::Duration::ZERO,
                fory::Duration::new(i64::MAX, 999_999_999).unwrap()
            ]
        );
        let strings = [
            "".to_owned(),
            "hello".to_owned(),
            "\u{4e2d}\u{1f600}".to_owned(),
        ];
        for input in [
            fory.serialize(&strings).unwrap(),
            fory.serialize(&strings.to_vec()).unwrap(),
        ] {
            let schema = schema(scalar(ScalarKind::String, None, false), None, mode);
            let validated = schema
                .validate(&input, InputFormat::XlangMessage, &ParseLimits::default())
                .unwrap();
            for (index, text) in strings.iter().enumerate() {
                assert_eq!(
                    validated
                        .root()
                        .unwrap()
                        .element_at(index)
                        .unwrap()
                        .read_string()
                        .unwrap()
                        .as_str(),
                    text
                );
            }
        }
        let binaries = [vec![], vec![0u8, 128, 255]];
        let input = fory.serialize(&binaries).unwrap();
        let schema = schema(scalar(ScalarKind::Binary, None, false), Some(Some(2)), mode);
        let validated = schema
            .validate(&input, InputFormat::XlangMessage, &ParseLimits::default())
            .unwrap();
        for (index, bytes) in binaries.iter().enumerate() {
            assert_eq!(
                validated
                    .root()
                    .unwrap()
                    .element_at(index)
                    .unwrap()
                    .read_binary()
                    .unwrap()
                    .as_bytes(),
                bytes
            );
        }
    }
}

#[test]
fn actual_nullable_integer_and_time_sequences() {
    for mode in modes() {
        let fory = producer(mode);
        macro_rules! check {
            ($kind:ident,$encoding:expr,$read:ident,$value:expr) => {
                let value = $value;
                for values in [
                    vec![],
                    vec![None],
                    vec![Some(value), None, Some(value)],
                    vec![Some(value), Some(value)],
                ] {
                    let input = fory.serialize(&values).unwrap();
                    let schema = schema(scalar(ScalarKind::$kind, $encoding, true), None, mode);
                    let view = schema
                        .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
                        .unwrap();
                    let root = view.root().unwrap();
                    for (index, value) in values.iter().enumerate() {
                        let node = root.element_at(index).unwrap();
                        assert_eq!(node.is_null().unwrap(), value.is_none());
                        match value {
                            Some(value) => assert_eq!(node.$read().unwrap(), *value),
                            None => assert!(matches!(node.$read(), Err(DataError::AccessError(_)))),
                        }
                        assert!(node.node_type().nullable);
                    }
                    let validated = view.validate().unwrap();
                    for (index, value) in values.iter().enumerate() {
                        let node = validated
                            .root()
                            .unwrap()
                            .element_at(index)
                            .unwrap()
                            .to_owned()
                            .unwrap();
                        let node = node.as_view().unwrap();
                        assert_eq!(node.is_null().unwrap(), value.is_none());
                        if let Some(value) = value {
                            assert_eq!(node.$read().unwrap(), *value);
                        }
                    }
                }
            };
        }
        check!(Bool, None, read_bool, true);
        check!(I32, Some(IntegerEncoding::Varint), read_i32, -123i32);
        check!(I64, Some(IntegerEncoding::Varint), read_i64, i64::MIN);
        check!(U32, Some(IntegerEncoding::Varint), read_u32, u32::MAX);
        check!(U64, Some(IntegerEncoding::Varint), read_u64, u64::MAX);
        check!(Date, None, read_date, fory::Date::from_epoch_days(-1));
        check!(
            Timestamp,
            None,
            read_timestamp,
            fory::Timestamp::new(-1, 999_999_999).unwrap()
        );
        check!(
            Duration,
            None,
            read_duration,
            fory::Duration::new(0, -1).unwrap()
        );
    }
}

fn scalar(
    kind: ScalarKind,
    integer_encoding: Option<IntegerEncoding>,
    nullable: bool,
) -> TypeDescriptor {
    TypeDescriptor {
        nullable,
        track_ref: false,
        integer_encoding,
        kind: TypeKind::Scalar(kind),
    }
}
fn schema(
    element: TypeDescriptor,
    length: Option<Option<u64>>,
    mode: SchemaMode,
) -> std::sync::Arc<Schema> {
    std::sync::Arc::new(
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
                    integer_encoding: None,
                    kind: match length {
                        None => TypeKind::List(Box::new(element)),
                        Some(length) => TypeKind::Array {
                            element: Box::new(element),
                            length,
                        },
                    },
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
fn producer(mode: SchemaMode) -> fory::Fory {
    fory::Fory::builder()
        .xlang(true)
        .compatible(matches!(mode, SchemaMode::Compatible))
        .check_struct_version(matches!(
            mode,
            SchemaMode::SameSchema { version_hash: true }
        ))
        .build()
}

#[test]
fn actual_fory_dense_and_generic_sequences() {
    for mode in modes() {
        let fory = producer(mode);
        let input = fory.serialize(&vec![1i32, -2, 3]).unwrap();
        assert_eq!(input[2], fory::TypeId::INT32_ARRAY as u8);
        let schema = schema(
            scalar(ScalarKind::I32, Some(IntegerEncoding::Fixed), false),
            None,
            mode,
        );
        let view = schema
            .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
            .unwrap();
        let node = view.root().unwrap();
        assert_eq!(node.len().unwrap(), 3);
        assert_eq!(node.element_at(1).unwrap().read_i32().unwrap(), -2);
        let values: Vec<_> = node
            .elements()
            .unwrap()
            .map(|node| node.unwrap().read_i32().unwrap())
            .collect();
        assert_eq!(values, vec![1, -2, 3]);
        let validated = view.validate().unwrap();
        assert_eq!(
            validated
                .root()
                .unwrap()
                .element_at(2)
                .unwrap()
                .read_i32()
                .unwrap(),
            3
        );
        let input = fory
            .serialize(&vec![Some("one".to_owned()), None, Some("two".to_owned())])
            .unwrap();
        assert_eq!(input[2], fory::TypeId::LIST as u8);
        let schema = self::schema(scalar(ScalarKind::String, None, true), None, mode);
        let view = schema
            .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
            .unwrap();
        let node = view.root().unwrap();
        assert_eq!(node.len().unwrap(), 3);
        assert!(node.element_at(1).unwrap().is_null().unwrap());
        assert_eq!(
            node.element_at(2).unwrap().read_string().unwrap().as_str(),
            "two"
        );
        let owned = node.element_at(0).unwrap().to_owned().unwrap();
        drop(node);
        let validated = view.validate().unwrap();
        let values: Vec<_> = validated
            .root()
            .unwrap()
            .elements()
            .unwrap()
            .map(|node| node.unwrap().is_null().unwrap())
            .collect();
        assert_eq!(values, vec![false, true, false]);
        assert_eq!(
            owned.as_view().unwrap().read_string().unwrap().as_str(),
            "one"
        );
    }
}
