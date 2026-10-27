use super::{ParseLimits, ParseLimitsInner, SchemaLimits, SchemaLimitsInner};

impl SchemaLimitsInner {
    pub(super) fn default() -> SchemaLimits {
        SchemaLimits {
            max_document_bytes: 1024 * 1024,
            max_json_depth: 16,
            max_definitions: 512,
            max_type_nodes: 8192,
            max_members: 8192,
            max_metadata_bytes: 512 * 1024,
        }
    }
}

impl ParseLimitsInner {
    pub(super) fn default() -> ParseLimits {
        ParseLimits {
            max_input_bytes: 1024 * 1024,
        }
    }
}
