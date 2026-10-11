//! Independent schema JSON document and key-semantics version boundary.
//! Import must reject duplicate and unknown properties and invalid integer
//! lexemes; exact range rules remain pending. No JSON parser exists here.
use crate::{Schema, SchemaDescriptor, SchemaError, SchemaLimits};

/// Schema document format identifier, independent of the Fory wire protocol.
pub const SCHEMA_DOCUMENT_FORMAT: &str = "pi_fory_schema";
/// Version of the schema JSON document contract.
pub const SCHEMA_DOCUMENT_VERSION: u32 = 1;
/// Version of logical-key equality, ordering, and stable hashing semantics.
pub const KEY_SEMANTICS_VERSION: u32 = 1;

/// Reserved flow: strictly decode a descriptor, then call Schema::compile.
pub fn import_json(_json: &str, _limits: &SchemaLimits) -> Result<Schema, SchemaError> {
    Err(SchemaError::NotImplemented("document::import_json"))
}

/// Exports a self-contained descriptor document; does not compile the schema.
/// Serialization is unimplemented and returns `SchemaError::NotImplemented`.
pub fn export_json(_descriptor: &SchemaDescriptor) -> Result<String, SchemaError> {
    Err(SchemaError::NotImplemented("document::export_json"))
}
