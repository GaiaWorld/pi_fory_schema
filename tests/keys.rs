use pi_fory_schema::*;
use std::{cmp::Ordering, sync::Arc};

fn expected_hash(node_bytes: &[u8]) -> u64 {
    let mut bytes = b"pi_fory_schema:key\0".to_vec();
    bytes.extend_from_slice(&KEY_SEMANTICS_VERSION.to_le_bytes());
    bytes.extend_from_slice(node_bytes);
    rapidhash::v3::rapidhash_v3(&bytes)
}

#[test]
fn enum_hash_uses_descriptor_position_not_protocol_id() {
    let scalar = TypeDescriptor {
        kind: TypeKind::Scalar(ScalarKind::Bool),
        integer_encoding: None,
        nullable: false,
        track_ref: false,
    };
    let s = Schema::compile(
        SchemaDescriptor {
            encoding: RootEncoding {
                protocol_version: ProtocolVersion::Fory1_7,
                mode: SchemaMode::SameSchema {
                    version_hash: false,
                },
                root_reference: RootReferenceMode::NullOnly,
            },
            key_semantics_version: KEY_SEMANTICS_VERSION,
            root: TypeDescriptor {
                kind: TypeKind::User(UserIdentity::Numeric(102)),
                ..scalar
            },
            definitions: vec![UserDefinition {
                identity: UserIdentity::Numeric(102),
                kind: UserDefinitionKind::Enum {
                    variants: vec![
                        VariantDescriptor {
                            id: 9,
                            name: "First".into(),
                            payload: None,
                        },
                        VariantDescriptor {
                            id: 2,
                            name: "Second".into(),
                            payload: None,
                        },
                    ],
                },
            }],
        },
        &SchemaLimits::default(),
    )
    .unwrap();
    let inputs = [[255, 25, 102, 9], [255, 25, 102, 2]];
    let keys = inputs
        .iter()
        .map(|input| {
            s.validate(input, InputFormat::RootValue, &ParseLimits::default())
                .unwrap()
                .into_key(KeyRequirement::HashAndOrdered)
                .unwrap()
        })
        .collect::<Vec<_>>();
    for (index, key) in keys.iter().enumerate() {
        let mut expected = vec![17, 1];
        expected.extend_from_slice(&(index as u64).to_le_bytes());
        assert_eq!(key.stable_hash().unwrap(), expected_hash(&expected));
    }
    assert_eq!(keys[0].try_cmp(&keys[1]).unwrap(), Ordering::Less);
}

#[test]
fn nested_list_hash_preserves_boundaries_and_owned_lifetime() {
    let leaf = TypeDescriptor {
        kind: TypeKind::Scalar(ScalarKind::I32),
        integer_encoding: Some(IntegerEncoding::Varint),
        nullable: true,
        track_ref: false,
    };
    let inner = TypeDescriptor {
        kind: TypeKind::List(Box::new(leaf)),
        integer_encoding: None,
        nullable: false,
        track_ref: false,
    };
    let s = Arc::new(schema(TypeKind::List(Box::new(inner)), None, false));
    let value = vec![vec![], vec![None, Some(-129i32)], vec![Some(0)]];
    let input = fory::Fory::builder()
        .xlang(true)
        .build()
        .serialize(&value)
        .unwrap();
    let mut expected = vec![14, 1];
    expected.extend_from_slice(&3u64.to_le_bytes());
    for row in &value {
        expected.extend_from_slice(&[14, 1]);
        expected.extend_from_slice(&(row.len() as u64).to_le_bytes());
        for value in row {
            expected.extend_from_slice(&[4, u8::from(value.is_some())]);
            if let Some(value) = value {
                expected.extend_from_slice(&value.to_le_bytes());
            }
        }
    }
    let owned = s
        .validate_owned(
            bytes::Bytes::from(input.clone()),
            InputFormat::XlangMessage,
            &ParseLimits::default(),
        )
        .unwrap()
        .into_key(KeyRequirement::HashAndOrdered)
        .unwrap();
    {
        let borrowed = s
            .validate(&input, InputFormat::XlangMessage, &ParseLimits::default())
            .unwrap()
            .into_key(KeyRequirement::HashAndOrdered)
            .unwrap();
        assert!(borrowed.try_eq(&owned).unwrap());
        assert_eq!(borrowed.stable_hash().unwrap(), expected_hash(&expected));
    }
    drop(input);
    drop(s);
    assert_eq!(owned.stable_hash().unwrap(), expected_hash(&expected));
}

#[test]
fn stable_hash_fixed_vectors() {
    // 锁定语义版本与 portable V3 输出；这些值由 4.1.0 基线记录。
    assert_eq!(KEY_SEMANTICS_VERSION, 1);
    let f = fory::Fory::builder().xlang(true).build();
    let cases = [
        (
            ScalarKind::Bool,
            false,
            f.serialize(&true).unwrap(),
            0x827e_3a04_f0d0_8b80u64,
        ),
        (
            ScalarKind::I32,
            true,
            f.serialize(&None::<i32>).unwrap(),
            0x66e0_de0e_5cac_a08d,
        ),
        (
            ScalarKind::F32,
            false,
            f.serialize(&f32::from_bits(0xffc0_1234)).unwrap(),
            0x014e_c6ec_a4cf_3a85,
        ),
    ];
    for (kind, nullable, input, expected) in cases {
        let encoding = if matches!(kind, ScalarKind::I32) {
            Some(IntegerEncoding::Varint)
        } else {
            None
        };
        let s = schema(TypeKind::Scalar(kind), encoding, nullable);
        let key = s
            .validate(&input, InputFormat::XlangMessage, &ParseLimits::default())
            .unwrap()
            .into_key(KeyRequirement::Hash)
            .unwrap();
        assert_eq!(key.stable_hash().unwrap(), expected);
    }
}

#[test]
fn recursive_union_keys_at_256_levels_match_canonical_stream() {
    let node = TypeDescriptor {
        kind: TypeKind::User(UserIdentity::Numeric(103)),
        integer_encoding: None,
        nullable: false,
        track_ref: false,
    };
    let descriptor = SchemaDescriptor {
        encoding: RootEncoding {
            protocol_version: ProtocolVersion::Fory1_7,
            mode: SchemaMode::SameSchema {
                version_hash: false,
            },
            root_reference: RootReferenceMode::NullOnly,
        },
        key_semantics_version: KEY_SEMANTICS_VERSION,
        root: node.clone(),
        definitions: vec![UserDefinition {
            identity: UserIdentity::Numeric(103),
            kind: UserDefinitionKind::Union {
                variants: vec![
                    VariantDescriptor {
                        id: 9,
                        name: "End".into(),
                        payload: None,
                    },
                    VariantDescriptor {
                        id: 2,
                        name: "Next".into(),
                        payload: Some(node),
                    },
                ],
            },
        }],
    };
    let schema = Schema::compile(descriptor.clone(), &SchemaLimits::default()).unwrap();
    let independent = Arc::new(Schema::compile(descriptor, &SchemaLimits::default()).unwrap());
    let mut input = Vec::new();
    let mut expected = Vec::new();
    for _ in 0..256 {
        input.extend_from_slice(&[255, 34, 103, 2]);
        expected.extend_from_slice(&[18, 1]);
        expected.extend_from_slice(&1u64.to_le_bytes());
        expected.push(1);
    }
    input.extend_from_slice(&[255, 34, 103, 9, 253]);
    expected.extend_from_slice(&[18, 1]);
    expected.extend_from_slice(&0u64.to_le_bytes());
    expected.push(0);
    let borrowed = schema
        .validate(&input, InputFormat::RootValue, &ParseLimits::default())
        .unwrap()
        .into_key(KeyRequirement::HashAndOrdered)
        .unwrap();
    let owned = independent
        .validate_owned(
            bytes::Bytes::from(input.clone()),
            InputFormat::RootValue,
            &ParseLimits::default(),
        )
        .unwrap()
        .into_key(KeyRequirement::HashAndOrdered)
        .unwrap();
    assert!(borrowed.try_eq(&owned).unwrap());
    assert_eq!(borrowed.try_cmp(&owned).unwrap(), Ordering::Equal);
    assert_eq!(borrowed.stable_hash().unwrap(), expected_hash(&expected));
    assert_eq!(owned.stable_hash().unwrap(), expected_hash(&expected));
    let end = [255, 34, 103, 9, 253];
    let end = schema
        .validate(&end, InputFormat::RootValue, &ParseLimits::default())
        .unwrap()
        .into_key(KeyRequirement::HashAndOrdered)
        .unwrap();
    assert_eq!(end.try_cmp(&borrowed).unwrap(), Ordering::Less);
}

#[test]
fn scalar_hashes_match_canonical_numeric_bytes_not_wire_codecs() {
    let f = fory::Fory::builder().xlang(true).build();
    macro_rules! check {
        ($kind:ident, $enc:expr, $value:expr, $tag:expr, $raw:expr) => {{
            let s = schema(TypeKind::Scalar(ScalarKind::$kind), $enc, false);
            let input = f.serialize(&$value).unwrap();
            let key = s
                .validate(&input, InputFormat::XlangMessage, &ParseLimits::default())
                .unwrap()
                .into_key(KeyRequirement::Hash)
                .unwrap();
            let mut expected = vec![$tag, 1];
            expected.extend_from_slice(&$raw);
            assert_eq!(key.stable_hash().unwrap(), expected_hash(&expected));
        }};
    }
    check!(Bool, None, true, 1, [1]);
    check!(I8, None, i8::MIN, 2, i8::MIN.to_le_bytes());
    check!(I16, None, i16::MIN, 3, i16::MIN.to_le_bytes());
    check!(
        I32,
        Some(IntegerEncoding::Varint),
        i32::MIN,
        4,
        i32::MIN.to_le_bytes()
    );
    check!(
        I64,
        Some(IntegerEncoding::Varint),
        i64::MAX,
        5,
        i64::MAX.to_le_bytes()
    );
    check!(U8, None, u8::MAX, 6, [255]);
    check!(U16, None, u16::MAX, 7, u16::MAX.to_le_bytes());
    check!(
        U32,
        Some(IntegerEncoding::Varint),
        u32::MAX,
        8,
        u32::MAX.to_le_bytes()
    );
    check!(
        U64,
        Some(IntegerEncoding::Varint),
        u64::MAX,
        9,
        u64::MAX.to_le_bytes()
    );
    check!(F32, None, -0.0f32, 10, 0u32.to_le_bytes());
    check!(
        F32,
        None,
        f32::from_bits(0xffc0_1234),
        10,
        0x7fc0_0000u32.to_le_bytes()
    );
    check!(
        F64,
        None,
        f64::from_bits(0xfff8_0000_0000_1234),
        11,
        0x7ff8_0000_0000_0000u64.to_le_bytes()
    );
    for (encoding, id) in [
        (IntegerEncoding::Fixed, 6),
        (IntegerEncoding::Varint, 7),
        (IntegerEncoding::Tagged, 8),
    ] {
        let s = schema(TypeKind::Scalar(ScalarKind::I64), Some(encoding), false);
        let mut input = vec![255, id];
        let mut writer = fory::Writer::from_buffer(&mut input);
        match encoding {
            IntegerEncoding::Fixed => writer.write_i64(i64::MIN),
            IntegerEncoding::Varint => writer.write_var_i64(i64::MIN),
            IntegerEncoding::Tagged => writer.write_tagged_i64(i64::MIN),
        }
        let key = s
            .validate(&input, InputFormat::RootValue, &ParseLimits::default())
            .unwrap()
            .into_key(KeyRequirement::Hash)
            .unwrap();
        let mut expected = vec![5, 1];
        expected.extend_from_slice(&i64::MIN.to_le_bytes());
        assert_eq!(key.stable_hash().unwrap(), expected_hash(&expected));
    }
}

#[test]
fn container_hashes_match_lengths_tags_nulls_and_logical_order() {
    let child = TypeDescriptor {
        kind: TypeKind::Scalar(ScalarKind::I32),
        integer_encoding: Some(IntegerEncoding::Varint),
        nullable: true,
        track_ref: false,
    };
    let f = fory::Fory::builder().xlang(true).build();
    let input = f.serialize(&vec![None, Some(-129i32)]).unwrap();
    let mut expected = vec![14, 1];
    expected.extend_from_slice(&2u64.to_le_bytes());
    expected.extend_from_slice(&[4, 0, 4, 1]);
    expected.extend_from_slice(&(-129i32).to_le_bytes());
    let s = schema(TypeKind::List(Box::new(child.clone())), None, false);
    let key = s
        .validate(&input, InputFormat::XlangMessage, &ParseLimits::default())
        .unwrap()
        .into_key(KeyRequirement::Hash)
        .unwrap();
    assert_eq!(key.stable_hash().unwrap(), expected_hash(&expected));
    let array = schema(
        TypeKind::Array {
            element: Box::new(child),
            length: Some(2),
        },
        None,
        false,
    );
    let key = array
        .validate(&input, InputFormat::XlangMessage, &ParseLimits::default())
        .unwrap()
        .into_key(KeyRequirement::Hash)
        .unwrap();
    expected[0] = 15;
    assert_eq!(key.stable_hash().unwrap(), expected_hash(&expected));
    let s = schema(TypeKind::Scalar(ScalarKind::Binary), None, true);
    for (input, expected) in [
        (vec![253], vec![13, 0]),
        (vec![255, 41, 0], {
            let mut v = vec![13, 1];
            v.extend_from_slice(&0u64.to_le_bytes());
            v
        }),
        (vec![255, 41, 2, 0, 255], {
            let mut v = vec![13, 1];
            v.extend_from_slice(&2u64.to_le_bytes());
            v.extend_from_slice(&[0, 255]);
            v
        }),
    ] {
        let key = s
            .validate(&input, InputFormat::RootValue, &ParseLimits::default())
            .unwrap()
            .into_key(KeyRequirement::Hash)
            .unwrap();
        assert_eq!(key.stable_hash().unwrap(), expected_hash(&expected));
    }
}

#[test]
fn logical_hash_normalizes_equal_floats_and_survives_owned_backing() {
    let f = fory::Fory::builder().xlang(true).build();
    let s = Arc::new(schema(TypeKind::Scalar(ScalarKind::F64), None, false));
    for values in [
        [-0.0, 0.0],
        [
            f64::from_bits(0xfff8_0000_0000_1234),
            f64::from_bits(0x7ff8_0000_0000_5678),
        ],
    ] {
        let a = f.serialize(&values[0]).unwrap();
        let b = f.serialize(&values[1]).unwrap();
        let left = s
            .validate(&a, InputFormat::XlangMessage, &ParseLimits::default())
            .unwrap()
            .into_key(KeyRequirement::HashAndOrdered)
            .unwrap();
        let right = s
            .validate_owned(
                bytes::Bytes::from(b),
                InputFormat::XlangMessage,
                &ParseLimits::default(),
            )
            .unwrap()
            .into_key(KeyRequirement::HashAndOrdered)
            .unwrap();
        assert!(left.try_eq(&right).unwrap());
        assert_eq!(left.stable_hash().unwrap(), right.stable_hash().unwrap());
        assert_eq!(left.stable_hash().unwrap(), left.stable_hash().unwrap());
    }
}

#[test]
fn union_keys_order_by_logical_variant_position_then_payload() {
    let payload = TypeDescriptor {
        kind: TypeKind::Scalar(ScalarKind::I32),
        integer_encoding: Some(IntegerEncoding::Varint),
        nullable: true,
        track_ref: false,
    };
    let s = Schema::compile(
        SchemaDescriptor {
            encoding: RootEncoding {
                protocol_version: ProtocolVersion::Fory1_7,
                mode: SchemaMode::SameSchema {
                    version_hash: false,
                },
                root_reference: RootReferenceMode::NullOnly,
            },
            key_semantics_version: KEY_SEMANTICS_VERSION,
            root: TypeDescriptor {
                kind: TypeKind::User(UserIdentity::Numeric(101)),
                integer_encoding: None,
                nullable: false,
                track_ref: false,
            },
            definitions: vec![UserDefinition {
                identity: UserIdentity::Numeric(101),
                kind: UserDefinitionKind::Union {
                    variants: vec![
                        VariantDescriptor {
                            id: 9,
                            name: "Unit".into(),
                            payload: None,
                        },
                        VariantDescriptor {
                            id: 2,
                            name: "Value".into(),
                            payload: Some(payload),
                        },
                    ],
                },
            }],
        },
        &SchemaLimits::default(),
    )
    .unwrap();
    let mut values = vec![vec![255, 34, 101, 9, 253], vec![255, 34, 101, 2, 253]];
    for value in [-129, i32::MAX] {
        let mut input = vec![255, 34, 101, 2, 255, 5];
        fory::Writer::from_buffer(&mut input).write_var_i32(value);
        values.push(input);
    }
    let keys = values
        .iter()
        .map(|input| {
            s.validate(input, InputFormat::RootValue, &ParseLimits::default())
                .unwrap()
                .into_key(KeyRequirement::HashAndOrdered)
                .unwrap()
        })
        .collect::<Vec<_>>();
    for (index, key) in keys.iter().enumerate() {
        let mut expected = vec![18, 1];
        expected.extend_from_slice(&(u64::from(index != 0)).to_le_bytes());
        expected.push(u8::from(index != 0));
        if index == 1 {
            expected.extend_from_slice(&[4, 0]);
        }
        if index >= 2 {
            expected.extend_from_slice(&[4, 1]);
            expected.extend_from_slice(&([-129i32, i32::MAX][index - 2]).to_le_bytes());
        }
        assert_eq!(key.stable_hash().unwrap(), expected_hash(&expected));
    }
    for i in 0..keys.len() {
        for j in 0..keys.len() {
            assert_eq!(keys[i].try_cmp(&keys[j]).unwrap(), i.cmp(&j));
            assert_eq!(keys[i].try_eq(&keys[j]).unwrap(), i == j);
        }
    }
}

#[test]
fn scalar_key_order_is_logical_for_numbers_strings_binary_and_nulls() {
    let f = fory::Fory::builder().xlang(true).build();
    macro_rules! check {
        ($kind:ident, $encoding:expr, $values:expr) => {{
            let s = schema(TypeKind::Scalar(ScalarKind::$kind), $encoding, false);
            let inputs = $values.map(|v| f.serialize(&v).unwrap());
            let keys = inputs
                .iter()
                .map(|input| {
                    s.validate(input, InputFormat::XlangMessage, &ParseLimits::default())
                        .unwrap()
                        .into_key(KeyRequirement::HashAndOrdered)
                        .unwrap()
                })
                .collect::<Vec<_>>();
            for i in 0..keys.len() {
                for j in 0..keys.len() {
                    assert_eq!(keys[i].try_cmp(&keys[j]).unwrap(), i.cmp(&j));
                }
            }
        }};
    }
    check!(Bool, None, [false, true]);
    check!(I8, None, [i8::MIN, 0, i8::MAX]);
    check!(I16, None, [i16::MIN, 0, i16::MAX]);
    check!(
        I32,
        Some(IntegerEncoding::Varint),
        [i32::MIN, -129, 0, i32::MAX]
    );
    check!(
        I64,
        Some(IntegerEncoding::Varint),
        [i64::MIN, -129, 0, i64::MAX]
    );
    check!(U8, None, [0u8, 128, u8::MAX]);
    check!(U16, None, [0u16, 128, u16::MAX]);
    check!(U32, Some(IntegerEncoding::Varint), [0u32, 128, u32::MAX]);
    check!(U64, Some(IntegerEncoding::Varint), [0u64, 128, u64::MAX]);
    check!(String, None, ["", "a", "é", "中"].map(str::to_owned));
    let s = schema(TypeKind::Scalar(ScalarKind::Binary), None, true);
    let inputs = [
        vec![253],
        vec![255, 41, 0],
        vec![255, 41, 1, 0],
        vec![255, 41, 2, 0, 1],
    ];
    let keys = inputs
        .iter()
        .map(|input| {
            s.validate(input, InputFormat::RootValue, &ParseLimits::default())
                .unwrap()
                .into_key(KeyRequirement::Ordered)
                .unwrap()
        })
        .collect::<Vec<_>>();
    for i in 0..keys.len() {
        for j in 0..keys.len() {
            assert_eq!(keys[i].try_cmp(&keys[j]).unwrap(), i.cmp(&j));
        }
    }
}

#[test]
fn struct_keys_use_logical_order_and_reject_different_physical_contracts() {
    let scalar = TypeDescriptor {
        kind: TypeKind::Scalar(ScalarKind::Bool),
        integer_encoding: None,
        nullable: false,
        track_ref: false,
    };
    let descriptor = SchemaDescriptor {
        encoding: RootEncoding {
            protocol_version: ProtocolVersion::Fory1_7,
            mode: SchemaMode::SameSchema {
                version_hash: false,
            },
            root_reference: RootReferenceMode::NullOnly,
        },
        key_semantics_version: KEY_SEMANTICS_VERSION,
        root: TypeDescriptor {
            kind: TypeKind::User(UserIdentity::Numeric(100)),
            ..scalar.clone()
        },
        definitions: vec![UserDefinition {
            identity: UserIdentity::Numeric(100),
            kind: UserDefinitionKind::Struct {
                fields: vec![
                    FieldDescriptor {
                        id: None,
                        name: "a".into(),
                        ty: scalar.clone(),
                    },
                    FieldDescriptor {
                        id: None,
                        name: "b".into(),
                        ty: scalar,
                    },
                ],
                physical_field_order: vec![1, 0],
            },
        }],
    };
    let s = Schema::compile(descriptor.clone(), &SchemaLimits::default()).unwrap();
    let a = [255, 27, 100, 1, 0]; // logical (false,true)
    let b = [255, 27, 100, 0, 1]; // logical (true,false)
    let left = s
        .validate(&a, InputFormat::RootValue, &ParseLimits::default())
        .unwrap()
        .into_key(KeyRequirement::HashAndOrdered)
        .unwrap();
    let right = s
        .validate(&b, InputFormat::RootValue, &ParseLimits::default())
        .unwrap()
        .into_key(KeyRequirement::HashAndOrdered)
        .unwrap();
    assert_eq!(left.try_cmp(&right).unwrap(), Ordering::Less);
    let mut expected = vec![16, 1];
    expected.extend_from_slice(&2u64.to_le_bytes());
    expected.extend_from_slice(&[1, 1, 0, 1, 1, 1]);
    assert_eq!(left.stable_hash().unwrap(), expected_hash(&expected));
    expected[12] = 1;
    expected[15] = 0;
    assert_eq!(right.stable_hash().unwrap(), expected_hash(&expected));
    let mut changed = descriptor;
    if let UserDefinitionKind::Struct {
        physical_field_order,
        ..
    } = &mut changed.definitions[0].kind
    {
        *physical_field_order = vec![0, 1];
    }
    let other = Schema::compile(changed, &SchemaLimits::default()).unwrap();
    let other = other
        .validate(&a, InputFormat::RootValue, &ParseLimits::default())
        .unwrap()
        .into_key(KeyRequirement::Hash)
        .unwrap();
    assert!(matches!(
        left.try_eq(&other),
        Err(KeyError::SchemaMismatch(_))
    ));
    assert!(matches!(
        left.try_cmp(&other),
        Err(KeyError::UnsupportedKeyCapability(_))
    ));
    assert_eq!(left.try_cmp(&right).unwrap(), Ordering::Less);
}

fn schema(kind: TypeKind, encoding: Option<IntegerEncoding>, nullable: bool) -> Schema {
    Schema::compile(
        SchemaDescriptor {
            encoding: RootEncoding {
                protocol_version: ProtocolVersion::Fory1_7,
                mode: SchemaMode::Compatible,
                root_reference: RootReferenceMode::NullOnly,
            },
            key_semantics_version: KEY_SEMANTICS_VERSION,
            root: TypeDescriptor {
                kind,
                integer_encoding: encoding,
                nullable,
                track_ref: false,
            },
            definitions: vec![],
        },
        &SchemaLimits::default(),
    )
    .unwrap()
}

#[test]
fn logical_scalar_keys_compare_owned_borrowed_and_normalize_floats() {
    let f = fory::Fory::builder().xlang(true).build();
    let a = f.serialize(&-0.0f64).unwrap();
    let b = f.serialize(&0.0f64).unwrap();
    let s = schema(TypeKind::Scalar(ScalarKind::F64), None, false);
    let independent = Arc::new(schema(TypeKind::Scalar(ScalarKind::F64), None, false));
    let left = s
        .validate(&a, InputFormat::XlangMessage, &ParseLimits::default())
        .unwrap()
        .into_key(KeyRequirement::HashAndOrdered)
        .unwrap();
    let right = independent
        .validate_owned(
            bytes::Bytes::from(b),
            InputFormat::XlangMessage,
            &ParseLimits::default(),
        )
        .unwrap()
        .into_key(KeyRequirement::Ordered)
        .unwrap();
    assert!(left.try_eq(&right).unwrap());
    assert_eq!(right.try_cmp(&left).unwrap(), Ordering::Equal);
    let nan_a = f.serialize(&f64::from_bits(0xfff8_0000_0000_1234)).unwrap();
    let nan_b = f.serialize(&f64::from_bits(0x7ff8_0000_0000_5678)).unwrap();
    let infinity = f.serialize(&f64::INFINITY).unwrap();
    let key = |input| {
        s.validate(input, InputFormat::XlangMessage, &ParseLimits::default())
            .unwrap()
            .into_key(KeyRequirement::HashAndOrdered)
            .unwrap()
    };
    assert!(key(&nan_a).try_eq(&key(&nan_b)).unwrap());
    assert_eq!(
        key(&nan_a).try_cmp(&key(&infinity)).unwrap(),
        Ordering::Greater
    );
    let hash_only = s
        .validate(&a, InputFormat::XlangMessage, &ParseLimits::default())
        .unwrap()
        .into_key(KeyRequirement::Hash)
        .unwrap();
    assert!(hash_only.try_eq(&right).unwrap());
    assert!(matches!(
        hash_only.try_cmp(&right),
        Err(KeyError::UnsupportedKeyCapability(_))
    ));
    assert!(matches!(
        right.stable_hash(),
        Err(KeyError::UnsupportedKeyCapability(_))
    ));
    assert!(left.try_eq(&right).unwrap());
}

#[test]
fn logical_lists_are_lexicographic_and_schema_mismatch_is_an_error() {
    let f = fory::Fory::builder().xlang(true).build();
    let child = TypeDescriptor {
        kind: TypeKind::Scalar(ScalarKind::I32),
        integer_encoding: Some(IntegerEncoding::Varint),
        nullable: true,
        track_ref: false,
    };
    let s = schema(TypeKind::List(Box::new(child)), None, false);
    let inputs = [
        vec![None],
        vec![Some(-1)],
        vec![Some(-1), Some(0)],
        vec![Some(0)],
    ]
    .map(|v| f.serialize(&v).unwrap());
    let keys = inputs
        .iter()
        .map(|input| {
            s.validate(input, InputFormat::XlangMessage, &ParseLimits::default())
                .unwrap()
                .into_key(KeyRequirement::HashAndOrdered)
                .unwrap()
        })
        .collect::<Vec<_>>();
    for i in 0..keys.len() {
        for j in 0..keys.len() {
            assert_eq!(keys[i].try_cmp(&keys[j]).unwrap(), i.cmp(&j));
            assert_eq!(keys[i].try_eq(&keys[j]).unwrap(), i == j);
        }
    }
    let other_schema = schema(TypeKind::Scalar(ScalarKind::Bool), None, false);
    let input = f.serialize(&true).unwrap();
    let other = other_schema
        .validate(&input, InputFormat::XlangMessage, &ParseLimits::default())
        .unwrap()
        .into_key(KeyRequirement::Ordered)
        .unwrap();
    assert!(matches!(
        keys[0].try_eq(&other),
        Err(KeyError::SchemaMismatch(_))
    ));
    assert!(matches!(
        keys[0].try_cmp(&other),
        Err(KeyError::SchemaMismatch(_))
    ));
    let time = schema(TypeKind::Scalar(ScalarKind::Date), None, false);
    let input = f.serialize(&fory::Date::UNIX_EPOCH).unwrap();
    assert!(matches!(
        time.validate(&input, InputFormat::XlangMessage, &ParseLimits::default())
            .unwrap()
            .into_key(KeyRequirement::Hash),
        Err(KeyError::UnsupportedKeyCapability(_))
    ));
}
