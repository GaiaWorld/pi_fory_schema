//! Fory 1.7 root protocol choices; adapter behavior is not implemented.
/// Wire protocol version, not the version of the Rust dependency crate.
#[derive(Clone, Copy, Debug)]
pub enum ProtocolVersion {
    /// Fory 1.7 wire contract; dependency patch releases do not change this tag.
    Fory1_7,
}

/// Schema compatibility mode for the root encoding.
#[derive(Clone, Copy, Debug)]
pub enum SchemaMode {
    /// Carries compatibility metadata.
    Compatible,
    /// Uses the same-schema contract with an explicit version-hash choice.
    SameSchema { version_hash: bool },
}

/// Actual root reference flags must be confirmed against the Fory adapter.
#[derive(Clone, Copy, Debug)]
pub enum RootReferenceMode {
    Tracking,
    NullOnly,
}

/// Encoding is part of the schema contract, not an inferred wire default.
#[derive(Clone, Copy, Debug)]
pub struct RootEncoding {
    /// Explicit wire version; no dependency version is inferred.
    pub protocol_version: ProtocolVersion,
    /// Schema compatibility and version-hash policy.
    pub mode: SchemaMode,
    /// Reference handling at the root boundary.
    pub root_reference: RootReferenceMode,
}

/// Both forms contain one root and no trailing bytes. RootValue is not a body.
#[derive(Clone, Copy, Debug)]
pub enum InputFormat {
    /// Complete cross-language message, including its message framing.
    XlangMessage,
    /// Root value encoding without cross-language message framing.
    RootValue,
}
