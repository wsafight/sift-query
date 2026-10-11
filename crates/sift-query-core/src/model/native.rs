//! Optional structured query values; no database driver dependency.
use std::ops::Bound;
use std::str::FromStr;

/// Canonical discrete int8range. Nonempty bounds use PostgreSQL's [lower,upper).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Int8Range {
    Empty,
    Bounds {
        lower: Option<i64>,
        upper: Option<i64>,
    },
}

impl Int8Range {
    pub fn new(start: Bound<i64>, end: Bound<i64>) -> Result<Self, String> {
        let lower = match start {
            Bound::Unbounded => None,
            Bound::Included(value) => Some(value),
            Bound::Excluded(value) => Some(value.checked_add(1).ok_or("range lower overflow")?),
        };
        let upper = match end {
            Bound::Unbounded => None,
            Bound::Excluded(value) => Some(value),
            Bound::Included(value) => Some(value.checked_add(1).ok_or("range upper overflow")?),
        };
        if let (Some(lower), Some(upper)) = (lower, upper) {
            if lower > upper {
                return Err("range lower exceeds upper".to_owned());
            }
            if lower == upper {
                return Ok(Self::Empty);
            }
        }
        Ok(Self::Bounds { lower, upper })
    }

    pub fn validate(self) -> Result<(), String> {
        if let Self::Bounds {
            lower: Some(lower),
            upper: Some(upper),
        } = self
            && lower >= upper
        {
            return Err("nonempty range must use canonical increasing bounds".to_owned());
        }
        Ok(())
    }
}

impl FromStr for Int8Range {
    type Err = String;
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        if input == "empty" {
            return Ok(Self::Empty);
        }
        let bytes = input.as_bytes();
        if bytes.len() < 3
            || !matches!(bytes[0], b'[' | b'(')
            || !matches!(bytes[bytes.len() - 1], b']' | b')')
        {
            return Err("expected empty or int8range bounds".to_owned());
        }
        let (lower, upper) = input
            .get(1..input.len() - 1)
            .and_then(|inner| inner.split_once(','))
            .ok_or("expected two range bounds")?;
        let parse = |value: &str, included: bool| -> Result<Bound<i64>, String> {
            if value.is_empty() {
                return Ok(Bound::Unbounded);
            }
            let value = value.parse().map_err(|_| "expected an int64 range bound")?;
            Ok(if included {
                Bound::Included(value)
            } else {
                Bound::Excluded(value)
            })
        };
        Self::new(
            parse(lower, bytes[0] == b'[')?,
            parse(upper, bytes[bytes.len() - 1] == b']')?,
        )
    }
}

impl std::fmt::Display for Int8Range {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => f.write_str("empty"),
            Self::Bounds { lower, upper } => write!(
                f,
                "{}{},{})",
                if lower.is_some() { "[" } else { "(" },
                lower.map_or_else(String::new, |value| value.to_string()),
                upper.map_or_else(String::new, |value| value.to_string())
            ),
        }
    }
}
