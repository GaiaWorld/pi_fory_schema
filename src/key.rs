//! Key contracts only. Time scalars have no key support.
//! Logical ordering and full schema equivalence are confirmed contracts.
//! Stable hashes are 64-bit and governed by the key-semantics version.
//! Normal key operations do not revalidate data or check resource budgets.
//! Equality and ordering require full schema equivalence, not a hash match.
//! Logical equality, ordering, and versioned portable V3 stable hashing are implemented.
use crate::value::{RootState, ValidatedState};
use crate::{KeyError, OwnedValidatedValue, Schema, ValidatedValue};
use std::cmp::Ordering;

/// 单次比较的延续帧；逐项推进容器，不将全部子节点压栈。
enum CompareFrame<'a, 'b> {
    Node(crate::ValidatedNodeView<'a>, crate::ValidatedNodeView<'b>),
    Children {
        left: crate::ValidatedNodeView<'a>,
        right: crate::ValidatedNodeView<'b>,
        index: usize,
        common: usize,
        lengths: Ordering,
        fields: bool,
    },
}

/// 规范逻辑流的深度优先延续，不物化整个规范缓冲。
enum HashFrame<'a> {
    Chunk(HashChunk<'a>),
    Node(crate::ValidatedNodeView<'a>),
    Children {
        node: crate::ValidatedNodeView<'a>,
        index: usize,
        count: usize,
        fields: bool,
    },
}

enum HashChunk<'a> {
    Inline { bytes: [u8; 32], len: usize },
    String(crate::StringView<'a>),
    Binary(crate::BinaryView<'a>),
}

struct KeyHashReader<'a> {
    parts: sealed::KeyParts<'a>,
    stack: Vec<HashFrame<'a>>,
    pending: Option<HashChunk<'a>>,
    offset: usize,
    error: Option<KeyError>,
}

/// Required logical-key capabilities, checked before wrapping validated data.
#[derive(Clone, Copy, Debug)]
pub enum KeyRequirement {
    /// Logical equality and stable hashing.
    Hash,
    /// Logical equality and ordering.
    Ordered,
    /// Both stable hashing and logical ordering.
    HashAndOrdered,
}

mod sealed {
    pub trait Sealed {
        fn key_parts(&self) -> Option<KeyParts<'_>>;
    }

    /// Internal borrowed operands; inaccessible outside this private module boundary.
    /// None means no prepared operand, never fabricated data.
    pub struct KeyParts<'a> {
        pub(crate) _schema: &'a super::Schema,
        pub(crate) _input: &'a [u8],
        pub(crate) _context: &'a std::rc::Rc<super::RootState>,
        pub(crate) _validated: &'a std::rc::Rc<super::ValidatedState>,
        /// Stable-root node for explicit traversal; no input copy or parent borrow.
        pub(crate) _root: crate::ValidatedNodeView<'a>,
        pub(crate) _requirement: super::KeyRequirement,
    }
}
/// Sealed boundary for owned/borrowed cross-key operations.
pub trait KeyView: sealed::Sealed {}
/// Borrowed validated data whose requested key capabilities have been checked.
pub struct ValidatedKey<'a> {
    pub(crate) _value: ValidatedValue<'a>,
    pub(crate) _requirement: KeyRequirement,
}
/// Owned validated key retaining its schema and data context.
pub struct OwnedValidatedKey {
    pub(crate) _value: OwnedValidatedValue,
    pub(crate) _requirement: KeyRequirement,
}
#[path = "key_inner.rs"]
pub(crate) mod key_inner;

struct ValidatedKeyInner;
struct OwnedValidatedKeyInner;

impl KeyView for ValidatedKey<'_> {}
impl ValidatedKey<'_> {
    /// Compares logical values under fully equivalent schemas, without
    /// revalidation or budget checks; a schema mismatch is an error.
    pub fn try_eq<K: KeyView + ?Sized>(&self, _other: &K) -> Result<bool, KeyError> {
        ValidatedKeyInner::try_eq::<K>(self, _other)
    }
    /// Compares by confirmed logical ordering, not encoded byte order.
    /// Requires ordered capability and full schema equivalence; no budget check.
    pub fn try_cmp<K: KeyView + ?Sized>(&self, _other: &K) -> Result<Ordering, KeyError> {
        ValidatedKeyInner::try_cmp::<K>(self, _other)
    }
    /// Returns the versioned 64-bit logical hash using portable rapidhash V3.
    pub fn stable_hash(&self) -> Result<u64, KeyError> {
        ValidatedKeyInner::stable_hash(self)
    }
}
impl KeyView for OwnedValidatedKey {}
impl OwnedValidatedKey {
    /// Compares logical values under fully equivalent schemas, without
    /// revalidation or budget checks; a schema mismatch is an error.
    pub fn try_eq<K: KeyView + ?Sized>(&self, _other: &K) -> Result<bool, KeyError> {
        OwnedValidatedKeyInner::try_eq::<K>(self, _other)
    }
    /// Compares by confirmed logical ordering, not encoded byte order.
    /// Requires ordered capability and full schema equivalence; no budget check.
    pub fn try_cmp<K: KeyView + ?Sized>(&self, _other: &K) -> Result<Ordering, KeyError> {
        OwnedValidatedKeyInner::try_cmp::<K>(self, _other)
    }
    /// Returns the versioned 64-bit logical hash using portable rapidhash V3.
    pub fn stable_hash(&self) -> Result<u64, KeyError> {
        OwnedValidatedKeyInner::stable_hash(self)
    }
}
