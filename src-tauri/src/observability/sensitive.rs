use std::fmt::{self, Debug, Display, Formatter};
use serde::{Serialize, Serializer, Deserialize, Deserializer};

/// A wrapper for sensitive data that redacts it when formatted for logging.
/// To access the inner value, use the `.expose()` method.
#[derive(Clone, Eq, PartialEq, Hash)]
pub struct Sensitive<T>(T);

impl<T> Sensitive<T> {
    pub fn new(value: T) -> Self {
        Self(value)
    }

    /// Expose the sensitive value. Use this only when absolutely necessary and safe.
    pub fn expose(&self) -> &T {
        &self.0
    }
    
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> Debug for Sensitive<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "[REDACTED]")
    }
}

impl<T> Display for Sensitive<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "[REDACTED]")
    }
}

impl<T: Serialize> Serialize for Sensitive<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str("***")
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Sensitive<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        T::deserialize(deserializer).map(Sensitive)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sensitive_debug_display() {
        let secret = Sensitive::new("password");
        assert_eq!(format!("{:?}", secret), "[REDACTED]");
        assert_eq!(format!("{}", secret), "[REDACTED]");
    }

    #[test]
    fn test_sensitive_serialize() {
        let secret = Sensitive::new("password");
        let json = serde_json::to_string(&secret).unwrap();
        assert_eq!(json, "\"***\"");
    }
}
