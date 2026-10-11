use crate::{ConfigError, FieldId, ParamError, ScalarValue};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegisteredOperator {
    pub operator: CustomOperator,
    pub field_type: crate::ValueType,
    pub input_type: crate::ValueType,
}

/// Trusted server-side SQL template for a custom operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CustomOperator {
    name: &'static str,
    template: &'static str,
    arity: u8,
    types: Option<(crate::ValueType, crate::ValueType)>,
    exists_kind: Option<ExistsKind>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ExistsKind {
    Exists,
    NotExists,
}

impl CustomOperator {
    pub fn new(name: &'static str, template: &'static str, arity: u8) -> Result<Self, ConfigError> {
        if name.is_empty()
            || name.len() > 64
            || template.len() > 4096
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            return Err(ConfigError::Invalid(
                "custom operator name must be ASCII alphanumeric or underscore".to_owned(),
            ));
        }
        if !template.contains("{field}")
            || template.contains(';')
            || template.contains("--")
            || template.contains("/*")
            || template.contains('$')
            || template.contains('\0')
        {
            return Err(ConfigError::Invalid(
                "custom operator template contains a forbidden token".to_owned(),
            ));
        }
        validate_template_placeholders(template, arity)?;
        Ok(Self {
            name,
            template,
            arity,
            types: None,
            exists_kind: None,
        })
    }

    /// Declares a trusted correlated `EXISTS (SELECT ...)` relation filter.
    /// The complete SQL fragment is server-owned; all `{0}` values remain
    /// bound parameters and `{field}` is replaced by the selected outer field.
    pub fn exists(
        name: &'static str,
        template: &'static str,
        arity: u8,
    ) -> Result<Self, ConfigError> {
        Self::exists_kind(name, template, arity, ExistsKind::Exists)
    }

    /// Declares a trusted correlated `NOT EXISTS (SELECT ...)` relation filter.
    pub fn not_exists(
        name: &'static str,
        template: &'static str,
        arity: u8,
    ) -> Result<Self, ConfigError> {
        Self::exists_kind(name, template, arity, ExistsKind::NotExists)
    }

    fn exists_kind(
        name: &'static str,
        template: &'static str,
        arity: u8,
        kind: ExistsKind,
    ) -> Result<Self, ConfigError> {
        let upper = template.trim_start().to_ascii_uppercase();
        let prefix = match kind {
            ExistsKind::Exists => "EXISTS (SELECT",
            ExistsKind::NotExists => "NOT EXISTS (SELECT",
        };
        if !upper.starts_with(prefix) {
            return Err(ConfigError::Invalid(
                "exists template must start with EXISTS (SELECT ...) or NOT EXISTS (SELECT ...)"
                    .to_owned(),
            ));
        }
        let mut operator = Self::new(name, template, arity)?;
        operator.exists_kind = Some(kind);
        Ok(operator)
    }

    /// A trusted operator with distinct field/result and query input types.
    pub fn typed(
        name: &'static str,
        template: &'static str,
        arity: u8,
        field_type: crate::ValueType,
        input_type: crate::ValueType,
    ) -> Result<Self, ConfigError> {
        let mut operator = Self::new(name, template, arity)?;
        operator.types = Some((field_type, input_type));
        Ok(operator)
    }

    pub const fn types(self) -> Option<(crate::ValueType, crate::ValueType)> {
        self.types
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        self.name
    }

    #[must_use]
    pub const fn template(self) -> &'static str {
        self.template
    }

    #[must_use]
    pub const fn arity(self) -> u8 {
        self.arity
    }

    #[must_use]
    pub const fn is_exists(self) -> bool {
        matches!(self.exists_kind, Some(ExistsKind::Exists))
    }

    #[must_use]
    pub const fn is_not_exists(self) -> bool {
        matches!(self.exists_kind, Some(ExistsKind::NotExists))
    }
}

fn validate_template_placeholders(template: &str, arity: u8) -> Result<(), ConfigError> {
    let mut referenced = vec![false; usize::from(arity)];
    let mut remaining = template;
    while let Some(start) = remaining.find('{') {
        let after = &remaining[start + 1..];
        let Some(end) = after.find('}') else {
            return Err(ConfigError::Invalid(
                "custom operator template has an unclosed placeholder".to_owned(),
            ));
        };
        let token = &after[..end];
        if token != "field" {
            let index = token.parse::<usize>().map_err(|_| {
                ConfigError::Invalid(format!("unknown custom operator placeholder {{{token}}}"))
            })?;
            let Some(slot) = referenced.get_mut(index) else {
                return Err(ConfigError::Invalid(format!(
                    "custom operator placeholder {{{index}}} exceeds arity {arity}"
                )));
            };
            *slot = true;
        }
        remaining = &after[end + 1..];
    }
    if referenced.iter().any(|seen| !seen) {
        return Err(ConfigError::Invalid(
            "custom operator template must reference every value".to_owned(),
        ));
    }
    Ok(())
}

/// M1 field operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Operator {
    Eq,
    Ne,
    Gt,
    Ge,
    Lt,
    Le,
    Between,
    NotBetween,
    InList,
    NotIn,
    IsNull,
    NotNull,
    Empty,
    NotEmpty,
    StartWith,
    EndWith,
    Contain,
    OrLike,
    NotLike,
    AlwaysTrue,
    AlwaysFalse,
    Custom(CustomOperator),
}

impl Operator {
    pub fn input_type(self, field_type: crate::ValueType) -> crate::ValueType {
        match self {
            Self::Custom(operator) => operator.types.map_or(field_type, |types| types.1),
            _ => field_type,
        }
    }

    pub fn accepts_field_type(self, field_type: crate::ValueType) -> bool {
        match self {
            Self::Custom(operator) => operator.types.is_none_or(|types| types.0 == field_type),
            _ => true,
        }
    }
    #[must_use]
    pub const fn short_name(self) -> &'static str {
        match self {
            Self::Eq => "eq",
            Self::Ne => "ne",
            Self::Gt => "gt",
            Self::Ge => "ge",
            Self::Lt => "lt",
            Self::Le => "le",
            Self::Between => "bt",
            Self::NotBetween => "nb",
            Self::InList => "il",
            Self::NotIn => "ni",
            Self::IsNull => "nl",
            Self::NotNull => "nn",
            Self::Empty => "ey",
            Self::NotEmpty => "ny",
            Self::StartWith => "sw",
            Self::EndWith => "ew",
            Self::Contain => "ct",
            Self::OrLike => "ol",
            Self::NotLike => "nk",
            Self::AlwaysTrue => "at",
            Self::AlwaysFalse => "af",
            Self::Custom(operator) => operator.name(),
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "eq" => Some(Self::Eq),
            "ne" => Some(Self::Ne),
            "gt" => Some(Self::Gt),
            "ge" => Some(Self::Ge),
            "lt" => Some(Self::Lt),
            "le" => Some(Self::Le),
            "bt" => Some(Self::Between),
            "nb" => Some(Self::NotBetween),
            "il" => Some(Self::InList),
            "ni" => Some(Self::NotIn),
            "nl" => Some(Self::IsNull),
            "nn" => Some(Self::NotNull),
            "ey" => Some(Self::Empty),
            "ny" => Some(Self::NotEmpty),
            "sw" => Some(Self::StartWith),
            "ew" => Some(Self::EndWith),
            "ct" => Some(Self::Contain),
            "ol" => Some(Self::OrLike),
            "nk" => Some(Self::NotLike),
            "at" => Some(Self::AlwaysTrue),
            "af" => Some(Self::AlwaysFalse),
            _ => None,
        }
    }

    #[must_use]
    pub const fn expected_arity(self) -> &'static str {
        match self {
            Self::Between | Self::NotBetween => "2",
            Self::InList | Self::NotIn | Self::OrLike => "1..N",
            Self::IsNull
            | Self::NotNull
            | Self::Empty
            | Self::NotEmpty
            | Self::AlwaysTrue
            | Self::AlwaysFalse => "0",
            Self::Custom(_) => "custom",
            _ => "1",
        }
    }

    #[must_use]
    pub fn accepts_arity(self, actual: usize) -> bool {
        match self {
            Self::Between | Self::NotBetween => actual == 2,
            Self::InList | Self::NotIn | Self::OrLike => actual > 0,
            Self::IsNull
            | Self::NotNull
            | Self::Empty
            | Self::NotEmpty
            | Self::AlwaysTrue
            | Self::AlwaysFalse => actual == 0,
            Self::Custom(operator) => actual == usize::from(operator.arity()),
            _ => actual == 1,
        }
    }

    #[must_use]
    pub const fn supports_ignore_case(self) -> bool {
        matches!(
            self,
            Self::Eq
                | Self::Ne
                | Self::StartWith
                | Self::EndWith
                | Self::Contain
                | Self::OrLike
                | Self::NotLike
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PredicatePhase {
    Row,
    Group,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Condition {
    pub field: FieldId,
    pub operator: Operator,
    pub values: Vec<ScalarValue>,
    pub ignore_case: bool,
    pub phase: PredicatePhase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct NodeId(pub u16);

#[derive(Debug, Clone, PartialEq)]
pub enum PredicateNode {
    Condition(Condition),
    And(Vec<NodeId>),
    Or(Vec<NodeId>),
}

/// Compact predicate storage with index-based edges.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PredicateArena {
    nodes: Vec<PredicateNode>,
    root: Option<NodeId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PhaseClass {
    None,
    Row,
    Group,
    Mixed,
}

impl PredicateArena {
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn from_condition(condition: Condition) -> Self {
        let mut arena = Self::default();
        let root = arena.push(PredicateNode::Condition(condition));
        arena.root = Some(root);
        arena
    }

    #[must_use]
    pub fn nodes(&self) -> &[PredicateNode] {
        &self.nodes
    }

    #[must_use]
    pub const fn root(&self) -> Option<NodeId> {
        self.root
    }

    #[must_use]
    pub fn node(&self, id: NodeId) -> Option<&PredicateNode> {
        self.nodes.get(usize::from(id.0))
    }

    fn push(&mut self, node: PredicateNode) -> NodeId {
        let id = u16::try_from(self.nodes.len()).expect("predicate budget exceeds u16");
        self.nodes.push(node);
        NodeId(id)
    }

    pub(crate) fn combine_and(arenas: impl IntoIterator<Item = Self>) -> Self {
        Self::combine(arenas, true)
    }

    /// Combines independent predicates with AND.
    pub fn all(arenas: impl IntoIterator<Item = Self>) -> Self {
        Self::combine_and(arenas)
    }

    fn combine(arenas: impl IntoIterator<Item = Self>, and: bool) -> Self {
        let mut output = Self::default();
        let mut roots = Vec::new();
        for arena in arenas {
            let offset = u16::try_from(output.nodes.len()).expect("predicate budget exceeds u16");
            for node in arena.nodes {
                output.nodes.push(remap_node(node, offset));
            }
            if let Some(root) = arena.root {
                roots.push(NodeId(root.0 + offset));
            }
        }
        output.root = match roots.len() {
            0 => None,
            1 => roots.first().copied(),
            _ => Some(output.push(if and {
                PredicateNode::And(roots)
            } else {
                PredicateNode::Or(roots)
            })),
        };
        output
    }

    pub(crate) fn combine_or(arenas: impl IntoIterator<Item = Self>) -> Self {
        Self::combine(arenas, false)
    }

    /// Combines independent predicates with OR.
    pub fn any(arenas: impl IntoIterator<Item = Self>) -> Self {
        Self::combine_or(arenas)
    }

    /// Splits an AND-separable tree into WHERE and HAVING trees.
    pub fn partition(&self) -> Result<(Self, Self), ParamError> {
        let Some(root) = self.root else {
            return Ok((Self::default(), Self::default()));
        };
        self.validate_or_phases(root)?;
        let mut row = Self::default();
        let mut group = Self::default();
        row.root = self.copy_phase(root, PredicatePhase::Row, &mut row);
        group.root = self.copy_phase(root, PredicatePhase::Group, &mut group);
        Ok((row, group))
    }

    /// Returns a predicate with conditions on `field` removed.
    ///
    /// This is used by facet self-exclusion. The remaining boolean tree keeps
    /// its original shape; a branch that contains only the excluded field is
    /// treated as an empty branch so the facet can evaluate the rest of the
    /// query without accidentally broadening the server-owned predicates.
    #[must_use]
    pub fn without_field(&self, field: FieldId) -> Self {
        let mut output = Self::default();
        let Some(root) = self.root else {
            return output;
        };
        output.root = self.copy_without_field(root, field, &mut output);
        output
    }

    fn copy_without_field(&self, id: NodeId, field: FieldId, output: &mut Self) -> Option<NodeId> {
        match self.node(id).expect("valid predicate node") {
            PredicateNode::Condition(condition) => (condition.field != field)
                .then(|| output.push(PredicateNode::Condition(condition.clone()))),
            PredicateNode::And(children) => {
                let copied = children
                    .iter()
                    .filter_map(|child| self.copy_without_field(*child, field, output))
                    .collect::<Vec<_>>();
                collapse(output, copied, true)
            }
            PredicateNode::Or(children) => {
                let copied = children
                    .iter()
                    .filter_map(|child| self.copy_without_field(*child, field, output))
                    .collect::<Vec<_>>();
                collapse(output, copied, false)
            }
        }
    }

    fn validate_or_phases(&self, id: NodeId) -> Result<PhaseClass, ParamError> {
        let node = self.node(id).expect("valid predicate node");
        match node {
            PredicateNode::Condition(condition) => Ok(match condition.phase {
                PredicatePhase::Row => PhaseClass::Row,
                PredicatePhase::Group => PhaseClass::Group,
            }),
            PredicateNode::And(children) => {
                let mut class = PhaseClass::None;
                for child in children {
                    class = merge_phase(class, self.validate_or_phases(*child)?);
                }
                Ok(class)
            }
            PredicateNode::Or(children) => {
                let mut class = PhaseClass::None;
                for child in children {
                    let child_class = self.validate_or_phases(*child)?;
                    if child_class == PhaseClass::Mixed {
                        return Err(ParamError::MixedPredicatePhase);
                    }
                    class = merge_phase(class, child_class);
                    if class == PhaseClass::Mixed {
                        return Err(ParamError::MixedPredicatePhase);
                    }
                }
                Ok(class)
            }
        }
    }

    fn copy_phase(&self, id: NodeId, phase: PredicatePhase, output: &mut Self) -> Option<NodeId> {
        match self.node(id).expect("valid predicate node") {
            PredicateNode::Condition(condition) => (condition.phase == phase)
                .then(|| output.push(PredicateNode::Condition(condition.clone()))),
            PredicateNode::And(children) => {
                let copied = children
                    .iter()
                    .filter_map(|child| self.copy_phase(*child, phase, output))
                    .collect::<Vec<_>>();
                collapse(output, copied, true)
            }
            PredicateNode::Or(children) => {
                let copied = children
                    .iter()
                    .filter_map(|child| self.copy_phase(*child, phase, output))
                    .collect::<Vec<_>>();
                collapse(output, copied, false)
            }
        }
    }
}

fn collapse(arena: &mut PredicateArena, children: Vec<NodeId>, and: bool) -> Option<NodeId> {
    match children.len() {
        0 => None,
        1 => children.first().copied(),
        _ => Some(arena.push(if and {
            PredicateNode::And(children)
        } else {
            PredicateNode::Or(children)
        })),
    }
}

fn remap_node(node: PredicateNode, offset: u16) -> PredicateNode {
    match node {
        PredicateNode::Condition(condition) => PredicateNode::Condition(condition),
        PredicateNode::And(children) => PredicateNode::And(
            children
                .into_iter()
                .map(|id| NodeId(id.0 + offset))
                .collect(),
        ),
        PredicateNode::Or(children) => PredicateNode::Or(
            children
                .into_iter()
                .map(|id| NodeId(id.0 + offset))
                .collect(),
        ),
    }
}

const fn merge_phase(left: PhaseClass, right: PhaseClass) -> PhaseClass {
    match (left, right) {
        (PhaseClass::None, value) | (value, PhaseClass::None) => value,
        (PhaseClass::Row, PhaseClass::Row) => PhaseClass::Row,
        (PhaseClass::Group, PhaseClass::Group) => PhaseClass::Group,
        _ => PhaseClass::Mixed,
    }
}
