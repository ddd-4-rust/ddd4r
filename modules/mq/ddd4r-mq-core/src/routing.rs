//! Tag expression parsing and broker selector translation.

use std::collections::BTreeSet;

/// Frozen ddd4j tag include/exclude expression.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TagExpression {
    wildcard: bool,
    includes: BTreeSet<String>,
    excludes: BTreeSet<String>,
}

impl TagExpression {
    /// Parses `*`, `paid || shipped` and `* -cancelled` forms.
    pub fn parse(raw: &str) -> Self {
        let mut expression = Self::default();
        for token in raw.replace("||", " ").split_whitespace() {
            match token {
                "*" => expression.wildcard = true,
                value if value.starts_with('-') && value.len() > 1 => {
                    expression.excludes.insert(value[1..].to_owned());
                }
                value if !value.is_empty() => {
                    expression.includes.insert(value.to_owned());
                }
                _ => {}
            }
        }
        expression
    }

    /// Matches application-side tag filtering.
    pub fn matches(&self, tag: Option<&str>) -> bool {
        if tag.is_some_and(|tag| self.excludes.contains(tag)) {
            return false;
        }
        self.wildcard
            || self.includes.is_empty()
            || tag.is_none()
            || tag.is_some_and(|tag| self.includes.contains(tag))
    }

    /// Translates to the same SQL-92 selector subset used by ddd4j.
    pub fn to_sql92_selector(&self, property: &str) -> Option<String> {
        if self.includes.is_empty() && self.excludes.is_empty() {
            return None;
        }
        let escape = |value: &str| value.replace('\'', "''");
        let include = if self.wildcard || self.includes.is_empty() {
            None
        } else {
            Some(format!(
                "({} OR {property} IS NULL)",
                self.includes
                    .iter()
                    .map(|value| format!("{property} = '{}'", escape(value)))
                    .collect::<Vec<_>>()
                    .join(" OR ")
            ))
        };
        let exclude = if self.excludes.is_empty() {
            None
        } else {
            Some(
                self.excludes
                    .iter()
                    .map(|value| {
                        format!("({property} <> '{}' OR {property} IS NULL)", escape(value))
                    })
                    .collect::<Vec<_>>()
                    .join(" AND "),
            )
        };
        match (include, exclude) {
            (Some(include), Some(exclude)) => Some(format!("({include} AND {exclude})")),
            (Some(include), None) => Some(include),
            (None, Some(exclude)) => Some(format!("(1=1 AND {exclude})")),
            (None, None) => None,
        }
    }
}
