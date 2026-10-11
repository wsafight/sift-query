use std::collections::{BTreeMap, BTreeSet};

use crate::policy::names::is_name;
use crate::{ParamError, PredicateArena};

pub(super) fn parse_group_expression(
    expression: &str,
    groups: &BTreeMap<String, PredicateArena>,
    max_depth: usize,
    max_nodes: usize,
) -> Result<PredicateArena, ParamError> {
    let parsed = GroupExprParser::new(expression, max_depth, max_nodes).parse()?;
    let mut referenced = BTreeSet::new();
    parsed.names(&mut referenced);
    for name in &referenced {
        if !groups.contains_key(name) {
            return Err(ParamError::UndefinedGroup(name.clone()));
        }
    }
    for name in groups.keys() {
        if !referenced.contains(name) {
            return Err(ParamError::UnusedGroup(name.clone()));
        }
    }
    parsed.simplify().into_arena(groups)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum GroupExpr {
    Name(String),
    And(Box<Self>, Box<Self>),
    Or(Box<Self>, Box<Self>),
}

impl GroupExpr {
    fn names(&self, output: &mut BTreeSet<String>) {
        match self {
            Self::Name(name) => {
                output.insert(name.clone());
            }
            Self::And(left, right) | Self::Or(left, right) => {
                left.names(output);
                right.names(output);
            }
        }
    }

    fn into_arena(
        self,
        groups: &BTreeMap<String, PredicateArena>,
    ) -> Result<PredicateArena, ParamError> {
        match self {
            Self::Name(name) => groups
                .get(&name)
                .cloned()
                .ok_or(ParamError::UndefinedGroup(name)),
            Self::And(left, right) => Ok(PredicateArena::all([
                left.into_arena(groups)?,
                right.into_arena(groups)?,
            ])),
            Self::Or(left, right) => Ok(PredicateArena::any([
                left.into_arena(groups)?,
                right.into_arena(groups)?,
            ])),
        }
    }

    fn simplify(self) -> Self {
        match self {
            Self::Name(_) => self,
            Self::And(left, right) => {
                let left = left.simplify();
                let right = right.simplify();
                if left == right {
                    left
                } else if contains_or_term(&left, &right) {
                    right
                } else if contains_or_term(&right, &left) {
                    left
                } else {
                    Self::And(Box::new(left), Box::new(right))
                }
            }
            Self::Or(left, right) => {
                let left = left.simplify();
                let right = right.simplify();
                if left == right {
                    left
                } else if contains_and_term(&left, &right) {
                    right
                } else if contains_and_term(&right, &left) {
                    left
                } else {
                    Self::Or(Box::new(left), Box::new(right))
                }
            }
        }
    }
}

fn contains_and_term(expression: &GroupExpr, needle: &GroupExpr) -> bool {
    match expression {
        GroupExpr::And(left, right) => {
            left.as_ref() == needle
                || right.as_ref() == needle
                || contains_and_term(left, needle)
                || contains_and_term(right, needle)
        }
        _ => false,
    }
}

fn contains_or_term(expression: &GroupExpr, needle: &GroupExpr) -> bool {
    match expression {
        GroupExpr::Or(left, right) => {
            left.as_ref() == needle
                || right.as_ref() == needle
                || contains_or_term(left, needle)
                || contains_or_term(right, needle)
        }
        _ => false,
    }
}

struct GroupExprParser<'input> {
    input: &'input [u8],
    position: usize,
    max_depth: usize,
    max_nodes: usize,
    nodes: usize,
}

impl<'input> GroupExprParser<'input> {
    fn new(input: &'input str, max_depth: usize, max_nodes: usize) -> Self {
        Self {
            input: input.as_bytes(),
            position: 0,
            max_depth,
            max_nodes,
            nodes: 0,
        }
    }

    fn parse(mut self) -> Result<GroupExpr, ParamError> {
        if self.input.is_empty() {
            return Err(ParamError::InvalidGroupExpr("empty expression".to_owned()));
        }
        let expression = self.parse_or(1)?;
        self.skip_whitespace();
        if self.position != self.input.len() {
            return Err(ParamError::InvalidGroupExpr(format!(
                "unexpected token at byte {}",
                self.position
            )));
        }
        Ok(expression)
    }

    fn parse_or(&mut self, depth: usize) -> Result<GroupExpr, ParamError> {
        let mut left = self.parse_and(depth)?;
        loop {
            self.skip_whitespace();
            if !self.consume(b'|') {
                break;
            }
            let right = self.parse_and(depth)?;
            self.bump_node()?;
            left = GroupExpr::Or(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_and(&mut self, depth: usize) -> Result<GroupExpr, ParamError> {
        let mut left = self.parse_primary(depth)?;
        loop {
            self.skip_whitespace();
            if !self.consume(b'&') {
                break;
            }
            let right = self.parse_primary(depth)?;
            self.bump_node()?;
            left = GroupExpr::And(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_primary(&mut self, depth: usize) -> Result<GroupExpr, ParamError> {
        if depth > self.max_depth {
            return Err(ParamError::TooComplex("gexpr depth"));
        }
        self.skip_whitespace();
        if self.consume(b'(') {
            let expression = self.parse_or(depth + 1)?;
            self.skip_whitespace();
            if !self.consume(b')') {
                return Err(ParamError::InvalidGroupExpr(
                    "missing closing parenthesis".to_owned(),
                ));
            }
            return Ok(expression);
        }
        let start = self.position;
        while self
            .input
            .get(self.position)
            .is_some_and(u8::is_ascii_alphanumeric)
            || self.input.get(self.position) == Some(&b'_')
        {
            self.position += 1;
        }
        if self.position == start {
            return Err(ParamError::InvalidGroupExpr(format!(
                "expected group name at byte {}",
                self.position
            )));
        }
        let name = std::str::from_utf8(&self.input[start..self.position])
            .map_err(|_| ParamError::InvalidGroupExpr("group name is not UTF-8".to_owned()))?;
        if !is_name(name) || name.len() > 64 {
            return Err(ParamError::InvalidGroupExpr(format!(
                "invalid group name {name}"
            )));
        }
        self.bump_node()?;
        Ok(GroupExpr::Name(name.to_owned()))
    }

    fn bump_node(&mut self) -> Result<(), ParamError> {
        self.nodes += 1;
        if self.nodes > self.max_nodes {
            Err(ParamError::TooComplex("gexpr node count"))
        } else {
            Ok(())
        }
    }

    fn skip_whitespace(&mut self) {
        while self
            .input
            .get(self.position)
            .is_some_and(u8::is_ascii_whitespace)
        {
            self.position += 1;
        }
    }

    fn consume(&mut self, expected: u8) -> bool {
        if self.input.get(self.position) == Some(&expected) {
            self.position += 1;
            true
        } else {
            false
        }
    }
}
