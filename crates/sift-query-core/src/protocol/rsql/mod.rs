//! Strict RSQL subset: ==, !=, =gt=, =ge=, =lt=, =le=, =in=, =out=;
//! AND (;), OR (,), grouping and quoted values. Wildcards are not supported.
use crate::{ParamError, ParamParser, QueryPolicy, RawParams, SearchEntity, UserQuery};

pub fn parse<E: SearchEntity>(
    input: &str,
    policy: &QueryPolicy,
) -> Result<UserQuery<E>, ParamError> {
    if input.len() > policy.limits().max_raw_query_bytes {
        return Err(ParamError::TooComplex("RSQL input length"));
    }
    if input.trim().is_empty() {
        return ParamParser::new(policy)?
            .parse(&RawParams::from_entries(Vec::new(), policy.limits())?);
    }
    let mut parser = Reader {
        input: input.as_bytes(),
        position: 0,
        count: 0,
        entries: Vec::new(),
        policy,
    };
    let expression = parser.expression(0)?;
    parser.space();
    if parser.position != parser.input.len() {
        return Err(invalid("unexpected trailing syntax"));
    }
    parser
        .entries
        .push((policy.names().group_expr.clone(), expression));
    let raw = RawParams::from_entries(parser.entries, policy.limits())?;
    ParamParser::new(policy)?.parse(&raw)
}
fn invalid(reason: &str) -> ParamError {
    ParamError::InvalidValue {
        field: "rsql".to_owned(),
        reason: reason.to_owned(),
    }
}
struct Reader<'a> {
    input: &'a [u8],
    position: usize,
    count: usize,
    entries: Vec<(String, String)>,
    policy: &'a QueryPolicy,
}
impl Reader<'_> {
    fn space(&mut self) {
        while self
            .input
            .get(self.position)
            .is_some_and(u8::is_ascii_whitespace)
        {
            self.position += 1;
        }
    }
    fn take(&mut self, byte: u8) -> bool {
        self.space();
        if self.input.get(self.position) == Some(&byte) {
            self.position += 1;
            true
        } else {
            false
        }
    }
    fn expression(&mut self, depth: usize) -> Result<String, ParamError> {
        if depth > self.policy.limits().max_group_depth {
            return Err(ParamError::TooComplex("RSQL nesting"));
        }
        let mut branches = vec![self.conjunction(depth)?];
        while self.take(b',') {
            branches.push(self.conjunction(depth)?);
        }
        Ok(branches.join("|"))
    }
    fn conjunction(&mut self, depth: usize) -> Result<String, ParamError> {
        let mut nodes = vec![self.node(depth)?];
        while self.take(b';') {
            nodes.push(self.node(depth)?);
        }
        Ok(nodes.join("&"))
    }
    fn node(&mut self, depth: usize) -> Result<String, ParamError> {
        if self.take(b'(') {
            let expression = self.expression(depth + 1)?;
            if !self.take(b')') {
                return Err(invalid("unclosed group"));
            }
            return Ok(format!("({expression})"));
        }
        self.space();
        let start = self.position;
        while self
            .input
            .get(self.position)
            .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
        {
            self.position += 1;
        }
        if start == self.position {
            return Err(invalid("expected public field name"));
        }
        let field = std::str::from_utf8(&self.input[start..self.position])
            .map_err(|_| invalid("invalid field"))?;
        self.space();
        let (token, op, list) = [
            ("==", "eq", false),
            ("!=", "ne", false),
            ("=gt=", "gt", false),
            ("=ge=", "ge", false),
            ("=lt=", "lt", false),
            ("=le=", "le", false),
            ("=in=", "il", true),
            ("=out=", "ni", true),
        ]
        .into_iter()
        .find(|(token, _, _)| self.input[self.position..].starts_with(token.as_bytes()))
        .ok_or_else(|| invalid("unsupported operator"))?;
        self.position += token.len();
        self.count += 1;
        if self.count > self.policy.limits().max_conditions {
            return Err(ParamError::TooComplex("RSQL condition count"));
        }
        let group = format!("r{}", self.count);
        let key = format!("{group}.{field}");
        self.entries.push((format!("{key}-op"), op.to_owned()));
        if list {
            if !self.take(b'(') {
                return Err(invalid("list operator requires parentheses"));
            }
            let mut count = 0;
            loop {
                let value = self.value()?;
                self.entries.push((key.clone(), value));
                count += 1;
                if count > self.policy.limits().max_values_per_condition {
                    return Err(ParamError::TooComplex("RSQL list length"));
                }
                if self.take(b')') {
                    break;
                }
                if !self.take(b',') {
                    return Err(invalid("expected list separator"));
                }
            }
        } else {
            let value = self.value()?;
            self.entries.push((key, value));
        }
        Ok(group)
    }
    fn value(&mut self) -> Result<String, ParamError> {
        self.space();
        let start = self.position;
        let quoted = self
            .input
            .get(self.position)
            .copied()
            .filter(|byte| matches!(byte, b'\'' | b'"'));
        let mut bytes = Vec::new();
        if let Some(quote) = quoted {
            self.position += 1;
            let mut closed = false;
            while let Some(byte) = self.input.get(self.position).copied() {
                self.position += 1;
                if byte == quote {
                    closed = true;
                    break;
                }
                if byte == b'\\' {
                    let next = self
                        .input
                        .get(self.position)
                        .copied()
                        .ok_or_else(|| invalid("unclosed escape"))?;
                    if next != quote && next != b'\\' {
                        return Err(invalid("unsupported escape"));
                    }
                    bytes.push(next);
                    self.position += 1;
                } else {
                    bytes.push(byte);
                }
            }
            if !closed {
                return Err(invalid("unclosed quoted value"));
            }
        } else {
            while self
                .input
                .get(self.position)
                .is_some_and(|byte| !matches!(byte, b';' | b',' | b'(' | b')'))
            {
                self.position += 1;
            }
            bytes.extend_from_slice(&self.input[start..self.position]);
        }
        let value = String::from_utf8(bytes).map_err(|_| invalid("invalid UTF-8"))?;
        let value = if quoted.is_some() {
            value
        } else {
            value.trim().to_owned()
        };
        if quoted.is_none() && value.is_empty() {
            return Err(invalid("expected value"));
        }
        if value.contains('*') {
            return Err(invalid("wildcard matching is outside this RSQL subset"));
        }
        Ok(value)
    }
}
