use super::{
    DataError, DataErrorInner, Error, KeyError, KeyErrorInner, SchemaError, SchemaErrorInner, fmt,
};

impl SchemaErrorInner {
    pub(super) fn fmt(this: &SchemaError, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{this:?}")
    }

    pub(super) fn source(this: &SchemaError) -> Option<&(dyn Error + 'static)> {
        match this {
            SchemaError::ForyRegistration { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl DataErrorInner {
    pub(super) fn fmt(this: &DataError, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{this:?}")
    }
}

impl KeyErrorInner {
    pub(super) fn fmt(this: &KeyError, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{this:?}")
    }
}
