use pi_fory_schema::*;
use std::rc::Rc;

#[derive(Debug, PartialEq, fory::ForyStruct)]
struct ListCycle {
    children: std::cell::RefCell<Vec<Rc<ListCycle>>>,
}
#[derive(Debug, PartialEq, fory::ForyStruct)]
struct MapCycle {
    children: std::cell::RefCell<std::collections::BTreeMap<String, Rc<MapCycle>>>,
}

#[test]
fn real_recursive_list_and_map_graphs_scan_refs_without_following_cycles() {
    for compatible in [false, true] {
        for named in [false, true] {
            let identity = if named {
                UserIdentity::Name {
                    namespace: "cycles".into(),
                    type_name: "Node".into(),
                }
            } else {
                UserIdentity::Numeric(520)
            };
            for map in [false, true] {
                let mut f = fory::Fory::builder()
                    .xlang(true)
                    .compatible(compatible)
                    .check_struct_version(false)
                    .build();
                let input = if map {
                    if named {
                        f.register_by_name::<MapCycle>("cycles.Node").unwrap();
                    } else {
                        f.register::<MapCycle>(520).unwrap();
                    }
                    let node = Rc::new(MapCycle {
                        children: std::cell::RefCell::new(std::collections::BTreeMap::new()),
                    });
                    node.children
                        .borrow_mut()
                        .insert("self".into(), Rc::clone(&node));
                    let result = f.serialize(&node);
                    node.children.borrow_mut().clear();
                    result.unwrap()
                } else {
                    if named {
                        f.register_by_name::<ListCycle>("cycles.Node").unwrap();
                    } else {
                        f.register::<ListCycle>(520).unwrap();
                    }
                    let node = Rc::new(ListCycle {
                        children: std::cell::RefCell::new(vec![]),
                    });
                    node.children.borrow_mut().push(Rc::clone(&node));
                    let result = f.serialize(&node);
                    node.children.borrow_mut().clear();
                    result.unwrap()
                };
                let identity = identity.clone();
                let child = ty(TypeKind::User(identity.clone()), true);
                let container = if map {
                    TypeKind::Map {
                        key: Box::new(ty(TypeKind::Scalar(ScalarKind::String), false)),
                        value: Box::new(child),
                    }
                } else {
                    TypeKind::List(Box::new(child))
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
                            root_reference: RootReferenceMode::Tracking,
                        },
                        key_semantics_version: KEY_SEMANTICS_VERSION,
                        root: ty(TypeKind::User(identity.clone()), true),
                        definitions: vec![UserDefinition {
                            identity,
                            kind: UserDefinitionKind::Struct {
                                fields: vec![FieldDescriptor {
                                    id: None,
                                    name: "children".into(),
                                    ty: ty(container, false),
                                }],
                                physical_field_order: vec![0],
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
                assert!(root.object_id().unwrap().is_none());
                let outer = root.field_at(0).unwrap();
                assert_eq!(outer.len().unwrap(), 1);
                let object = if map {
                    outer.entries().unwrap().next().unwrap().unwrap().1
                } else {
                    outer.element_at(0).unwrap()
                };
                let id = object.object_id().unwrap().unwrap();
                let body = object.field_at(0).unwrap();
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
                let owned = edge.to_owned().unwrap();
                assert_eq!(object.fields().unwrap().count(), 1);
                drop(edge);
                drop(body);
                drop(object);
                drop(outer);
                drop(root);
                let validated = view.validate().unwrap();
                let object = validated
                    .root()
                    .unwrap()
                    .resolve_reference(&id)
                    .unwrap()
                    .to_owned()
                    .unwrap();
                assert_eq!(
                    object
                        .as_view()
                        .unwrap()
                        .field_at(0)
                        .unwrap()
                        .len()
                        .unwrap(),
                    1
                );
                drop(validated);
                drop(input);
                assert_eq!(
                    owned
                        .as_view()
                        .unwrap()
                        .resolve_reference(&id)
                        .unwrap()
                        .field_at(0)
                        .unwrap()
                        .len()
                        .unwrap(),
                    1
                );
            }
        }
    }
}

#[test]
fn writer_recursive_set_and_union_container_cycles_keep_ancestor_slots() {
    for union in [false, true] {
        let identity = UserIdentity::Numeric(530);
        let child = ty(TypeKind::User(identity.clone()), true);
        let container = ty(
            if union {
                TypeKind::List(Box::new(child.clone()))
            } else {
                TypeKind::Set(Box::new(child.clone()))
            },
            false,
        );
        let definition = if union {
            UserDefinitionKind::Union {
                variants: vec![VariantDescriptor {
                    id: 1,
                    name: "Children".into(),
                    payload: Some(container),
                }],
            }
        } else {
            UserDefinitionKind::Struct {
                fields: vec![FieldDescriptor {
                    id: None,
                    name: "children".into(),
                    ty: container,
                }],
                physical_field_order: vec![0],
            }
        };
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
                root: child,
                definitions: vec![UserDefinition {
                    identity,
                    kind: definition,
                }],
            },
            &SchemaLimits::default(),
        );
        let schema = schema.unwrap();
        let mut input = vec![];
        let mut writer = fory::Writer::from_buffer(&mut input);
        writer.write_i8(0);
        writer.write_var_u32(if union { 34 } else { 27 });
        writer.write_var_u32(530);
        if union {
            writer.write_var_u32(1);
            writer.write_i8(-1);
            writer.write_var_u32(22);
        }
        writer.write_var_u32(1);
        writer.write_u8(9);
        writer.write_var_u32(if union { 34 } else { 27 });
        writer.write_var_u32(530);
        writer.write_i8(-2);
        writer.write_var_u32(0);
        let view = schema
            .view(&input, InputFormat::RootValue, &ParseLimits::default())
            .unwrap();
        let root = view.root().unwrap();
        let id = root.object_id().unwrap().unwrap();
        let body = if union {
            root.variant_payload().unwrap().unwrap()
        } else {
            root.field_at(0).unwrap()
        };
        assert_eq!(body.len().unwrap(), 1);
        let edge = body.elements().unwrap().next().unwrap().unwrap();
        assert_eq!(edge.reference_id().unwrap().unwrap(), id);
        assert!(matches!(edge.variant(), Err(DataError::AccessError(_))));
        let resolved = edge.resolve_reference(&id).unwrap();
        if union {
            assert_eq!(resolved.variant().unwrap().id, 1);
        } else {
            assert_eq!(resolved.field_at(0).unwrap().len().unwrap(), 1);
        }
        drop(resolved);
        drop(edge);
        drop(body);
        drop(root);
        let validated = view.validate().unwrap();
        let root = validated.root().unwrap();
        let body = if union {
            root.variant_payload().unwrap().unwrap()
        } else {
            root.field_at(0).unwrap()
        };
        assert_eq!(
            body.element_at(0).unwrap().reference_id().unwrap().unwrap(),
            id
        );
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
        let mut trailing = input.clone();
        trailing.push(0);
        assert!(matches!(
            schema.validate(&trailing, InputFormat::RootValue, &ParseLimits::default()),
            Err(DataError::MalformedInput(_))
        ));
    }
}

#[test]
fn direct_ref_reads_are_recoverable_across_scalar_content_and_containers() {
    let f = fory::Fory::builder()
        .xlang(true)
        .check_struct_version(false)
        .build();
    macro_rules! check {
        ($value:expr, $child:expr, $method:ident) => {{
            let shared = Rc::new($value);
            let input = f.serialize(&vec![Rc::clone(&shared), shared]).unwrap();
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
                    root: ty(TypeKind::List(Box::new($child)), false),
                    definitions: vec![],
                },
                &SchemaLimits::default(),
            )
            .unwrap();
            let view = schema
                .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
                .unwrap();
            let root = view.root().unwrap();
            let edge = root.element_at(1).unwrap();
            let id = edge.reference_id().unwrap().unwrap();
            assert!(matches!(edge.$method(), Err(DataError::AccessError(_))));
            let owned = edge.to_owned().unwrap();
            assert!(matches!(
                owned.as_view().unwrap().$method(),
                Err(DataError::AccessError(_))
            ));
            assert!(edge.resolve_reference(&id).unwrap().$method().is_ok());
            assert_eq!(root.len().unwrap(), 2);
            drop(edge);
            drop(root);
            let validated = view.validate().unwrap();
            let root = validated.root().unwrap();
            let edge = root.element_at(1).unwrap();
            assert!(matches!(edge.$method(), Err(DataError::AccessError(_))));
            let owned = edge.to_owned().unwrap();
            assert!(matches!(
                owned.as_view().unwrap().$method(),
                Err(DataError::AccessError(_))
            ));
            assert!(edge.resolve_reference(&id).unwrap().$method().is_ok());
            assert_eq!(root.len().unwrap(), 2);
        }};
    }
    check!(
        "edge".to_owned(),
        ty(TypeKind::Scalar(ScalarKind::String), true),
        read_string
    );
    check!(
        vec![1u8, 2],
        ty(TypeKind::Scalar(ScalarKind::Binary), true),
        read_binary
    );
    check!(
        23i32,
        TypeDescriptor {
            integer_encoding: Some(IntegerEncoding::Varint),
            ..ty(TypeKind::Scalar(ScalarKind::I32), true)
        },
        read_i32
    );
    check!(
        vec!["x".to_owned()],
        ty(
            TypeKind::List(Box::new(ty(TypeKind::Scalar(ScalarKind::String), false))),
            true
        ),
        len
    );
    check!(
        std::collections::BTreeMap::from([("k".to_owned(), "v".to_owned())]),
        ty(
            TypeKind::Map {
                key: Box::new(ty(TypeKind::Scalar(ScalarKind::String), false)),
                value: Box::new(ty(TypeKind::Scalar(ScalarKind::String), false))
            },
            true
        ),
        len
    );
}

#[derive(Debug, PartialEq, fory::ForyStruct)]
struct Item {
    value: i32,
}
#[derive(Debug, PartialEq, fory::ForyStruct)]
struct Pair {
    first: Rc<Item>,
    second: Rc<Item>,
}

fn ty(kind: TypeKind, track_ref: bool) -> TypeDescriptor {
    TypeDescriptor {
        kind,
        track_ref,
        nullable: false,
        integer_encoding: None,
    }
}
fn pair_schema() -> Schema {
    Schema::compile(
        SchemaDescriptor {
            encoding: RootEncoding {
                protocol_version: ProtocolVersion::Fory1_7,
                mode: SchemaMode::SameSchema {
                    version_hash: false,
                },
                root_reference: RootReferenceMode::Tracking,
            },
            key_semantics_version: KEY_SEMANTICS_VERSION,
            root: ty(TypeKind::User(UserIdentity::Numeric(401)), false),
            definitions: vec![
                UserDefinition {
                    identity: UserIdentity::Numeric(401),
                    kind: UserDefinitionKind::Struct {
                        fields: vec![
                            FieldDescriptor {
                                id: None,
                                name: "first".into(),
                                ty: ty(TypeKind::User(UserIdentity::Numeric(400)), true),
                            },
                            FieldDescriptor {
                                id: None,
                                name: "second".into(),
                                ty: ty(TypeKind::User(UserIdentity::Numeric(400)), true),
                            },
                        ],
                        physical_field_order: vec![0, 1],
                    },
                },
                UserDefinition {
                    identity: UserIdentity::Numeric(400),
                    kind: UserDefinitionKind::Struct {
                        fields: vec![FieldDescriptor {
                            id: None,
                            name: "value".into(),
                            ty: TypeDescriptor {
                                integer_encoding: Some(IntegerEncoding::Varint),
                                ..ty(TypeKind::Scalar(ScalarKind::I32), false)
                            },
                        }],
                        physical_field_order: vec![0],
                    },
                },
            ],
        },
        &SchemaLimits::default(),
    )
    .unwrap()
}

#[test]
fn actual_shared_rc_fields_have_root_associated_ids_and_explicit_resolution() {
    let schema = pair_schema();
    let mut f = fory::Fory::builder()
        .xlang(true)
        .compatible(false)
        .check_struct_version(false)
        .build();
    f.register::<Item>(400).unwrap();
    f.register::<Pair>(401).unwrap();
    let item = Rc::new(Item { value: 71 });
    let pair = Pair {
        first: Rc::clone(&item),
        second: item,
    };
    let input = f.serialize(&pair).unwrap();
    let decoded = f.deserialize::<Pair>(&input).unwrap();
    assert_eq!(decoded, pair);
    assert!(Rc::ptr_eq(&decoded.first, &decoded.second));
    let view = schema
        .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
        .unwrap();
    let root = view.root().unwrap();
    let second = root.field_at(1).unwrap();
    let id = second.reference_id().unwrap().unwrap();
    assert_eq!(
        root.resolve_reference(&id)
            .unwrap()
            .field_at(0)
            .unwrap()
            .read_i32()
            .unwrap(),
        71
    );
    assert!(second.object_id().unwrap().is_none());
    assert!(matches!(second.field_at(0), Err(DataError::AccessError(_))));
    let first = root.field_at(0).unwrap();
    assert_eq!(first.object_id().unwrap().unwrap(), id);
    assert!(first.reference_id().unwrap().is_none());
    assert_eq!(first.field_at(0).unwrap().read_i32().unwrap(), 71);
    let resolved = second.resolve_reference(&id).unwrap();
    assert_eq!(resolved.object_id().unwrap().unwrap(), id);
    assert_eq!(resolved.field_at(0).unwrap().read_i32().unwrap(), 71);
    let owned = second.to_owned().unwrap();
    let other = schema
        .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
        .unwrap();
    assert!(matches!(
        other.root().unwrap().resolve_reference(&id),
        Err(DataError::AccessError(_))
    ));
    drop(resolved);
    drop((first, second));
    drop(root);
    let validated = view.validate().unwrap();
    let root = validated.root().unwrap();
    let edge = root.field_at(1).unwrap();
    assert_eq!(
        root.resolve_reference(&id)
            .unwrap()
            .field_at(0)
            .unwrap()
            .read_i32()
            .unwrap(),
        71
    );
    assert_eq!(edge.reference_id().unwrap().unwrap(), id);
    assert_eq!(
        edge.resolve_reference(&id)
            .unwrap()
            .field_at(0)
            .unwrap()
            .read_i32()
            .unwrap(),
        71
    );
    drop(edge);
    drop(root);
    drop(validated);
    drop(other);
    drop(input);
    assert_eq!(
        owned
            .as_view()
            .unwrap()
            .resolve_reference(&id)
            .unwrap()
            .field_at(0)
            .unwrap()
            .read_i32()
            .unwrap(),
        71
    );
}

#[test]
fn actual_rc_list_repeated_edges_do_not_follow_bodies() {
    let mut d = SchemaDescriptor {
        encoding: RootEncoding {
            protocol_version: ProtocolVersion::Fory1_7,
            mode: SchemaMode::SameSchema {
                version_hash: false,
            },
            root_reference: RootReferenceMode::Tracking,
        },
        key_semantics_version: KEY_SEMANTICS_VERSION,
        root: ty(
            TypeKind::List(Box::new(ty(
                TypeKind::User(UserIdentity::Numeric(400)),
                true,
            ))),
            false,
        ),
        definitions: vec![UserDefinition {
            identity: UserIdentity::Numeric(400),
            kind: UserDefinitionKind::Struct {
                fields: vec![FieldDescriptor {
                    id: None,
                    name: "value".into(),
                    ty: TypeDescriptor {
                        integer_encoding: Some(IntegerEncoding::Varint),
                        ..ty(TypeKind::Scalar(ScalarKind::I32), false)
                    },
                }],
                physical_field_order: vec![0],
            },
        }],
    };
    let mut f = fory::Fory::builder()
        .xlang(true)
        .compatible(false)
        .check_struct_version(false)
        .build();
    f.register::<Item>(400).unwrap();
    let item = Rc::new(Item { value: 12 });
    let values = vec![Rc::clone(&item), item];
    let input = f.serialize(&values).unwrap();
    let decoded = f.deserialize::<Vec<Rc<Item>>>(&input).unwrap();
    assert!(Rc::ptr_eq(&decoded[0], &decoded[1]));
    let schema = Schema::compile(d.clone(), &SchemaLimits::default()).unwrap();
    let view = schema
        .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
        .unwrap();
    let root = view.root().unwrap();
    let second = root.element_at(1).unwrap();
    let id = second.reference_id().unwrap().unwrap();
    assert_eq!(
        root.element_at(0).unwrap().object_id().unwrap().unwrap(),
        id
    );
    assert_eq!(
        second
            .resolve_reference(&id)
            .unwrap()
            .field_at(0)
            .unwrap()
            .read_i32()
            .unwrap(),
        12
    );
    for node in root.elements().unwrap() {
        node.unwrap();
    }
    drop(second);
    drop(root);
    view.validate().unwrap();
    d.root = ty(TypeKind::User(UserIdentity::Numeric(400)), true);
    let schema = Schema::compile(d, &SchemaLimits::default()).unwrap();
    let input = f.serialize(&values[0]).unwrap();
    let view = schema
        .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
        .unwrap();
    assert!(view.root().unwrap().object_id().unwrap().is_none());
    view.validate().unwrap();
}

#[test]
fn malformed_reference_indices_and_truncated_graph_preserve_first_failure() {
    let schema = pair_schema();
    let mut f = fory::Fory::builder()
        .xlang(true)
        .compatible(false)
        .check_struct_version(false)
        .build();
    f.register::<Item>(400).unwrap();
    f.register::<Pair>(401).unwrap();
    let shared = Rc::new(Item { value: 7 });
    let input = f
        .serialize(&Pair {
            first: Rc::clone(&shared),
            second: shared,
        })
        .unwrap();
    assert_eq!(input[input.len() - 2], 254);
    for id in [1, 127, 255] {
        let mut bad = input.clone();
        *bad.last_mut().unwrap() = id;
        let view = schema
            .view(&bad, InputFormat::XlangMessage, &ParseLimits::default())
            .unwrap();
        let root = view.root().unwrap();
        let edge = root.field_at(1).unwrap();
        let error = edge.reference_id().unwrap_err();
        assert!(matches!(error, DataError::MalformedInput(_)));
        assert_eq!(
            format!("{error:?}"),
            format!("{:?}", root.object_id().unwrap_err())
        );
        drop(edge);
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
    let mut bad = input.clone();
    bad.push(0);
    assert!(matches!(
        schema.validate(&bad, InputFormat::XlangMessage, &ParseLimits::default()),
        Err(DataError::MalformedInput(_))
    ));
}

#[derive(Debug, PartialEq, fory::ForyStruct)]
struct Empty {}
#[derive(Debug, PartialEq, fory::ForyStruct)]
struct EmptyPair {
    first: Rc<Empty>,
    second: Rc<Empty>,
}

#[test]
fn distinct_zero_body_objects_have_distinct_marker_offsets_and_ids() {
    let mut descriptor = SchemaDescriptor {
        encoding: RootEncoding {
            protocol_version: ProtocolVersion::Fory1_7,
            mode: SchemaMode::SameSchema {
                version_hash: false,
            },
            root_reference: RootReferenceMode::Tracking,
        },
        key_semantics_version: KEY_SEMANTICS_VERSION,
        root: ty(TypeKind::User(UserIdentity::Numeric(411)), false),
        definitions: vec![
            UserDefinition {
                identity: UserIdentity::Numeric(410),
                kind: UserDefinitionKind::Struct {
                    fields: vec![],
                    physical_field_order: vec![],
                },
            },
            UserDefinition {
                identity: UserIdentity::Numeric(411),
                kind: UserDefinitionKind::Struct {
                    fields: vec![
                        FieldDescriptor {
                            id: None,
                            name: "first".into(),
                            ty: ty(TypeKind::User(UserIdentity::Numeric(410)), true),
                        },
                        FieldDescriptor {
                            id: None,
                            name: "second".into(),
                            ty: ty(TypeKind::User(UserIdentity::Numeric(410)), true),
                        },
                    ],
                    physical_field_order: vec![0, 1],
                },
            },
        ],
    };
    let mut f = fory::Fory::builder()
        .xlang(true)
        .compatible(false)
        .check_struct_version(false)
        .build();
    f.register::<Empty>(410).unwrap();
    f.register::<EmptyPair>(411).unwrap();
    let value = EmptyPair {
        first: Rc::new(Empty {}),
        second: Rc::new(Empty {}),
    };
    let input = f.serialize(&value).unwrap();
    assert_eq!(f.deserialize::<EmptyPair>(&input).unwrap(), value);
    let schema = Schema::compile(descriptor.clone(), &SchemaLimits::default()).unwrap();
    let view = schema
        .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
        .unwrap();
    let root = view.root().unwrap();
    let first = root.field_at(0).unwrap().object_id().unwrap().unwrap();
    let second = root.field_at(1).unwrap().object_id().unwrap().unwrap();
    assert_ne!(first, second);
    assert_eq!(
        root.field_at(0).unwrap().object_id().unwrap().unwrap(),
        first
    );
    drop(root);
    view.validate().unwrap();
    descriptor.root.nullable = true;
    let schema = Schema::compile(descriptor, &SchemaLimits::default()).unwrap();
    let input = f.serialize(&None::<EmptyPair>).unwrap();
    let view = schema
        .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
        .unwrap();
    assert!(view.root().unwrap().object_id().unwrap().is_none());
    view.validate().unwrap();
}

#[test]
fn tracked_root_writer_reserves_ancestor_before_self_edge_and_resolves_explicitly() {
    let identity = UserIdentity::Numeric(420);
    let mut node = ty(TypeKind::User(identity.clone()), true);
    node.nullable = true;
    let descriptor = SchemaDescriptor {
        encoding: RootEncoding {
            protocol_version: ProtocolVersion::Fory1_7,
            mode: SchemaMode::SameSchema {
                version_hash: false,
            },
            root_reference: RootReferenceMode::Tracking,
        },
        key_semantics_version: KEY_SEMANTICS_VERSION,
        root: node.clone(),
        definitions: vec![UserDefinition {
            identity,
            kind: UserDefinitionKind::Struct {
                fields: vec![FieldDescriptor {
                    id: None,
                    name: "next".into(),
                    ty: node,
                }],
                physical_field_order: vec![0],
            },
        }],
    };
    let schema = Schema::compile(descriptor, &SchemaLimits::default()).unwrap();
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
    let root = view.root().unwrap();
    let id = root.object_id().unwrap().unwrap();
    let edge = root.field_at(0).unwrap();
    assert_eq!(edge.reference_id().unwrap().unwrap(), id);
    assert!(matches!(edge.field_at(0), Err(DataError::AccessError(_))));
    assert_eq!(
        edge.resolve_reference(&id)
            .unwrap()
            .object_id()
            .unwrap()
            .unwrap(),
        id
    );
    drop(edge);
    drop(root);
    let validated = view.validate().unwrap();
    let root = validated.root().unwrap();
    assert_eq!(root.object_id().unwrap().unwrap(), id);
    assert_eq!(
        root.field_at(0).unwrap().reference_id().unwrap().unwrap(),
        id
    );
}

#[test]
fn shared_fields_numeric_and_name_in_compatible_mode_bind_before_reference() {
    for named in [false, true] {
        let mut d = SchemaDescriptor {
            encoding: RootEncoding {
                protocol_version: ProtocolVersion::Fory1_7,
                mode: SchemaMode::Compatible,
                root_reference: RootReferenceMode::Tracking,
            },
            key_semantics_version: KEY_SEMANTICS_VERSION,
            root: ty(TypeKind::User(UserIdentity::Numeric(401)), false),
            definitions: vec![
                UserDefinition {
                    identity: UserIdentity::Numeric(401),
                    kind: UserDefinitionKind::Struct {
                        fields: vec![
                            FieldDescriptor {
                                id: None,
                                name: "first".into(),
                                ty: ty(TypeKind::User(UserIdentity::Numeric(400)), true),
                            },
                            FieldDescriptor {
                                id: None,
                                name: "second".into(),
                                ty: ty(TypeKind::User(UserIdentity::Numeric(400)), true),
                            },
                        ],
                        physical_field_order: vec![0, 1],
                    },
                },
                UserDefinition {
                    identity: UserIdentity::Numeric(400),
                    kind: UserDefinitionKind::Struct {
                        fields: vec![FieldDescriptor {
                            id: None,
                            name: "value".into(),
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
        let mut f = fory::Fory::builder()
            .xlang(true)
            .compatible(true)
            .check_struct_version(false)
            .build();
        if named {
            let parent = UserIdentity::Name {
                namespace: "refs".into(),
                type_name: "Pair".into(),
            };
            let child = UserIdentity::Name {
                namespace: "refs".into(),
                type_name: "Item".into(),
            };
            d.root.kind = TypeKind::User(parent.clone());
            d.definitions[0].identity = parent;
            d.definitions[1].identity = child.clone();
            let UserDefinitionKind::Struct { fields, .. } = &mut d.definitions[0].kind else {
                unreachable!()
            };
            for field in fields {
                field.ty.kind = TypeKind::User(child.clone());
            }
            f.register_by_name::<Pair>("refs.Pair").unwrap();
            f.register_by_name::<Item>("refs.Item").unwrap();
        } else {
            f.register::<Pair>(401).unwrap();
            f.register::<Item>(400).unwrap();
        }
        let item = Rc::new(Item { value: 44 });
        let value = Pair {
            first: Rc::clone(&item),
            second: item,
        };
        let input = f.serialize(&value).unwrap();
        assert_eq!(f.deserialize::<Pair>(&input).unwrap(), value);
        let schema = Schema::compile(d.clone(), &SchemaLimits::default()).unwrap();
        let view = schema
            .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
            .unwrap();
        let root = view.root().unwrap();
        assert!(!root.field_at(0).unwrap().is_null().unwrap());
        let edge = root.field_at(1).unwrap();
        assert!(!edge.is_null().unwrap());
        let id = edge.reference_id().unwrap().unwrap();
        assert_eq!(
            edge.resolve_reference(&id)
                .unwrap()
                .field_at(0)
                .unwrap()
                .read_i32()
                .unwrap(),
            44
        );
        drop(edge);
        drop(root);
        let validated = view.validate().unwrap();
        let root = validated.root().unwrap();
        assert!(!root.is_null().unwrap());
        assert!(!root.field_at(0).unwrap().is_null().unwrap());
        assert!(!root.field_at(1).unwrap().is_null().unwrap());
        assert_eq!(
            root.field_at(0)
                .unwrap()
                .field_at(0)
                .unwrap()
                .read_i32()
                .unwrap(),
            44
        );
        // Force an actual root RefValue marker; the existing child becomes slot 1.
        let mut tracked = input.clone();
        tracked[1] = 0;
        *tracked.last_mut().unwrap() = 1;
        let view = schema
            .view(&tracked, InputFormat::XlangMessage, &ParseLimits::default())
            .unwrap();
        let root = view.root().unwrap();
        assert!(!root.is_null().unwrap());
        assert!(!root.field_at(0).unwrap().is_null().unwrap());
        assert_eq!(
            root.field_at(0)
                .unwrap()
                .field_at(0)
                .unwrap()
                .read_i32()
                .unwrap(),
            44
        );
        drop(root);
        let validated = view.validate().unwrap();
        assert!(!validated.root().unwrap().is_null().unwrap());
        let mut nullable = d;
        nullable.root.nullable = true;
        let nullable = Schema::compile(nullable, &SchemaLimits::default()).unwrap();
        let null = [1, 253];
        let view = nullable
            .view(&null, InputFormat::XlangMessage, &ParseLimits::default())
            .unwrap();
        assert!(view.root().unwrap().is_null().unwrap());
        assert!(view.validate().unwrap().root().unwrap().is_null().unwrap());
        assert!(matches!(
            schema.validate(&null, InputFormat::XlangMessage, &ParseLimits::default()),
            Err(DataError::SchemaMismatch(_))
        ));
    }
}

#[test]
fn arc_map_values_share_wire_slots_across_entries() {
    let mut f = fory::Fory::builder()
        .xlang(true)
        .compatible(false)
        .check_struct_version(false)
        .build();
    f.register::<Item>(400).unwrap();
    let item = std::sync::Arc::new(Item { value: 93 });
    let values =
        std::collections::BTreeMap::from([(1i32, std::sync::Arc::clone(&item)), (2, item)]);
    let input = f.serialize(&values).unwrap();
    let decoded = f
        .deserialize::<std::collections::BTreeMap<i32, std::sync::Arc<Item>>>(&input)
        .unwrap();
    assert!(std::sync::Arc::ptr_eq(&decoded[&1], &decoded[&2]));
    let descriptor = SchemaDescriptor {
        encoding: RootEncoding {
            protocol_version: ProtocolVersion::Fory1_7,
            mode: SchemaMode::SameSchema {
                version_hash: false,
            },
            root_reference: RootReferenceMode::Tracking,
        },
        key_semantics_version: KEY_SEMANTICS_VERSION,
        root: ty(
            TypeKind::Map {
                key: Box::new(TypeDescriptor {
                    integer_encoding: Some(IntegerEncoding::Varint),
                    ..ty(TypeKind::Scalar(ScalarKind::I32), false)
                }),
                value: Box::new(ty(TypeKind::User(UserIdentity::Numeric(400)), true)),
            },
            false,
        ),
        definitions: vec![UserDefinition {
            identity: UserIdentity::Numeric(400),
            kind: UserDefinitionKind::Struct {
                fields: vec![FieldDescriptor {
                    id: None,
                    name: "value".into(),
                    ty: TypeDescriptor {
                        integer_encoding: Some(IntegerEncoding::Varint),
                        ..ty(TypeKind::Scalar(ScalarKind::I32), false)
                    },
                }],
                physical_field_order: vec![0],
            },
        }],
    };
    let schema = Schema::compile(descriptor, &SchemaLimits::default()).unwrap();
    let view = schema
        .view(&input, InputFormat::XlangMessage, &ParseLimits::default())
        .unwrap();
    let root = view.root().unwrap();
    let nodes = root
        .entries()
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let id = nodes[1].1.reference_id().unwrap().unwrap();
    assert_eq!(nodes[0].1.object_id().unwrap().unwrap(), id);
    assert_eq!(
        root.resolve_reference(&id)
            .unwrap()
            .field_at(0)
            .unwrap()
            .read_i32()
            .unwrap(),
        93
    );
    drop(nodes);
    drop(root);
    let validated = view.validate().unwrap();
    let root = validated.root().unwrap();
    let nodes = root
        .entries()
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(nodes[1].1.reference_id().unwrap().unwrap(), id);
    assert_eq!(
        root.resolve_reference(&id)
            .unwrap()
            .field_at(0)
            .unwrap()
            .read_i32()
            .unwrap(),
        93
    );
}

fn self_cycle_fixture() -> (Schema, Vec<u8>) {
    let mut node = ty(TypeKind::User(UserIdentity::Numeric(420)), true);
    node.nullable = true;
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
            root: node.clone(),
            definitions: vec![UserDefinition {
                identity: UserIdentity::Numeric(420),
                kind: UserDefinitionKind::Struct {
                    fields: vec![
                        FieldDescriptor {
                            id: None,
                            name: "next".into(),
                            ty: node.clone(),
                        },
                        FieldDescriptor {
                            id: None,
                            name: "other".into(),
                            ty: node,
                        },
                    ],
                    physical_field_order: vec![0, 1],
                },
            }],
        },
        &SchemaLimits::default(),
    )
    .unwrap();
    let mut input = vec![];
    let mut writer = fory::Writer::from_buffer(&mut input);
    writer.write_i8(0);
    writer.write_var_u32(27);
    writer.write_var_u32(420);
    writer.write_i8(-2);
    writer.write_var_u32(0);
    writer.write_i8(-2);
    writer.write_var_u32(0);
    (schema, input)
}

#[test]
fn self_cycle_fields_reserve_parent_before_scanning_body() {
    let (schema, input) = self_cycle_fixture();
    let view = schema
        .view(&input, InputFormat::RootValue, &ParseLimits::default())
        .unwrap();
    let root = view.root().unwrap();
    let fields = root
        .fields()
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(fields.len(), 2);
    assert_eq!(
        root.field_at(1).unwrap().reference_id().unwrap(),
        root.object_id().unwrap()
    );
    assert_eq!(
        fields[0].1.reference_id().unwrap(),
        root.object_id().unwrap()
    );
}

#[test]
fn validated_self_cycle_fields_reserve_parent_before_scanning_body() {
    let (schema, input) = self_cycle_fixture();
    let validated = schema
        .validate(&input, InputFormat::RootValue, &ParseLimits::default())
        .unwrap();
    let root = validated.root().unwrap();
    let fields = root
        .fields()
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(fields.len(), 2);
    assert_eq!(
        root.field_at(1).unwrap().reference_id().unwrap(),
        root.object_id().unwrap()
    );
    assert_eq!(
        fields[0].1.reference_id().unwrap(),
        root.object_id().unwrap()
    );
}

#[test]
fn many_unique_rc_empty_objects_keep_distinct_slots_and_repeated_edges() {
    let descriptor = SchemaDescriptor {
        encoding: RootEncoding {
            protocol_version: ProtocolVersion::Fory1_7,
            mode: SchemaMode::SameSchema {
                version_hash: false,
            },
            root_reference: RootReferenceMode::Tracking,
        },
        key_semantics_version: KEY_SEMANTICS_VERSION,
        root: ty(
            TypeKind::List(Box::new(ty(
                TypeKind::User(UserIdentity::Numeric(410)),
                true,
            ))),
            false,
        ),
        definitions: vec![UserDefinition {
            identity: UserIdentity::Numeric(410),
            kind: UserDefinitionKind::Struct {
                fields: vec![],
                physical_field_order: vec![],
            },
        }],
    };
    let schema = Schema::compile(descriptor, &SchemaLimits::default()).unwrap();
    let mut f = fory::Fory::builder()
        .xlang(true)
        .compatible(false)
        .check_struct_version(false)
        .build();
    f.register::<Empty>(410).unwrap();
    let mut values: Vec<_> = (0..4096).map(|_| Rc::new(Empty {})).collect();
    values.push(Rc::clone(&values[0]));
    values.push(Rc::clone(&values[4095]));
    let input = f.serialize(&values).unwrap();
    let decoded = f.deserialize::<Vec<Rc<Empty>>>(&input).unwrap();
    assert!(Rc::ptr_eq(&decoded[0], &decoded[4096]));
    assert!(!Rc::ptr_eq(&decoded[0], &decoded[1]));
    let validated = schema
        .validate(&input, InputFormat::XlangMessage, &ParseLimits::default())
        .unwrap();
    let root = validated.root().unwrap();
    let nodes = root
        .elements()
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let ids: std::collections::HashSet<_> = nodes[..4096]
        .iter()
        .map(|node| node.object_id().unwrap().unwrap())
        .collect();
    assert_eq!(ids.len(), 4096);
    assert_eq!(
        nodes[4096].reference_id().unwrap(),
        nodes[0].object_id().unwrap()
    );
    assert_eq!(
        nodes[4097].reference_id().unwrap(),
        nodes[4095].object_id().unwrap()
    );
}

#[test]
fn shared_builtin_collection_elements_use_actual_markers_not_rust_ownership() {
    for compatible in [false, true] {
        let f = fory::Fory::builder()
            .xlang(true)
            .compatible(compatible)
            .check_struct_version(false)
            .build();
        macro_rules! check {($value:expr, $t:ty, $ty:expr, $read:expr) => {{
            let item = Rc::new($value); let values = vec![Rc::clone(&item), item];
            let input = f.serialize(&values).unwrap();
            let decoded = f.deserialize::<Vec<Rc<$t>>>(&input);
            if let Ok(decoded) = decoded {assert!(Rc::ptr_eq(&decoded[0], &decoded[1]));}
            else {assert!(matches!(decoded, Err(fory::Error::TypeError(ref message)) if message == "ID harness not found"));}
            let child = $ty;
            let descriptor = SchemaDescriptor {encoding: RootEncoding {protocol_version: ProtocolVersion::Fory1_7, mode: if compatible {SchemaMode::Compatible} else {SchemaMode::SameSchema {version_hash: false}}, root_reference: RootReferenceMode::Tracking}, key_semantics_version: KEY_SEMANTICS_VERSION, root: ty(TypeKind::List(Box::new(child.clone())), false), definitions: vec![]};
            let schema = Schema::compile(descriptor.clone(), &SchemaLimits::default()).unwrap();
            let view = schema.view(&input, InputFormat::XlangMessage, &ParseLimits::default()).unwrap();
            let root = view.root().unwrap(); let edge = root.element_at(1).unwrap(); let id = edge.reference_id().unwrap().unwrap();
            assert!(!edge.is_null().unwrap());
            let other = schema.view(&input, InputFormat::XlangMessage, &ParseLimits::default()).unwrap();
            assert!(matches!(other.root().unwrap().resolve_reference(&id), Err(DataError::AccessError(_))));
            assert_eq!(root.element_at(0).unwrap().object_id().unwrap().unwrap(), id);
            let resolved = edge.resolve_reference(&id).unwrap(); ($read)(&resolved);
            let owned = resolved.to_owned().unwrap(); drop(resolved); drop(edge); drop(root);
            let validated = view.validate().unwrap();
            assert_eq!(validated.root().unwrap().element_at(1).unwrap().reference_id().unwrap().unwrap(), id);
            drop(validated); drop(input); ($read)(&owned.as_view().unwrap());
            let mut d = descriptor; d.root = child;
            let schema = Schema::compile(d, &SchemaLimits::default()).unwrap();
            let input = f.serialize(&values[0]).unwrap(); assert_eq!(input[1], 255);
            let view = schema.view(&input, InputFormat::XlangMessage, &ParseLimits::default()).unwrap();
            assert!(view.root().unwrap().object_id().unwrap().is_none()); view.validate().unwrap();
            let mut tracked = input.clone(); tracked[1] = 0;
            let view = schema.view(&tracked, InputFormat::XlangMessage, &ParseLimits::default()).unwrap();
            let id = view.root().unwrap().object_id().unwrap().unwrap();
            let validated = view.validate().unwrap();
            assert_eq!(validated.root().unwrap().object_id().unwrap().unwrap(), id);
            let direct = schema.validate(&tracked, InputFormat::XlangMessage, &ParseLimits::default()).unwrap();
            assert!(direct.root().unwrap().object_id().unwrap().is_some());
            for end in 1..tracked.len() {assert!(schema.validate(&tracked[..end], InputFormat::XlangMessage, &ParseLimits::default()).is_err());}
            let mut trailing = tracked.clone(); trailing.push(0);
            assert!(matches!(schema.validate(&trailing, InputFormat::XlangMessage, &ParseLimits::default()), Err(DataError::MalformedInput(_))));
        }};}
        check!(
            "shared".to_owned(),
            String,
            ty(TypeKind::Scalar(ScalarKind::String), true),
            |n: &NodeView<'_>| {
                assert_eq!(n.read_string().unwrap().as_str(), "shared");
            }
        );
        check!(
            vec![1u8, 2, 3],
            Vec<u8>,
            ty(TypeKind::Scalar(ScalarKind::Binary), true),
            |n: &NodeView<'_>| {
                assert_eq!(n.read_binary().unwrap().as_bytes(), &[1, 2, 3]);
            }
        );
        check!(
            vec!["x".to_owned(), "y".to_owned()],
            Vec<String>,
            ty(
                TypeKind::List(Box::new(ty(TypeKind::Scalar(ScalarKind::String), false))),
                true
            ),
            |n: &NodeView<'_>| {
                assert_eq!(n.len().unwrap(), 2);
                assert_eq!(
                    n.element_at(1).unwrap().read_string().unwrap().as_str(),
                    "y"
                );
            }
        );
        check!(std::collections::BTreeMap::from([("k".to_owned(), "v".to_owned())]), std::collections::BTreeMap<String, String>, ty(TypeKind::Map {key: Box::new(ty(TypeKind::Scalar(ScalarKind::String), false)), value: Box::new(ty(TypeKind::Scalar(ScalarKind::String), false))}, true), |n: &NodeView<'_>| {assert_eq!(n.len().unwrap(), 1); assert_eq!(n.entries().unwrap().next().unwrap().unwrap().1.read_string().unwrap().as_str(), "v");});
        check!(
            std::collections::BTreeSet::from(["s".to_owned()]),
            std::collections::BTreeSet<String>,
            ty(
                TypeKind::Set(Box::new(ty(TypeKind::Scalar(ScalarKind::String), false))),
                true
            ),
            |n: &NodeView<'_>| {
                assert_eq!(n.len().unwrap(), 1);
            }
        );
        check!(
            71i32,
            i32,
            TypeDescriptor {
                integer_encoding: Some(IntegerEncoding::Varint),
                ..ty(TypeKind::Scalar(ScalarKind::I32), true)
            },
            |n: &NodeView<'_>| {
                assert_eq!(n.read_i32().unwrap(), 71);
            }
        );
        check!(
            true,
            bool,
            ty(TypeKind::Scalar(ScalarKind::Bool), true),
            |n: &NodeView<'_>| {
                assert!(n.read_bool().unwrap());
            }
        );
        check!(
            -3i8,
            i8,
            ty(TypeKind::Scalar(ScalarKind::I8), true),
            |n: &NodeView<'_>| {
                assert_eq!(n.read_i8().unwrap(), -3);
            }
        );
        check!(
            -300i16,
            i16,
            ty(TypeKind::Scalar(ScalarKind::I16), true),
            |n: &NodeView<'_>| {
                assert_eq!(n.read_i16().unwrap(), -300);
            }
        );
        check!(
            -901i64,
            i64,
            TypeDescriptor {
                integer_encoding: Some(IntegerEncoding::Varint),
                ..ty(TypeKind::Scalar(ScalarKind::I64), true)
            },
            |n: &NodeView<'_>| {
                assert_eq!(n.read_i64().unwrap(), -901);
            }
        );
        check!(
            17u8,
            u8,
            ty(TypeKind::Scalar(ScalarKind::U8), true),
            |n: &NodeView<'_>| {
                assert_eq!(n.read_u8().unwrap(), 17);
            }
        );
        check!(
            301u16,
            u16,
            ty(TypeKind::Scalar(ScalarKind::U16), true),
            |n: &NodeView<'_>| {
                assert_eq!(n.read_u16().unwrap(), 301);
            }
        );
        check!(
            901u32,
            u32,
            TypeDescriptor {
                integer_encoding: Some(IntegerEncoding::Varint),
                ..ty(TypeKind::Scalar(ScalarKind::U32), true)
            },
            |n: &NodeView<'_>| {
                assert_eq!(n.read_u32().unwrap(), 901);
            }
        );
        check!(
            902u64,
            u64,
            TypeDescriptor {
                integer_encoding: Some(IntegerEncoding::Varint),
                ..ty(TypeKind::Scalar(ScalarKind::U64), true)
            },
            |n: &NodeView<'_>| {
                assert_eq!(n.read_u64().unwrap(), 902);
            }
        );
        check!(
            1.25f32,
            f32,
            ty(TypeKind::Scalar(ScalarKind::F32), true),
            |n: &NodeView<'_>| {
                assert_eq!(n.read_f32().unwrap(), 1.25);
            }
        );
        check!(
            2.5f64,
            f64,
            ty(TypeKind::Scalar(ScalarKind::F64), true),
            |n: &NodeView<'_>| {
                assert_eq!(n.read_f64().unwrap(), 2.5);
            }
        );
    }
}

#[test]
fn builtin_static_shared_fields_use_body_codec_without_type_headers() {
    for compatible in [false, true] {
        macro_rules! check {([$($t:tt)+], $value:expr, $child:expr, $read:expr) => {{
            #[derive(Debug, PartialEq, fory::ForyStruct)]
            struct Fields {first: std::sync::Arc<$($t)+>, second: std::sync::Arc<$($t)+>}
            let mut f = fory::Fory::builder().xlang(true).compatible(compatible).check_struct_version(false).build();
            f.register::<Fields>(450).unwrap();
            let item = std::sync::Arc::new($value); let value = Fields {first: std::sync::Arc::clone(&item), second: item};
            let input = f.serialize(&value).unwrap(); let decoded = f.deserialize::<Fields>(&input).unwrap();
            assert!(std::sync::Arc::ptr_eq(&decoded.first, &decoded.second));
            let fields = vec![FieldDescriptor {id: None, name: "first".into(), ty: $child}, FieldDescriptor {id: None, name: "second".into(), ty: $child}];
            let schema = Schema::compile(SchemaDescriptor {
                encoding: RootEncoding {protocol_version: ProtocolVersion::Fory1_7, mode: if compatible {SchemaMode::Compatible} else {SchemaMode::SameSchema {version_hash: false}}, root_reference: RootReferenceMode::Tracking}, key_semantics_version: KEY_SEMANTICS_VERSION, root: ty(TypeKind::User(UserIdentity::Numeric(450)), false),
                definitions: vec![UserDefinition {identity: UserIdentity::Numeric(450), kind: UserDefinitionKind::Struct {fields, physical_field_order: vec![0, 1]}}],
            }, &SchemaLimits::default()).unwrap();
            let view = schema.view(&input, InputFormat::XlangMessage, &ParseLimits::default()).unwrap();
            let root = view.root().unwrap(); let first = root.field_at(0).unwrap(); let second = root.field_at(1).unwrap();
            let id = second.reference_id().unwrap().unwrap_or_else(|| panic!("carrier {} compatible {compatible} bytes {input:?}", stringify!($($t)+))); assert_eq!(first.object_id().unwrap().unwrap(), id);
            ($read)(&second.resolve_reference(&id).unwrap());
            drop((first, second)); drop(root);
            let validated = view.validate().unwrap();
            assert_eq!(validated.root().unwrap().field_at(1).unwrap().reference_id().unwrap().unwrap(), id);
        }};}
        check!(
            [String],
            "field".to_owned(),
            ty(TypeKind::Scalar(ScalarKind::String), true),
            |n: &NodeView<'_>| {
                assert_eq!(n.read_string().unwrap().as_str(), "field");
            }
        );
        check!(
            [Vec<String>],
            vec!["a".to_owned()],
            ty(
                TypeKind::List(Box::new(ty(TypeKind::Scalar(ScalarKind::String), false))),
                true
            ),
            |n: &NodeView<'_>| {
                assert_eq!(
                    n.element_at(0).unwrap().read_string().unwrap().as_str(),
                    "a"
                );
            }
        );
        check!(
            [std::collections::BTreeMap<String, String>],
            std::collections::BTreeMap::from([("k".to_owned(), "v".to_owned())]),
            ty(
                TypeKind::Map {
                    key: Box::new(ty(TypeKind::Scalar(ScalarKind::String), false)),
                    value: Box::new(ty(TypeKind::Scalar(ScalarKind::String), false))
                },
                true
            ),
            |n: &NodeView<'_>| {
                assert_eq!(n.len().unwrap(), 1);
            }
        );
        check!(
            [std::collections::BTreeSet<String>],
            std::collections::BTreeSet::from(["s".to_owned()]),
            ty(
                TypeKind::Set(Box::new(ty(TypeKind::Scalar(ScalarKind::String), false))),
                true
            ),
            |n: &NodeView<'_>| {
                assert_eq!(n.len().unwrap(), 1);
            }
        );
    }
}

#[derive(Debug, PartialEq, fory::ForyStruct)]
struct NonReferenceable {
    #[fory(ref = true)]
    number: bool,
    #[fory(ref = true)]
    text: String,
}

#[derive(Debug, PartialEq, fory::ForyStruct)]
struct NullableAlias {
    first: Rc<Item>,
    second: Option<Rc<Item>>,
}

#[test]
fn same_object_can_be_referenced_through_nullable_use_site() {
    for compatible in [false, true] {
        let mut f = fory::Fory::builder()
            .xlang(true)
            .compatible(compatible)
            .check_struct_version(false)
            .build();
        f.register::<Item>(400).unwrap();
        f.register::<NullableAlias>(452).unwrap();
        let item = Rc::new(Item { value: 29 });
        let input = f
            .serialize(&NullableAlias {
                first: Rc::clone(&item),
                second: Some(item),
            })
            .unwrap();
        let decoded = f.deserialize::<NullableAlias>(&input).unwrap();
        assert!(Rc::ptr_eq(&decoded.first, decoded.second.as_ref().unwrap()));
        let mut second = ty(TypeKind::User(UserIdentity::Numeric(400)), true);
        second.nullable = true;
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
                root: ty(TypeKind::User(UserIdentity::Numeric(452)), false),
                definitions: vec![
                    UserDefinition {
                        identity: UserIdentity::Numeric(452),
                        kind: UserDefinitionKind::Struct {
                            fields: vec![
                                FieldDescriptor {
                                    id: None,
                                    name: "first".into(),
                                    ty: ty(TypeKind::User(UserIdentity::Numeric(400)), true),
                                },
                                FieldDescriptor {
                                    id: None,
                                    name: "second".into(),
                                    ty: second,
                                },
                            ],
                            physical_field_order: vec![0, 1],
                        },
                    },
                    UserDefinition {
                        identity: UserIdentity::Numeric(400),
                        kind: UserDefinitionKind::Struct {
                            fields: vec![FieldDescriptor {
                                id: None,
                                name: "value".into(),
                                ty: TypeDescriptor {
                                    integer_encoding: Some(IntegerEncoding::Varint),
                                    ..ty(TypeKind::Scalar(ScalarKind::I32), false)
                                },
                            }],
                            physical_field_order: vec![0],
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
        let edge = root.field_at(1).unwrap();
        let id = edge.reference_id().unwrap().unwrap();
        assert_eq!(root.field_at(0).unwrap().object_id().unwrap().unwrap(), id);
        assert_eq!(
            edge.resolve_reference(&id)
                .unwrap()
                .field_at(0)
                .unwrap()
                .read_i32()
                .unwrap(),
            29
        );
        drop(edge);
        drop(root);
        view.validate().unwrap();
    }
}

#[test]
fn tracking_policy_does_not_invent_slots_for_plain_notnull_values() {
    for compatible in [false, true] {
        let mut f = fory::Fory::builder()
            .xlang(true)
            .compatible(compatible)
            .check_struct_version(false)
            .build();
        f.register::<NonReferenceable>(451).unwrap();
        let input = f
            .serialize(&NonReferenceable {
                number: true,
                text: "plain".into(),
            })
            .unwrap();
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
                root: ty(TypeKind::User(UserIdentity::Numeric(451)), false),
                definitions: vec![UserDefinition {
                    identity: UserIdentity::Numeric(451),
                    kind: UserDefinitionKind::Struct {
                        fields: vec![
                            FieldDescriptor {
                                id: None,
                                name: "number".into(),
                                ty: ty(TypeKind::Scalar(ScalarKind::Bool), true),
                            },
                            FieldDescriptor {
                                id: None,
                                name: "text".into(),
                                ty: ty(TypeKind::Scalar(ScalarKind::String), true),
                            },
                        ],
                        physical_field_order: vec![0, 1],
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
        for field in root.fields().unwrap() {
            assert!(field.unwrap().1.object_id().unwrap().is_none());
        }
        assert!(root.field_at(0).unwrap().read_bool().unwrap());
        assert_eq!(
            root.field_at(1).unwrap().read_string().unwrap().as_str(),
            "plain"
        );
        drop(root);
        view.validate().unwrap();
    }
}

#[test]
fn builtin_reference_rejects_wrong_body_type_and_preserves_first_failure() {
    let string = ty(TypeKind::Scalar(ScalarKind::String), true);
    let number = ty(TypeKind::Scalar(ScalarKind::Bool), true);
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
            root: ty(TypeKind::User(UserIdentity::Numeric(470)), false),
            definitions: vec![UserDefinition {
                identity: UserIdentity::Numeric(470),
                kind: UserDefinitionKind::Struct {
                    fields: vec![
                        FieldDescriptor {
                            id: None,
                            name: "text".into(),
                            ty: string,
                        },
                        FieldDescriptor {
                            id: None,
                            name: "number".into(),
                            ty: number,
                        },
                    ],
                    physical_field_order: vec![0, 1],
                },
            }],
        },
        &SchemaLimits::default(),
    )
    .unwrap();
    let mut input = vec![];
    let mut writer = fory::Writer::from_buffer(&mut input);
    writer.write_i8(-1);
    writer.write_var_u32(27);
    writer.write_var_u32(470);
    writer.write_i8(0);
    writer.write_var_u32(4);
    writer.write_u8(b'x');
    writer.write_i8(-2);
    writer.write_var_u32(0);
    let view = schema
        .view(&input, InputFormat::RootValue, &ParseLimits::default())
        .unwrap();
    let root = view.root().unwrap();
    let edge = root.field_at(1).unwrap();
    let error = edge.reference_id().unwrap_err();
    assert!(matches!(error, DataError::SchemaMismatch(_)));
    assert_eq!(
        format!("{error:?}"),
        format!("{:?}", root.is_null().unwrap_err())
    );
    drop(edge);
    drop(root);
    assert_eq!(
        format!("{error:?}"),
        format!("{:?}", view.validate().err().unwrap())
    );
}

#[derive(Debug, PartialEq, fory::ForyStruct)]
struct PlainTrackedInteger {
    #[fory(ref = true)]
    value: i32,
}
#[derive(Debug, PartialEq, fory::ForyStruct)]
struct SharedTrackedInteger {
    value: std::sync::Arc<i32>,
}

#[derive(Debug, PartialEq, fory::ForyStruct)]
struct SharedBinaryFields {
    #[fory(bytes)]
    first: std::sync::Arc<Vec<u8>>,
    #[fory(bytes)]
    second: std::sync::Arc<Vec<u8>>,
}

#[test]
fn explicit_binary_static_arc_fields_keep_content_and_identity() {
    for compatible in [false, true] {
        let mut f = fory::Fory::builder()
            .xlang(true)
            .compatible(compatible)
            .check_struct_version(false)
            .build();
        f.register::<SharedBinaryFields>(481).unwrap();
        let value = std::sync::Arc::new(vec![9u8, 8]);
        let input = f
            .serialize(&SharedBinaryFields {
                first: std::sync::Arc::clone(&value),
                second: value,
            })
            .unwrap();
        let decoded = f.deserialize::<SharedBinaryFields>(&input).unwrap();
        assert!(std::sync::Arc::ptr_eq(&decoded.first, &decoded.second));
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
                root: ty(TypeKind::User(UserIdentity::Numeric(481)), false),
                definitions: vec![UserDefinition {
                    identity: UserIdentity::Numeric(481),
                    kind: UserDefinitionKind::Struct {
                        fields: vec![
                            FieldDescriptor {
                                id: None,
                                name: "first".into(),
                                ty: ty(TypeKind::Scalar(ScalarKind::Binary), true),
                            },
                            FieldDescriptor {
                                id: None,
                                name: "second".into(),
                                ty: ty(TypeKind::Scalar(ScalarKind::Binary), true),
                            },
                        ],
                        physical_field_order: vec![0, 1],
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
        let first = root.field_at(0).unwrap();
        let edge = root.field_at(1).unwrap();
        let id = edge.reference_id().unwrap().unwrap();
        assert_eq!(first.object_id().unwrap().unwrap(), id);
        let owned = edge.to_owned().unwrap();
        assert_eq!(
            edge.resolve_reference(&id)
                .unwrap()
                .read_binary()
                .unwrap()
                .as_bytes(),
            &[9, 8]
        );
        drop((first, edge));
        drop(root);
        view.validate().unwrap();
        drop(input);
        assert_eq!(
            owned
                .as_view()
                .unwrap()
                .resolve_reference(&id)
                .unwrap()
                .read_binary()
                .unwrap()
                .as_bytes(),
            &[9, 8]
        );
    }
}

#[test]
fn static_integer_carrier_framing_is_proven_ambiguous_not_guessed() {
    for compatible in [false, true] {
        let mut plain = fory::Fory::builder()
            .xlang(true)
            .compatible(compatible)
            .check_struct_version(false)
            .build();
        let mut shared = fory::Fory::builder()
            .xlang(true)
            .compatible(compatible)
            .check_struct_version(false)
            .build();
        plain.register::<PlainTrackedInteger>(480).unwrap();
        shared.register::<SharedTrackedInteger>(480).unwrap();
        let a = plain.serialize(&PlainTrackedInteger { value: 17 }).unwrap();
        let b = shared
            .serialize(&SharedTrackedInteger {
                value: std::sync::Arc::new(17),
            })
            .unwrap();
        assert_eq!(
            plain.deserialize::<PlainTrackedInteger>(&a).unwrap().value,
            17
        );
        assert_eq!(
            *shared
                .deserialize::<SharedTrackedInteger>(&b)
                .unwrap()
                .value,
            17
        );
        assert_eq!(b.len(), a.len() + 1);
        assert_eq!(&a[..a.len() - 1], &b[..b.len() - 2]);
        assert_eq!(b[b.len() - 2], 0);
        assert_eq!(a[a.len() - 1], b[b.len() - 1]);
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
                root: ty(TypeKind::User(UserIdentity::Numeric(480)), false),
                definitions: vec![UserDefinition {
                    identity: UserIdentity::Numeric(480),
                    kind: UserDefinitionKind::Struct {
                        fields: vec![FieldDescriptor {
                            id: None,
                            name: "value".into(),
                            ty: TypeDescriptor {
                                integer_encoding: Some(IntegerEncoding::Varint),
                                ..ty(TypeKind::Scalar(ScalarKind::I32), true)
                            },
                        }],
                        physical_field_order: vec![0],
                    },
                }],
            },
            &SchemaLimits::default(),
        )
        .unwrap();
        for input in [&a, &b] {
            assert!(matches!(
                schema.validate(input, InputFormat::XlangMessage, &ParseLimits::default()),
                Err(DataError::NotImplemented(
                    "ambiguous tracked static integer field framing"
                ))
            ));
        }
    }
}
