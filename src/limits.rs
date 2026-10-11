//! Schema limits are configurable; defaults are policy, not proven safety bounds.
//! Both configurations are publicly constructible; limit enforcement remains unimplemented.
/// Per-call schema and import limits; checks are not implemented.
/// Bounds are inclusive; checked-counter overflow is a resource-limit failure.
/// Import checks incrementally, then compilation rechecks without double charging.
/// These are structural limits, not exact heap-byte or execution-time guarantees.
/// Direct compilation cannot undo caller allocations or guarantee deep-tree drop safety.
/// Zero is a strict zero bound, never unlimited; large bounds do not disable checks.
#[derive(Clone, Debug)]
pub struct SchemaLimits {
    /// Raw JSON UTF-8 bytes, including whitespace and escapes; import only.
    pub max_document_bytes: usize,
    /// JSON object/array nesting; root object is 1, scalars add no depth.
    pub max_json_depth: usize,
    /// All definition entries, including entries later rejected as invalid.
    pub max_definitions: usize,
    /// TypeDescriptor occurrences; user references count once without expansion.
    pub max_type_nodes: usize,
    /// Total field and variant entries, including unit variants.
    pub max_members: usize,
    /// UTF-8 bytes of every identity component, field name, and variant name.
    /// Count repeated occurrences; exclude JSON keys, fixed tags, and numeric lexemes.
    pub max_metadata_bytes: usize,
}
impl Default for SchemaLimits {
    fn default() -> Self {
        Self {
            max_document_bytes: 1024 * 1024,
            max_json_depth: 16,
            max_definitions: 512,
            max_type_nodes: 8192,
            max_members: 8192,
            max_metadata_bytes: 512 * 1024,
        }
    }
}

/// Per-root input size limit; the 1 MiB default is adjustable policy.
/// Zero is a strict zero bound; large bounds do not disable format checks.
/// A new root snapshots configuration and checks the full input, including framing.
/// Derived nodes and ownership conversion retain the context without resetting it.
/// Repeated access and key operations do not consume an access-count budget.
/// No decoded-size, container-length, heap-byte, or execution-time guarantee.
/// Declared lengths still require checked arithmetic and wire-format validation.
#[derive(Clone, Debug)]
pub struct ParseLimits {
    /// Inclusive full input byte bound, checked at the root entry; unimplemented.
    pub max_input_bytes: usize,
}

impl Default for ParseLimits {
    fn default() -> Self {
        Self {
            max_input_bytes: 1024 * 1024,
        }
    }
}
