//! Unvalidated, self-contained descriptors; no wire encoding is inferred.

/// Unvalidated schema declaration for document version 1 and key semantics 1.
/// Definitions must contain exactly the root-reachable user definitions.
#[derive(Clone, Debug)]
pub struct SchemaDescriptor {
    /// Explicit root wire encoding, including the protocol version.
    pub encoding: crate::RootEncoding,
    /// Logical-key semantics version; currently `crate::KEY_SEMANTICS_VERSION`.
    pub key_semantics_version: u32,
    /// Root type declaration.
    pub root: TypeDescriptor,
    /// Exactly the user definitions reachable from the root.
    pub definitions: Vec<UserDefinition>,
}

/// Each concrete generic instantiation has an independent identity.
/// Numeric identity ranges still require validation rules.
/// Numeric and named identities remain distinct, with exact name matching.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum UserIdentity {
    Numeric(u32),
    /// Fory 1.7 adapter contract, not enforced by this unvalidated enum.
    /// Namespace may be empty or contain dots. Type name is nonempty and dot-free.
    /// Registration and schema compilation must enforce the same rules.
    /// Preserve exact strings without trimming, normalization, or escaping.
    /// Each component is at most 32766 original UTF-8 bytes in Fory 1.7.3.
    /// Unicode is allowed; this is not a cross-language interoperability proof.
    /// Smaller resource limits may reject otherwise valid identities.
    Name {
        namespace: String,
        type_name: String,
    },
}

/// Type declaration with independent nullability and reference tracking.
#[derive(Clone, Debug)]
pub struct TypeDescriptor {
    pub nullable: bool,
    pub track_ref: bool,
    pub kind: TypeKind,
}

/// Logical scalar kinds; integer wire encodings still need adapter verification.
/// Floating-point reads preserve values; time scalars do not support keys.
#[derive(Clone, Copy, Debug)]
pub enum ScalarKind {
    Bool,
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    F32,
    F64,
    String,
    Binary,
    Date,
    Timestamp,
    Duration,
}

/// Scalar, container, or user-type shape; user identities refer to definitions.
#[derive(Clone, Debug)]
pub enum TypeKind {
    Scalar(ScalarKind),
    List(Box<TypeDescriptor>),
    Array {
        element: Box<TypeDescriptor>,
        /// None does not impose a fixed logical length.
        length: Option<u64>,
    },
    Map {
        key: Box<TypeDescriptor>,
        value: Box<TypeDescriptor>,
    },
    Set(Box<TypeDescriptor>),
    User(UserIdentity),
}

/// A user type's identity and its logical definition.
#[derive(Clone, Debug)]
pub struct UserDefinition {
    pub identity: UserIdentity,
    pub kind: UserDefinitionKind,
}

/// Vector order is logical order. Skipped fields and runtime Unknown are omitted.
#[derive(Clone, Debug)]
pub enum UserDefinitionKind {
    Struct { fields: Vec<FieldDescriptor> },
    Enum { variants: Vec<VariantDescriptor> },
    Union { variants: Vec<VariantDescriptor> },
}

/// ID ranges, uniqueness, and name constraints await validation rules.
#[derive(Clone, Debug)]
pub struct FieldDescriptor {
    pub id: Option<u32>,
    pub name: String,
    pub ty: TypeDescriptor,
}

/// Variant IDs are required; accepted numeric ranges remain unverified.
#[derive(Clone, Debug)]
pub struct VariantDescriptor {
    pub id: u32,
    pub name: String,
    pub payload: Option<TypeDescriptor>,
}
