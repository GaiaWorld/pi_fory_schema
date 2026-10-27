use super::{DocumentInner, Schema, SchemaDescriptor, SchemaError, SchemaLimits};
use crate::{
    ErrorLocation, FieldDescriptor, IntegerEncoding, ProtocolVersion, RootEncoding,
    RootReferenceMode, ScalarKind, SchemaMode, TypeDescriptor, TypeKind, UserDefinition,
    UserDefinitionKind, UserIdentity, VariantDescriptor,
};

fn location(offset: usize) -> ErrorLocation {
    ErrorLocation {
        byte_offset: Some(offset),
        ..ErrorLocation::default()
    }
}
fn invalid(offset: usize) -> SchemaError {
    SchemaError::InvalidSchema(location(offset))
}
fn malformed(offset: usize) -> SchemaError {
    SchemaError::MalformedInput(location(offset))
}
fn charge(
    counter: &mut usize,
    amount: usize,
    limit: usize,
    offset: usize,
) -> Result<(), SchemaError> {
    *counter = counter
        .checked_add(amount)
        .filter(|n| *n <= limit)
        .ok_or_else(|| SchemaError::ResourceLimit(location(offset)))?;
    Ok(())
}
fn space(json: &str, cursor: &mut usize) {
    while matches!(
        json.as_bytes().get(*cursor),
        Some(b' ' | b'\r' | b'\n' | b'\t')
    ) {
        *cursor += 1;
    }
}
fn peek(json: &str, cursor: &mut usize) -> Option<u8> {
    space(json, cursor);
    json.as_bytes().get(*cursor).copied()
}
fn expect(json: &str, cursor: &mut usize, byte: u8) -> Result<(), SchemaError> {
    if peek(json, cursor) != Some(byte) {
        return Err(malformed(*cursor));
    }
    *cursor += 1;
    Ok(())
}
fn boundary(json: &str, cursor: usize) -> bool {
    matches!(
        json.as_bytes().get(cursor),
        None | Some(b' ' | b'\r' | b'\n' | b'\t' | b',' | b']' | b'}')
    )
}
fn literal(json: &str, cursor: &mut usize, value: &str) -> Result<(), SchemaError> {
    space(json, cursor);
    if !json[*cursor..].starts_with(value) || !boundary(json, *cursor + value.len()) {
        return Err(malformed(*cursor));
    }
    *cursor += value.len();
    Ok(())
}
fn boolean(json: &str, cursor: &mut usize) -> Result<bool, SchemaError> {
    match peek(json, cursor) {
        Some(b't') => {
            literal(json, cursor, "true")?;
            Ok(true)
        }
        Some(b'f') => {
            literal(json, cursor, "false")?;
            Ok(false)
        }
        None => Err(malformed(*cursor)),
        _ => Err(invalid(*cursor)),
    }
}
fn hex(json: &str, cursor: &mut usize) -> Result<u32, SchemaError> {
    let mut value = 0;
    for _ in 0..4 {
        let digit = json
            .as_bytes()
            .get(*cursor)
            .and_then(|b| (*b as char).to_digit(16))
            .ok_or_else(|| malformed(*cursor))?;
        *cursor += 1;
        value = value * 16 + digit;
    }
    Ok(value)
}
// Metadata is charged before each decoded character is appended. Fixed keys/tags
// have a small bounded spelling; they never consume the metadata budget.
fn decoded_character(json: &str, cursor: &mut usize) -> Result<char, SchemaError> {
    let offset = *cursor;
    let byte = *json
        .as_bytes()
        .get(*cursor)
        .ok_or_else(|| malformed(*cursor))?;
    let ch = if byte == b'\\' {
        *cursor += 1;
        let escape = *json
            .as_bytes()
            .get(*cursor)
            .ok_or_else(|| malformed(*cursor))?;
        *cursor += 1;
        match escape {
            b'"' => '"',
            b'\\' => '\\',
            b'/' => '/',
            b'b' => '\u{8}',
            b'f' => '\u{c}',
            b'n' => '\n',
            b'r' => '\r',
            b't' => '\t',
            b'u' => {
                let first = hex(json, cursor)?;
                let code = if (0xd800..=0xdbff).contains(&first) {
                    if !json[*cursor..].starts_with("\\u") {
                        return Err(malformed(*cursor));
                    }
                    *cursor += 2;
                    let second = hex(json, cursor)?;
                    if !(0xdc00..=0xdfff).contains(&second) {
                        return Err(malformed(*cursor));
                    }
                    0x10000 + ((first - 0xd800) << 10) + (second - 0xdc00)
                } else {
                    first
                };
                char::from_u32(code).ok_or_else(|| malformed(offset))?
            }
            _ => return Err(malformed(offset)),
        }
    } else {
        if byte < 0x20 {
            return Err(malformed(offset));
        }
        let ch = json[*cursor..]
            .chars()
            .next()
            .ok_or_else(|| malformed(offset))?;
        *cursor += ch.len_utf8();
        ch
    };
    Ok(ch)
}
// Compare all decoded characters without allocating unknown version text.
// Mismatches do not short-circuit escape and surrogate validation.
fn version_string(json: &str, cursor: &mut usize, expected: &str) -> Result<bool, SchemaError> {
    match peek(json, cursor) {
        Some(b'"') => {}
        None => return Err(malformed(*cursor)),
        _ => return Err(invalid(*cursor)),
    }
    *cursor += 1;
    let mut expected = expected.chars();
    let mut equal = true;
    loop {
        if json.as_bytes().get(*cursor) == Some(&b'"') {
            *cursor += 1;
            return Ok(equal && expected.next().is_none());
        }
        let ch = decoded_character(json, cursor)?;
        equal &= expected.next() == Some(ch);
    }
}

fn string(
    json: &str,
    cursor: &mut usize,
    metadata: Option<(&mut usize, usize)>,
) -> Result<String, SchemaError> {
    match peek(json, cursor) {
        Some(b'"') => {}
        None => return Err(malformed(*cursor)),
        _ => return Err(invalid(*cursor)),
    }
    *cursor += 1;
    let mut output = String::new();
    let mut metadata = metadata;
    loop {
        let byte = *json
            .as_bytes()
            .get(*cursor)
            .ok_or_else(|| malformed(*cursor))?;
        if byte == b'"' {
            *cursor += 1;
            return Ok(output);
        }
        let offset = *cursor;
        let ch = decoded_character(json, cursor)?;
        if let Some((counter, limit)) = metadata.as_mut() {
            charge(counter, ch.len_utf8(), *limit, offset)?;
        } else if output.len() + ch.len_utf8() > 32 {
            return Err(invalid(offset));
        }
        output.push(ch);
    }
}

#[cfg(test)]
#[test]
fn checked_import_counter_overflow() {
    let mut counter = usize::MAX;
    assert!(matches!(
        charge(&mut counter, 1, usize::MAX, 7),
        Err(SchemaError::ResourceLimit(_))
    ));
    assert_eq!(counter, usize::MAX);
}

#[cfg(test)]
#[test]
fn imported_descriptor_preserves_all_declared_fields() {
    let json = r#"{"definitions":[{"kind":{"physical_field_order":[1,0],"fields":[{"ty":{"kind":{"identity":{"type_name":"T","namespace":"n","kind":"name"},"kind":"user"},"integer_encoding":null,"track_ref":true,"nullable":true},"name":"a","id":null},{"ty":{"kind":{"scalar":"u64","kind":"scalar"},"integer_encoding":"tagged","track_ref":false,"nullable":false},"name":"b","id":42}],"kind":"struct"},"identity":{"kind":"name","namespace":"n","type_name":"T"}}],"root":{"kind":{"kind":"user","identity":{"kind":"name","namespace":"n","type_name":"T"}},"integer_encoding":null,"nullable":true,"track_ref":false},"encoding":{"root_reference":"tracking","mode":{"version_hash":true,"kind":"same_schema"},"protocol_version":"fory1_7"},"key_semantics_version":1,"format_version":1,"format":"pi_fory_schema"}"#;
    let schema = DocumentInner::import_json(json, &SchemaLimits::default()).unwrap();
    let d = &schema._data._descriptor;
    assert!(d.root.nullable);
    assert!(!d.root.track_ref);
    assert!(matches!(
        d.encoding.mode,
        SchemaMode::SameSchema { version_hash: true }
    ));
    assert!(matches!(
        d.encoding.root_reference,
        RootReferenceMode::Tracking
    ));
    let UserDefinitionKind::Struct {
        fields,
        physical_field_order,
    } = &d.definitions[0].kind
    else {
        panic!("struct")
    };
    assert_eq!(physical_field_order, &[1, 0]);
    assert_eq!(fields[0].name, "a");
    assert_eq!(fields[0].id, None);
    assert!(fields[0].ty.nullable && fields[0].ty.track_ref);
    assert_eq!(fields[1].id, Some(42));
    assert_eq!(fields[1].ty.integer_encoding, Some(IntegerEncoding::Tagged));
    let exported = DocumentInner::export_json(d).unwrap();
    let again = DocumentInner::import_json(&exported, &SchemaLimits::default()).unwrap();
    assert_eq!(
        DocumentInner::export_json(&again._data._descriptor).unwrap(),
        exported
    );
}
fn number(json: &str, cursor: &mut usize, max: u64) -> Result<u64, SchemaError> {
    space(json, cursor);
    let start = *cursor;
    let negative = peek(json, cursor) == Some(b'-');
    if negative {
        *cursor += 1;
    }
    let digits = *cursor;
    match json.as_bytes().get(*cursor) {
        Some(b'0') => {
            *cursor += 1;
            if matches!(json.as_bytes().get(*cursor), Some(b'0'..=b'9')) {
                return Err(malformed(*cursor));
            }
        }
        Some(b'1'..=b'9') => {
            while matches!(json.as_bytes().get(*cursor), Some(b'0'..=b'9')) {
                *cursor += 1;
            }
        }
        None => return Err(malformed(*cursor)),
        _ => {
            return Err(if negative {
                malformed(*cursor)
            } else {
                invalid(*cursor)
            });
        }
    }
    let end = *cursor;
    let mut fractional = false;
    if json.as_bytes().get(*cursor) == Some(&b'.') {
        fractional = true;
        *cursor += 1;
        let begin = *cursor;
        while matches!(json.as_bytes().get(*cursor), Some(b'0'..=b'9')) {
            *cursor += 1;
        }
        if *cursor == begin {
            return Err(malformed(*cursor));
        }
    }
    if matches!(json.as_bytes().get(*cursor), Some(b'e' | b'E')) {
        fractional = true;
        *cursor += 1;
        if matches!(json.as_bytes().get(*cursor), Some(b'+' | b'-')) {
            *cursor += 1;
        }
        let begin = *cursor;
        while matches!(json.as_bytes().get(*cursor), Some(b'0'..=b'9')) {
            *cursor += 1;
        }
        if *cursor == begin {
            return Err(malformed(*cursor));
        }
    }
    if !boundary(json, *cursor) {
        return Err(malformed(*cursor));
    }
    if negative || fractional {
        return Err(invalid(start));
    }
    json[digits..end]
        .parse::<u64>()
        .ok()
        .filter(|n| *n <= max)
        .ok_or_else(|| invalid(start))
}
fn optional_number(json: &str, cursor: &mut usize, max: u64) -> Result<Option<u64>, SchemaError> {
    if peek(json, cursor) == Some(b'n') {
        literal(json, cursor, "null")?;
        Ok(None)
    } else {
        number(json, cursor, max).map(Some)
    }
}
fn enter(
    json: &str,
    cursor: &mut usize,
    depth: usize,
    limits: &SchemaLimits,
    byte: u8,
) -> Result<usize, SchemaError> {
    let next = depth
        .checked_add(1)
        .filter(|n| *n <= limits.max_json_depth)
        .ok_or_else(|| SchemaError::ResourceLimit(location(*cursor)))?;
    match peek(json, cursor) {
        Some(b) if b == byte => {
            *cursor += 1;
            Ok(next)
        }
        None | Some(b'/' | b',' | b']' | b'}') => Err(malformed(*cursor)),
        _ => Err(invalid(*cursor)),
    }
}
fn object<F>(
    json: &str,
    cursor: &mut usize,
    depth: usize,
    limits: &SchemaLimits,
    keys: &[&str],
    mut field: F,
) -> Result<u64, SchemaError>
where
    F: FnMut(usize, &mut usize, usize) -> Result<(), SchemaError>,
{
    let depth = enter(json, cursor, depth, limits, b'{')?;
    let mut seen = 0u64;
    if peek(json, cursor) == Some(b'}') {
        *cursor += 1;
        return Ok(seen);
    }
    loop {
        if peek(json, cursor) != Some(b'"') {
            return Err(malformed(*cursor));
        }
        let offset = *cursor;
        let key = string(json, cursor, None)?;
        let index = keys
            .iter()
            .position(|candidate| *candidate == key)
            .ok_or_else(|| invalid(offset))?;
        if seen & (1 << index) != 0 {
            return Err(invalid(offset));
        }
        seen |= 1 << index;
        expect(json, cursor, b':')?;
        field(index, cursor, depth)?;
        match peek(json, cursor) {
            Some(b'}') => {
                *cursor += 1;
                break;
            }
            Some(b',') => {
                *cursor += 1;
            }
            _ => return Err(malformed(*cursor)),
        }
    }
    Ok(seen)
}
fn array<T, F>(
    json: &str,
    cursor: &mut usize,
    depth: usize,
    limits: &SchemaLimits,
    mut element: F,
) -> Result<Vec<T>, SchemaError>
where
    F: FnMut(&mut usize, usize) -> Result<T, SchemaError>,
{
    let depth = enter(json, cursor, depth, limits, b'[')?;
    let mut output = Vec::new();
    if peek(json, cursor) == Some(b']') {
        *cursor += 1;
        return Ok(output);
    }
    loop {
        if matches!(peek(json, cursor), Some(b',' | b']') | None) {
            return Err(malformed(*cursor));
        }
        output.push(element(cursor, depth)?);
        match peek(json, cursor) {
            Some(b']') => {
                *cursor += 1;
                break;
            }
            Some(b',') => {
                *cursor += 1;
            }
            _ => return Err(malformed(*cursor)),
        }
    }
    Ok(output)
}
fn required<T>(value: Option<T>, offset: usize) -> Result<T, SchemaError> {
    value.ok_or_else(|| invalid(offset))
}
fn exact(seen: u64, wanted: u64, offset: usize) -> Result<(), SchemaError> {
    if seen == wanted {
        Ok(())
    } else {
        Err(invalid(offset))
    }
}
fn identity(
    json: &str,
    cursor: &mut usize,
    depth: usize,
    limits: &SchemaLimits,
    budget: &mut [usize; 4],
) -> Result<UserIdentity, SchemaError> {
    let mut tag = None;
    let mut id = None;
    let mut namespace = None;
    let mut name = None;
    let seen = object(
        json,
        cursor,
        depth,
        limits,
        &["kind", "id", "namespace", "type_name"],
        |key, c, d| {
            let _ = d;
            match key {
                0 => tag = Some(string(json, c, None)?),
                1 => id = Some(number(json, c, u32::MAX as u64)? as u32),
                2 => {
                    namespace = Some(string(
                        json,
                        c,
                        Some((&mut budget[3], limits.max_metadata_bytes)),
                    )?)
                }
                _ => {
                    name = Some(string(
                        json,
                        c,
                        Some((&mut budget[3], limits.max_metadata_bytes)),
                    )?)
                }
            }
            Ok(())
        },
    )?;
    match required(tag, *cursor)?.as_str() {
        "numeric" => {
            exact(seen, 3, *cursor)?;
            Ok(UserIdentity::Numeric(required(id, *cursor)?))
        }
        "name" => {
            exact(seen, 13, *cursor)?;
            Ok(UserIdentity::Name {
                namespace: required(namespace, *cursor)?,
                type_name: required(name, *cursor)?,
            })
        }
        _ => Err(invalid(*cursor)),
    }
}
fn scalar(tag: &str, offset: usize) -> Result<ScalarKind, SchemaError> {
    Ok(match tag {
        "bool" => ScalarKind::Bool,
        "i8" => ScalarKind::I8,
        "i16" => ScalarKind::I16,
        "i32" => ScalarKind::I32,
        "i64" => ScalarKind::I64,
        "u8" => ScalarKind::U8,
        "u16" => ScalarKind::U16,
        "u32" => ScalarKind::U32,
        "u64" => ScalarKind::U64,
        "f32" => ScalarKind::F32,
        "f64" => ScalarKind::F64,
        "string" => ScalarKind::String,
        "binary" => ScalarKind::Binary,
        "date" => ScalarKind::Date,
        "timestamp" => ScalarKind::Timestamp,
        "duration" => ScalarKind::Duration,
        _ => return Err(invalid(offset)),
    })
}
// Each tuple is one syntactic TypeDescriptor frame: depth, node/kind property
// masks, phase, node fields, kind fields, and the pending child property.
// Phases 0/1 read the node's first/next property; 2/3 do the same for kind.
fn ty(
    json: &str,
    cursor: &mut usize,
    depth: usize,
    limits: &SchemaLimits,
    budget: &mut [usize; 4],
) -> Result<TypeDescriptor, SchemaError> {
    let frame = |depth| {
        (
            depth,
            0u64,
            0u64,
            0u8,
            None::<bool>,
            None::<bool>,
            None::<Option<IntegerEncoding>>,
            None::<TypeKind>,
            None::<String>,
            None::<ScalarKind>,
            None::<Box<TypeDescriptor>>,
            None::<Option<u64>>,
            None::<Box<TypeDescriptor>>,
            None::<Box<TypeDescriptor>>,
            None::<UserIdentity>,
            0usize,
        )
    };
    charge(&mut budget[1], 1, limits.max_type_nodes, *cursor)?;
    let depth = enter(json, cursor, depth, limits, b'{')?;
    let mut frames = vec![frame(depth)];
    loop {
        let current = frames.last_mut().ok_or_else(|| invalid(*cursor))?;
        let in_kind = current.3 >= 2;
        let first = current.3 == 0 || current.3 == 2;
        let closing = if first {
            peek(json, cursor) == Some(b'}')
        } else {
            match peek(json, cursor) {
                Some(b'}') => true,
                Some(b',') => {
                    *cursor += 1;
                    false
                }
                _ => return Err(malformed(*cursor)),
            }
        };
        if closing {
            *cursor += 1;
            if in_kind {
                let tag = required(current.8.take(), *cursor)?;
                let kind = match tag.as_str() {
                    "scalar" => {
                        exact(current.2, 3, *cursor)?;
                        TypeKind::Scalar(required(current.9.take(), *cursor)?)
                    }
                    "list" | "set" => {
                        exact(current.2, 5, *cursor)?;
                        let element = required(current.10.take(), *cursor)?;
                        if tag == "list" {
                            TypeKind::List(element)
                        } else {
                            TypeKind::Set(element)
                        }
                    }
                    "array" => {
                        exact(current.2, 13, *cursor)?;
                        TypeKind::Array {
                            element: required(current.10.take(), *cursor)?,
                            length: required(current.11.take(), *cursor)?,
                        }
                    }
                    "map" => {
                        exact(current.2, 49, *cursor)?;
                        TypeKind::Map {
                            key: required(current.12.take(), *cursor)?,
                            value: required(current.13.take(), *cursor)?,
                        }
                    }
                    "user" => {
                        exact(current.2, 65, *cursor)?;
                        TypeKind::User(required(current.14.take(), *cursor)?)
                    }
                    _ => return Err(invalid(*cursor)),
                };
                current.7 = Some(kind);
                current.3 = 1;
                continue;
            }
            exact(current.1, 15, *cursor)?;
            let node = TypeDescriptor {
                nullable: required(current.4.take(), *cursor)?,
                track_ref: required(current.5.take(), *cursor)?,
                integer_encoding: required(current.6.take(), *cursor)?,
                kind: required(current.7.take(), *cursor)?,
            };
            frames.pop();
            let Some(parent) = frames.last_mut() else {
                return Ok(node);
            };
            let child = Box::new(node);
            match parent.15 {
                2 => parent.10 = Some(child),
                4 => parent.12 = Some(child),
                5 => parent.13 = Some(child),
                _ => return Err(invalid(*cursor)),
            }
            continue;
        }
        if peek(json, cursor) != Some(b'"') {
            return Err(malformed(*cursor));
        }
        let offset = *cursor;
        let key = string(json, cursor, None)?;
        let keys: &[&str] = if in_kind {
            &[
                "kind", "scalar", "element", "length", "key", "value", "identity",
            ]
        } else {
            &["nullable", "track_ref", "integer_encoding", "kind"]
        };
        let index = keys
            .iter()
            .position(|candidate| *candidate == key)
            .ok_or_else(|| invalid(offset))?;
        let seen = if in_kind {
            &mut current.2
        } else {
            &mut current.1
        };
        if *seen & (1 << index) != 0 {
            return Err(invalid(offset));
        }
        *seen |= 1 << index;
        expect(json, cursor, b':')?;
        if !in_kind {
            current.3 = 1;
            match index {
                0 => current.4 = Some(boolean(json, cursor)?),
                1 => current.5 = Some(boolean(json, cursor)?),
                2 => {
                    current.6 = Some(if peek(json, cursor) == Some(b'n') {
                        literal(json, cursor, "null")?;
                        None
                    } else {
                        Some(match string(json, cursor, None)?.as_str() {
                            "fixed" => IntegerEncoding::Fixed,
                            "varint" => IntegerEncoding::Varint,
                            "tagged" => IntegerEncoding::Tagged,
                            _ => return Err(invalid(*cursor)),
                        })
                    })
                }
                _ => {
                    enter(json, cursor, current.0, limits, b'{')?;
                    current.3 = 2;
                }
            }
        } else {
            current.3 = 3;
            let kind_depth = current
                .0
                .checked_add(1)
                .ok_or_else(|| SchemaError::ResourceLimit(location(*cursor)))?;
            match index {
                0 => current.8 = Some(string(json, cursor, None)?),
                1 => current.9 = Some(scalar(&string(json, cursor, None)?, *cursor)?),
                3 => current.11 = Some(optional_number(json, cursor, u64::MAX)?),
                6 => current.14 = Some(identity(json, cursor, kind_depth, limits, budget)?),
                child => {
                    current.15 = child;
                    charge(&mut budget[1], 1, limits.max_type_nodes, *cursor)?;
                    let depth = enter(json, cursor, kind_depth, limits, b'{')?;
                    frames.push(frame(depth));
                }
            }
        }
    }
}
fn field(
    json: &str,
    cursor: &mut usize,
    depth: usize,
    limits: &SchemaLimits,
    budget: &mut [usize; 4],
) -> Result<FieldDescriptor, SchemaError> {
    charge(&mut budget[2], 1, limits.max_members, *cursor)?;
    let (mut id, mut name, mut field_ty) = (None, None, None);
    let seen = object(
        json,
        cursor,
        depth,
        limits,
        &["id", "name", "ty"],
        |key, c, d| {
            match key {
                0 => id = Some(optional_number(json, c, u32::MAX as u64)?.map(|n| n as u32)),
                1 => {
                    name = Some(string(
                        json,
                        c,
                        Some((&mut budget[3], limits.max_metadata_bytes)),
                    )?)
                }
                _ => field_ty = Some(ty(json, c, d, limits, budget)?),
            }
            Ok(())
        },
    )?;
    exact(seen, 7, *cursor)?;
    Ok(FieldDescriptor {
        id: required(id, *cursor)?,
        name: required(name, *cursor)?,
        ty: required(field_ty, *cursor)?,
    })
}
fn variant(
    json: &str,
    cursor: &mut usize,
    depth: usize,
    limits: &SchemaLimits,
    budget: &mut [usize; 4],
) -> Result<VariantDescriptor, SchemaError> {
    charge(&mut budget[2], 1, limits.max_members, *cursor)?;
    let (mut id, mut name, mut payload) = (None, None, None);
    let seen = object(
        json,
        cursor,
        depth,
        limits,
        &["id", "name", "payload"],
        |key, c, d| {
            match key {
                0 => id = Some(number(json, c, u32::MAX as u64)? as u32),
                1 => {
                    name = Some(string(
                        json,
                        c,
                        Some((&mut budget[3], limits.max_metadata_bytes)),
                    )?)
                }
                _ => {
                    payload = Some(if peek(json, c) == Some(b'n') {
                        literal(json, c, "null")?;
                        None
                    } else {
                        Some(ty(json, c, d, limits, budget)?)
                    })
                }
            }
            Ok(())
        },
    )?;
    exact(seen, 7, *cursor)?;
    Ok(VariantDescriptor {
        id: required(id, *cursor)?,
        name: required(name, *cursor)?,
        payload: required(payload, *cursor)?,
    })
}
fn definition_kind(
    json: &str,
    cursor: &mut usize,
    depth: usize,
    limits: &SchemaLimits,
    budget: &mut [usize; 4],
) -> Result<UserDefinitionKind, SchemaError> {
    let (mut tag, mut fields, mut order, mut variants) = (None, None, None, None);
    let mut positions = 0;
    let seen = object(
        json,
        cursor,
        depth,
        limits,
        &["kind", "fields", "physical_field_order", "variants"],
        |key, c, d| {
            match key {
                0 => tag = Some(string(json, c, None)?),
                1 => {
                    fields = Some(array(json, c, d, limits, |c, d| {
                        field(json, c, d, limits, budget)
                    })?)
                }
                2 => {
                    order = Some(array(json, c, d, limits, |c, _| {
                        charge(&mut positions, 1, limits.max_members, *c)?;
                        number(json, c, usize::MAX as u64).map(|n| n as usize)
                    })?)
                }
                _ => {
                    variants = Some(array(json, c, d, limits, |c, d| {
                        variant(json, c, d, limits, budget)
                    })?)
                }
            }
            Ok(())
        },
    )?;
    let tag = required(tag, *cursor)?;
    match tag.as_str() {
        "struct" => {
            exact(seen, 7, *cursor)?;
            Ok(UserDefinitionKind::Struct {
                fields: required(fields, *cursor)?,
                physical_field_order: required(order, *cursor)?,
            })
        }
        "enum" | "union" => {
            exact(seen, 9, *cursor)?;
            let variants = required(variants, *cursor)?;
            Ok(if tag == "enum" {
                UserDefinitionKind::Enum { variants }
            } else {
                UserDefinitionKind::Union { variants }
            })
        }
        _ => Err(invalid(*cursor)),
    }
}
fn definition(
    json: &str,
    cursor: &mut usize,
    depth: usize,
    limits: &SchemaLimits,
    budget: &mut [usize; 4],
) -> Result<UserDefinition, SchemaError> {
    charge(&mut budget[0], 1, limits.max_definitions, *cursor)?;
    let (mut user, mut kind) = (None, None);
    let seen = object(
        json,
        cursor,
        depth,
        limits,
        &["identity", "kind"],
        |key, c, d| {
            if key == 0 {
                user = Some(identity(json, c, d, limits, budget)?)
            } else {
                kind = Some(definition_kind(json, c, d, limits, budget)?)
            }
            Ok(())
        },
    )?;
    exact(seen, 3, *cursor)?;
    Ok(UserDefinition {
        identity: required(user, *cursor)?,
        kind: required(kind, *cursor)?,
    })
}
fn encoding(
    json: &str,
    cursor: &mut usize,
    depth: usize,
    limits: &SchemaLimits,
) -> Result<RootEncoding, SchemaError> {
    let (mut protocol, mut mode, mut reference) = (None, None, None);
    let seen = object(
        json,
        cursor,
        depth,
        limits,
        &["protocol_version", "mode", "root_reference"],
        |key, c, d| {
            match key {
                0 => {
                    if !version_string(json, c, "fory1_7")? {
                        return Err(SchemaError::UnsupportedVersion(location(*c)));
                    }
                    protocol = Some(ProtocolVersion::Fory1_7);
                }
                1 => {
                    let (mut tag, mut hash) = (None, None);
                    let seen = object(
                        json,
                        c,
                        d,
                        limits,
                        &["kind", "version_hash"],
                        |key, c, _| {
                            if key == 0 {
                                tag = Some(string(json, c, None)?)
                            } else {
                                hash = Some(boolean(json, c)?)
                            }
                            Ok(())
                        },
                    )?;
                    mode = Some(match required(tag, *c)?.as_str() {
                        "compatible" => {
                            exact(seen, 1, *c)?;
                            SchemaMode::Compatible
                        }
                        "same_schema" => {
                            exact(seen, 3, *c)?;
                            SchemaMode::SameSchema {
                                version_hash: required(hash, *c)?,
                            }
                        }
                        _ => return Err(invalid(*c)),
                    });
                }
                _ => {
                    reference = Some(match string(json, c, None)?.as_str() {
                        "tracking" => RootReferenceMode::Tracking,
                        "null_only" => RootReferenceMode::NullOnly,
                        _ => return Err(invalid(*c)),
                    })
                }
            }
            Ok(())
        },
    )?;
    exact(seen, 7, *cursor)?;
    Ok(RootEncoding {
        protocol_version: required(protocol, *cursor)?,
        mode: required(mode, *cursor)?,
        root_reference: required(reference, *cursor)?,
    })
}
fn quoted(output: &mut String, value: &str) {
    output.push('"');
    for ch in value.chars() {
        match ch {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            c if c < '\u{20}' => output.push_str(&format!("\\u{:04x}", c as u32)),
            c => output.push(c),
        }
    }
    output.push('"');
}
fn emit_identity(output: &mut String, user: &UserIdentity) {
    match user {
        UserIdentity::Numeric(id) => {
            output.push_str(&format!("{{\"kind\":\"numeric\",\"id\":{id}}}"))
        }
        UserIdentity::Name {
            namespace,
            type_name,
        } => {
            output.push_str("{\"kind\":\"name\",\"namespace\":");
            quoted(output, namespace);
            output.push_str(",\"type_name\":");
            quoted(output, type_name);
            output.push('}');
        }
    }
}
fn scalar_tag(kind: ScalarKind) -> &'static str {
    match kind {
        ScalarKind::Bool => "bool",
        ScalarKind::I8 => "i8",
        ScalarKind::I16 => "i16",
        ScalarKind::I32 => "i32",
        ScalarKind::I64 => "i64",
        ScalarKind::U8 => "u8",
        ScalarKind::U16 => "u16",
        ScalarKind::U32 => "u32",
        ScalarKind::U64 => "u64",
        ScalarKind::F32 => "f32",
        ScalarKind::F64 => "f64",
        ScalarKind::String => "string",
        ScalarKind::Binary => "binary",
        ScalarKind::Date => "date",
        ScalarKind::Timestamp => "timestamp",
        ScalarKind::Duration => "duration",
    }
}
// A borrowed node and an operation code suffice: no recursive type-tree walk,
// no cloning, and User identities are never expanded into their definitions.
fn emit_type(output: &mut String, root: &TypeDescriptor) {
    let mut stack = vec![(root, 0u8)];
    while let Some((node, operation)) = stack.pop() {
        if operation == 1 {
            output.push_str("}}");
            continue;
        }
        if operation == 2 {
            output.push_str(",\"value\":");
            if let TypeKind::Map { value, .. } = &node.kind {
                stack.push((value, 0));
            }
            continue;
        }
        if operation == 3 {
            if let TypeKind::Array { length, .. } = &node.kind {
                output.push_str(",\"length\":");
                output.push_str(&length.map_or_else(|| "null".into(), |n| n.to_string()));
            }
            continue;
        }
        output.push_str(&format!(
            "{{\"nullable\":{},\"track_ref\":{},\"integer_encoding\":",
            node.nullable, node.track_ref
        ));
        match node.integer_encoding {
            None => output.push_str("null"),
            Some(e) => quoted(
                output,
                match e {
                    IntegerEncoding::Fixed => "fixed",
                    IntegerEncoding::Varint => "varint",
                    IntegerEncoding::Tagged => "tagged",
                },
            ),
        }
        output.push_str(",\"kind\":{");
        stack.push((node, 1));
        match &node.kind {
            TypeKind::Scalar(kind) => {
                output.push_str("\"kind\":\"scalar\",\"scalar\":");
                quoted(output, scalar_tag(*kind));
            }
            TypeKind::User(user) => {
                output.push_str("\"kind\":\"user\",\"identity\":");
                emit_identity(output, user);
            }
            TypeKind::List(element) | TypeKind::Set(element) => {
                output.push_str(if matches!(node.kind, TypeKind::List(_)) {
                    "\"kind\":\"list\",\"element\":"
                } else {
                    "\"kind\":\"set\",\"element\":"
                });
                stack.push((element, 0));
            }
            TypeKind::Array { element, .. } => {
                output.push_str("\"kind\":\"array\",\"element\":");
                stack.push((node, 3));
                stack.push((element, 0));
            }
            TypeKind::Map { key, .. } => {
                output.push_str("\"kind\":\"map\",\"key\":");
                stack.push((node, 2));
                stack.push((key, 0));
            }
        }
    }
}
impl DocumentInner {
    pub(super) fn import_json(json: &str, limits: &SchemaLimits) -> Result<Schema, SchemaError> {
        if json.len() > limits.max_document_bytes {
            return Err(SchemaError::ResourceLimit(location(0)));
        }
        let mut cursor = 0;
        let mut budget = [0; 4];
        let (mut format, mut version, mut semantics, mut enc, mut root, mut definitions) =
            (None, None, None, None, None, None);
        let seen = object(
            json,
            &mut cursor,
            0,
            limits,
            &[
                "format",
                "format_version",
                "key_semantics_version",
                "encoding",
                "root",
                "definitions",
            ],
            |key, c, d| {
                match key {
                    0 => format = Some(version_string(json, c, crate::SCHEMA_DOCUMENT_FORMAT)?),
                    1 => version = Some(number(json, c, u32::MAX as u64)? as u32),
                    2 => semantics = Some(number(json, c, u32::MAX as u64)? as u32),
                    3 => enc = Some(encoding(json, c, d, limits)?),
                    4 => root = Some(ty(json, c, d, limits, &mut budget)?),
                    _ => {
                        definitions = Some(array(json, c, d, limits, |c, d| {
                            definition(json, c, d, limits, &mut budget)
                        })?)
                    }
                }
                Ok(())
            },
        )?;
        space(json, &mut cursor);
        if cursor != json.len() {
            return Err(malformed(cursor));
        }
        exact(seen, 63, cursor)?;
        if !required(format, cursor)?
            || required(version, cursor)? != crate::SCHEMA_DOCUMENT_VERSION
            || required(semantics, cursor)? != crate::KEY_SEMANTICS_VERSION
        {
            return Err(SchemaError::UnsupportedVersion(location(cursor)));
        }
        let descriptor = SchemaDescriptor {
            encoding: required(enc, cursor)?,
            key_semantics_version: crate::KEY_SEMANTICS_VERSION,
            root: required(root, cursor)?,
            definitions: required(definitions, cursor)?,
        };
        // Compilation independently rechecks the constructed declaration, not the
        // import counters. Preserve its known descriptor coordinates unchanged.
        Schema::compile(descriptor, limits)
    }
    pub(super) fn export_json(descriptor: &SchemaDescriptor) -> Result<String, SchemaError> {
        let mut output = format!(
            "{{\"format\":\"pi_fory_schema\",\"format_version\":1,\"key_semantics_version\":{},\"encoding\":{{\"protocol_version\":\"fory1_7\",\"mode\":",
            descriptor.key_semantics_version
        );
        match descriptor.encoding.mode {
            SchemaMode::Compatible => output.push_str("{\"kind\":\"compatible\"}"),
            SchemaMode::SameSchema { version_hash } => output.push_str(&format!(
                "{{\"kind\":\"same_schema\",\"version_hash\":{version_hash}}}"
            )),
        }
        output.push_str(",\"root_reference\":");
        quoted(
            &mut output,
            match descriptor.encoding.root_reference {
                RootReferenceMode::Tracking => "tracking",
                RootReferenceMode::NullOnly => "null_only",
            },
        );
        output.push_str("},\"root\":");
        emit_type(&mut output, &descriptor.root);
        output.push_str(",\"definitions\":[");
        for (i, definition) in descriptor.definitions.iter().enumerate() {
            if i > 0 {
                output.push(',');
            }
            output.push_str("{\"identity\":");
            emit_identity(&mut output, &definition.identity);
            output.push_str(",\"kind\":{");
            match &definition.kind {
                UserDefinitionKind::Struct {
                    fields,
                    physical_field_order,
                } => {
                    output.push_str("\"kind\":\"struct\",\"fields\":[");
                    for (i, field) in fields.iter().enumerate() {
                        if i > 0 {
                            output.push(',');
                        }
                        output.push_str("{\"id\":");
                        output.push_str(&field.id.map_or_else(|| "null".into(), |n| n.to_string()));
                        output.push_str(",\"name\":");
                        quoted(&mut output, &field.name);
                        output.push_str(",\"ty\":");
                        emit_type(&mut output, &field.ty);
                        output.push('}');
                    }
                    output.push_str("],\"physical_field_order\":[");
                    for (i, position) in physical_field_order.iter().enumerate() {
                        if i > 0 {
                            output.push(',');
                        }
                        output.push_str(&position.to_string());
                    }
                    output.push(']');
                }
                UserDefinitionKind::Enum { variants } | UserDefinitionKind::Union { variants } => {
                    output.push_str(
                        if matches!(definition.kind, UserDefinitionKind::Enum { .. }) {
                            "\"kind\":\"enum\",\"variants\":["
                        } else {
                            "\"kind\":\"union\",\"variants\":["
                        },
                    );
                    for (i, variant) in variants.iter().enumerate() {
                        if i > 0 {
                            output.push(',');
                        }
                        output.push_str(&format!("{{\"id\":{},\"name\":", variant.id));
                        quoted(&mut output, &variant.name);
                        output.push_str(",\"payload\":");
                        if let Some(payload) = &variant.payload {
                            emit_type(&mut output, payload);
                        } else {
                            output.push_str("null");
                        }
                        output.push('}');
                    }
                    output.push(']');
                }
            }
            output.push_str("}}");
        }
        output.push_str("]}");
        Ok(output)
    }
}
