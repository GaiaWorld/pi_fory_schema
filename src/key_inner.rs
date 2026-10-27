use super::CompareFrame;
use super::{HashChunk, HashFrame, KeyHashReader};
use std::io::Read;

impl HashChunk<'_> {
    fn bytes(&self) -> &[u8] {
        match self {
            Self::Inline { bytes, len } => &bytes[..*len],
            Self::String(value) => value.as_str().as_bytes(),
            Self::Binary(value) => value.as_bytes(),
        }
    }
}

impl<'a> KeyHashReader<'a> {
    fn new(parts: KeyParts<'a>) -> Result<Self, KeyError> {
        let root = reborrow(&parts, &parts._root)?;
        let mut bytes = [0; 32];
        let domain = b"pi_fory_schema:key\0";
        bytes[..domain.len()].copy_from_slice(domain);
        bytes[domain.len()..domain.len() + 4].copy_from_slice(
            &parts
                ._schema
                ._data
                ._descriptor
                .key_semantics_version
                .to_le_bytes(),
        );
        Ok(Self {
            parts,
            stack: vec![HashFrame::Node(root)],
            pending: Some(HashChunk::Inline {
                bytes,
                len: domain.len() + 4,
            }),
            offset: 0,
            error: None,
        })
    }

    fn next(&mut self) -> Result<Option<HashChunk<'a>>, KeyError> {
        while let Some(frame) = self.stack.pop() {
            let node = match frame {
                HashFrame::Chunk(chunk) => return Ok(Some(chunk)),
                HashFrame::Node(node) => node,
                HashFrame::Children {
                    node,
                    index,
                    count,
                    fields,
                } => {
                    if index == count {
                        continue;
                    }
                    let child = if fields {
                        node.field_at(index)
                    } else {
                        node.element_at(index)
                    }
                    .map_err(data_error)?;
                    let child = reborrow(&self.parts, &child)?;
                    self.stack.push(HashFrame::Children {
                        node,
                        index: index + 1,
                        count,
                        fields,
                    });
                    self.stack.push(HashFrame::Node(child));
                    continue;
                }
            };
            let tag = match &node.node_type().kind {
                TypeKind::Scalar(kind) => match kind {
                    ScalarKind::Bool => 1,
                    ScalarKind::I8 => 2,
                    ScalarKind::I16 => 3,
                    ScalarKind::I32 => 4,
                    ScalarKind::I64 => 5,
                    ScalarKind::U8 => 6,
                    ScalarKind::U16 => 7,
                    ScalarKind::U32 => 8,
                    ScalarKind::U64 => 9,
                    ScalarKind::F32 => 10,
                    ScalarKind::F64 => 11,
                    ScalarKind::String => 12,
                    ScalarKind::Binary => 13,
                    _ => return Err(KeyError::UnsupportedKeyCapability(ErrorLocation::default())),
                },
                TypeKind::List(_) => 14,
                TypeKind::Array { .. } => 15,
                TypeKind::User(_) => match definition(self.parts._schema, node.node_type())? {
                    UserDefinitionKind::Struct { .. } => 16,
                    UserDefinitionKind::Enum { .. } => 17,
                    UserDefinitionKind::Union { .. } => 18,
                },
                _ => return Err(KeyError::UnsupportedKeyCapability(ErrorLocation::default())),
            };
            let mut bytes = [0; 32];
            bytes[0] = tag;
            let mut len = 2;
            if node.is_null().map_err(data_error)? {
                return Ok(Some(HashChunk::Inline { bytes, len }));
            }
            bytes[1] = 1;
            macro_rules! number {
                ($value:expr) => {{
                    let value = $value;
                    let raw = value.to_le_bytes();
                    bytes[len..len + raw.len()].copy_from_slice(&raw);
                    len += raw.len();
                }};
            }
            match tag {
                1 => {
                    bytes[len] = u8::from(node.read_bool().map_err(data_error)?);
                    len += 1;
                }
                2 => number!(node.read_i8().map_err(data_error)?),
                3 => number!(node.read_i16().map_err(data_error)?),
                4 => number!(node.read_i32().map_err(data_error)?),
                5 => number!(node.read_i64().map_err(data_error)?),
                6 => number!(node.read_u8().map_err(data_error)?),
                7 => number!(node.read_u16().map_err(data_error)?),
                8 => number!(node.read_u32().map_err(data_error)?),
                9 => number!(node.read_u64().map_err(data_error)?),
                10 => {
                    let value = node.read_f32().map_err(data_error)?;
                    number!(if value.is_nan() {
                        0x7fc0_0000u32
                    } else if value == 0.0 {
                        0
                    } else {
                        value.to_bits()
                    });
                }
                11 => {
                    let value = node.read_f64().map_err(data_error)?;
                    number!(if value.is_nan() {
                        0x7ff8_0000_0000_0000u64
                    } else if value == 0.0 {
                        0
                    } else {
                        value.to_bits()
                    });
                }
                12 | 13 => {
                    let chunk = if tag == 12 {
                        HashChunk::String(
                            crate::value::ValidatedState::key_string(&node).map_err(data_error)?,
                        )
                    } else {
                        HashChunk::Binary(
                            crate::value::ValidatedState::key_binary(&node).map_err(data_error)?,
                        )
                    };
                    number!(
                        u64::try_from(chunk.bytes().len())
                            .map_err(|_| KeyError::AccessError(ErrorLocation::default()))?
                    );
                    self.stack.push(HashFrame::Chunk(chunk));
                }
                14..=16 => {
                    let count = if tag == 16 {
                        let UserDefinitionKind::Struct { fields, .. } =
                            definition(self.parts._schema, node.node_type())?
                        else {
                            return Err(KeyError::InvalidSchema(ErrorLocation::default()));
                        };
                        fields.len()
                    } else {
                        node.len().map_err(data_error)?
                    };
                    number!(
                        u64::try_from(count)
                            .map_err(|_| KeyError::AccessError(ErrorLocation::default()))?
                    );
                    self.stack.push(HashFrame::Children {
                        node,
                        index: 0,
                        count,
                        fields: tag == 16,
                    });
                }
                17 | 18 => {
                    let id = node.variant().map_err(data_error)?.id;
                    let (UserDefinitionKind::Enum { variants }
                    | UserDefinitionKind::Union { variants }) =
                        definition(self.parts._schema, node.node_type())?
                    else {
                        return Err(KeyError::InvalidSchema(ErrorLocation::default()));
                    };
                    let index = variants
                        .iter()
                        .position(|v| v.id == id)
                        .ok_or_else(|| KeyError::InvalidSchema(ErrorLocation::default()))?;
                    number!(
                        u64::try_from(index)
                            .map_err(|_| KeyError::AccessError(ErrorLocation::default()))?
                    );
                    if tag == 18 {
                        let child = node.variant_payload().map_err(data_error)?;
                        bytes[len] = u8::from(child.is_some());
                        len += 1;
                        if let Some(child) = child {
                            self.stack
                                .push(HashFrame::Node(reborrow(&self.parts, &child)?));
                        }
                    }
                }
                _ => return Err(KeyError::InvalidSchema(ErrorLocation::default())),
            }
            return Ok(Some(HashChunk::Inline { bytes, len }));
        }
        Ok(None)
    }
}

impl Read for KeyHashReader<'_> {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        let mut written = 0;
        loop {
            if self.error.is_some() {
                return if written > 0 {
                    Ok(written)
                } else {
                    Err(std::io::Error::other("logical key stream failed"))
                };
            }
            if let Some(chunk) = &self.pending {
                let bytes = chunk.bytes();
                let count = (bytes.len() - self.offset).min(output.len() - written);
                output[written..written + count]
                    .copy_from_slice(&bytes[self.offset..self.offset + count]);
                written += count;
                self.offset += count;
                if self.offset == bytes.len() {
                    self.pending = None;
                    self.offset = 0;
                }
                if written == output.len() {
                    return Ok(written);
                }
            } else {
                match self.next() {
                    Ok(Some(chunk)) => self.pending = Some(chunk),
                    Ok(None) => return Ok(written),
                    Err(error) => self.error = Some(error),
                }
            }
        }
    }
}

fn hash(parts: KeyParts<'_>) -> Result<u64, KeyError> {
    let mut reader = KeyHashReader::new(parts)?;
    let result = rapidhash::v3::rapidhash_v3_file(&mut reader);
    if let Some(error) = reader.error {
        return Err(error);
    }
    result.map_err(|_| KeyError::AccessError(ErrorLocation::default()))
}

#[cfg(test)]
#[test]
fn canonical_hash_stream_matches_exact_bytes_all_chunk_sizes_and_saved_errors() {
    let schema = Schema::compile(
        crate::SchemaDescriptor {
            encoding: crate::RootEncoding {
                protocol_version: crate::ProtocolVersion::Fory1_7,
                mode: crate::SchemaMode::Compatible,
                root_reference: crate::RootReferenceMode::NullOnly,
            },
            key_semantics_version: crate::KEY_SEMANTICS_VERSION,
            root: TypeDescriptor {
                kind: TypeKind::Scalar(ScalarKind::String),
                integer_encoding: None,
                nullable: false,
                track_ref: false,
            },
            definitions: vec![],
        },
        &crate::SchemaLimits::default(),
    )
    .unwrap();
    let value = "é中abc".repeat(2000);
    let input = fory::Fory::builder()
        .xlang(true)
        .build()
        .serialize(&value)
        .unwrap();
    let key = schema
        .validate(
            &input,
            crate::InputFormat::XlangMessage,
            &crate::ParseLimits::default(),
        )
        .unwrap()
        .into_key(KeyRequirement::Hash)
        .unwrap();
    let mut expected = b"pi_fory_schema:key\0".to_vec();
    expected.extend_from_slice(&crate::KEY_SEMANTICS_VERSION.to_le_bytes());
    expected.extend_from_slice(&[12, 1]);
    expected.extend_from_slice(&(value.len() as u64).to_le_bytes());
    expected.extend_from_slice(value.as_bytes());
    for size in [1, 2, 7, 16, 223, 224, 225, 8192] {
        let mut reader = KeyHashReader::new(parts(&key).unwrap()).unwrap();
        let mut bytes = vec![];
        let mut buffer = vec![0; size];
        assert_eq!(reader.read(&mut []).unwrap(), 0);
        loop {
            let n = reader.read(&mut buffer).unwrap();
            if n == 0 {
                break;
            }
            bytes.extend_from_slice(&buffer[..n]);
        }
        assert_eq!(bytes, expected);
        assert_eq!(
            rapidhash::v3::rapidhash_v3(&bytes),
            key.stable_hash().unwrap()
        );
    }
    // 错误发生在已输出前缀之后，保留原分类及位置，不吞成 EOF。
    let mut reader = KeyHashReader::new(parts(&key).unwrap()).unwrap();
    reader.stack.clear();
    // 注入不合法延续，让 next() 真实调用 String 的 element_at 并失败；
    // 不直接设置 error，以覆盖同次 read 先写前缀再遇错的路径。
    let node = reborrow(&reader.parts, &reader.parts._root).unwrap();
    let expected_error = match node.element_at(0) {
        Err(error) => data_error(error),
        Ok(_) => panic!("String must reject element access"),
    };
    reader.stack.push(HashFrame::Children {
        node,
        index: 0,
        count: 1,
        fields: false,
    });
    let mut buffer = [0; 64];
    let prefix = reader.read(&mut buffer).unwrap();
    assert_eq!(prefix, 23);
    assert_eq!(
        format!("{:?}", reader.error.as_ref().unwrap()),
        format!("{:?}", expected_error)
    );
    assert_eq!(reader.read(&mut []).unwrap(), 0);
    assert!(reader.read(&mut buffer).is_err());
    assert_eq!(
        format!("{:?}", reader.error.as_ref().unwrap()),
        format!("{:?}", expected_error)
    );
    assert!(rapidhash::v3::rapidhash_v3_file(&mut reader).is_err());
    assert!(key.try_eq(&key).unwrap());
}
use super::sealed::KeyParts;
use crate::{
    DataError, ErrorLocation, KeyRequirement, ScalarKind, Schema, SchemaError, TypeDescriptor,
    TypeKind, UserDefinitionKind,
};
use std::{collections::HashSet, sync::Arc};

pub(crate) fn schema_error(error: SchemaError) -> KeyError {
    match error {
        SchemaError::UnsupportedKeyCapability(at) => KeyError::UnsupportedKeyCapability(at),
        SchemaError::InvalidSchema(at) => KeyError::InvalidSchema(at),
        SchemaError::UnsupportedVersion(at) => KeyError::UnsupportedVersion(at),
        SchemaError::NotImplemented(what) => KeyError::NotImplemented(what),
        _ => KeyError::InvalidSchema(ErrorLocation::default()),
    }
}

fn data_error(error: DataError) -> KeyError {
    match error {
        DataError::NotImplemented(v) => KeyError::NotImplemented(v),
        DataError::InvalidSchema(v) => KeyError::InvalidSchema(v),
        DataError::UnsupportedVersion(v) => KeyError::UnsupportedVersion(v),
        DataError::UnsupportedType(v) => KeyError::UnsupportedType(v),
        DataError::MalformedInput(v) => KeyError::MalformedInput(v),
        DataError::SchemaMismatch(v) => KeyError::SchemaMismatch(v),
        DataError::UnsupportedKeyCapability(v) => KeyError::UnsupportedKeyCapability(v),
        DataError::AccessError(v) => KeyError::AccessError(v),
        DataError::ResourceLimit(v) => KeyError::ResourceLimit(v),
    }
}

fn equivalent(left: &Schema, right: &Schema) -> bool {
    if Arc::ptr_eq(&left._data, &right._data) {
        return true;
    }
    let a = &left._data._descriptor;
    let b = &right._data._descriptor;
    let mode = match (a.encoding.mode, b.encoding.mode) {
        (crate::SchemaMode::Compatible, crate::SchemaMode::Compatible) => true,
        (
            crate::SchemaMode::SameSchema { version_hash: a },
            crate::SchemaMode::SameSchema { version_hash: b },
        ) => a == b,
        _ => false,
    };
    if !mode
        || std::mem::discriminant(&a.encoding.protocol_version)
            != std::mem::discriminant(&b.encoding.protocol_version)
        || std::mem::discriminant(&a.encoding.root_reference)
            != std::mem::discriminant(&b.encoding.root_reference)
        || a.key_semantics_version != b.key_semantics_version
        || a.definitions.len() != b.definitions.len()
    {
        return false;
    }
    let mut stack = vec![(&a.root, &b.root)];
    let mut visited = HashSet::new();
    while let Some((a, b)) = stack.pop() {
        if a.nullable != b.nullable
            || a.track_ref != b.track_ref
            || a.integer_encoding != b.integer_encoding
        {
            return false;
        }
        match (&a.kind, &b.kind) {
            (TypeKind::Scalar(a), TypeKind::Scalar(b))
                if std::mem::discriminant(a) == std::mem::discriminant(b) => {}
            (TypeKind::List(a), TypeKind::List(b)) | (TypeKind::Set(a), TypeKind::Set(b)) => {
                stack.push((a, b))
            }
            (
                TypeKind::Array {
                    element: a,
                    length: al,
                },
                TypeKind::Array {
                    element: b,
                    length: bl,
                },
            ) if al == bl => stack.push((a, b)),
            (TypeKind::Map { key: ak, value: av }, TypeKind::Map { key: bk, value: bv }) => {
                stack.push((ak, bk));
                stack.push((av, bv));
            }
            (TypeKind::User(a), TypeKind::User(b)) if a == b => {
                if !visited.insert(a) {
                    continue;
                }
                let ad = &left._data._descriptor.definitions
                    [left._data._type_access._definition_by_identity[a]]
                    .kind;
                let bd = &right._data._descriptor.definitions
                    [right._data._type_access._definition_by_identity[b]]
                    .kind;
                match (ad, bd) {
                    (
                        UserDefinitionKind::Struct {
                            fields: a,
                            physical_field_order: ao,
                        },
                        UserDefinitionKind::Struct {
                            fields: b,
                            physical_field_order: bo,
                        },
                    ) => {
                        if ao != bo || a.len() != b.len() {
                            return false;
                        }
                        for (a, b) in a.iter().zip(b) {
                            if a.id != b.id || a.name != b.name {
                                return false;
                            }
                            stack.push((&a.ty, &b.ty));
                        }
                    }
                    (
                        UserDefinitionKind::Enum { variants: a },
                        UserDefinitionKind::Enum { variants: b },
                    )
                    | (
                        UserDefinitionKind::Union { variants: a },
                        UserDefinitionKind::Union { variants: b },
                    ) => {
                        if a.len() != b.len() {
                            return false;
                        }
                        for (a, b) in a.iter().zip(b) {
                            if a.id != b.id || a.name != b.name {
                                return false;
                            }
                            match (&a.payload, &b.payload) {
                                (Some(a), Some(b)) => stack.push((a, b)),
                                (None, None) => {}
                                _ => return false,
                            }
                        }
                    }
                    _ => return false,
                }
            }
            _ => return false,
        }
    }
    true
}

#[cfg(test)]
#[test]
fn schema_equivalence_single_difference_matrix() {
    use crate::{
        FieldDescriptor, SchemaDescriptor, SchemaLimits, UserDefinition, UserIdentity,
        VariantDescriptor,
    };
    let name = UserIdentity::Name {
        namespace: "keys".into(),
        type_name: "Root".into(),
    };
    let leaf = TypeDescriptor {
        kind: TypeKind::Scalar(ScalarKind::I32),
        nullable: true,
        track_ref: false,
        integer_encoding: Some(crate::IntegerEncoding::Varint),
    };
    let child = TypeDescriptor {
        kind: TypeKind::User(UserIdentity::Numeric(100)),
        nullable: false,
        track_ref: false,
        integer_encoding: None,
    };
    let base = SchemaDescriptor {
        encoding: crate::RootEncoding {
            protocol_version: crate::ProtocolVersion::Fory1_7,
            mode: crate::SchemaMode::SameSchema {
                version_hash: false,
            },
            root_reference: crate::RootReferenceMode::NullOnly,
        },
        key_semantics_version: crate::KEY_SEMANTICS_VERSION,
        root: TypeDescriptor {
            kind: TypeKind::User(name.clone()),
            nullable: true,
            track_ref: false,
            integer_encoding: None,
        },
        definitions: vec![
            UserDefinition {
                identity: name,
                kind: UserDefinitionKind::Struct {
                    fields: vec![FieldDescriptor {
                        id: None,
                        name: "child".into(),
                        ty: child,
                    }],
                    physical_field_order: vec![0],
                },
            },
            UserDefinition {
                identity: UserIdentity::Numeric(100),
                kind: UserDefinitionKind::Union {
                    variants: vec![VariantDescriptor {
                        id: 9,
                        name: "Value".into(),
                        payload: Some(leaf),
                    }],
                },
            },
        ],
    };
    let left = Schema::compile(base.clone(), &SchemaLimits::default()).unwrap();
    let left_key = left
        .validate(
            &[253],
            crate::InputFormat::RootValue,
            &crate::ParseLimits::default(),
        )
        .unwrap()
        .into_key(KeyRequirement::HashAndOrdered)
        .unwrap();
    // 每个变体均由同一基线独立生成，避免多个差异相互遮蔽。
    for case in [
        "independent",
        "definition_order",
        "namespace",
        "type_name",
        "field_name",
        "field_id",
        "variant_name",
        "variant_id",
        "payload_nullable",
        "payload_codec",
        "payload_kind",
        "mode",
        "version_hash",
        "root_reference",
        "root_nullable",
        "payload_tracking",
    ] {
        let mut descriptor = base.clone();
        match case {
            "definition_order" => descriptor.definitions.reverse(),
            "namespace" | "type_name" => {
                let UserIdentity::Name {
                    namespace,
                    type_name,
                } = &mut descriptor.definitions[0].identity
                else {
                    panic!()
                };
                if case == "namespace" {
                    namespace.push('x');
                } else {
                    type_name.push('x');
                }
                descriptor.root.kind = TypeKind::User(descriptor.definitions[0].identity.clone());
            }
            "field_name" | "field_id" => {
                let UserDefinitionKind::Struct { fields, .. } = &mut descriptor.definitions[0].kind
                else {
                    panic!()
                };
                if case == "field_name" {
                    fields[0].name.push('x');
                } else {
                    fields[0].id = Some(1);
                }
            }
            "variant_name" | "variant_id" | "payload_nullable" | "payload_codec"
            | "payload_kind" | "payload_tracking" => {
                let UserDefinitionKind::Union { variants } = &mut descriptor.definitions[1].kind
                else {
                    panic!()
                };
                match case {
                    "variant_name" => variants[0].name.push('x'),
                    "variant_id" => variants[0].id = 2,
                    _ => {
                        let payload = variants[0].payload.as_mut().unwrap();
                        match case {
                            "payload_nullable" => payload.nullable = false,
                            "payload_codec" => {
                                payload.integer_encoding = Some(crate::IntegerEncoding::Fixed)
                            }
                            "payload_kind" => payload.kind = TypeKind::Scalar(ScalarKind::U32),
                            _ => payload.track_ref = true,
                        }
                    }
                }
            }
            "mode" => descriptor.encoding.mode = crate::SchemaMode::Compatible,
            "version_hash" => {
                descriptor.encoding.mode = crate::SchemaMode::SameSchema { version_hash: true }
            }
            "root_reference" => {
                descriptor.encoding.root_reference = crate::RootReferenceMode::Tracking
            }
            "root_nullable" => descriptor.root.nullable = false,
            _ => {}
        }
        let right = Schema::compile(descriptor, &SchemaLimits::default()).unwrap();
        let expected = matches!(case, "independent" | "definition_order");
        assert_eq!(equivalent(&left, &right), expected, "{case}");
        assert_eq!(equivalent(&right, &left), expected, "reverse {case}");
        // 非nullable根无法用null夹具验证，tracking则应在能力入口拒绝。
        if case == "root_nullable" {
            continue;
        }
        let value = right
            .validate(
                &[253],
                crate::InputFormat::RootValue,
                &crate::ParseLimits::default(),
            )
            .unwrap();
        if case == "payload_tracking" {
            assert!(matches!(
                value.into_key(KeyRequirement::HashAndOrdered),
                Err(KeyError::UnsupportedKeyCapability(_))
            ));
            continue;
        }
        let right_key = value.into_key(KeyRequirement::HashAndOrdered).unwrap();
        if expected {
            assert!(left_key.try_eq(&right_key).unwrap(), "{case}");
            assert_eq!(
                left_key.try_cmp(&right_key).unwrap(),
                Ordering::Equal,
                "{case}"
            );
            assert_eq!(
                left_key.stable_hash().unwrap(),
                right_key.stable_hash().unwrap()
            );
        } else {
            assert!(
                matches!(
                    left_key.try_eq(&right_key),
                    Err(KeyError::SchemaMismatch(_))
                ),
                "{case}"
            );
            assert!(
                matches!(
                    right_key.try_cmp(&left_key),
                    Err(KeyError::SchemaMismatch(_))
                ),
                "{case}"
            );
        }
    }
}

fn reborrow<'a>(
    parts: &KeyParts<'a>,
    node: &crate::ValidatedNodeView<'_>,
) -> Result<crate::ValidatedNodeView<'a>, KeyError> {
    crate::value::ValidatedState::key_node(
        parts._validated,
        parts._schema,
        parts._input,
        parts._context,
        Some(node),
    )
    .map_err(data_error)
}

fn definition<'a>(
    schema: &'a Schema,
    ty: &TypeDescriptor,
) -> Result<&'a UserDefinitionKind, KeyError> {
    let TypeKind::User(id) = &ty.kind else {
        return Err(KeyError::InvalidSchema(ErrorLocation::default()));
    };
    Ok(
        &schema._data._descriptor.definitions
            [schema._data._type_access._definition_by_identity[id]]
            .kind,
    )
}

fn compare(left: KeyParts<'_>, right: KeyParts<'_>, ordered: bool) -> Result<Ordering, KeyError> {
    if ordered
        && (matches!(left._requirement, KeyRequirement::Hash)
            || matches!(right._requirement, KeyRequirement::Hash))
    {
        return Err(KeyError::UnsupportedKeyCapability(ErrorLocation::default()));
    }
    if !equivalent(left._schema, right._schema) {
        return Err(KeyError::SchemaMismatch(ErrorLocation::default()));
    }
    let mut stack = vec![CompareFrame::Node(
        reborrow(&left, &left._root)?,
        reborrow(&right, &right._root)?,
    )];
    while let Some(frame) = stack.pop() {
        let (a, b) = match frame {
            CompareFrame::Node(a, b) => (a, b),
            CompareFrame::Children {
                left: a,
                right: b,
                index,
                common,
                lengths,
                fields,
            } => {
                if index == common {
                    if lengths != Ordering::Equal {
                        return Ok(lengths);
                    }
                    continue;
                }
                let ac = if fields {
                    a.field_at(index)
                } else {
                    a.element_at(index)
                }
                .map_err(data_error)?;
                let bc = if fields {
                    b.field_at(index)
                } else {
                    b.element_at(index)
                }
                .map_err(data_error)?;
                let ac = reborrow(&left, &ac)?;
                let bc = reborrow(&right, &bc)?;
                stack.push(CompareFrame::Children {
                    left: a,
                    right: b,
                    index: index + 1,
                    common,
                    lengths,
                    fields,
                });
                stack.push(CompareFrame::Node(ac, bc));
                continue;
            }
        };
        let an = a.is_null().map_err(data_error)?;
        let bn = b.is_null().map_err(data_error)?;
        if an || bn {
            let order = bn.cmp(&an);
            if order != Ordering::Equal {
                return Ok(order);
            }
            continue;
        }
        let order = match &a.node_type().kind {
            TypeKind::Scalar(kind) => {
                macro_rules! cmp {
                    ($read:ident) => {
                        a.$read()
                            .map_err(data_error)?
                            .cmp(&b.$read().map_err(data_error)?)
                    };
                }
                match kind {
                    ScalarKind::Bool => cmp!(read_bool),
                    ScalarKind::I8 => cmp!(read_i8),
                    ScalarKind::I16 => cmp!(read_i16),
                    ScalarKind::I32 => cmp!(read_i32),
                    ScalarKind::I64 => cmp!(read_i64),
                    ScalarKind::U8 => cmp!(read_u8),
                    ScalarKind::U16 => cmp!(read_u16),
                    ScalarKind::U32 => cmp!(read_u32),
                    ScalarKind::U64 => cmp!(read_u64),
                    ScalarKind::F32 => float_cmp(
                        f64::from(a.read_f32().map_err(data_error)?),
                        f64::from(b.read_f32().map_err(data_error)?),
                    ),
                    ScalarKind::F64 => float_cmp(
                        a.read_f64().map_err(data_error)?,
                        b.read_f64().map_err(data_error)?,
                    ),
                    ScalarKind::String => a
                        .read_string()
                        .map_err(data_error)?
                        .as_str()
                        .as_bytes()
                        .cmp(b.read_string().map_err(data_error)?.as_str().as_bytes()),
                    ScalarKind::Binary => a
                        .read_binary()
                        .map_err(data_error)?
                        .as_bytes()
                        .cmp(b.read_binary().map_err(data_error)?.as_bytes()),
                    _ => return Err(KeyError::UnsupportedKeyCapability(ErrorLocation::default())),
                }
            }
            TypeKind::List(_) | TypeKind::Array { .. } => {
                let al = a.len().map_err(data_error)?;
                let bl = b.len().map_err(data_error)?;
                stack.push(CompareFrame::Children {
                    left: a,
                    right: b,
                    index: 0,
                    common: al.min(bl),
                    lengths: al.cmp(&bl),
                    fields: false,
                });
                continue;
            }
            TypeKind::User(_) => match definition(left._schema, a.node_type())? {
                UserDefinitionKind::Struct { fields, .. } => {
                    stack.push(CompareFrame::Children {
                        left: a,
                        right: b,
                        index: 0,
                        common: fields.len(),
                        lengths: Ordering::Equal,
                        fields: true,
                    });
                    continue;
                }
                UserDefinitionKind::Enum { variants } | UserDefinitionKind::Union { variants } => {
                    let av = a.variant().map_err(data_error)?;
                    let bv = b.variant().map_err(data_error)?;
                    let ai = variants
                        .iter()
                        .position(|v| v.id == av.id)
                        .ok_or_else(|| KeyError::InvalidSchema(ErrorLocation::default()))?;
                    let bi = variants
                        .iter()
                        .position(|v| v.id == bv.id)
                        .ok_or_else(|| KeyError::InvalidSchema(ErrorLocation::default()))?;
                    if ai != bi {
                        return Ok(ai.cmp(&bi));
                    }
                    if let Some(ac) = a.variant_payload().map_err(data_error)? {
                        let bc = b
                            .variant_payload()
                            .map_err(data_error)?
                            .ok_or_else(|| KeyError::InvalidSchema(ErrorLocation::default()))?;
                        stack.push(CompareFrame::Node(
                            reborrow(&left, &ac)?,
                            reborrow(&right, &bc)?,
                        ));
                    }
                    continue;
                }
            },
            _ => return Err(KeyError::UnsupportedKeyCapability(ErrorLocation::default())),
        };
        if order != Ordering::Equal {
            return Ok(order);
        }
    }
    Ok(Ordering::Equal)
}

fn float_cmp(a: f64, b: f64) -> Ordering {
    match (a.is_nan(), b.is_nan()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        _ => a.partial_cmp(&b).unwrap_or(Ordering::Equal),
    }
}

fn parts<K: KeyView + ?Sized>(key: &K) -> Result<KeyParts<'_>, KeyError> {
    key.key_parts()
        .ok_or_else(|| KeyError::AccessError(ErrorLocation::default()))
}
use super::{
    KeyError, KeyView, Ordering, OwnedValidatedKey, OwnedValidatedKeyInner, ValidatedKey,
    ValidatedKeyInner, sealed,
};

impl sealed::Sealed for ValidatedKey<'_> {
    fn key_parts(&self) -> Option<KeyParts<'_>> {
        Some(KeyParts {
            _schema: self._value._backing.schema(),
            _input: self._value._backing.input(),
            _context: &self._value._context,
            _validated: &self._value._validated,
            _root: crate::value::ValidatedState::key_node(
                &self._value._validated,
                self._value._backing.schema(),
                self._value._backing.input(),
                &self._value._context,
                None,
            )
            .ok()?,
            _requirement: self._requirement,
        })
    }
}

impl sealed::Sealed for OwnedValidatedKey {
    fn key_parts(&self) -> Option<KeyParts<'_>> {
        Some(KeyParts {
            _schema: &self._value._backing._schema,
            _input: &self._value._backing._input,
            _context: &self._value._context,
            _validated: &self._value._validated,
            _root: crate::value::ValidatedState::key_node(
                &self._value._validated,
                &self._value._backing._schema,
                &self._value._backing._input,
                &self._value._context,
                None,
            )
            .ok()?,
            _requirement: self._requirement,
        })
    }
}

impl ValidatedKeyInner {
    pub(super) fn try_eq<'a, K: KeyView + ?Sized>(
        _this: &ValidatedKey<'a>,
        _other: &K,
    ) -> Result<bool, KeyError> {
        compare(parts(_this)?, parts(_other)?, false).map(|order| order == Ordering::Equal)
    }

    pub(super) fn try_cmp<'a, K: KeyView + ?Sized>(
        _this: &ValidatedKey<'a>,
        _other: &K,
    ) -> Result<Ordering, KeyError> {
        compare(parts(_this)?, parts(_other)?, true)
    }

    pub(super) fn stable_hash<'a>(_this: &ValidatedKey<'a>) -> Result<u64, KeyError> {
        if matches!(_this._requirement, KeyRequirement::Ordered) {
            return Err(KeyError::UnsupportedKeyCapability(ErrorLocation::default()));
        }
        hash(parts(_this)?)
    }
}

impl OwnedValidatedKeyInner {
    pub(super) fn try_eq<K: KeyView + ?Sized>(
        _this: &OwnedValidatedKey,
        _other: &K,
    ) -> Result<bool, KeyError> {
        compare(parts(_this)?, parts(_other)?, false).map(|order| order == Ordering::Equal)
    }

    pub(super) fn try_cmp<K: KeyView + ?Sized>(
        _this: &OwnedValidatedKey,
        _other: &K,
    ) -> Result<Ordering, KeyError> {
        compare(parts(_this)?, parts(_other)?, true)
    }

    pub(super) fn stable_hash(_this: &OwnedValidatedKey) -> Result<u64, KeyError> {
        if matches!(_this._requirement, KeyRequirement::Ordered) {
            return Err(KeyError::UnsupportedKeyCapability(ErrorLocation::default()));
        }
        hash(parts(_this)?)
    }
}
