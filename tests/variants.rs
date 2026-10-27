use pi_fory_schema::*;

#[derive(Debug, PartialEq, fory::ForyEnum)]
enum Color {
    #[fory(default)]
    Red,
    Blue,
}

#[derive(Debug, PartialEq, fory::ForyUnion)]
enum Choice {
    #[fory(id = 9, default)]
    Unit,
    #[fory(id = 3)]
    Count(i32),
    #[fory(id = 17)]
    Maybe(Option<String>),
    #[fory(unknown)]
    Unknown(fory::UnknownCase),
}

#[derive(Debug, PartialEq, fory::ForyStruct)]
struct Holder {
    color: Color,
    choice: Choice,
}

#[derive(Debug, PartialEq, fory::ForyStruct)]
struct Item {
    flag: bool,
}

#[derive(Debug, PartialEq, fory::ForyUnion)]
enum SharedPayload {
    #[fory(id = 1, default)]
    Unit,
    #[fory(id = 2)]
    Item(std::rc::Rc<Item>),
    #[fory(id = 3)]
    Text(std::rc::Rc<String>),
    #[fory(unknown)]
    Unknown(fory::UnknownCase),
}

#[derive(Debug, PartialEq, fory::ForyUnion)]
enum SharedContent {
    #[fory(id = 0, default)]
    Unit,
    #[fory(id = 1)]
    Text(std::rc::Rc<String>),
    #[fory(id = 2)]
    Binary(#[fory(bytes)] std::rc::Rc<Vec<u8>>),
    #[fory(unknown)]
    Unknown(fory::UnknownCase),
}

#[test]
fn validated_content_reference_edges_never_decode_the_following_unit() {
    for compatible in [false, true] {
        let mut f = fory::Fory::builder()
            .xlang(true)
            .compatible(compatible)
            .check_struct_version(false)
            .build();
        f.register_union::<SharedContent>(491).unwrap();
        let text = std::rc::Rc::new("shared text".to_owned());
        let binary = std::rc::Rc::new(vec![9u8, 8, 7]);
        let values = vec![
            SharedContent::Text(std::rc::Rc::clone(&text)),
            SharedContent::Text(text),
            SharedContent::Unit,
            SharedContent::Binary(std::rc::Rc::clone(&binary)),
            SharedContent::Binary(binary),
            SharedContent::Unit,
        ];
        let input = f.serialize(&values).unwrap();
        let decoded = f.deserialize::<Vec<SharedContent>>(&input).unwrap();
        assert_eq!(decoded, values);
        let mut text = ty(TypeKind::Scalar(ScalarKind::String), false);
        text.track_ref = true;
        let mut binary = ty(TypeKind::Scalar(ScalarKind::Binary), false);
        binary.track_ref = true;
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
                    root_reference: RootReferenceMode::Tracking,
                },
                key_semantics_version: KEY_SEMANTICS_VERSION,
                root: ty(
                    TypeKind::List(Box::new(ty(
                        TypeKind::User(UserIdentity::Numeric(491)),
                        false,
                    ))),
                    false,
                ),
                definitions: vec![UserDefinition {
                    identity: UserIdentity::Numeric(491),
                    kind: UserDefinitionKind::Union {
                        variants: vec![
                            VariantDescriptor {
                                id: 0,
                                name: "Unit".into(),
                                payload: None,
                            },
                            VariantDescriptor {
                                id: 1,
                                name: "Text".into(),
                                payload: Some(text),
                            },
                            VariantDescriptor {
                                id: 2,
                                name: "Binary".into(),
                                payload: Some(binary),
                            },
                        ],
                    },
                }],
            },
            &SchemaLimits::default(),
        )
        .unwrap();
        let view = schema
            .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
            .unwrap();
        let root = view.root().unwrap();
        for (index, binary) in [(1, false), (4, true)] {
            let node = root.element_at(index).unwrap();
            let edge = node.variant_payload().unwrap().unwrap();
            let id = edge.reference_id().unwrap().unwrap();
            let owned = edge.to_owned().unwrap();
            if binary {
                assert!(matches!(edge.read_binary(), Err(DataError::AccessError(_))));
            } else {
                assert!(matches!(edge.read_string(), Err(DataError::AccessError(_))));
            }
            if binary {
                assert!(matches!(
                    owned.as_view().unwrap().read_binary(),
                    Err(DataError::AccessError(_))
                ));
                assert_eq!(
                    edge.resolve_reference(&id)
                        .unwrap()
                        .read_binary()
                        .unwrap()
                        .as_bytes(),
                    &[9, 8, 7]
                );
            } else {
                assert!(matches!(
                    owned.as_view().unwrap().read_string(),
                    Err(DataError::AccessError(_))
                ));
                assert_eq!(
                    edge.resolve_reference(&id)
                        .unwrap()
                        .read_string()
                        .unwrap()
                        .as_str(),
                    "shared text"
                );
            }
            assert_eq!(root.element_at(index + 1).unwrap().variant().unwrap().id, 0);
        }
        drop(root);
        let validated = view.validate().unwrap();
        let root = validated.root().unwrap();
        for (index, binary) in [(4, true), (1, false)] {
            let node = root.element_at(index).unwrap();
            let edge = node.variant_payload().unwrap().unwrap();
            let id = edge.reference_id().unwrap().unwrap();
            let owned = edge.to_owned().unwrap();
            if binary {
                let direct = edge.read_binary().map(|value| value.as_bytes().to_vec());
                assert!(
                    matches!(direct, Err(DataError::AccessError(_))),
                    "binary Ref decoded following node: {direct:?}"
                );
                assert_eq!(
                    edge.resolve_reference(&id)
                        .unwrap()
                        .read_binary()
                        .unwrap()
                        .as_bytes(),
                    &[9, 8, 7]
                );
                assert!(matches!(
                    owned.as_view().unwrap().read_binary(),
                    Err(DataError::AccessError(_))
                ));
            } else {
                let direct = edge.read_string().map(|value| value.as_str().to_owned());
                assert!(
                    matches!(direct, Err(DataError::AccessError(_))),
                    "string Ref decoded following node: {direct:?}"
                );
                assert_eq!(
                    edge.resolve_reference(&id)
                        .unwrap()
                        .read_string()
                        .unwrap()
                        .as_str(),
                    "shared text"
                );
                assert!(matches!(
                    owned.as_view().unwrap().read_string(),
                    Err(DataError::AccessError(_))
                ));
            }
            assert_eq!(root.element_at(index + 1).unwrap().variant().unwrap().id, 0);
            assert_eq!(root.len().unwrap(), 6);
        }
    }
}

#[test]
fn union_shared_struct_and_string_payloads_register_once_and_resolve() {
    for compatible in [false, true] {
        let mut f = fory::Fory::builder()
            .xlang(true)
            .compatible(compatible)
            .check_struct_version(false)
            .build();
        f.register::<Item>(303).unwrap();
        f.register_union::<SharedPayload>(305).unwrap();
        let item = std::rc::Rc::new(Item { flag: true });
        let text = std::rc::Rc::new("payload".to_owned());
        let values = vec![
            SharedPayload::Item(std::rc::Rc::clone(&item)),
            SharedPayload::Item(item),
            SharedPayload::Text(std::rc::Rc::clone(&text)),
            SharedPayload::Text(text),
            SharedPayload::Unit,
        ];
        let input = f.serialize(&values).unwrap();
        let decoded = f.deserialize::<Vec<SharedPayload>>(&input).unwrap();
        let (SharedPayload::Item(a), SharedPayload::Item(b)) = (&decoded[0], &decoded[1]) else {
            panic!("wrong variants")
        };
        assert!(std::rc::Rc::ptr_eq(a, b));
        let mut item = ty(TypeKind::User(UserIdentity::Numeric(303)), false);
        item.track_ref = true;
        let mut text = ty(TypeKind::Scalar(ScalarKind::String), false);
        text.track_ref = true;
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
                    root_reference: RootReferenceMode::Tracking,
                },
                key_semantics_version: KEY_SEMANTICS_VERSION,
                root: ty(
                    TypeKind::List(Box::new(ty(
                        TypeKind::User(UserIdentity::Numeric(305)),
                        false,
                    ))),
                    false,
                ),
                definitions: vec![
                    UserDefinition {
                        identity: UserIdentity::Numeric(303),
                        kind: UserDefinitionKind::Struct {
                            fields: vec![FieldDescriptor {
                                id: None,
                                name: "flag".into(),
                                ty: ty(TypeKind::Scalar(ScalarKind::Bool), false),
                            }],
                            physical_field_order: vec![0],
                        },
                    },
                    UserDefinition {
                        identity: UserIdentity::Numeric(305),
                        kind: UserDefinitionKind::Union {
                            variants: vec![
                                VariantDescriptor {
                                    id: 1,
                                    name: "Unit".into(),
                                    payload: None,
                                },
                                VariantDescriptor {
                                    id: 2,
                                    name: "Item".into(),
                                    payload: Some(item),
                                },
                                VariantDescriptor {
                                    id: 3,
                                    name: "Text".into(),
                                    payload: Some(text),
                                },
                            ],
                        },
                    },
                ],
            },
            &SchemaLimits::default(),
        )
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
        let first = nodes[0].variant_payload().unwrap().unwrap();
        let edge = nodes[1].variant_payload().unwrap().unwrap();
        let id = edge.reference_id().unwrap().unwrap();
        assert_eq!(first.object_id().unwrap().unwrap(), id);
        assert!(
            edge.resolve_reference(&id)
                .unwrap()
                .field_at(0)
                .unwrap()
                .read_bool()
                .unwrap()
        );
        let text = nodes[2].variant_payload().unwrap().unwrap();
        let text_edge = nodes[3].variant_payload().unwrap().unwrap();
        let text_id = text_edge.reference_id().unwrap().unwrap();
        assert_ne!(id, text_id);
        assert_eq!(text.object_id().unwrap().unwrap(), text_id);
        assert_eq!(
            text_edge
                .resolve_reference(&text_id)
                .unwrap()
                .read_string()
                .unwrap()
                .as_str(),
            "payload"
        );
        assert!(nodes[4].variant_payload().unwrap().is_none());
        drop((first, edge, text, text_edge));
        drop(nodes);
        drop(root);
        let validated = view.validate().unwrap();
        assert_eq!(
            validated
                .root()
                .unwrap()
                .resolve_reference(&text_id)
                .unwrap()
                .read_string()
                .unwrap()
                .as_str(),
            "payload"
        );
    }
}

#[derive(Debug, PartialEq, fory::ForyUnion)]
enum Nested {
    #[fory(id = 11, default)]
    Unit,
    #[fory(id = 2)]
    Item(Item),
    #[fory(id = 7)]
    Colors(Vec<Color>),
    #[fory(unknown)]
    Unknown(fory::UnknownCase),
}

#[test]
fn nested_user_and_list_payloads_keep_actual_binding_after_owners_drop() {
    for compatible in [false, true] {
        let mode = if compatible {
            SchemaMode::Compatible
        } else {
            SchemaMode::SameSchema {
                version_hash: false,
            }
        };
        let mut d = descriptor(false, false, mode);
        d.root = ty(TypeKind::User(UserIdentity::Numeric(304)), false);
        d.definitions.push(UserDefinition {
            identity: UserIdentity::Numeric(303),
            kind: UserDefinitionKind::Struct {
                fields: vec![FieldDescriptor {
                    id: None,
                    name: "flag".into(),
                    ty: ty(TypeKind::Scalar(ScalarKind::Bool), false),
                }],
                physical_field_order: vec![0],
            },
        });
        d.definitions.push(UserDefinition {
            identity: UserIdentity::Numeric(304),
            kind: UserDefinitionKind::Union {
                variants: vec![
                    VariantDescriptor {
                        id: 7,
                        name: "Colors".into(),
                        payload: Some(ty(
                            TypeKind::List(Box::new(ty(
                                TypeKind::User(UserIdentity::Numeric(300)),
                                false,
                            ))),
                            false,
                        )),
                    },
                    VariantDescriptor {
                        id: 11,
                        name: "Unit".into(),
                        payload: None,
                    },
                    VariantDescriptor {
                        id: 2,
                        name: "Item".into(),
                        payload: Some(ty(TypeKind::User(UserIdentity::Numeric(303)), false)),
                    },
                ],
            },
        });
        let schema = Schema::compile(d, &SchemaLimits::default()).unwrap();
        let mut f = fory::Fory::builder()
            .xlang(true)
            .compatible(compatible)
            .check_struct_version(false)
            .build();
        f.register::<Color>(300).unwrap();
        f.register::<Item>(303).unwrap();
        f.register_union::<Nested>(304).unwrap();
        for value in [
            Nested::Item(Item { flag: true }),
            Nested::Colors(vec![Color::Blue, Color::Red]),
        ] {
            let input = f.serialize(&value).unwrap();
            assert_eq!(f.deserialize::<Nested>(&input).unwrap(), value);
            let view = schema
                .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
                .unwrap();
            let owned = view
                .root()
                .unwrap()
                .variant_payload()
                .unwrap()
                .unwrap()
                .to_owned()
                .unwrap();
            let validated = view.validate().unwrap();
            let checked = validated
                .root()
                .unwrap()
                .variant_payload()
                .unwrap()
                .unwrap()
                .to_owned()
                .unwrap();
            drop(validated);
            drop(input);
            match value {
                Nested::Item(_) => {
                    assert!(
                        owned
                            .as_view()
                            .unwrap()
                            .field_at(0)
                            .unwrap()
                            .read_bool()
                            .unwrap()
                    );
                    assert!(
                        checked
                            .as_view()
                            .unwrap()
                            .field_at(0)
                            .unwrap()
                            .read_bool()
                            .unwrap()
                    );
                }
                Nested::Colors(_) => {
                    assert_eq!(
                        owned
                            .as_view()
                            .unwrap()
                            .element_at(0)
                            .unwrap()
                            .variant()
                            .unwrap()
                            .id,
                        1
                    );
                    assert_eq!(
                        checked
                            .as_view()
                            .unwrap()
                            .element_at(1)
                            .unwrap()
                            .variant()
                            .unwrap()
                            .id,
                        0
                    );
                }
                _ => unreachable!(),
            }
        }
    }
}

#[test]
fn static_variant_fields_and_map_values_use_body_not_repeated_identity() {
    for mode in [
        SchemaMode::SameSchema {
            version_hash: false,
        },
        SchemaMode::Compatible,
    ] {
        let mut d = descriptor(true, false, mode);
        let choice = d.root.clone();
        let mut color_d = descriptor(false, false, mode);
        color_d.definitions[0].identity = UserIdentity::Numeric(301);
        let color = ty(TypeKind::User(UserIdentity::Numeric(301)), false);
        d.definitions.extend(color_d.definitions);
        d.root = ty(TypeKind::User(UserIdentity::Numeric(302)), false);
        let fields = vec![
            FieldDescriptor {
                id: None,
                name: "color".into(),
                ty: color,
            },
            FieldDescriptor {
                id: None,
                name: "choice".into(),
                ty: choice.clone(),
            },
        ];
        let order = <Holder as fory::StructSerializer>::sorted_field_names()
            .iter()
            .map(|name| fields.iter().position(|field| field.name == *name).unwrap())
            .collect();
        d.definitions.push(UserDefinition {
            identity: UserIdentity::Numeric(302),
            kind: UserDefinitionKind::Struct {
                fields,
                physical_field_order: order,
            },
        });
        let schema = Schema::compile(d, &SchemaLimits::default()).unwrap();
        let mut f = fory::Fory::builder()
            .xlang(true)
            .compatible(matches!(mode, SchemaMode::Compatible))
            .check_struct_version(false)
            .build();
        f.register_union::<Choice>(300).unwrap();
        f.register::<Color>(301).unwrap();
        f.register::<Holder>(302).unwrap();
        let value = Holder {
            color: Color::Blue,
            choice: Choice::Count(81),
        };
        let input = f.serialize(&value).unwrap();
        assert_eq!(f.deserialize::<Holder>(&input).unwrap(), value);
        let validated = schema
            .validate(&input, InputFormat::XlangMessage, &ParseLimits::default())
            .unwrap();
        let root = validated.root().unwrap();
        assert_eq!(
            root.field_by_name("color").unwrap().variant().unwrap().id,
            1
        );
        assert_eq!(
            root.field_by_name("choice")
                .unwrap()
                .variant_payload()
                .unwrap()
                .unwrap()
                .read_i32()
                .unwrap(),
            81
        );
        let mut d = descriptor(true, false, mode);
        d.root = ty(
            TypeKind::Map {
                key: Box::new(ty(TypeKind::Scalar(ScalarKind::String), false)),
                value: Box::new(choice),
            },
            false,
        );
        let schema = Schema::compile(d, &SchemaLimits::default()).unwrap();
        let value = std::collections::HashMap::from([
            ("unit".to_owned(), Choice::Unit),
            ("count".to_owned(), Choice::Count(2)),
        ]);
        let input = f.serialize(&value).unwrap();
        assert_eq!(
            f.deserialize::<std::collections::HashMap<String, Choice>>(&input)
                .unwrap(),
            value
        );
        let validated = schema
            .validate(&input, InputFormat::XlangMessage, &ParseLimits::default())
            .unwrap();
        for pair in validated.root().unwrap().entries().unwrap() {
            pair.unwrap().1.variant().unwrap();
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

fn descriptor(union: bool, named: bool, mode: SchemaMode) -> SchemaDescriptor {
    let identity = if named {
        UserIdentity::Name {
            namespace: "variants".into(),
            type_name: if union { "Choice" } else { "Color" }.into(),
        }
    } else {
        UserIdentity::Numeric(300)
    };
    let variants = if union {
        vec![
            VariantDescriptor {
                id: 17,
                name: "Maybe".into(),
                payload: Some(ty(TypeKind::Scalar(ScalarKind::String), true)),
            },
            VariantDescriptor {
                id: 9,
                name: "Unit".into(),
                payload: None,
            },
            VariantDescriptor {
                id: 3,
                name: "Count".into(),
                payload: Some(TypeDescriptor {
                    integer_encoding: Some(IntegerEncoding::Varint),
                    ..ty(TypeKind::Scalar(ScalarKind::I32), false)
                }),
            },
        ]
    } else {
        vec![
            VariantDescriptor {
                id: 1,
                name: "Blue".into(),
                payload: None,
            },
            VariantDescriptor {
                id: 0,
                name: "Red".into(),
                payload: None,
            },
        ]
    };
    SchemaDescriptor {
        encoding: RootEncoding {
            protocol_version: ProtocolVersion::Fory1_7,
            mode,
            root_reference: RootReferenceMode::NullOnly,
        },
        key_semantics_version: KEY_SEMANTICS_VERSION,
        root: ty(TypeKind::User(identity.clone()), false),
        definitions: vec![UserDefinition {
            identity,
            kind: if union {
                UserDefinitionKind::Union { variants }
            } else {
                UserDefinitionKind::Enum { variants }
            },
        }],
    }
}

#[test]
fn shared_variant_carriers_and_root_refvalue_preserve_identity() {
    for compatible in [false, true] {
        for named in [false, true] {
            for union in [false, true] {
                let mode = if compatible {
                    SchemaMode::Compatible
                } else {
                    SchemaMode::SameSchema {
                        version_hash: false,
                    }
                };
                let mut d = descriptor(union, named, mode);
                d.encoding.root_reference = RootReferenceMode::Tracking;
                d.root.track_ref = true;
                let mut f = fory::Fory::builder()
                    .xlang(true)
                    .compatible(compatible)
                    .check_struct_version(false)
                    .build();
                if named {
                    if union {
                        f.register_union_by_name::<Choice>("variants.Choice")
                            .unwrap();
                    } else {
                        f.register_by_name::<Color>("variants.Color").unwrap();
                    }
                } else if union {
                    f.register_union::<Choice>(300).unwrap();
                } else {
                    f.register::<Color>(300).unwrap();
                }
                macro_rules! check {
                    ($value:expr, $carrier:ty, $tag:expr) => {{
                        let value = std::rc::Rc::new($value);
                        let values = vec![std::rc::Rc::clone(&value), value];
                        let input = f.serialize(&values).unwrap();
                        let decoded = f.deserialize::<Vec<std::rc::Rc<$carrier>>>(&input).unwrap();
                        assert!(std::rc::Rc::ptr_eq(&decoded[0], &decoded[1]));
                        let mut list = d.clone();
                        list.root = ty(TypeKind::List(Box::new(d.root.clone())), false);
                        let schema = Schema::compile(list, &SchemaLimits::default()).unwrap();
                        let view = schema
                            .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
                            .unwrap();
                        let root = view.root().unwrap();
                        let edge = root.element_at(1).unwrap();
                        let id = edge.reference_id().unwrap().unwrap();
                        assert_eq!(
                            root.element_at(0).unwrap().object_id().unwrap().unwrap(),
                            id
                        );
                        assert!(matches!(edge.variant(), Err(DataError::AccessError(_))));
                        assert_eq!(
                            edge.resolve_reference(&id).unwrap().variant().unwrap().id,
                            $tag
                        );
                        drop(edge);
                        drop(root);
                        let validated = view.validate().unwrap();
                        assert_eq!(
                            validated
                                .root()
                                .unwrap()
                                .resolve_reference(&id)
                                .unwrap()
                                .variant()
                                .unwrap()
                                .id,
                            $tag
                        );
                        #[derive(Debug, PartialEq, fory::ForyStruct)]
                        struct SharedFields {
                            first: std::sync::Arc<$carrier>,
                            second: std::sync::Arc<$carrier>,
                        }
                        let mut fields_fory = fory::Fory::builder()
                            .xlang(true)
                            .compatible(compatible)
                            .check_struct_version(false)
                            .build();
                        if named {
                            if union {
                                fields_fory
                                    .register_union_by_name::<Choice>("variants.Choice")
                                    .unwrap();
                            } else {
                                fields_fory
                                    .register_by_name::<Color>("variants.Color")
                                    .unwrap();
                            }
                        } else if union {
                            fields_fory.register_union::<Choice>(300).unwrap();
                        } else {
                            fields_fory.register::<Color>(300).unwrap();
                        }
                        fields_fory.register::<SharedFields>(490).unwrap();
                        let shared = std::sync::Arc::new($value);
                        let input = fields_fory
                            .serialize(&SharedFields {
                                first: std::sync::Arc::clone(&shared),
                                second: shared,
                            })
                            .unwrap();
                        let decoded = fields_fory.deserialize::<SharedFields>(&input).unwrap();
                        assert!(std::sync::Arc::ptr_eq(&decoded.first, &decoded.second));
                        let mut fields = d.clone();
                        fields.root = ty(TypeKind::User(UserIdentity::Numeric(490)), false);
                        fields.definitions.push(UserDefinition {
                            identity: UserIdentity::Numeric(490),
                            kind: UserDefinitionKind::Struct {
                                fields: vec![
                                    FieldDescriptor {
                                        id: None,
                                        name: "first".into(),
                                        ty: d.root.clone(),
                                    },
                                    FieldDescriptor {
                                        id: None,
                                        name: "second".into(),
                                        ty: d.root.clone(),
                                    },
                                ],
                                physical_field_order: vec![0, 1],
                            },
                        });
                        let schema = Schema::compile(fields, &SchemaLimits::default()).unwrap();
                        let view = schema
                            .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
                            .unwrap();
                        let root = view.root().unwrap();
                        let edge = root.field_at(1).unwrap();
                        let id = edge.reference_id().unwrap().unwrap();
                        assert_eq!(
                            edge.resolve_reference(&id).unwrap().variant().unwrap().id,
                            $tag
                        );
                        drop(edge);
                        drop(root);
                        view.validate().unwrap();
                        let mut input = f.serialize(&values[0]).unwrap();
                        assert_eq!(input[1], 255);
                        input[1] = 0;
                        let schema = Schema::compile(d.clone(), &SchemaLimits::default()).unwrap();
                        let view = schema
                            .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
                            .unwrap();
                        let root = view.root().unwrap();
                        assert!(root.object_id().unwrap().is_some());
                        assert_eq!(root.variant().unwrap().id, $tag);
                        let owned = root.to_owned().unwrap();
                        drop(root);
                        view.validate().unwrap();
                        drop(input);
                        assert_eq!(owned.as_view().unwrap().variant().unwrap().id, $tag);
                    }};
                }
                if union {
                    check!(Choice::Unit, Choice, 9);
                    check!(Choice::Maybe(None), Choice, 17);
                    check!(Choice::Count(63), Choice, 3);
                } else {
                    check!(Color::Blue, Color, 1);
                }
            }
        }
    }
}

#[test]
fn tracked_union_writer_ancestor_cycle_is_explicit_and_bad_refs_are_terminal() {
    let mut user = ty(TypeKind::User(UserIdentity::Numeric(460)), false);
    user.track_ref = true;
    let schema = Schema::compile(
        SchemaDescriptor {
            encoding: RootEncoding {
                protocol_version: ProtocolVersion::Fory1_7,
                mode: SchemaMode::SameSchema {
                    version_hash: false,
                },
                root_reference: RootReferenceMode::Tracking,
            },
            key_semantics_version: KEY_SEMANTICS_VERSION,
            root: user.clone(),
            definitions: vec![UserDefinition {
                identity: UserIdentity::Numeric(460),
                kind: UserDefinitionKind::Union {
                    variants: vec![VariantDescriptor {
                        id: 1,
                        name: "Next".into(),
                        payload: Some(user),
                    }],
                },
            }],
        },
        &SchemaLimits::default(),
    )
    .unwrap();
    let mut input = vec![];
    let mut writer = fory::Writer::from_buffer(&mut input);
    writer.write_i8(0);
    writer.write_var_u32(34);
    writer.write_var_u32(460);
    writer.write_var_u32(1);
    writer.write_i8(-2);
    writer.write_var_u32(0);
    let view = schema
        .view(&input, InputFormat::RootValue, &ParseLimits::default())
        .unwrap();
    let root = view.root().unwrap();
    let id = root.object_id().unwrap().unwrap();
    let edge = root.variant_payload().unwrap().unwrap();
    assert_eq!(edge.reference_id().unwrap().unwrap(), id);
    assert!(matches!(edge.variant(), Err(DataError::AccessError(_))));
    assert_eq!(
        edge.resolve_reference(&id).unwrap().variant().unwrap().id,
        1
    );
    drop(edge);
    drop(root);
    let validated = view.validate().unwrap();
    assert_eq!(validated.root().unwrap().object_id().unwrap().unwrap(), id);
    for slot in [1, 127, 255] {
        let mut bad = input.clone();
        *bad.last_mut().unwrap() = slot;
        assert!(matches!(
            schema.validate(&bad, InputFormat::RootValue, &ParseLimits::default()),
            Err(DataError::MalformedInput(_))
        ));
    }
    for end in 1..input.len() {
        assert!(
            schema
                .validate(
                    &input[..end],
                    InputFormat::RootValue,
                    &ParseLimits::default()
                )
                .is_err()
        );
    }
    let mut bad = input.clone();
    bad.push(0);
    assert!(matches!(
        schema.validate(&bad, InputFormat::RootValue, &ParseLimits::default()),
        Err(DataError::MalformedInput(_))
    ));
}

#[test]
fn actual_enum_numeric_roots_select_logical_variant_without_payload() {
    for mode in [
        SchemaMode::SameSchema {
            version_hash: false,
        },
        SchemaMode::SameSchema { version_hash: true },
        SchemaMode::Compatible,
    ] {
        let schema =
            Schema::compile(descriptor(false, false, mode), &SchemaLimits::default()).unwrap();
        let mut f = fory::Fory::builder()
            .xlang(true)
            .compatible(matches!(mode, SchemaMode::Compatible))
            .build();
        f.register::<Color>(300).unwrap();
        for (value, id) in [(Color::Red, 0), (Color::Blue, 1)] {
            let input = f.serialize(&value).unwrap();
            assert_eq!(f.deserialize::<Color>(&input).unwrap(), value);
            let view = schema
                .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
                .unwrap();
            let root = view.root().unwrap();
            assert_eq!(root.variant().unwrap().id, id);
            assert!(root.variant_payload().unwrap().is_none());
            assert!(matches!(root.field_at(0), Err(DataError::AccessError(_))));
            drop(root);
            assert_eq!(
                view.validate()
                    .unwrap()
                    .root()
                    .unwrap()
                    .variant()
                    .unwrap()
                    .id,
                id
            );
        }
    }
}

#[test]
fn named_same_schema_variants_and_container_names_reuse_real_slots() {
    for union in [false, true] {
        let mut descriptor = descriptor(
            union,
            true,
            SchemaMode::SameSchema {
                version_hash: false,
            },
        );
        let root = descriptor.root.clone();
        descriptor.root = ty(
            TypeKind::List(Box::new(ty(TypeKind::List(Box::new(root)), false))),
            false,
        );
        let schema = Schema::compile(descriptor, &SchemaLimits::default()).unwrap();
        let mut f = fory::Fory::builder().xlang(true).compatible(false).build();
        if union {
            f.register_union_by_name::<Choice>("variants.Choice")
                .unwrap();
            let values = vec![
                vec![Choice::Count(7)],
                vec![Choice::Maybe(None), Choice::Unit],
            ];
            let input = f.serialize(&values).unwrap();
            assert_eq!(f.deserialize::<Vec<Vec<Choice>>>(&input).unwrap(), values);
            let view = schema
                .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
                .unwrap();
            let root = view.root().unwrap();
            assert_eq!(
                root.element_at(1)
                    .unwrap()
                    .element_at(1)
                    .unwrap()
                    .variant()
                    .unwrap()
                    .id,
                9
            );
            drop(root);
            view.validate().unwrap();
        } else {
            f.register_by_name::<Color>("variants.Color").unwrap();
            let values = vec![vec![Color::Red], vec![Color::Blue]];
            let input = f.serialize(&values).unwrap();
            assert_eq!(f.deserialize::<Vec<Vec<Color>>>(&input).unwrap(), values);
            let validated = schema
                .validate(&input, InputFormat::XlangMessage, &ParseLimits::default())
                .unwrap();
            assert_eq!(
                validated
                    .root()
                    .unwrap()
                    .element_at(1)
                    .unwrap()
                    .element_at(0)
                    .unwrap()
                    .variant()
                    .unwrap()
                    .id,
                1
            );
        }
    }
}

#[test]
fn compatible_named_variants_use_nonstruct_typemeta_new_and_ref() {
    for union in [false, true] {
        let mut descriptor = descriptor(union, true, SchemaMode::Compatible);
        descriptor.root = ty(
            TypeKind::List(Box::new(ty(
                TypeKind::List(Box::new(descriptor.root)),
                false,
            ))),
            false,
        );
        let schema = Schema::compile(descriptor, &SchemaLimits::default()).unwrap();
        let mut f = fory::Fory::builder().xlang(true).compatible(true).build();
        if union {
            f.register_union_by_name::<Choice>("variants.Choice")
                .unwrap();
            let value = vec![vec![Choice::Count(6)], vec![Choice::Unit]];
            let input = f.serialize(&value).unwrap();
            assert_eq!(f.deserialize::<Vec<Vec<Choice>>>(&input).unwrap(), value);
            let view = schema
                .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
                .unwrap();
            assert_eq!(
                view.root()
                    .unwrap()
                    .element_at(1)
                    .unwrap()
                    .element_at(0)
                    .unwrap()
                    .variant()
                    .unwrap()
                    .id,
                9
            );
            view.validate().unwrap();
        } else {
            f.register_by_name::<Color>("variants.Color").unwrap();
            let value = vec![vec![Color::Red], vec![Color::Blue]];
            let input = f.serialize(&value).unwrap();
            assert_eq!(f.deserialize::<Vec<Vec<Color>>>(&input).unwrap(), value);
            let view = schema
                .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
                .unwrap();
            assert_eq!(
                view.root()
                    .unwrap()
                    .element_at(1)
                    .unwrap()
                    .element_at(0)
                    .unwrap()
                    .variant()
                    .unwrap()
                    .id,
                1
            );
            view.validate().unwrap();
        }
    }
}

#[test]
fn actual_union_noncontiguous_tags_unit_and_nullable_payload_are_distinct() {
    for mode in [
        SchemaMode::SameSchema {
            version_hash: false,
        },
        SchemaMode::SameSchema { version_hash: true },
        SchemaMode::Compatible,
    ] {
        let schema =
            Schema::compile(descriptor(true, false, mode), &SchemaLimits::default()).unwrap();
        let mut f = fory::Fory::builder()
            .xlang(true)
            .compatible(matches!(mode, SchemaMode::Compatible))
            .build();
        f.register_union::<Choice>(300).unwrap();
        for value in [
            Choice::Unit,
            Choice::Count(-42),
            Choice::Maybe(None),
            Choice::Maybe(Some("text".into())),
        ] {
            let input = f.serialize(&value).unwrap();
            assert_eq!(f.deserialize::<Choice>(&input).unwrap(), value);
            let view = schema
                .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
                .unwrap();
            let root = view.root().unwrap();
            match &value {
                Choice::Unit => {
                    assert_eq!(root.variant().unwrap().id, 9);
                    assert!(root.variant_payload().unwrap().is_none());
                }
                Choice::Count(count) => {
                    assert_eq!(root.variant().unwrap().id, 3);
                    assert_eq!(
                        root.variant_payload().unwrap().unwrap().read_i32().unwrap(),
                        *count
                    );
                }
                Choice::Maybe(value) => {
                    assert_eq!(root.variant().unwrap().id, 17);
                    let payload = root.variant_payload().unwrap().unwrap();
                    assert_eq!(payload.is_null().unwrap(), value.is_none());
                    if let Some(value) = value {
                        assert_eq!(payload.read_string().unwrap().as_str(), value);
                    }
                }
                _ => unreachable!(),
            }
            let owned = root.to_owned().unwrap();
            drop(root);
            let validated = view.validate().unwrap();
            assert_eq!(
                validated.root().unwrap().variant().unwrap().id,
                owned.as_view().unwrap().variant().unwrap().id
            );
        }
    }
}

#[test]
fn variant_errors_nulls_truncation_and_first_failure_are_distinct() {
    for union in [false, true] {
        let schema = Schema::compile(
            descriptor(
                union,
                false,
                SchemaMode::SameSchema {
                    version_hash: false,
                },
            ),
            &SchemaLimits::default(),
        )
        .unwrap();
        let mut f = fory::Fory::builder().xlang(true).compatible(false).build();
        let input = if union {
            f.register_union::<Choice>(300).unwrap();
            f.serialize(&Choice::Count(42)).unwrap()
        } else {
            f.register::<Color>(300).unwrap();
            f.serialize(&Color::Blue).unwrap()
        };
        let mut reader = fory::Reader::new(&input);
        reader.skip(3).unwrap();
        reader.read_var_u32().unwrap();
        let tag = reader.get_cursor();
        let mut bad = input.clone();
        bad[tag] = 99;
        let view = schema
            .view(&bad, InputFormat::XlangMessage, &ParseLimits::default())
            .unwrap();
        let root = view.root().unwrap();
        assert!(matches!(root.read_bool(), Err(DataError::AccessError(_))));
        let error = root.variant().unwrap_err();
        assert!(matches!(error, DataError::SchemaMismatch(_)));
        assert_eq!(
            format!("{error:?}"),
            format!("{:?}", root.variant_payload().err().unwrap())
        );
        drop(root);
        assert_eq!(
            format!("{error:?}"),
            format!("{:?}", view.validate().err().unwrap())
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
        let mut wrong = input.clone();
        wrong[2] = if union { 25 } else { 34 };
        assert!(matches!(
            schema.view(&wrong, InputFormat::XlangMessage, &ParseLimits::default()),
            Err(DataError::SchemaMismatch(_))
        ));
        if union {
            let mut wrong = input.clone();
            wrong[tag + 2] = 21;
            assert!(matches!(
                schema.validate(&wrong, InputFormat::XlangMessage, &ParseLimits::default()),
                Err(DataError::SchemaMismatch(_))
            ));
        }
        let mut d = descriptor(
            union,
            false,
            SchemaMode::SameSchema {
                version_hash: false,
            },
        );
        d.root.nullable = true;
        let schema = Schema::compile(d, &SchemaLimits::default()).unwrap();
        let input = if union {
            f.serialize(&None::<Choice>).unwrap()
        } else {
            f.serialize(&None::<Color>).unwrap()
        };
        let view = schema
            .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
            .unwrap();
        assert!(view.root().unwrap().is_null().unwrap());
        assert!(matches!(
            view.root().unwrap().variant(),
            Err(DataError::AccessError(_))
        ));
        view.validate().unwrap();
    }
}

#[test]
fn compatible_named_metadata_hash_does_not_replace_exact_identity_check() {
    let mut f = fory::Fory::builder().xlang(true).compatible(true).build();
    f.register_by_name::<Color>("variants.Color").unwrap();
    let schema = Schema::compile(
        descriptor(false, true, SchemaMode::Compatible),
        &SchemaLimits::default(),
    )
    .unwrap();
    let input = f.serialize(&Color::Blue).unwrap();
    let mut reader = fory::Reader::new(&input);
    reader.skip(4).unwrap();
    let header_offset = reader.get_cursor();
    let header = reader.read_i64().unwrap();
    let len = (header & 255) as usize;
    assert!(len < 255);
    let start = reader.get_cursor();
    let mut bad = input.clone();
    bad[start] = 5;
    let mut hash_input = bad[start..start + len].to_vec();
    hash_input.extend_from_slice(&((header as u64 & 0xfff) as u16).to_le_bytes());
    let hash = (fory::__private::util::murmurhash3_x64_128(&hash_input, 47).0 as i64)
        .wrapping_shl(12)
        .wrapping_abs() as u64;
    bad[header_offset..header_offset + 8]
        .copy_from_slice(&((hash & !0xfff) | (header as u64 & 0xfff)).to_le_bytes());
    assert!(matches!(
        schema.view(&bad, InputFormat::XlangMessage, &ParseLimits::default()),
        Err(DataError::SchemaMismatch(_))
    ));
    let mut bad = input.clone();
    bad[header_offset + 4] ^= 1;
    assert!(matches!(
        schema.view(&bad, InputFormat::XlangMessage, &ParseLimits::default()),
        Err(DataError::MalformedInput(_))
    ));
}

#[test]
fn named_variant_roots_preserve_identity_payload_and_nullable_ownership() {
    for compatible in [false, true] {
        for union in [false, true] {
            let mode = if compatible {
                SchemaMode::Compatible
            } else {
                SchemaMode::SameSchema { version_hash: true }
            };
            let schema =
                Schema::compile(descriptor(union, true, mode), &SchemaLimits::default()).unwrap();
            let mut f = fory::Fory::builder()
                .xlang(true)
                .compatible(compatible)
                .build();
            let input = if union {
                f.register_union_by_name::<Choice>("variants.Choice")
                    .unwrap();
                let input = f.serialize(&Choice::Maybe(None)).unwrap();
                assert_eq!(
                    f.deserialize::<Choice>(&input).unwrap(),
                    Choice::Maybe(None)
                );
                input
            } else {
                f.register_by_name::<Color>("variants.Color").unwrap();
                let input = f.serialize(&Color::Blue).unwrap();
                assert_eq!(f.deserialize::<Color>(&input).unwrap(), Color::Blue);
                input
            };
            let view = schema
                .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
                .unwrap();
            let owned = view.root().unwrap().to_owned().unwrap();
            let validated = view.validate().unwrap();
            let checked = validated.root().unwrap().to_owned().unwrap();
            drop(validated);
            drop(input);
            assert_eq!(
                owned.as_view().unwrap().variant().unwrap().id,
                if union { 17 } else { 1 }
            );
            assert_eq!(
                checked.as_view().unwrap().variant().unwrap().id,
                if union { 17 } else { 1 }
            );
            if union {
                assert!(
                    owned
                        .as_view()
                        .unwrap()
                        .variant_payload()
                        .unwrap()
                        .unwrap()
                        .is_null()
                        .unwrap()
                );
                assert!(
                    checked
                        .as_view()
                        .unwrap()
                        .variant_payload()
                        .unwrap()
                        .unwrap()
                        .is_null()
                        .unwrap()
                );
            }
        }
    }
}
