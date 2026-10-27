use pi_fory_schema::*;

#[derive(Debug, PartialEq, fory::ForyStruct)]
struct Record {
    value: i32,
}

impl DescribeSchema for Record {
    fn describe_type(
        _: &RootEncoding,
        registry: &SchemaRegistry,
    ) -> Result<TypeDescriptor, SchemaError> {
        Ok(TypeDescriptor {
            nullable: false,
            track_ref: false,
            integer_encoding: None,
            kind: TypeKind::User(registry.identity_of::<Self>()?.clone()),
        })
    }
    fn describe_definition(
        _: &RootEncoding,
        registry: &SchemaRegistry,
    ) -> Result<Option<UserDefinition>, SchemaError> {
        Ok(Some(UserDefinition {
            identity: registry.identity_of::<Self>()?.clone(),
            kind: UserDefinitionKind::Struct {
                physical_field_order: vec![0],
                fields: vec![FieldDescriptor {
                    id: None,
                    name: "value".into(),
                    ty: TypeDescriptor {
                        nullable: false,
                        track_ref: false,
                        integer_encoding: Some(IntegerEncoding::Varint),
                        kind: TypeKind::Scalar(ScalarKind::I32),
                    },
                }],
            },
        }))
    }
}

#[test]
fn numeric_registration_is_idempotent_and_queryable() {
    let mut builder = SchemaRegistryBuilder::new();
    builder.register_by_id::<Record>(100).unwrap();
    builder.register_by_id::<Record>(100).unwrap();
    let registry = builder.build().unwrap();
    assert_eq!(
        registry.identity_of::<Record>().unwrap(),
        &UserIdentity::Numeric(100)
    );
}

fn encoding(mode: SchemaMode) -> RootEncoding {
    RootEncoding {
        protocol_version: ProtocolVersion::Fory1_7,
        mode,
        root_reference: RootReferenceMode::NullOnly,
    }
}

fn assert_kind(error: SchemaError, expected: RegistryErrorKind) -> ErrorLocation {
    match error {
        SchemaError::Registry { kind, location } => {
            assert_eq!(kind, expected);
            location
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[derive(fory::ForyStruct)]
struct Other {
    value: i32,
}
impl DescribeSchema for Other {
    fn describe_type(
        _: &RootEncoding,
        registry: &SchemaRegistry,
    ) -> Result<TypeDescriptor, SchemaError> {
        Ok(ty(TypeKind::User(registry.identity_of::<Self>()?.clone())))
    }
    fn describe_definition(
        _: &RootEncoding,
        registry: &SchemaRegistry,
    ) -> Result<Option<UserDefinition>, SchemaError> {
        Ok(Some(UserDefinition {
            identity: registry.identity_of::<Self>()?.clone(),
            kind: UserDefinitionKind::Struct {
                fields: vec![],
                physical_field_order: vec![],
            },
        }))
    }
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

#[test]
fn conflicts_are_classified_without_changing_existing_facts() {
    let mut b = SchemaRegistryBuilder::default();
    b.register_by_id::<Record>(0).unwrap();
    assert_kind(
        b.register_by_id::<Record>(u32::MAX).unwrap_err(),
        RegistryErrorKind::TypeIdentityConflict,
    );
    assert_kind(
        b.register_by_name::<Record>("", "Record").unwrap_err(),
        RegistryErrorKind::TypeIdentityConflict,
    );
    assert_kind(
        b.register_by_id::<Other>(0).unwrap_err(),
        RegistryErrorKind::IdentityTypeConflict,
    );
    let r = b.build().unwrap();
    assert_eq!(
        r.identity_of::<Record>().unwrap(),
        &UserIdentity::Numeric(0)
    );
    assert_kind(
        r.identity_of::<Other>().unwrap_err(),
        RegistryErrorKind::UnregisteredType,
    );
    let mut b = SchemaRegistryBuilder::new();
    b.register_by_id::<Record>(u32::MAX).unwrap();
    assert_eq!(
        b.build().unwrap().identity_of::<Record>().unwrap(),
        &UserIdentity::Numeric(u32::MAX)
    );
}

#[test]
fn names_preserve_exact_components_and_enforce_utf8_boundaries() {
    let mut b = SchemaRegistryBuilder::new();
    for (ns, name) in [
        ("", ""),
        ("", "a.b"),
        (&"n".repeat(32767), "A"),
        ("", &"é".repeat(16384)),
    ] {
        assert_kind(
            b.register_by_name::<Record>(ns, name).unwrap_err(),
            RegistryErrorKind::InvalidIdentity,
        );
    }
    b.register_by_name::<Record>(" a.b ", " É ").unwrap();
    b.register_by_name::<Record>(" a.b ", " É ").unwrap();
    assert_kind(
        b.register_by_name::<Other>(" a.b ", " É ").unwrap_err(),
        RegistryErrorKind::IdentityTypeConflict,
    );
    assert_kind(
        b.register_by_name::<Record>("a.b", " É ").unwrap_err(),
        RegistryErrorKind::TypeIdentityConflict,
    );
    assert_eq!(
        b.build().unwrap().identity_of::<Record>().unwrap(),
        &UserIdentity::Name {
            namespace: " a.b ".into(),
            type_name: " É ".into()
        }
    );
    let mut b = SchemaRegistryBuilder::new();
    b.register_by_name::<Record>(&"é".repeat(16383), &"a".repeat(32766))
        .unwrap();
    b.build().unwrap();
}

#[test]
fn missing_root_and_empty_builtin_root() {
    let r = SchemaRegistryBuilder::new().build().unwrap();
    assert_kind(
        r.describe::<Record>(&encoding(SchemaMode::Compatible))
            .unwrap_err(),
        RegistryErrorKind::UnregisteredType,
    );
    let d = r
        .describe::<Builtin>(&encoding(SchemaMode::Compatible))
        .unwrap();
    assert!(matches!(d.root.kind, TypeKind::Scalar(ScalarKind::I32)));
    assert!(d.definitions.is_empty());
}
struct Builtin;
impl DescribeSchema for Builtin {
    fn describe_type(_: &RootEncoding, _: &SchemaRegistry) -> Result<TypeDescriptor, SchemaError> {
        Ok(ty(TypeKind::Scalar(ScalarKind::I32)))
    }
    fn describe_definition(
        _: &RootEncoding,
        _: &SchemaRegistry,
    ) -> Result<Option<UserDefinition>, SchemaError> {
        Ok(None)
    }
}

#[test]
fn root_description_and_adapter_preserve_encoding() {
    let mut b = SchemaRegistryBuilder::new();
    b.register_by_id::<Record>(100).unwrap();
    b.register_by_id::<Other>(101).unwrap();
    let r = b.build().unwrap();
    for mode in [
        SchemaMode::Compatible,
        SchemaMode::SameSchema {
            version_hash: false,
        },
        SchemaMode::SameSchema { version_hash: true },
    ] {
        let e = encoding(mode);
        for d in [
            r.describe::<Record>(&e).unwrap(),
            Record::describe_schema(&e, &r).unwrap(),
            fory_adapter::describe::<Record>(&r, &e).unwrap(),
        ] {
            assert_eq!(d.key_semantics_version, KEY_SEMANTICS_VERSION);
            assert_eq!(format!("{:?}", d.encoding), format!("{e:?}"));
            assert!(matches!(
                d.root.kind,
                TypeKind::User(UserIdentity::Numeric(100))
            ));
            assert_eq!(d.definitions.len(), 1);
            assert_eq!(d.definitions[0].identity, UserIdentity::Numeric(100));
            let UserDefinitionKind::Struct { fields, .. } = &d.definitions[0].kind else {
                panic!("not struct")
            };
            assert_eq!(fields[0].name, "value");
        }
    }
}

#[test]
fn apply_to_fory_roundtrips_numeric_and_named_in_each_mode() {
    for named in [false, true] {
        for mode in [
            SchemaMode::Compatible,
            SchemaMode::SameSchema {
                version_hash: false,
            },
            SchemaMode::SameSchema { version_hash: true },
        ] {
            let mut b = SchemaRegistryBuilder::new();
            if named {
                b.register_by_name::<Record>("a.b", "Record").unwrap();
            } else {
                b.register_by_id::<Record>(100).unwrap();
            }
            let r = b.build().unwrap();
            let (compatible, version_hash) = match encoding(mode).mode {
                SchemaMode::Compatible => (true, false),
                SchemaMode::SameSchema { version_hash } => (false, version_hash),
            };
            let mut f = fory::Fory::builder()
                .xlang(true)
                .compatible(compatible)
                .check_struct_version(version_hash)
                .build();
            fory_adapter::apply_to_fory(&r, &mut f).unwrap();
            let value = Record { value: 42 };
            let bytes = f.serialize(&value).unwrap();
            assert_eq!(f.deserialize::<Record>(&bytes).unwrap(), value);
        }
    }
}

macro_rules! fixture {
    ($name:ident, $definition:expr) => {
        #[derive(fory::ForyStruct)]
        struct $name {
            value: i32,
        }
        impl DescribeSchema for $name {
            fn describe_type(
                _: &RootEncoding,
                r: &SchemaRegistry,
            ) -> Result<TypeDescriptor, SchemaError> {
                Ok(ty(TypeKind::User(r.identity_of::<Self>()?.clone())))
            }
            fn describe_definition(
                e: &RootEncoding,
                r: &SchemaRegistry,
            ) -> Result<Option<UserDefinition>, SchemaError> {
                ($definition)(e, r)
            }
            fn describe_schema(
                _: &RootEncoding,
                _: &SchemaRegistry,
            ) -> Result<SchemaDescriptor, SchemaError> {
                Err(SchemaError::NotImplemented(
                    "must not call whole-root override",
                ))
            }
        }
    };
}
fn field(name: &str, kind: TypeKind) -> FieldDescriptor {
    FieldDescriptor {
        id: None,
        name: name.into(),
        ty: ty(kind),
    }
}
fn user(id: u32) -> TypeKind {
    TypeKind::User(UserIdentity::Numeric(id))
}
fn definition<T: 'static>(
    r: &SchemaRegistry,
    fields: Vec<FieldDescriptor>,
) -> Result<Option<UserDefinition>, SchemaError> {
    Ok(Some(UserDefinition {
        identity: r.identity_of::<T>()?.clone(),
        kind: UserDefinitionKind::Struct {
            physical_field_order: (0..fields.len()).collect(),
            fields,
        },
    }))
}
static GRAPH_CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
fixture!(Graph, |_: &RootEncoding, r: &SchemaRegistry| {
    GRAPH_CALLS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    definition::<Graph>(
        r,
        vec![
            field("first", TypeKind::List(Box::new(ty(user(202))))),
            field("second", user(203)),
            field("repeat", user(202)),
        ],
    )
});
fixture!(Cycle, |_: &RootEncoding, r: &SchemaRegistry| definition::<
    Cycle,
>(
    r,
    vec![field("back", user(201)), field("self", user(202))]
));
fixture!(Leaf, |_: &RootEncoding, r: &SchemaRegistry| definition::<
    Leaf,
>(r, vec![]));
fixture!(Absent, |_: &RootEncoding, _: &SchemaRegistry| Ok(None));
fixture!(WrongIdentity, |_: &RootEncoding, _: &SchemaRegistry| Ok(
    Some(UserDefinition {
        identity: UserIdentity::Numeric(999),
        kind: UserDefinitionKind::Struct {
            fields: vec![],
            physical_field_order: vec![]
        }
    })
));
fixture!(WrongCategory, |_: &RootEncoding, r: &SchemaRegistry| Ok(
    Some(UserDefinition {
        identity: r.identity_of::<WrongCategory>()?.clone(),
        kind: UserDefinitionKind::Enum { variants: vec![] }
    })
));
fixture!(Fails, |e: &RootEncoding, _: &SchemaRegistry| Err(
    SchemaError::AccessError(ErrorLocation {
        byte_offset: Some(7),
        descriptor_index: Some(11),
        path: match e.mode {
            SchemaMode::Compatible => None,
            SchemaMode::SameSchema {
                version_hash: false,
            } => Some(vec![]),
            SchemaMode::SameSchema { version_hash: true } =>
                Some(vec![PathSegment::Field("known".into())]),
        }
    })
));
fixture!(MissingEdge, |_: &RootEncoding, r: &SchemaRegistry| {
    let value = ty(TypeKind::Set(Box::new(ty(user(999)))));
    let map = ty(TypeKind::Map {
        key: Box::new(ty(TypeKind::Scalar(ScalarKind::I32))),
        value: Box::new(value),
    });
    definition::<MissingEdge>(
        r,
        vec![field(
            "items",
            TypeKind::Array {
                element: Box::new(map),
                length: Some(3),
            },
        )],
    )
});

struct MapRoot;
impl DescribeSchema for MapRoot {
    fn describe_type(_: &RootEncoding, _: &SchemaRegistry) -> Result<TypeDescriptor, SchemaError> {
        Ok(ty(TypeKind::Map {
            key: Box::new(ty(TypeKind::List(Box::new(ty(user(201)))))),
            value: Box::new(ty(user(201))),
        }))
    }
    fn describe_definition(
        _: &RootEncoding,
        _: &SchemaRegistry,
    ) -> Result<Option<UserDefinition>, SchemaError> {
        Err(SchemaError::NotImplemented(
            "container definition must not be called",
        ))
    }
}

#[test]
fn fifo_collection_excludes_unreachable_and_processes_cycles_once() {
    GRAPH_CALLS.store(0, std::sync::atomic::Ordering::SeqCst);
    let mut b = SchemaRegistryBuilder::new();
    b.register_by_id::<Other>(200).unwrap();
    b.register_by_id::<Leaf>(203).unwrap();
    b.register_by_id::<Cycle>(202).unwrap();
    b.register_by_id::<Graph>(201).unwrap();
    let r = b.build().unwrap();
    let d = r
        .describe::<MapRoot>(&encoding(SchemaMode::Compatible))
        .unwrap();
    assert_eq!(
        d.definitions
            .iter()
            .map(|d| d.identity.clone())
            .collect::<Vec<_>>(),
        vec![
            UserIdentity::Numeric(201),
            UserIdentity::Numeric(202),
            UserIdentity::Numeric(203)
        ]
    );
    assert_eq!(GRAPH_CALLS.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert!(matches!(d.root.kind, TypeKind::Map { .. }));
}

#[test]
fn registered_definition_checks_none_identity_and_category() {
    let mut b = SchemaRegistryBuilder::new();
    b.register_by_id::<Absent>(201).unwrap();
    let r = b.build().unwrap();
    let loc = assert_kind(
        r.describe::<MapRoot>(&encoding(SchemaMode::Compatible))
            .unwrap_err(),
        RegistryErrorKind::MissingDefinition,
    );
    assert_eq!(
        loc.path,
        Some(vec![PathSegment::MapKeyType, PathSegment::ElementType])
    );
    assert_eq!(loc.descriptor_index, None);
    let mut b = SchemaRegistryBuilder::new();
    b.register_by_id::<WrongIdentity>(201).unwrap();
    assert_kind(
        b.build()
            .unwrap()
            .describe::<WrongIdentity>(&encoding(SchemaMode::Compatible))
            .unwrap_err(),
        RegistryErrorKind::DefinitionIdentityMismatch,
    );
    let mut b = SchemaRegistryBuilder::new();
    b.register_by_id::<WrongCategory>(201).unwrap();
    assert_kind(
        b.build()
            .unwrap()
            .describe::<WrongCategory>(&encoding(SchemaMode::Compatible))
            .unwrap_err(),
        RegistryErrorKind::DefinitionCategoryMismatch,
    );
}

#[test]
fn callback_errors_only_fill_missing_path_and_preserve_other_coordinates() {
    let mut b = SchemaRegistryBuilder::new();
    b.register_by_id::<Fails>(201).unwrap();
    let r = b.build().unwrap();
    for (mode, path) in [
        (
            SchemaMode::Compatible,
            vec![PathSegment::MapKeyType, PathSegment::ElementType],
        ),
        (
            SchemaMode::SameSchema {
                version_hash: false,
            },
            vec![],
        ),
        (
            SchemaMode::SameSchema { version_hash: true },
            vec![PathSegment::Field("known".into())],
        ),
    ] {
        let SchemaError::AccessError(loc) = r.describe::<MapRoot>(&encoding(mode)).unwrap_err()
        else {
            panic!("callback error changed")
        };
        assert_eq!(loc.path, Some(path));
        assert_eq!(loc.byte_offset, Some(7));
        assert_eq!(loc.descriptor_index, Some(11));
    }
}

#[test]
fn missing_dependencies_have_structural_container_paths() {
    let r = SchemaRegistryBuilder::new().build().unwrap();
    let loc = assert_kind(
        r.describe::<MapRoot>(&encoding(SchemaMode::Compatible))
            .unwrap_err(),
        RegistryErrorKind::UnregisteredDependency,
    );
    assert_eq!(
        loc.path,
        Some(vec![PathSegment::MapKeyType, PathSegment::ElementType])
    );
    let mut b = SchemaRegistryBuilder::new();
    b.register_by_id::<MissingEdge>(201).unwrap();
    let loc = assert_kind(
        b.build()
            .unwrap()
            .describe::<MissingEdge>(&encoding(SchemaMode::Compatible))
            .unwrap_err(),
        RegistryErrorKind::UnregisteredDependency,
    );
    assert_eq!(
        loc.path,
        Some(vec![
            PathSegment::Field("items".into()),
            PathSegment::ElementType,
            PathSegment::MapValueType,
            PathSegment::ElementType
        ])
    );
    assert_eq!(loc.byte_offset, None);
    assert_eq!(loc.descriptor_index, None);
}

#[derive(Debug, PartialEq, fory::ForyEnum)]
enum Choice {
    First,
    Second,
}
impl DescribeSchema for Choice {
    fn describe_type(_: &RootEncoding, r: &SchemaRegistry) -> Result<TypeDescriptor, SchemaError> {
        Ok(ty(TypeKind::User(r.identity_of::<Self>()?.clone())))
    }
    fn describe_definition(
        _: &RootEncoding,
        r: &SchemaRegistry,
    ) -> Result<Option<UserDefinition>, SchemaError> {
        Ok(Some(UserDefinition {
            identity: r.identity_of::<Self>()?.clone(),
            kind: UserDefinitionKind::Enum {
                variants: vec![
                    VariantDescriptor {
                        id: 0,
                        name: "First".into(),
                        payload: None,
                    },
                    VariantDescriptor {
                        id: 1,
                        name: "Second".into(),
                        payload: None,
                    },
                ],
            },
        }))
    }
}
#[derive(Debug, fory::ForyUnion)]
enum Payload {
    #[fory(default)]
    Value(i32),
    #[fory(unknown)]
    Unknown(fory::UnknownCase),
}
impl DescribeSchema for Payload {
    fn describe_type(_: &RootEncoding, r: &SchemaRegistry) -> Result<TypeDescriptor, SchemaError> {
        Ok(ty(TypeKind::User(r.identity_of::<Self>()?.clone())))
    }
    fn describe_definition(
        _: &RootEncoding,
        r: &SchemaRegistry,
    ) -> Result<Option<UserDefinition>, SchemaError> {
        Ok(Some(UserDefinition {
            identity: r.identity_of::<Self>()?.clone(),
            kind: UserDefinitionKind::Union {
                variants: vec![VariantDescriptor {
                    id: 0,
                    name: "Value".into(),
                    payload: Some(ty(TypeKind::Scalar(ScalarKind::I32))),
                }],
            },
        }))
    }
}

#[test]
fn enum_and_union_use_their_fixed_registration_categories() {
    for named in [false, true] {
        let mut b = SchemaRegistryBuilder::new();
        if named {
            b.register_by_name::<Choice>("", "Choice").unwrap();
            b.register_by_name::<Payload>("a.b", "Payload").unwrap();
        } else {
            b.register_by_id::<Choice>(300).unwrap();
            b.register_by_id::<Payload>(301).unwrap();
        }
        let r = b.build().unwrap();
        assert!(matches!(
            r.describe::<Choice>(&encoding(SchemaMode::Compatible))
                .unwrap()
                .definitions[0]
                .kind,
            UserDefinitionKind::Enum { .. }
        ));
        assert!(matches!(
            r.describe::<Payload>(&encoding(SchemaMode::Compatible))
                .unwrap()
                .definitions[0]
                .kind,
            UserDefinitionKind::Union { .. }
        ));
        let mut f = fory::Fory::builder().xlang(true).compatible(true).build();
        r.apply_to_fory(&mut f).unwrap();
        assert_eq!(
            f.deserialize::<Choice>(&f.serialize(&Choice::Second).unwrap())
                .unwrap(),
            Choice::Second
        );
        assert!(matches!(
            f.deserialize::<Payload>(&f.serialize(&Payload::Value(42)).unwrap())
                .unwrap(),
            Payload::Value(42)
        ));
    }
}

#[test]
fn fory_registration_failure_preserves_source_and_unknown_location() {
    use std::error::Error;
    let mut b = SchemaRegistryBuilder::new();
    b.register_by_id::<Record>(100).unwrap();
    let r = b.build().unwrap();
    let mut f = fory::Fory::builder().xlang(true).build();
    f.register::<Other>(100).unwrap();
    let error = r.apply_to_fory(&mut f).unwrap_err();
    assert!(error.source().is_some());
    let SchemaError::ForyRegistration { source, location } = error else {
        panic!("source discarded")
    };
    assert!(!source.to_string().is_empty());
    assert_eq!(location.path, None);
    assert_eq!(location.descriptor_index, None);
    assert_eq!(location.byte_offset, None);
}

#[derive(fory::ForyUnion)]
enum Branches {
    #[fory(default)]
    First(i32),
    Second(i32),
    #[fory(unknown)]
    Unknown(fory::UnknownCase),
}
impl DescribeSchema for Branches {
    fn describe_type(_: &RootEncoding, r: &SchemaRegistry) -> Result<TypeDescriptor, SchemaError> {
        Ok(ty(TypeKind::User(r.identity_of::<Self>()?.clone())))
    }
    fn describe_definition(
        _: &RootEncoding,
        r: &SchemaRegistry,
    ) -> Result<Option<UserDefinition>, SchemaError> {
        Ok(Some(UserDefinition {
            identity: r.identity_of::<Self>()?.clone(),
            kind: UserDefinitionKind::Union {
                variants: vec![
                    VariantDescriptor {
                        id: 0,
                        name: "First".into(),
                        payload: Some(ty(TypeKind::Set(Box::new(ty(user(202)))))),
                    },
                    VariantDescriptor {
                        id: 1,
                        name: "Second".into(),
                        payload: Some(ty(user(202))),
                    },
                ],
            },
        }))
    }
}

#[test]
fn branch_payloads_preserve_logical_first_discovery_path() {
    let mut b = SchemaRegistryBuilder::new();
    b.register_by_id::<Absent>(202).unwrap();
    b.register_by_id::<Branches>(201).unwrap();
    let loc = assert_kind(
        b.build()
            .unwrap()
            .describe::<Branches>(&encoding(SchemaMode::Compatible))
            .unwrap_err(),
        RegistryErrorKind::MissingDefinition,
    );
    assert_eq!(
        loc.path,
        Some(vec![
            PathSegment::Variant("First".into()),
            PathSegment::ElementType
        ])
    );
    assert_eq!(loc.descriptor_index, None);
}

fixture!(OriginalError, |_: &RootEncoding, _: &SchemaRegistry| Err(
    SchemaError::NotImplemented("original callback error")
));
#[test]
fn original_callback_error_is_not_reclassified() {
    let mut b = SchemaRegistryBuilder::new();
    b.register_by_id::<OriginalError>(201).unwrap();
    assert!(matches!(
        b.build()
            .unwrap()
            .describe::<OriginalError>(&encoding(SchemaMode::Compatible)),
        Err(SchemaError::NotImplemented("original callback error"))
    ));
}
