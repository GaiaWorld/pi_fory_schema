use pi_fory_schema::*;

#[test]
fn explicit_frames_preserve_map_child_order_and_strict_nested_properties() {
    let child = r#"{"kind":{"scalar":"bool","kind":"scalar"},"integer_encoding":null,"track_ref":false,"nullable":false}"#;
    let root = format!(
        r#"{{"kind":{{"value":{child},"key":{child},"kind":"map"}},"nullable":true,"integer_encoding":null,"track_ref":false}}"#
    );
    let json=simple().replace(r#"{"nullable":false,"track_ref":false,"integer_encoding":null,"kind":{"kind":"scalar","scalar":"bool"}}"#,&root);
    let schema = import_json(&json, &SchemaLimits::default()).unwrap();
    assert!(matches!(
        schema.check_key_support(KeyRequirement::Ordered),
        Err(SchemaError::UnsupportedKeyCapability(_))
    ));
    for (from, to) in [
        (
            "\"scalar\":\"bool\",\"kind\":\"scalar\"",
            "\"scalar\":\"bool\",\"kind\":\"scalar\",\"kind\":\"scalar\"",
        ),
        ("\"track_ref\":false,", ""),
        ("\"kind\":\"map\"", "\"kind\":\"map\",\"unknown\":{}"),
        (
            "\"integer_encoding\":null",
            "\"integer_encoding\":null,\"integer_encoding\":null",
        ),
    ] {
        assert!(matches!(
            import_json(&json.replace(from, to), &SchemaLimits::default()),
            Err(SchemaError::InvalidSchema(_))
        ));
    }
    assert!(matches!(
        import_json(
            &json,
            &SchemaLimits {
                max_type_nodes: 2,
                ..SchemaLimits::default()
            }
        ),
        Err(SchemaError::ResourceLimit(_))
    ));
}

#[test]
fn regression_compile_location_is_preserved() {
    let mut d = users();
    let UserDefinitionKind::Struct {
        physical_field_order,
        ..
    } = &mut d.definitions[0].kind
    else {
        panic!("struct")
    };
    *physical_field_order = vec![0, 0];
    match import_json(&export_json(&d).unwrap(), &SchemaLimits::default()) {
        Err(SchemaError::InvalidSchema(location)) => {
            assert_eq!(location.descriptor_index, Some(0));
            assert_eq!(location.byte_offset, None);
        }
        _ => panic!("expected compile error"),
    }
}
#[test]
fn regression_long_version_strings() {
    for token in ["pi_fory_schema", "fory1_7"] {
        let unknown = "x".repeat(10000);
        assert!(matches!(
            import_json(&simple().replace(token, &unknown), &SchemaLimits::default()),
            Err(SchemaError::UnsupportedVersion(_))
        ));
        for suffix in [r"\uD800", r"\x", "\n"] {
            assert!(matches!(
                import_json(
                    &simple().replace(token, &(unknown.clone() + suffix)),
                    &SchemaLimits::default()
                ),
                Err(SchemaError::MalformedInput(_))
            ));
        }
    }
    import_json(
        &simple()
            .replace("pi_fory_schema", r"pi_fory_\u0073chema")
            .replace("fory1_7", r"fory1_\u0037"),
        &SchemaLimits::default(),
    )
    .unwrap();
}
#[test]
fn regression_deep_import_uses_explicit_frames() {
    let depth = 2000;
    let mut root=r#"{"nullable":false,"track_ref":false,"integer_encoding":null,"kind":{"kind":"scalar","scalar":"bool"}}"#.to_owned();
    for i in 0..depth {
        root = if i % 2 == 0 {
            format!(
                r#"{{"kind":{{"element":{root},"kind":"list"}},"integer_encoding":null,"track_ref":false,"nullable":false}}"#
            )
        } else {
            format!(
                r#"{{"nullable":false,"track_ref":false,"integer_encoding":null,"kind":{{"kind":"array","length":null,"element":{root}}}}}"#
            )
        };
    }
    let json=simple().replace(r#"{"nullable":false,"track_ref":false,"integer_encoding":null,"kind":{"kind":"scalar","scalar":"bool"}}"#,&root);
    let limits = SchemaLimits {
        max_json_depth: 2 * depth + 3,
        max_type_nodes: depth + 1,
        ..SchemaLimits::default()
    };
    let schema = import_json(&json, &limits).unwrap();
    schema.check_key_support(KeyRequirement::Ordered).unwrap();
    // The public API cannot extract the owned descriptor for iterative destruction.
    // This test isolates parsing/traversal, not recursive owned-drop guarantees.
    std::mem::forget(schema);
    assert!(matches!(
        import_json(
            &json,
            &SchemaLimits {
                max_json_depth: 2 * depth + 2,
                ..limits
            }
        ),
        Err(SchemaError::ResourceLimit(_))
    ));
}

#[test]
fn incremental_budgets_include_duplicate_and_unreachable_definitions() {
    let mut d = users();
    d.definitions.push(d.definitions[0].clone());
    let json = export_json(&d).unwrap();
    let limits = SchemaLimits {
        max_definitions: 3,
        ..SchemaLimits::default()
    };
    assert!(matches!(
        import_json(&json, &limits),
        Err(SchemaError::ResourceLimit(_))
    ));
    assert!(matches!(
        import_json(&json, &SchemaLimits::default()),
        Err(SchemaError::InvalidSchema(_))
    ));
    let mut d = descriptor(scalar(ScalarKind::Bool));
    d.definitions.push(UserDefinition {
        identity: UserIdentity::Numeric(9),
        kind: UserDefinitionKind::Enum {
            variants: vec![VariantDescriptor {
                id: 1,
                name: "x".into(),
                payload: None,
            }],
        },
    });
    let json = export_json(&d).unwrap();
    assert!(matches!(
        import_json(
            &json,
            &SchemaLimits {
                max_members: 0,
                ..SchemaLimits::default()
            }
        ),
        Err(SchemaError::ResourceLimit(_))
    ));
    assert!(matches!(
        import_json(&json, &SchemaLimits::default()),
        Err(SchemaError::InvalidSchema(_))
    ));
}
#[test]
fn metadata_decoded_utf8_is_charged_per_occurrence() {
    let identity = UserIdentity::Name {
        namespace: "".into(),
        type_name: "😀".into(),
    };
    let mut d = descriptor(TypeDescriptor {
        kind: TypeKind::User(identity.clone()),
        ..scalar(ScalarKind::Bool)
    });
    d.definitions.push(UserDefinition {
        identity,
        kind: UserDefinitionKind::Struct {
            fields: vec![],
            physical_field_order: vec![],
        },
    });
    let json = export_json(&d).unwrap().replace("😀", r"\uD83D\uDE00");
    import_json(
        &json,
        &SchemaLimits {
            max_metadata_bytes: 8,
            ..SchemaLimits::default()
        },
    )
    .unwrap();
    for max in [0, 1, 4, 7] {
        match import_json(
            &json,
            &SchemaLimits {
                max_metadata_bytes: max,
                ..SchemaLimits::default()
            },
        ) {
            Err(SchemaError::ResourceLimit(l)) => {
                assert!(l.byte_offset.is_some());
                assert!(l.descriptor_index.is_none());
            }
            _ => panic!("{max}"),
        }
    }
}
#[test]
fn all_zero_limits_and_depth_exact_boundary() {
    let json = simple();
    for limits in [
        SchemaLimits {
            max_document_bytes: 0,
            ..SchemaLimits::default()
        },
        SchemaLimits {
            max_json_depth: 0,
            ..SchemaLimits::default()
        },
        SchemaLimits {
            max_type_nodes: 0,
            ..SchemaLimits::default()
        },
    ] {
        assert!(matches!(
            import_json(&json, &limits),
            Err(SchemaError::ResourceLimit(_))
        ));
    }
    let json = export_json(&users()).unwrap();
    import_json(
        &json,
        &SchemaLimits {
            max_json_depth: 9,
            ..SchemaLimits::default()
        },
    )
    .unwrap();
    assert!(matches!(
        import_json(
            &json,
            &SchemaLimits {
                max_json_depth: 8,
                ..SchemaLimits::default()
            }
        ),
        Err(SchemaError::ResourceLimit(_))
    ));
    for limits in [
        SchemaLimits {
            max_definitions: 0,
            ..SchemaLimits::default()
        },
        SchemaLimits {
            max_members: 0,
            ..SchemaLimits::default()
        },
        SchemaLimits {
            max_metadata_bytes: 0,
            ..SchemaLimits::default()
        },
    ] {
        assert!(matches!(
            import_json(&json, &limits),
            Err(SchemaError::ResourceLimit(_))
        ));
    }
}
#[test]
fn nested_field_sets_numeric_bounds_and_missing_explicit_nulls() {
    let json = export_json(&users()).unwrap();
    for (from, to) in [
        ("\"id\":4294967295", "\"id\":4294967296"),
        ("\"id\":4294967295", "\"id\":-0"),
        ("\"id\":4294967295", "\"id\":1e0"),
        ("\"id\":null,", ""),
        ("\"payload\":null", "\"other\":null"),
        ("\"name\":\"a\"", "\"name\":\"a\",\"name\":\"a\""),
        (
            "\"kind\":\"numeric\",\"id\":1",
            "\"kind\":\"numeric\",\"id\":1,\"namespace\":\"x\"",
        ),
        (
            "\"physical_field_order\":[1,0]",
            "\"physical_field_order\":[18446744073709551616,0]",
        ),
    ] {
        assert!(
            matches!(
                import_json(&json.replace(from, to), &SchemaLimits::default()),
                Err(SchemaError::InvalidSchema(_))
            ),
            "{from}"
        );
    }
    for (from, to) in [
        ("\"id\":4294967295", "\"id\":01"),
        (
            "\"physical_field_order\":[1,0]",
            "\"physical_field_order\":[1,0,]",
        ),
    ] {
        assert!(matches!(
            import_json(&json.replace(from, to), &SchemaLimits::default()),
            Err(SchemaError::MalformedInput(_))
        ));
    }
    let json = export_json(&descriptor(TypeDescriptor {
        kind: TypeKind::Array {
            element: Box::new(scalar(ScalarKind::Bool)),
            length: None,
        },
        ..scalar(ScalarKind::Bool)
    }))
    .unwrap();
    assert!(matches!(
        import_json(
            &json.replace(",\"length\":null", ""),
            &SchemaLimits::default()
        ),
        Err(SchemaError::InvalidSchema(_))
    ));
}
#[test]
fn escaped_keys_whitespace_and_syntax_failures() {
    let json = simple().replace("\"nullable\"", r#""\u006eullable""#);
    import_json(
        &(" \r\n\t".to_owned() + &json + " \r\n\t"),
        &SchemaLimits::default(),
    )
    .unwrap();
    for to in ["truex", "tru", "1.", "1e+", "+1"] {
        let json = simple().replace("\"format_version\":1", &format!("\"format_version\":{to}"));
        assert!(
            import_json(&json, &SchemaLimits::default()).is_err(),
            "{to}"
        );
    }
    for to in ["fals", "false/*x*/", "false\u{b}"] {
        assert!(matches!(
            import_json(
                &simple().replace("\"nullable\":false", &format!("\"nullable\":{to}")),
                &SchemaLimits::default()
            ),
            Err(SchemaError::MalformedInput(_))
        ));
    }
    let json = simple().replace(
        "\"nullable\":false",
        r#""nullable":false,"\u006eullable":false"#,
    );
    assert!(matches!(
        import_json(&json, &SchemaLimits::default()),
        Err(SchemaError::InvalidSchema(_))
    ));
}

fn scalar(kind: ScalarKind) -> TypeDescriptor {
    TypeDescriptor {
        nullable: false,
        track_ref: false,
        integer_encoding: match kind {
            ScalarKind::I32 | ScalarKind::U32 | ScalarKind::I64 | ScalarKind::U64 => {
                Some(IntegerEncoding::Fixed)
            }
            _ => None,
        },
        kind: TypeKind::Scalar(kind),
    }
}
fn descriptor(root: TypeDescriptor) -> SchemaDescriptor {
    SchemaDescriptor {
        encoding: RootEncoding {
            protocol_version: ProtocolVersion::Fory1_7,
            mode: SchemaMode::Compatible,
            root_reference: RootReferenceMode::NullOnly,
        },
        key_semantics_version: 1,
        root,
        definitions: vec![],
    }
}
fn simple() -> String {
    r#"{"format":"pi_fory_schema","format_version":1,"key_semantics_version":1,"encoding":{"protocol_version":"fory1_7","mode":{"kind":"compatible"},"root_reference":"null_only"},"root":{"nullable":false,"track_ref":false,"integer_encoding":null,"kind":{"kind":"scalar","scalar":"bool"}},"definitions":[]}"#.into()
}
#[test]
fn simple_exact() {
    assert_eq!(
        export_json(&descriptor(scalar(ScalarKind::Bool))).unwrap(),
        simple()
    );
    import_json(&simple(), &SchemaLimits::default())
        .unwrap()
        .check_key_support(KeyRequirement::Ordered)
        .unwrap();
}
#[test]
fn all_scalars_encodings_and_containers() {
    for kind in [
        ScalarKind::Bool,
        ScalarKind::I8,
        ScalarKind::I16,
        ScalarKind::I32,
        ScalarKind::I64,
        ScalarKind::U8,
        ScalarKind::U16,
        ScalarKind::U32,
        ScalarKind::U64,
        ScalarKind::F32,
        ScalarKind::F64,
        ScalarKind::String,
        ScalarKind::Binary,
        ScalarKind::Date,
        ScalarKind::Timestamp,
        ScalarKind::Duration,
    ] {
        let json = export_json(&descriptor(scalar(kind))).unwrap();
        let schema = import_json(&json, &SchemaLimits::default()).unwrap();
        assert_eq!(
            schema.check_key_support(KeyRequirement::Ordered).is_ok(),
            !matches!(
                kind,
                ScalarKind::Date | ScalarKind::Timestamp | ScalarKind::Duration
            )
        );
    }
    for (encoding, tag) in [
        (IntegerEncoding::Fixed, "fixed"),
        (IntegerEncoding::Varint, "varint"),
        (IntegerEncoding::Tagged, "tagged"),
    ] {
        let mut root = scalar(ScalarKind::U64);
        root.integer_encoding = Some(encoding);
        let json = export_json(&descriptor(root)).unwrap();
        assert!(json.contains(&format!("\"integer_encoding\":\"{tag}\"")));
        import_json(&json, &SchemaLimits::default()).unwrap();
    }
    for kind in [
        TypeKind::List(Box::new(scalar(ScalarKind::Bool))),
        TypeKind::Set(Box::new(scalar(ScalarKind::Bool))),
        TypeKind::Array {
            element: Box::new(scalar(ScalarKind::U32)),
            length: Some(u64::MAX),
        },
        TypeKind::Array {
            element: Box::new(scalar(ScalarKind::Bool)),
            length: None,
        },
        TypeKind::Map {
            key: Box::new(scalar(ScalarKind::String)),
            value: Box::new(scalar(ScalarKind::I64)),
        },
    ] {
        let json = export_json(&descriptor(TypeDescriptor {
            kind,
            ..scalar(ScalarKind::Bool)
        }))
        .unwrap();
        import_json(&json, &SchemaLimits::default()).unwrap();
    }
}
fn users() -> SchemaDescriptor {
    let user = |id| TypeDescriptor {
        kind: TypeKind::User(UserIdentity::Numeric(id)),
        ..scalar(ScalarKind::Bool)
    };
    let mut d = descriptor(user(1));
    d.definitions = vec![
        UserDefinition {
            identity: UserIdentity::Numeric(1),
            kind: UserDefinitionKind::Struct {
                fields: vec![
                    FieldDescriptor {
                        id: None,
                        name: "z\"\\\n😀".into(),
                        ty: user(2),
                    },
                    FieldDescriptor {
                        id: Some(u32::MAX),
                        name: "a".into(),
                        ty: user(3),
                    },
                ],
                physical_field_order: vec![1, 0],
            },
        },
        UserDefinition {
            identity: UserIdentity::Numeric(2),
            kind: UserDefinitionKind::Enum {
                variants: vec![VariantDescriptor {
                    id: u32::MAX,
                    name: "unit".into(),
                    payload: None,
                }],
            },
        },
        UserDefinition {
            identity: UserIdentity::Numeric(3),
            kind: UserDefinitionKind::Union {
                variants: vec![
                    VariantDescriptor {
                        id: 0,
                        name: "back".into(),
                        payload: Some(user(1)),
                    },
                    VariantDescriptor {
                        id: 1,
                        name: "empty".into(),
                        payload: None,
                    },
                ],
            },
        },
    ];
    d
}
#[test]
fn definitions_recursion_and_escaping() {
    let json = export_json(&users()).unwrap();
    assert!(json.contains("\"physical_field_order\":[1,0]"));
    assert!(!json.contains("layout"));
    assert!(json.contains("\\n"));
    assert!(json.contains("\\\""));
    import_json(&json, &SchemaLimits::default())
        .unwrap()
        .check_key_support(KeyRequirement::Ordered)
        .unwrap();
}
#[test]
fn modes() {
    for mode in [
        SchemaMode::Compatible,
        SchemaMode::SameSchema {
            version_hash: false,
        },
        SchemaMode::SameSchema { version_hash: true },
    ] {
        for reference in [RootReferenceMode::Tracking, RootReferenceMode::NullOnly] {
            let mut d = descriptor(scalar(ScalarKind::Bool));
            d.encoding.mode = mode;
            d.encoding.root_reference = reference;
            import_json(&export_json(&d).unwrap(), &SchemaLimits::default()).unwrap();
        }
    }
}
#[test]
fn properties_and_values() {
    for (from, to) in [
        ("\"nullable\":false,", ""),
        ("\"integer_encoding\":null,", ""),
        (
            "\"nullable\":false",
            "\"nullable\":false,\"nullable\":false",
        ),
        ("\"scalar\":\"bool\"", "\"scalar\":\"bool\",\"extra\":[]"),
        (
            "\"kind\":\"compatible\"",
            "\"kind\":\"compatible\",\"version_hash\":false",
        ),
        ("\"definitions\":[]", "\"definitions\":[],\"unknown\":{}"),
        (
            "\"integer_encoding\":null",
            "\"integer_encoding\":\"fixed\"",
        ),
        ("\"scalar\":\"bool\"", "\"scalar\":\"i32\""),
        ("\"scalar\":\"bool\"", "\"scalar\":\"unknown\""),
        ("\"nullable\":false", "\"nullable\":null"),
    ] {
        assert!(
            matches!(
                import_json(&simple().replace(from, to), &SchemaLimits::default()),
                Err(SchemaError::InvalidSchema(_))
            ),
            "{from}"
        );
    }
    let json = export_json(&users()).unwrap();
    for to in ["\"layout\":null", "\"physical_field_order\":[0,0]"] {
        assert!(matches!(
            import_json(
                &json.replace("\"physical_field_order\":[1,0]", to),
                &SchemaLimits::default()
            ),
            Err(SchemaError::InvalidSchema(_))
        ));
    }
}
#[test]
fn syntax() {
    for json in ["", "{}{}", "/*x*/{}", "{", "{garbage"] {
        assert!(
            matches!(
                import_json(json, &SchemaLimits::default()),
                Err(SchemaError::MalformedInput(_))
            ),
            "{json}"
        );
    }
    for suffix in [",", "{}", "//comment", " x"] {
        assert!(matches!(
            import_json(&(simple() + suffix), &SchemaLimits::default()),
            Err(SchemaError::MalformedInput(_))
        ));
    }
    for to in ["\"definitions\":[,]", "\"definitions\":[],"] {
        assert!(matches!(
            import_json(
                &simple().replace("\"definitions\":[]", to),
                &SchemaLimits::default()
            ),
            Err(SchemaError::MalformedInput(_))
        ));
    }
}
#[test]
fn integer_lexemes_and_versions() {
    for number in [
        "-0",
        "-1",
        "1.0",
        "1e0",
        "4294967296",
        "18446744073709551616",
    ] {
        assert!(
            matches!(
                import_json(
                    &simple().replace(
                        "\"format_version\":1",
                        &format!("\"format_version\":{number}")
                    ),
                    &SchemaLimits::default()
                ),
                Err(SchemaError::InvalidSchema(_))
            ),
            "{number}"
        );
    }
    assert!(matches!(
        import_json(
            &simple().replace("\"format_version\":1", "\"format_version\":01"),
            &SchemaLimits::default()
        ),
        Err(SchemaError::MalformedInput(_))
    ));
    for (from, to) in [
        ("pi_fory_schema", "other"),
        ("\"format_version\":1", "\"format_version\":2"),
        ("\"key_semantics_version\":1", "\"key_semantics_version\":0"),
        ("fory1_7", "fory2_0"),
    ] {
        assert!(matches!(
            import_json(&simple().replace(from, to), &SchemaLimits::default()),
            Err(SchemaError::UnsupportedVersion(_))
        ));
    }
}
#[test]
fn unicode() {
    let identity = UserIdentity::Name {
        namespace: "".into(),
        type_name: "😀".into(),
    };
    let mut d = descriptor(TypeDescriptor {
        kind: TypeKind::User(identity.clone()),
        ..scalar(ScalarKind::Bool)
    });
    d.definitions.push(UserDefinition {
        identity,
        kind: UserDefinitionKind::Struct {
            fields: vec![],
            physical_field_order: vec![],
        },
    });
    let json = export_json(&d).unwrap();
    import_json(
        &json.replace("😀", r"\uD83D\uDE00"),
        &SchemaLimits::default(),
    )
    .unwrap();
    for bad in [
        r"\uD83D",
        r"\uDE00",
        r"\uD83D\u0041",
        r"\uZZZZ",
        r"\x20",
        "\n",
    ] {
        match import_json(&json.replace("😀", bad), &SchemaLimits::default()) {
            Err(SchemaError::MalformedInput(l)) => {
                assert!(l.byte_offset.is_some());
                assert!(l.descriptor_index.is_none());
            }
            _ => panic!("{bad}"),
        }
    }
}
#[test]
fn budgets() {
    let json = simple();
    let exact = SchemaLimits {
        max_document_bytes: json.len(),
        max_json_depth: 3,
        max_definitions: 0,
        max_type_nodes: 1,
        max_members: 0,
        max_metadata_bytes: 0,
    };
    import_json(&json, &exact).unwrap();
    for i in 0..3 {
        let mut l = exact.clone();
        match i {
            0 => l.max_document_bytes -= 1,
            1 => l.max_json_depth -= 1,
            _ => l.max_type_nodes = 0,
        }
        assert!(matches!(
            import_json(&json, &l),
            Err(SchemaError::ResourceLimit(_))
        ));
    }
    let json = export_json(&users()).unwrap();
    let exact = SchemaLimits {
        max_document_bytes: json.len(),
        max_json_depth: 9,
        max_definitions: 3,
        max_type_nodes: 4,
        max_members: 5,
        max_metadata_bytes: 22,
    };
    import_json(&json, &exact).unwrap();
    for i in 0..4 {
        let mut l = exact.clone();
        match i {
            0 => l.max_definitions = 2,
            1 => l.max_type_nodes = 3,
            2 => l.max_members = 4,
            _ => l.max_metadata_bytes = 21,
        }
        assert!(
            matches!(import_json(&json, &l), Err(SchemaError::ResourceLimit(_))),
            "{i}"
        );
    }
}
#[test]
fn export_without_compile_and_deep_tree() {
    let mut d = descriptor(scalar(ScalarKind::Bool));
    d.key_semantics_version = 99;
    d.root.integer_encoding = Some(IntegerEncoding::Tagged);
    assert!(
        export_json(&d)
            .unwrap()
            .contains("\"key_semantics_version\":99")
    );
    let mut root = scalar(ScalarKind::Bool);
    for _ in 0..2000 {
        root = TypeDescriptor {
            kind: TypeKind::List(Box::new(root)),
            ..scalar(ScalarKind::Bool)
        }
    }
    let d = descriptor(root);
    assert!(export_json(&d).unwrap().len() > 100000);
    let mut root = d.root;
    while let TypeKind::List(child) = root.kind {
        root = *child;
    }
}
