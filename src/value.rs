//! Private view and validation states. No operation reads input in this skeleton.
//! Variant inspection and field metadata are exposed as unimplemented contracts.
//! Floating-point reads preserve values without normalization. Time reads use
//! Fory types directly and do not imply key support. Owned promotion preserves
//! validation state and graph context; storage retention remains unimplemented.
//! Owned handles expose node access through `as_view`, rather than duplicating
//! the borrowed API. Root borrowed-to-owned promotion has no confirmed signature.
//! Access and traversal methods below return `DataError::NotImplemented`; only
//! bound type metadata and existing string/binary content can be borrowed.
//! Planned promotion shares input-independent parsing state, never reopening a root.
//! State owns metadata or root-relative locators, not input or schema references.
//! Each handle separately borrows or retains its complete input and schema.
//! Context identity and validation progress survive promotion; unvalidated handles
//! never automatically acquire a validated type.
//! Root, node, and key handles are provisionally single-threaded; no Send/Sync
//! promise or concurrent parsing is provided. Planned state uses Rc sharing,
//! short-lived RefCell updates, and a single-thread OnceCell publication.
//! Validated handles retain immutable state directly, without RefCell reads.
//! Use fallible dynamic borrows; never return guards or retain them in iterators.
//! Planned AccessError failures are recoverable, including dynamic borrow conflicts.
//! Malformed input or schema mismatch terminates an unpublished root; preserve
//! its first error and never publish partially validated state or retry that root.
//! Publish only after full validation and trailing-byte checks; reuse publication.
//! Ownership-copy failure leaves parsing state unchanged. State is declared only.
//! Schema and Bytes ownership are not made single-threaded by this decision.
//! Planned root/field nodes retain the same root backing and parsing state.
//! Node locators carry type-use and wire context, not merely an object identity.
//! No per-scalar record is required; field lookup may scan physical field order.
//! Schema type-use and actual wire read mode remain separate. Parent decoding
//! supplies envelope or inherited group metadata without replaying the root.
//! Inherited metadata is root-owned or addressed by a stable internal handle.
//! Input metadata remains independent of business serializer bindings.
//! Full schema equivalence is not established by hash matches alone.
//! Explicit reference resolution preserves the root and validation level.
//! Private offset caches, object tables, scan frontiers, pause/resume scheduling,
//! and combining scanning with validation are implementation alternatives only.
//! They are not requirements or frozen fields; see the method-level flow document.
use crate::{
    DataError, FieldDescriptor, InputFormat, KeyError, KeyRequirement, OwnedValidatedKey,
    ParseLimits, RootReferenceMode, Schema, TypeDescriptor, ValidatedKey, VariantDescriptor,
};
use std::{
    cell::{OnceCell, RefCell},
    ops::Range,
    rc::Rc,
    sync::Arc,
};

/// Complete root storage only; ownership does not imply validation.
/// Locators are relative to this input range, not to an enclosing storage page.
/// No cached length, self-references, or parsing state is stored here.
pub(crate) enum RootBacking<'a> {
    Borrowed {
        _schema: &'a Schema,
        _input: &'a [u8],
    },
    Retained(RetainedRoot),
}

/// Owned handles cannot accidentally carry a borrowed backing.
pub(crate) struct RetainedRoot {
    pub(crate) _schema: Arc<Schema>,
    pub(crate) _input: bytes::Bytes,
}

/// Input-independent state shared across promotion; Rc ownership identifies the root.
pub(crate) struct RootState {
    _format: InputFormat,
    _limits: ParseLimits,
    _progress: RefCell<RootProgress>,
    _validated: OnceCell<Rc<ValidatedState>>,
}

/// No scanner, frontier, or resume-frame contract is chosen here.
enum RootProgress {
    Parsing {
        _root: Option<NodeLocation>,
        /// Wire object slots retain logical declarations and complete-root locations.
        _objects: Vec<(TypeDescriptor, NodeLocation)>,
        /// Wire metadata slots: encoded range and bound schema definition index.
        /// This is not yet a decoded remote-field layout.
        _metadata: Vec<(Range<usize>, usize)>,
    },
    Failed(DataError),
    /// Publication has taken the tables; no mutable copy remains here.
    Published,
}

/// Only full validation may publish this result; construction is unimplemented.
/// Tables move here rather than being retained as mutable duplicates.
pub(crate) struct ValidatedState {
    _root: NodeLocation,
    _objects: Vec<(TypeDescriptor, NodeLocation)>,
    _metadata: Vec<(Range<usize>, usize)>,
}

impl ValidatedState {
    /// 内容句柄借用稳定输入，不借临时遍历节点；不改变验证状态。
    pub(crate) fn key_string<'a>(
        node: &ValidatedNodeView<'a>,
    ) -> Result<StringView<'a>, DataError> {
        ValidatedNodeViewInner::key_string(node)
    }
    pub(crate) fn key_binary<'a>(
        node: &ValidatedNodeView<'a>,
    ) -> Result<BinaryView<'a>, DataError> {
        ValidatedNodeViewInner::key_binary(node)
    }
    /// Borrows a key traversal node from stable root resources, not its parent.
    /// A supplied child must belong to this exact published root and backing.
    /// Does not reopen, validate, retain-copy input, or borrow mutable progress.
    pub(crate) fn key_node<'a>(
        this: &Rc<Self>,
        schema: &'a Schema,
        input: &'a [u8],
        context: &Rc<RootState>,
        child: Option<&ValidatedNodeView<'_>>,
    ) -> Result<ValidatedNodeView<'a>, DataError> {
        ValidatedNodeViewInner::key_node(this, schema, input, context, child)
    }
}

/// Logical declaration lives on the node, separately from its actual wire read mode.
struct NodeLocation {
    _byte_offset: usize,
    /// Only true when a parent header explicitly supplies null without a body.
    _parent_null: bool,
    /// Actual built-in wire ID confirmed by the parent group, not a metadata slot.
    _wire_type_id: Option<u32>,
    /// None means no reference marker, not a nullable/track_ref schema rewrite.
    _reference_mode: Option<RootReferenceMode>,
    _type_source: NodeTypeSource,
}

/// Extend this private boundary when fixed-version wire adaptation is implemented.
/// Slot indexes address this root's metadata, never definitions or ObjectId.
enum NodeTypeSource {
    Schema,
    Envelope,
    Inherited { _metadata_slot: usize },
}

/// Context-scoped opaque identity; no public constructor.
/// The internal context must be retained for context-local deduplication.
/// Not a stable business key; these traits are not logical-key traits.
#[derive(Clone)]
pub struct ObjectId {
    _context: Rc<RootState>,
    _local: u64,
}

impl std::fmt::Debug for ObjectId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        ObjectIdInner::fmt(self, f)
    }
}
impl PartialEq for ObjectId {
    fn eq(&self, other: &Self) -> bool {
        ObjectIdInner::eq(self, other)
    }
}
impl Eq for ObjectId {}
impl std::hash::Hash for ObjectId {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        ObjectIdInner::hash(self, state)
    }
}
/// Read-only content handle; creation and decoding remain unimplemented.
/// The private holder may borrow content or retain decoded content and its lease.
pub struct StringView<'a> {
    content: Box<dyn AsRef<str> + 'a>,
}
/// Read-only byte handle; no public constructor implies validated input.
pub struct BinaryView<'a> {
    content: Box<dyn AsRef<[u8]> + 'a>,
}
#[path = "value_inner.rs"]
mod value_inner;

struct StringViewInner;
struct ObjectIdInner;
struct BinaryViewInner;
struct ValueViewInner;
struct OwnedValueViewInner;
struct ValidatedValueInner;
struct OwnedValidatedValueInner;
struct OwnedNodeViewInner;
struct OwnedValidatedNodeViewInner;
struct NodeViewInner;
struct ValidatedNodeViewInner;

impl StringView<'_> {
    /// Borrows existing content for the lifetime of this handle borrow.
    /// Does not decode, allocate, validate, or consume a resource budget.
    pub fn as_str(&self) -> &str {
        StringViewInner::as_str(self)
    }
}
impl BinaryView<'_> {
    /// Borrows existing bytes; the returned slice cannot outlive this handle.
    pub fn as_bytes(&self) -> &[u8] {
        BinaryViewInner::as_bytes(self)
    }
}

/// Fallible sequence traversal tied to the borrowed context.
pub type NodeIter<'a, N> = Box<dyn Iterator<Item = Result<N, DataError>> + 'a>;
/// Fallible map traversal yielding key/value node pairs.
pub type NodeEntries<'a, N> = Box<dyn Iterator<Item = Result<(N, N), DataError>> + 'a>;
/// Iterates field metadata together with each corresponding node.
pub type NodeFields<'a, N> =
    Box<dyn Iterator<Item = Result<(&'a FieldDescriptor, N), DataError>> + 'a>;
/// Borrowed root context whose data has not been fully validated.
pub struct ValueView<'a> {
    pub(crate) _backing: RootBacking<'a>,
    pub(crate) _context: Rc<RootState>,
}
/// Borrowed root context carrying private, completed validation state.
pub struct ValidatedValue<'a> {
    pub(crate) _backing: RootBacking<'a>,
    pub(crate) _context: Rc<RootState>,
    pub(crate) _validated: Rc<ValidatedState>,
}
/// Borrowed node in an unvalidated root context.
pub struct NodeView<'a> {
    node_type: &'a TypeDescriptor,
    _backing: RootBacking<'a>,
    _context: Rc<RootState>,
    _location: Rc<NodeLocation>,
}
/// Borrowed node preserving its root's completed validation state.
pub struct ValidatedNodeView<'a> {
    node_type: &'a TypeDescriptor,
    _backing: RootBacking<'a>,
    _context: Rc<RootState>,
    _validated: Rc<ValidatedState>,
    _location: Rc<NodeLocation>,
}
/// Retained unvalidated root; node access is through `Self::as_view`.
pub struct OwnedValueView {
    pub(crate) _backing: RetainedRoot,
    pub(crate) _context: Rc<RootState>,
}
/// Retained validated root; node access is through `Self::as_view`.
pub struct OwnedValidatedValue {
    pub(crate) _backing: RetainedRoot,
    pub(crate) _context: Rc<RootState>,
    pub(crate) _validated: Rc<ValidatedState>,
}
/// Retained unvalidated node and graph context, accessed through `Self::as_view`.
pub struct OwnedNodeView {
    _backing: RetainedRoot,
    _context: Rc<RootState>,
    /// Owned declaration avoids a self-reference into the retained schema.
    _node_type: TypeDescriptor,
    _location: Rc<NodeLocation>,
}
/// Retained validated node and graph context, accessed through `Self::as_view`.
pub struct OwnedValidatedNodeView {
    _backing: RetainedRoot,
    _context: Rc<RootState>,
    _validated: Rc<ValidatedState>,
    _node_type: TypeDescriptor,
    _location: Rc<NodeLocation>,
}

impl<'a> ValueView<'a> {
    /// Validates using the existing context budget; no new budget is supplied.
    pub fn validate(self) -> Result<ValidatedValue<'a>, DataError> {
        ValueViewInner::validate(self)
    }
}
impl OwnedValueView {
    /// Validates using the existing context budget; no new budget is supplied.
    pub fn validate(self) -> Result<OwnedValidatedValue, DataError> {
        OwnedValueViewInner::validate(self)
    }
}
impl<'a> ValidatedValue<'a> {
    /// Checks only the requested key capabilities and wraps already validated data.
    /// Does not revalidate, create a budget, or check/consume the context budget.
    pub fn into_key(self, _requirement: KeyRequirement) -> Result<ValidatedKey<'a>, KeyError> {
        ValidatedValueInner::into_key(self, _requirement)
    }
}
impl OwnedValidatedValue {
    /// Checks only the requested key capabilities and wraps already validated data.
    /// Retains its context; does not revalidate or check/consume any parse budget.
    pub fn into_key(self, _requirement: KeyRequirement) -> Result<OwnedValidatedKey, KeyError> {
        OwnedValidatedValueInner::into_key(self, _requirement)
    }
}
impl OwnedValueView {
    /// Borrows without changing validation state; lease remains with the owner.
    pub fn as_view(&self) -> Result<ValueView<'_>, DataError> {
        OwnedValueViewInner::as_view(self)
    }
}
impl OwnedValidatedValue {
    /// Borrows without changing validation state; lease remains with the owner.
    pub fn as_view(&self) -> Result<ValidatedValue<'_>, DataError> {
        OwnedValidatedValueInner::as_view(self)
    }
}
impl OwnedNodeView {
    /// Borrows without changing validation state; lease remains with the owner.
    pub fn as_view(&self) -> Result<NodeView<'_>, DataError> {
        OwnedNodeViewInner::as_view(self)
    }
}
impl OwnedValidatedNodeView {
    /// Borrows without changing validation state; lease remains with the owner.
    pub fn as_view(&self) -> Result<ValidatedNodeView<'_>, DataError> {
        OwnedValidatedNodeViewInner::as_view(self)
    }
}
impl ValueView<'_> {
    /// Borrows the root without promoting validation state.
    pub fn root(&self) -> Result<NodeView<'_>, DataError> {
        ValueViewInner::root(self)
    }
}
impl ValidatedValue<'_> {
    /// Borrows the root while preserving completed validation state.
    pub fn root(&self) -> Result<ValidatedNodeView<'_>, DataError> {
        ValidatedValueInner::root(self)
    }
}
impl NodeView<'_> {
    /// Retains this node independently of the parent object's lifetime.
    /// Shares owned storage when available; borrowed input may require copying
    /// the root buffer. Retains schema, reference identity, and existing budget.
    /// Does not validate, reset limits, or encode a standalone root value.
    pub fn to_owned(&self) -> Result<OwnedNodeView, DataError> {
        NodeViewInner::to_owned(self)
    }

    /// Borrows the bound declaration without parsing, allocating, or validating.
    /// A reference edge reports its declared type without resolving its target.
    pub fn node_type(&self) -> &TypeDescriptor {
        NodeViewInner::node_type(self)
    }
    /// Queries the selected variant descriptor; unknown variants do not fall back.
    pub fn variant(&self) -> Result<&VariantDescriptor, DataError> {
        NodeViewInner::variant(self)
    }
    /// None means no payload; Some may contain a null node.
    /// Unknown variants do not fall back to another variant or payload.
    pub fn variant_payload(&self) -> Result<Option<NodeView<'_>>, DataError> {
        NodeViewInner::variant_payload(self)
    }
    pub fn is_null(&self) -> Result<bool, DataError> {
        NodeViewInner::is_null(self)
    }
    pub fn reference_id(&self) -> Result<Option<ObjectId>, DataError> {
        NodeViewInner::reference_id(self)
    }
    pub fn object_id(&self) -> Result<Option<ObjectId>, DataError> {
        NodeViewInner::object_id(self)
    }
    /// Resolves the supplied identity within this node's context.
    pub fn resolve_reference(&self, _id: &ObjectId) -> Result<NodeView<'_>, DataError> {
        NodeViewInner::resolve_reference(self, _id)
    }
    pub fn field_by_id(&self, _id: u32) -> Result<NodeView<'_>, DataError> {
        NodeViewInner::field_by_id(self, _id)
    }
    pub fn field_by_name(&self, _name: &str) -> Result<NodeView<'_>, DataError> {
        NodeViewInner::field_by_name(self, _name)
    }
    pub fn field_at(&self, _index: usize) -> Result<NodeView<'_>, DataError> {
        NodeViewInner::field_at(self, _index)
    }
    /// Iterates bound field descriptors and their corresponding nodes.
    pub fn fields(&self) -> Result<NodeFields<'_, NodeView<'_>>, DataError> {
        NodeViewInner::fields(self)
    }
    pub fn len(&self) -> Result<usize, DataError> {
        NodeViewInner::len(self)
    }
    pub fn element_at(&self, _index: usize) -> Result<NodeView<'_>, DataError> {
        NodeViewInner::element_at(self, _index)
    }
    pub fn elements(&self) -> Result<NodeIter<'_, NodeView<'_>>, DataError> {
        NodeViewInner::elements(self)
    }
    pub fn entries(&self) -> Result<NodeEntries<'_, NodeView<'_>>, DataError> {
        NodeViewInner::entries(self)
    }
    pub fn read_string(&self) -> Result<StringView<'_>, DataError> {
        NodeViewInner::read_string(self)
    }
    pub fn read_binary(&self) -> Result<BinaryView<'_>, DataError> {
        NodeViewInner::read_binary(self)
    }
    pub fn read_date(&self) -> Result<fory::Date, DataError> {
        NodeViewInner::read_date(self)
    }
    pub fn read_timestamp(&self) -> Result<fory::Timestamp, DataError> {
        NodeViewInner::read_timestamp(self)
    }
    pub fn read_duration(&self) -> Result<fory::Duration, DataError> {
        NodeViewInner::read_duration(self)
    }
    pub fn read_bool(&self) -> Result<bool, DataError> {
        NodeViewInner::read_bool(self)
    }
    pub fn read_i8(&self) -> Result<i8, DataError> {
        NodeViewInner::read_i8(self)
    }
    pub fn read_i16(&self) -> Result<i16, DataError> {
        NodeViewInner::read_i16(self)
    }
    pub fn read_i32(&self) -> Result<i32, DataError> {
        NodeViewInner::read_i32(self)
    }
    pub fn read_i64(&self) -> Result<i64, DataError> {
        NodeViewInner::read_i64(self)
    }
    pub fn read_u8(&self) -> Result<u8, DataError> {
        NodeViewInner::read_u8(self)
    }
    pub fn read_u16(&self) -> Result<u16, DataError> {
        NodeViewInner::read_u16(self)
    }
    pub fn read_u32(&self) -> Result<u32, DataError> {
        NodeViewInner::read_u32(self)
    }
    pub fn read_u64(&self) -> Result<u64, DataError> {
        NodeViewInner::read_u64(self)
    }
    pub fn read_f32(&self) -> Result<f32, DataError> {
        NodeViewInner::read_f32(self)
    }
    pub fn read_f64(&self) -> Result<f64, DataError> {
        NodeViewInner::read_f64(self)
    }
}
impl ValidatedNodeView<'_> {
    /// Retains the validated node and its graph context beyond the parent.
    /// Shares owned storage when available; borrowed input may require copying
    /// the root buffer. Does not revalidate or reset the context budget.
    /// Ownership is independent; encoding and reference targets need not be.
    pub fn to_owned(&self) -> Result<OwnedValidatedNodeView, DataError> {
        ValidatedNodeViewInner::to_owned(self)
    }

    /// Borrows the bound declaration without parsing, allocating, or validating.
    /// A reference edge reports its declared type without resolving its target.
    pub fn node_type(&self) -> &TypeDescriptor {
        ValidatedNodeViewInner::node_type(self)
    }
    /// Queries the selected variant descriptor; unknown variants do not fall back.
    pub fn variant(&self) -> Result<&VariantDescriptor, DataError> {
        ValidatedNodeViewInner::variant(self)
    }
    /// None means no payload; Some may contain a null node.
    /// Unknown variants do not fall back to another variant or payload.
    pub fn variant_payload(&self) -> Result<Option<ValidatedNodeView<'_>>, DataError> {
        ValidatedNodeViewInner::variant_payload(self)
    }
    pub fn is_null(&self) -> Result<bool, DataError> {
        ValidatedNodeViewInner::is_null(self)
    }
    pub fn reference_id(&self) -> Result<Option<ObjectId>, DataError> {
        ValidatedNodeViewInner::reference_id(self)
    }
    pub fn object_id(&self) -> Result<Option<ObjectId>, DataError> {
        ValidatedNodeViewInner::object_id(self)
    }
    /// Resolves the supplied identity within this node's context.
    pub fn resolve_reference(&self, _id: &ObjectId) -> Result<ValidatedNodeView<'_>, DataError> {
        ValidatedNodeViewInner::resolve_reference(self, _id)
    }
    pub fn field_by_id(&self, _id: u32) -> Result<ValidatedNodeView<'_>, DataError> {
        ValidatedNodeViewInner::field_by_id(self, _id)
    }
    pub fn field_by_name(&self, _name: &str) -> Result<ValidatedNodeView<'_>, DataError> {
        ValidatedNodeViewInner::field_by_name(self, _name)
    }
    pub fn field_at(&self, _index: usize) -> Result<ValidatedNodeView<'_>, DataError> {
        ValidatedNodeViewInner::field_at(self, _index)
    }
    /// Iterates bound field descriptors and their corresponding nodes.
    pub fn fields(&self) -> Result<NodeFields<'_, ValidatedNodeView<'_>>, DataError> {
        ValidatedNodeViewInner::fields(self)
    }
    pub fn len(&self) -> Result<usize, DataError> {
        ValidatedNodeViewInner::len(self)
    }
    pub fn element_at(&self, _index: usize) -> Result<ValidatedNodeView<'_>, DataError> {
        ValidatedNodeViewInner::element_at(self, _index)
    }
    pub fn elements(&self) -> Result<NodeIter<'_, ValidatedNodeView<'_>>, DataError> {
        ValidatedNodeViewInner::elements(self)
    }
    pub fn entries(&self) -> Result<NodeEntries<'_, ValidatedNodeView<'_>>, DataError> {
        ValidatedNodeViewInner::entries(self)
    }
    pub fn read_string(&self) -> Result<StringView<'_>, DataError> {
        ValidatedNodeViewInner::read_string(self)
    }
    pub fn read_binary(&self) -> Result<BinaryView<'_>, DataError> {
        ValidatedNodeViewInner::read_binary(self)
    }
    pub fn read_date(&self) -> Result<fory::Date, DataError> {
        ValidatedNodeViewInner::read_date(self)
    }
    pub fn read_timestamp(&self) -> Result<fory::Timestamp, DataError> {
        ValidatedNodeViewInner::read_timestamp(self)
    }
    pub fn read_duration(&self) -> Result<fory::Duration, DataError> {
        ValidatedNodeViewInner::read_duration(self)
    }
    pub fn read_bool(&self) -> Result<bool, DataError> {
        ValidatedNodeViewInner::read_bool(self)
    }
    pub fn read_i8(&self) -> Result<i8, DataError> {
        ValidatedNodeViewInner::read_i8(self)
    }
    pub fn read_i16(&self) -> Result<i16, DataError> {
        ValidatedNodeViewInner::read_i16(self)
    }
    pub fn read_i32(&self) -> Result<i32, DataError> {
        ValidatedNodeViewInner::read_i32(self)
    }
    pub fn read_i64(&self) -> Result<i64, DataError> {
        ValidatedNodeViewInner::read_i64(self)
    }
    pub fn read_u8(&self) -> Result<u8, DataError> {
        ValidatedNodeViewInner::read_u8(self)
    }
    pub fn read_u16(&self) -> Result<u16, DataError> {
        ValidatedNodeViewInner::read_u16(self)
    }
    pub fn read_u32(&self) -> Result<u32, DataError> {
        ValidatedNodeViewInner::read_u32(self)
    }
    pub fn read_u64(&self) -> Result<u64, DataError> {
        ValidatedNodeViewInner::read_u64(self)
    }
    pub fn read_f32(&self) -> Result<f32, DataError> {
        ValidatedNodeViewInner::read_f32(self)
    }
    pub fn read_f64(&self) -> Result<f64, DataError> {
        ValidatedNodeViewInner::read_f64(self)
    }
}
