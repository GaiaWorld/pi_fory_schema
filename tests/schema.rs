use pi_fory_schema::*;

fn user(id: u32) -> TypeDescriptor {
    ty(TypeKind::User(UserIdentity::Numeric(id)))
}
fn field(name: &str, id: Option<u32>, ty: TypeDescriptor) -> FieldDescriptor {
    FieldDescriptor {
        name: name.into(),
        id,
        ty,
    }
}
fn structure(
    identity: UserIdentity,
    fields: Vec<FieldDescriptor>,
    order: Vec<usize>,
) -> UserDefinition {
    UserDefinition {
        identity,
        kind: UserDefinitionKind::Struct {
            fields,
            physical_field_order: order,
        },
    }
}
fn compile(d: SchemaDescriptor) -> Result<Schema, SchemaError> {
    Schema::compile(d, &SchemaLimits::default())
}
fn invalid(d: SchemaDescriptor) {
    match compile(d) {
        Err(SchemaError::InvalidSchema(location)) => assert_eq!(location.byte_offset, None),
        Err(other) => panic!("unexpected error: {other:?}"),
        Ok(_) => panic!("invalid declaration accepted"),
    }
}
fn resource(d: SchemaDescriptor, limits: &SchemaLimits) {
    assert!(matches!(
        Schema::compile(d, limits),
        Err(SchemaError::ResourceLimit(_))
    ));
}

#[test]
fn nested_and_mutually_recursive_definitions_support_all_key_requirements() {
    let d = descriptor(
        ty(TypeKind::List(Box::new(user(0)))),
        vec![
            structure(
                UserIdentity::Numeric(0),
                vec![
                    field("next", None, user(u32::MAX)),
                    field("again", Some(0), user(0)),
                ],
                vec![1, 0],
            ),
            structure(
                UserIdentity::Numeric(u32::MAX),
                vec![field("back", None, user(0))],
                vec![0],
            ),
        ],
    );
    let schema = compile(d).unwrap();
    for requirement in [
        KeyRequirement::Hash,
        KeyRequirement::Ordered,
        KeyRequirement::HashAndOrdered,
    ] {
        schema.clone().check_key_support(requirement).unwrap();
    }
}

#[test]
fn explicit_integer_encoding_matrix() {
    for scalar in [
        ScalarKind::I32,
        ScalarKind::U32,
        ScalarKind::I64,
        ScalarKind::U64,
    ] {
        for encoding in [
            None,
            Some(IntegerEncoding::Fixed),
            Some(IntegerEncoding::Varint),
            Some(IntegerEncoding::Tagged),
        ] {
            let mut root = ty(TypeKind::Scalar(scalar));
            root.integer_encoding = encoding;
            let valid = encoding.is_some()
                && !(matches!(scalar, ScalarKind::I32 | ScalarKind::U32)
                    && encoding == Some(IntegerEncoding::Tagged));
            let d = descriptor(root, vec![]);
            if valid {
                compile(d).unwrap();
            } else {
                invalid(d);
            }
        }
    }
    for scalar in [
        ScalarKind::Bool,
        ScalarKind::I8,
        ScalarKind::I16,
        ScalarKind::U8,
        ScalarKind::U16,
        ScalarKind::F32,
        ScalarKind::F64,
        ScalarKind::String,
        ScalarKind::Binary,
        ScalarKind::Date,
        ScalarKind::Timestamp,
        ScalarKind::Duration,
    ] {
        let mut root = ty(TypeKind::Scalar(scalar));
        compile(descriptor(root.clone(), vec![])).unwrap();
        root.integer_encoding = Some(IntegerEncoding::Fixed);
        invalid(descriptor(root, vec![]));
    }
    let mut root = ty(TypeKind::List(Box::new(ty(TypeKind::Scalar(
        ScalarKind::I32,
    )))));
    compile(descriptor(root.clone(), vec![])).unwrap();
    root.integer_encoding = Some(IntegerEncoding::Varint);
    invalid(descriptor(root, vec![]));
    let mut bad_child = ty(TypeKind::Scalar(ScalarKind::I64));
    bad_child.integer_encoding = None;
    invalid(descriptor(
        ty(TypeKind::Array {
            element: Box::new(bad_child),
            length: Some(u64::MAX),
        }),
        vec![],
    ));
}

#[test]
fn unsupported_key_semantics_version_is_classified() {
    for version in [0, 2, u32::MAX] {
        let mut d = descriptor(ty(TypeKind::Scalar(ScalarKind::Bool)), vec![]);
        d.key_semantics_version = version;
        assert!(matches!(
            compile(d),
            Err(SchemaError::UnsupportedVersion(_))
        ));
    }
}

#[test]
fn definitions_must_be_exactly_reachable_without_duplicates_or_missing_edges() {
    invalid(descriptor(user(1), vec![]));
    let empty = structure(UserIdentity::Numeric(1), vec![], vec![]);
    compile(descriptor(user(1), vec![empty.clone()])).unwrap();
    invalid(descriptor(user(1), vec![empty.clone(), empty.clone()]));
    invalid(descriptor(
        ty(TypeKind::Scalar(ScalarKind::Bool)),
        vec![empty.clone()],
    ));
    let missing = structure(
        UserIdentity::Numeric(1),
        vec![field(
            "missing",
            None,
            ty(TypeKind::Map {
                key: Box::new(user(2)),
                value: Box::new(user(1)),
            }),
        )],
        vec![0],
    );
    invalid(descriptor(user(1), vec![missing]));
    let detached_cycle = structure(
        UserIdentity::Numeric(2),
        vec![field("self", None, user(2))],
        vec![0],
    );
    invalid(descriptor(user(1), vec![empty, detached_cycle]));
}

#[test]
fn names_follow_registry_exact_utf8_component_rules() {
    for (namespace, type_name) in [
        ("".into(), "".into()),
        ("".into(), "a.b".into()),
        ("n".repeat(32767), "A".into()),
        ("".into(), "é".repeat(16384)),
    ] {
        let identity = UserIdentity::Name {
            namespace,
            type_name,
        };
        invalid(descriptor(
            ty(TypeKind::User(identity.clone())),
            vec![structure(identity, vec![], vec![])],
        ));
    }
    for (namespace, type_name) in [
        (" a.b ".into(), " É ".into()),
        ("".into(), "A".into()),
        ("é".repeat(16383), "a".repeat(32766)),
    ] {
        let identity = UserIdentity::Name {
            namespace,
            type_name,
        };
        compile(descriptor(
            ty(TypeKind::User(identity.clone())),
            vec![structure(identity, vec![], vec![])],
        ))
        .unwrap();
    }
    let identity = UserIdentity::Name {
        namespace: "".into(),
        type_name: "1".into(),
    };
    let named = structure(
        identity.clone(),
        vec![field("numeric", None, user(1))],
        vec![0],
    );
    compile(descriptor(
        ty(TypeKind::User(identity)),
        vec![named, structure(UserIdentity::Numeric(1), vec![], vec![])],
    ))
    .unwrap();
}

#[test]
fn physical_order_is_a_complete_permutation_not_an_inferred_sort() {
    for order in [vec![], vec![0], vec![0, 0], vec![0, 2], vec![0, 1, 2]] {
        invalid(descriptor(
            user(1),
            vec![structure(
                UserIdentity::Numeric(1),
                vec![
                    field("z", None, user(1)),
                    field("a", Some(u32::MAX), user(1)),
                ],
                order,
            )],
        ));
    }
    for order in [vec![0, 1], vec![1, 0]] {
        compile(descriptor(
            user(1),
            vec![structure(
                UserIdentity::Numeric(1),
                vec![
                    field("z", None, user(1)),
                    field("a", Some(u32::MAX), user(1)),
                ],
                order,
            )],
        ))
        .unwrap();
    }
    invalid(descriptor(
        user(1),
        vec![structure(UserIdentity::Numeric(1), vec![], vec![0])],
    ));
}

#[test]
fn duplicate_field_ids_names_and_empty_names_are_rejected() {
    for fields in [
        vec![field("a", Some(7), user(1)), field("b", Some(7), user(1))],
        vec![field("a", None, user(1)), field("a", Some(8), user(1))],
        vec![field("", None, user(1)), field("b", None, user(1))],
    ] {
        invalid(descriptor(
            user(1),
            vec![structure(UserIdentity::Numeric(1), fields, vec![1, 0])],
        ));
    }
    compile(descriptor(
        user(1),
        vec![structure(
            UserIdentity::Numeric(1),
            vec![
                field(" a.b ", Some(0), user(1)),
                field("É", Some(u32::MAX), user(1)),
            ],
            vec![1, 0],
        )],
    ))
    .unwrap();
}

fn variant(id: u32, name: &str, payload: Option<TypeDescriptor>) -> VariantDescriptor {
    VariantDescriptor {
        id,
        name: name.into(),
        payload,
    }
}
#[test]
fn enum_union_categories_variant_ids_names_and_payloads() {
    for union in [false, true] {
        for variants in [
            vec![variant(1, "a", None), variant(1, "b", None)],
            vec![variant(1, "a", None), variant(2, "a", None)],
            vec![variant(1, "", None)],
        ] {
            let kind = if union {
                UserDefinitionKind::Union { variants }
            } else {
                UserDefinitionKind::Enum { variants }
            };
            invalid(descriptor(
                user(1),
                vec![UserDefinition {
                    identity: UserIdentity::Numeric(1),
                    kind,
                }],
            ));
        }
        let variants = vec![variant(u32::MAX, "last", None), variant(0, "first", None)];
        let kind = if union {
            UserDefinitionKind::Union { variants }
        } else {
            UserDefinitionKind::Enum { variants }
        };
        compile(descriptor(
            user(1),
            vec![UserDefinition {
                identity: UserIdentity::Numeric(1),
                kind,
            }],
        ))
        .unwrap();
    }
    invalid(descriptor(
        user(1),
        vec![UserDefinition {
            identity: UserIdentity::Numeric(1),
            kind: UserDefinitionKind::Enum {
                variants: vec![variant(0, "payload", Some(user(1)))],
            },
        }],
    ));
    let d = descriptor(
        user(1),
        vec![UserDefinition {
            identity: UserIdentity::Numeric(1),
            kind: UserDefinitionKind::Union {
                variants: vec![
                    variant(0, "recursive", Some(user(1))),
                    variant(2, "unit", None),
                ],
            },
        }],
    );
    compile(d)
        .unwrap()
        .check_key_support(KeyRequirement::Ordered)
        .unwrap();
}

#[test]
fn all_four_compile_limits_are_inclusive_and_json_limits_do_not_apply() {
    let identity = UserIdentity::Name {
        namespace: "é".into(),
        type_name: "A".into(),
    };
    let d = descriptor(
        ty(TypeKind::User(identity.clone())),
        vec![structure(
            identity.clone(),
            vec![field(
                "x",
                None,
                ty(TypeKind::List(Box::new(ty(TypeKind::User(identity))))),
            )],
            vec![0],
        )],
    );
    // Three identity occurrences (3 UTF-8 bytes each), one field name.
    let limits = SchemaLimits {
        max_document_bytes: 0,
        max_json_depth: 0,
        max_definitions: 1,
        max_type_nodes: 3,
        max_members: 1,
        max_metadata_bytes: 10,
    };
    Schema::compile(d.clone(), &limits).unwrap();
    for smaller in [
        SchemaLimits {
            max_definitions: 0,
            ..limits.clone()
        },
        SchemaLimits {
            max_type_nodes: 2,
            ..limits.clone()
        },
        SchemaLimits {
            max_members: 0,
            ..limits.clone()
        },
        SchemaLimits {
            max_metadata_bytes: 9,
            ..limits.clone()
        },
    ] {
        resource(d.clone(), &smaller);
    }
    let zero = SchemaLimits {
        max_definitions: 0,
        max_members: 0,
        max_metadata_bytes: 0,
        max_type_nodes: 1,
        ..limits
    };
    Schema::compile(
        descriptor(ty(TypeKind::Scalar(ScalarKind::Bool)), vec![]),
        &zero,
    )
    .unwrap();
    resource(
        descriptor(ty(TypeKind::Scalar(ScalarKind::Bool)), vec![]),
        &SchemaLimits {
            max_type_nodes: 0,
            ..zero
        },
    );
}

#[test]
fn rejected_unreachable_and_duplicate_entries_still_consume_budget() {
    let identity = UserIdentity::Name {
        namespace: "".into(),
        type_name: "A".into(),
    };
    let definition = structure(
        identity,
        vec![field("x", None, ty(TypeKind::Scalar(ScalarKind::Bool)))],
        vec![0],
    );
    for count in [1, 2] {
        let d = descriptor(
            ty(TypeKind::Scalar(ScalarKind::Bool)),
            vec![definition.clone(); count],
        );
        for limits in [
            SchemaLimits {
                max_definitions: count - 1,
                ..SchemaLimits::default()
            },
            SchemaLimits {
                max_members: count - 1,
                ..SchemaLimits::default()
            },
            SchemaLimits {
                max_type_nodes: count,
                ..SchemaLimits::default()
            },
            SchemaLimits {
                max_metadata_bytes: count * 2 - 1,
                ..SchemaLimits::default()
            },
        ] {
            resource(d.clone(), &limits);
        }
        invalid(d);
    }
    let unit = descriptor(
        user(1),
        vec![UserDefinition {
            identity: UserIdentity::Numeric(1),
            kind: UserDefinitionKind::Enum {
                variants: vec![variant(0, "unit", None)],
            },
        }],
    );
    resource(
        unit,
        &SchemaLimits {
            max_members: 0,
            ..SchemaLimits::default()
        },
    );
}

#[test]
fn key_capabilities_reject_tracking_maps_sets_and_all_time_scalars() {
    let boolean = || ty(TypeKind::Scalar(ScalarKind::Bool));
    let mut tracked = boolean();
    tracked.track_ref = true;
    for root in [
        tracked,
        ty(TypeKind::Map {
            key: Box::new(boolean()),
            value: Box::new(boolean()),
        }),
        ty(TypeKind::Set(Box::new(boolean()))),
        ty(TypeKind::Scalar(ScalarKind::Date)),
        ty(TypeKind::Scalar(ScalarKind::Timestamp)),
        ty(TypeKind::Scalar(ScalarKind::Duration)),
    ] {
        for requirement in [
            KeyRequirement::Hash,
            KeyRequirement::Ordered,
            KeyRequirement::HashAndOrdered,
        ] {
            let schema = compile(descriptor(
                ty(TypeKind::Array {
                    element: Box::new(root.clone()),
                    length: None,
                }),
                vec![],
            ))
            .unwrap();
            assert!(matches!(
                schema.check_key_support(requirement),
                Err(SchemaError::UnsupportedKeyCapability(_))
            ));
        }
    }
    let d = descriptor(
        user(1),
        vec![structure(
            UserIdentity::Numeric(1),
            vec![
                field("cycle", None, user(1)),
                field("date", None, ty(TypeKind::Scalar(ScalarKind::Date))),
            ],
            vec![1, 0],
        )],
    );
    assert!(matches!(
        compile(d).unwrap().check_key_support(KeyRequirement::Hash),
        Err(SchemaError::UnsupportedKeyCapability(_))
    ));
}

#[test]
fn non_time_scalars_nullable_lists_and_arrays_support_keys() {
    for scalar in [
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
    ] {
        let mut element = ty(TypeKind::Scalar(scalar));
        element.nullable = true;
        let root = ty(TypeKind::List(Box::new(ty(TypeKind::Array {
            element: Box::new(element),
            length: Some(0),
        }))));
        let schema = compile(descriptor(root, vec![])).unwrap();
        for requirement in [
            KeyRequirement::Hash,
            KeyRequirement::Ordered,
            KeyRequirement::HashAndOrdered,
        ] {
            schema.check_key_support(requirement).unwrap();
        }
    }
}

#[test]
fn deep_inline_list_traversal_has_an_exact_node_budget() {
    let mut root = ty(TypeKind::Scalar(ScalarKind::Bool));
    for _ in 0..512 {
        root = ty(TypeKind::List(Box::new(root)));
    }
    let d = descriptor(root, vec![]);
    let limits = SchemaLimits {
        max_type_nodes: 513,
        ..SchemaLimits::default()
    };
    Schema::compile(d.clone(), &limits)
        .unwrap()
        .check_key_support(KeyRequirement::Ordered)
        .unwrap();
    resource(
        d,
        &SchemaLimits {
            max_type_nodes: 512,
            ..limits
        },
    );
}

#[test]
fn repeated_user_tracking_is_checked_even_after_nontracking_discovery() {
    let mut tracked = user(2);
    tracked.track_ref = true;
    let schema = compile(descriptor(
        user(1),
        vec![
            structure(
                UserIdentity::Numeric(1),
                vec![
                    field("plain", None, user(2)),
                    field("tracked", None, tracked),
                ],
                vec![1, 0],
            ),
            structure(UserIdentity::Numeric(2), vec![], vec![]),
        ],
    ))
    .unwrap();
    for requirement in [
        KeyRequirement::Hash,
        KeyRequirement::Ordered,
        KeyRequirement::HashAndOrdered,
    ] {
        match schema.check_key_support(requirement) {
            Err(SchemaError::UnsupportedKeyCapability(location)) => {
                assert_eq!(location.descriptor_index, Some(0));
                assert_eq!(location.byte_offset, None);
            }
            other => panic!("tracking use was not rejected: {other:?}"),
        }
    }
}

#[test]
fn unreachable_definitions_are_validated_before_exact_reachability_rejection() {
    let mut bad_encoding = ty(TypeKind::Scalar(ScalarKind::I32));
    bad_encoding.integer_encoding = None;
    let malformed = [
        structure(
            UserIdentity::Name {
                namespace: "".into(),
                type_name: "bad.name".into(),
            },
            vec![],
            vec![],
        ),
        structure(
            UserIdentity::Numeric(2),
            vec![field("x", None, user(2))],
            vec![],
        ),
        structure(
            UserIdentity::Numeric(2),
            vec![field("x", Some(7), user(2)), field("y", Some(7), user(2))],
            vec![0, 1],
        ),
        structure(
            UserIdentity::Numeric(2),
            vec![field("x", None, user(2)), field("x", None, user(2))],
            vec![0, 1],
        ),
        structure(
            UserIdentity::Numeric(2),
            vec![field("x", None, bad_encoding)],
            vec![0],
        ),
        structure(
            UserIdentity::Numeric(2),
            vec![field("missing", None, user(99))],
            vec![0],
        ),
        UserDefinition {
            identity: UserIdentity::Numeric(2),
            kind: UserDefinitionKind::Enum {
                variants: vec![variant(0, "payload", Some(user(2)))],
            },
        },
        UserDefinition {
            identity: UserIdentity::Numeric(2),
            kind: UserDefinitionKind::Union {
                variants: vec![variant(7, "a", None), variant(7, "b", None)],
            },
        },
    ];
    for bad in malformed {
        // Index 0 is itself unreachable but valid. Reporting index 1 proves
        // validation did not stop at the first unreachable definition.
        let d = descriptor(
            ty(TypeKind::Scalar(ScalarKind::Bool)),
            vec![structure(UserIdentity::Numeric(1), vec![], vec![]), bad],
        );
        match compile(d) {
            Err(SchemaError::InvalidSchema(location)) => {
                assert_eq!(location.descriptor_index, Some(1));
                assert_eq!(location.byte_offset, None);
            }
            Err(other) => panic!("unexpected error: {other:?}"),
            Ok(_) => panic!("unreachable invalid definition accepted"),
        }
    }
}

fn nested_branch_descriptor(branch: usize, child: TypeDescriptor) -> SchemaDescriptor {
    match branch {
        0 => descriptor(
            ty(TypeKind::Map {
                key: Box::new(ty(TypeKind::Scalar(ScalarKind::Bool))),
                value: Box::new(child),
            }),
            vec![],
        ),
        1 => descriptor(ty(TypeKind::Set(Box::new(child))), vec![]),
        2 => descriptor(
            user(1),
            vec![UserDefinition {
                identity: UserIdentity::Numeric(1),
                kind: UserDefinitionKind::Union {
                    variants: vec![variant(0, "unit", None), variant(1, "payload", Some(child))],
                },
            }],
        ),
        _ => panic!("unknown test branch"),
    }
}

#[test]
fn map_value_set_element_and_union_payload_are_checked_for_integer_encoding() {
    for branch in 0..3 {
        compile(nested_branch_descriptor(
            branch,
            ty(TypeKind::Scalar(ScalarKind::I32)),
        ))
        .unwrap();
        for scalar in [ScalarKind::I32, ScalarKind::Bool] {
            let mut child = ty(TypeKind::Scalar(scalar));
            child.integer_encoding = if matches!(scalar, ScalarKind::I32) {
                None
            } else {
                Some(IntegerEncoding::Fixed)
            };
            match compile(nested_branch_descriptor(branch, child)) {
                Err(SchemaError::InvalidSchema(location)) => {
                    assert_eq!(
                        location.descriptor_index,
                        if branch == 2 { Some(0) } else { None }
                    );
                    assert_eq!(location.byte_offset, None);
                }
                Err(other) => panic!("unexpected error: {other:?}"),
                Ok(_) => panic!("invalid nested encoding accepted"),
            }
        }
    }
}

#[test]
fn map_value_set_element_and_union_payload_capability_routes_are_rejected() {
    for branch in 0..3 {
        for tracked in [false, true] {
            let mut child = ty(TypeKind::Scalar(if tracked {
                ScalarKind::Bool
            } else {
                ScalarKind::Timestamp
            }));
            child.track_ref = tracked;
            let schema = compile(nested_branch_descriptor(branch, child)).unwrap();
            for requirement in [
                KeyRequirement::Hash,
                KeyRequirement::Ordered,
                KeyRequirement::HashAndOrdered,
            ] {
                match schema.check_key_support(requirement) {
                    Err(SchemaError::UnsupportedKeyCapability(location)) => {
                        assert_eq!(
                            location.descriptor_index,
                            if branch == 2 { Some(0) } else { None }
                        );
                        assert_eq!(location.byte_offset, None);
                    }
                    other => panic!("unsupported capability accepted: {other:?}"),
                }
            }
        }
    }
    // Map/set are unsupported at the container itself. These cases do not
    // claim that key checking visits their child; union proves payload descent.
    compile(nested_branch_descriptor(
        2,
        ty(TypeKind::Scalar(ScalarKind::Bool)),
    ))
    .unwrap()
    .check_key_support(KeyRequirement::HashAndOrdered)
    .unwrap();
}

#[test]
fn mutual_user_recursion_counts_each_declared_type_occurrence_once() {
    let d = descriptor(
        user(1),
        vec![
            structure(
                UserIdentity::Numeric(1),
                vec![field("next", None, user(2)), field("repeat", None, user(2))],
                vec![1, 0],
            ),
            structure(
                UserIdentity::Numeric(2),
                vec![field("back", None, user(1)), field("self", None, user(2))],
                vec![0, 1],
            ),
        ],
    );
    // Root plus four field occurrences, regardless of repeated and cyclic edges.
    let limits = SchemaLimits {
        max_type_nodes: 5,
        ..SchemaLimits::default()
    };
    Schema::compile(d.clone(), &limits)
        .unwrap()
        .check_key_support(KeyRequirement::HashAndOrdered)
        .unwrap();
    resource(
        d,
        &SchemaLimits {
            max_type_nodes: 4,
            ..limits
        },
    );
}

fn ty(kind: TypeKind) -> TypeDescriptor {
    TypeDescriptor {
        nullable: false,
        track_ref: false,
        integer_encoding: match &kind {
            TypeKind::Scalar(
                ScalarKind::I32 | ScalarKind::U32 | ScalarKind::I64 | ScalarKind::U64,
            ) => Some(IntegerEncoding::Varint),
            _ => None,
        },
        kind,
    }
}
fn descriptor(root: TypeDescriptor, definitions: Vec<UserDefinition>) -> SchemaDescriptor {
    SchemaDescriptor {
        encoding: RootEncoding {
            protocol_version: ProtocolVersion::Fory1_7,
            mode: SchemaMode::Compatible,
            root_reference: RootReferenceMode::NullOnly,
        },
        key_semantics_version: KEY_SEMANTICS_VERSION,
        root,
        definitions,
    }
}

#[test]
fn compile_simple_in_each_explicit_root_encoding() {
    for mode in [
        SchemaMode::Compatible,
        SchemaMode::SameSchema {
            version_hash: false,
        },
        SchemaMode::SameSchema { version_hash: true },
    ] {
        for root_reference in [RootReferenceMode::NullOnly, RootReferenceMode::Tracking] {
            let mut d = descriptor(ty(TypeKind::Scalar(ScalarKind::I32)), vec![]);
            d.encoding.mode = mode;
            d.encoding.root_reference = root_reference;
            assert!(matches!(
                d.encoding.protocol_version,
                ProtocolVersion::Fory1_7
            ));
            let schema = Schema::compile(d, &SchemaLimits::default()).unwrap();
            schema
                .check_key_support(KeyRequirement::HashAndOrdered)
                .unwrap();
        }
    }
}
