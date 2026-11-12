//! Query parameters.

/// Query string parameters, written with the API names (`isArchived`, `vehicleId`...).
///
/// ```
/// use lexio_rent::Params;
/// let p = Params::new().set("isArchived", false).set("limit", 50).set_list("status", ["RESERVED", "IN_PROGRESS"]);
/// assert_eq!(p.pairs()[2], ("status".to_string(), "RESERVED,IN_PROGRESS".to_string()));
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Params(Vec<(String, String)>);

impl Params {
    /// Empty parameter set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add or replace a parameter. Booleans become `true` / `false`.
    pub fn set(mut self, key: &str, value: impl ToString) -> Self {
        self.0.retain(|(k, _)| k != key);
        self.0.push((key.to_string(), value.to_string()));
        self
    }

    /// Add a multi-valued parameter, sent comma-separated.
    pub fn set_list<I, S>(self, key: &str, values: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let joined = values.into_iter().map(|v| v.as_ref().to_string()).collect::<Vec<_>>().join(",");
        self.set(key, joined)
    }

    /// Value of a parameter, if set.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }

    /// The `(name, value)` pairs, in insertion order.
    pub fn pairs(&self) -> &[(String, String)] {
        &self.0
    }
}
