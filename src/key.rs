//! Key contracts only. Time scalars have no key support.
//! Logical ordering and full schema equivalence are confirmed contracts.
//! Stable hashes are 64-bit and governed by the key-semantics version.
//! Normal key operations do not revalidate data or check resource budgets.
//! Equality and ordering require full schema equivalence, not a hash match.
//! All key operations remain explicit `NotImplemented` placeholders.
use crate::KeyError;
use std::{cmp::Ordering, marker::PhantomData};

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
    pub trait Sealed {}
}
/// Sealed boundary for owned/borrowed cross-key operations.
pub trait KeyView: sealed::Sealed {}
/// Borrowed validated data whose requested key capabilities have been checked.
pub struct ValidatedKey<'a> {
    _private: PhantomData<&'a ()>,
}
/// Owned validated key retaining its schema and data context.
pub struct OwnedValidatedKey {
    _private: (),
}
impl sealed::Sealed for ValidatedKey<'_> {}
impl KeyView for ValidatedKey<'_> {}
impl ValidatedKey<'_> {
    /// Compares logical values under fully equivalent schemas, without
    /// revalidation or budget checks; a schema mismatch is an error.
    pub fn try_eq<K: KeyView + ?Sized>(&self, _other: &K) -> Result<bool, KeyError> {
        Err(KeyError::NotImplemented("try_eq"))
    }
    /// Compares by confirmed logical ordering, not encoded byte order.
    /// Requires ordered capability and full schema equivalence; no budget check.
    pub fn try_cmp<K: KeyView + ?Sized>(&self, _other: &K) -> Result<Ordering, KeyError> {
        Err(KeyError::NotImplemented("try_cmp"))
    }
    /// Returns the confirmed 64-bit hash; the algorithm is not implemented.
    pub fn stable_hash(&self) -> Result<u64, KeyError> {
        Err(KeyError::NotImplemented("stable_hash"))
    }
}
impl sealed::Sealed for OwnedValidatedKey {}
impl KeyView for OwnedValidatedKey {}
impl OwnedValidatedKey {
    /// Compares logical values under fully equivalent schemas, without
    /// revalidation or budget checks; a schema mismatch is an error.
    pub fn try_eq<K: KeyView + ?Sized>(&self, _other: &K) -> Result<bool, KeyError> {
        Err(KeyError::NotImplemented("try_eq"))
    }
    /// Compares by confirmed logical ordering, not encoded byte order.
    /// Requires ordered capability and full schema equivalence; no budget check.
    pub fn try_cmp<K: KeyView + ?Sized>(&self, _other: &K) -> Result<Ordering, KeyError> {
        Err(KeyError::NotImplemented("try_cmp"))
    }
    /// Returns the confirmed 64-bit hash; the algorithm is not implemented.
    pub fn stable_hash(&self) -> Result<u64, KeyError> {
        Err(KeyError::NotImplemented("stable_hash"))
    }
}
