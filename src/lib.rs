//! Schema, view, and logical-key contracts for the Fory 1.7 wire protocol.
//! Parsing, compilation, validation, registration, and key operations return
//! explicit `NotImplemented` errors. Existing metadata and content accessors
//! only borrow stored data; they do not implement decoding or validation.
pub mod descriptor;
pub mod document;
pub mod encoding;
pub mod error;
pub mod fory_adapter;
pub mod key;
pub mod limits;
pub mod registry;
pub mod schema;
pub mod value;

pub use descriptor::{
    FieldDescriptor, ScalarKind, SchemaDescriptor, TypeDescriptor, TypeKind, UserDefinition,
    UserDefinitionKind, UserIdentity, VariantDescriptor,
};
pub use document::{
    KEY_SEMANTICS_VERSION, SCHEMA_DOCUMENT_FORMAT, SCHEMA_DOCUMENT_VERSION, export_json,
    import_json,
};
pub use encoding::{InputFormat, ProtocolVersion, RootEncoding, RootReferenceMode, SchemaMode};
pub use error::{DataError, ErrorLocation, KeyError, PathSegment, RegistryErrorKind, SchemaError};
pub use key::{KeyRequirement, KeyView, OwnedValidatedKey, ValidatedKey};
pub use limits::{ParseLimits, SchemaLimits};
pub use pi_fory_schema_derive::ForySchema;
pub use registry::{DescribeSchema, SchemaRegistry, SchemaRegistryBuilder};
pub use schema::Schema;
pub use value::{
    BinaryView, NodeEntries, NodeFields, NodeIter, NodeView, ObjectId, OwnedNodeView,
    OwnedValidatedNodeView, OwnedValidatedValue, OwnedValueView, StringView, ValidatedNodeView,
    ValidatedValue, ValueView,
};
