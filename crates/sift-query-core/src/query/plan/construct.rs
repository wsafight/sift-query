use super::*;

impl<E: SearchEntity> QueryPlan<E> {
    pub fn new(
        query: &UserQuery<E>,
        scope: &ServerScope<E>,
        target: QueryTarget,
    ) -> Result<Self, PlanError> {
        Self::new_with_scopes(query, scope, &GroupScope::unrestricted(), target)
    }

    pub fn new_with_scopes(
        query: &UserQuery<E>,
        scope: &ServerScope<E>,
        group_scope: &GroupScope<E>,
        target: QueryTarget,
    ) -> Result<Self, PlanError> {
        if query.inner.schema_fingerprint != E::META.schema_fingerprint
            || scope.schema_fingerprint != E::META.schema_fingerprint
            || group_scope.schema_fingerprint != E::META.schema_fingerprint
        {
            return Err(PlanError::SchemaMismatch);
        }
        if !E::META.is_grouped() && group_scope.predicate.root().is_some() {
            return Err(ParamError::InvalidValue {
                field: "group scope".to_owned(),
                reason: "HAVING scope requires a grouped entity".to_owned(),
            }
            .into());
        }
        if matches!(
            target,
            QueryTarget::TypedPage | QueryTarget::TypedList | QueryTarget::First
        ) && query.inner.projection.is_dynamic()
        {
            return Err(ParamError::ProjectionNotAllowed.into());
        }
        if target == QueryTarget::First
            && (query.inner.paging.explicit_page
                || query.inner.paging.explicit_size
                || query.inner.paging.explicit_offset
                || query.inner.paging.explicit_cursor)
        {
            return Err(ParamError::PagingNotAllowed.into());
        }

        let projection = match target {
            QueryTarget::MapPage => match &query.inner.projection {
                Projection::All => E::META
                    .fields
                    .iter()
                    .filter(|field| {
                        field.selectable
                            && field.result_alias.is_some()
                            && scope.field_access.allows(field.id)
                    })
                    .map(|field| field.id)
                    .collect(),
                Projection::Fields(fields) => fields.clone(),
            },
            _ => E::META
                .fields
                .iter()
                .filter(|field| field.result_alias.is_some())
                .map(|field| field.id)
                .collect(),
        };

        if target == QueryTarget::MapPage && projection.is_empty() {
            return Err(ParamError::EmptyProjection.into());
        }
        if matches!(target, QueryTarget::MapPage) {
            if let Projection::Fields(fields) = &query.inner.projection {
                Self::validate_fields_with_access(
                    E::META,
                    &scope.field_access,
                    fields.iter().copied(),
                )?;
            }
        } else if matches!(
            target,
            QueryTarget::TypedPage | QueryTarget::TypedList | QueryTarget::First
        ) {
            Self::validate_fields_with_access(
                E::META,
                &scope.field_access,
                projection.iter().copied(),
            )?;
        }

        if let PagingMode::Cursor(cursor) = &query.inner.paging.mode {
            if E::META.is_grouped() {
                return Err(ParamError::InvalidValue {
                    field: "cursor".to_owned(),
                    reason: "cursor pagination does not support grouped entities in M2".to_owned(),
                }
                .into());
            }
            if cursor
                .as_ref()
                .is_some_and(|cursor| cursor.values().len() != query.inner.order.len())
            {
                return Err(ParamError::InvalidValue {
                    field: "cursor".to_owned(),
                    reason: "cursor value count does not match ordering".to_owned(),
                }
                .into());
            }
            if let Some(cursor) = cursor {
                for (order, value) in query.inner.order.iter().zip(cursor.values()) {
                    if E::cursor_key_non_null(order.field)
                        && matches!(value, crate::CursorValue::Null)
                    {
                        return Err(ParamError::InvalidValue {
                            field: "cursor".to_owned(),
                            reason: "non-null cursor key cannot contain NULL".to_owned(),
                        }
                        .into());
                    }
                }
            }
            for order in &query.inner.order {
                if E::META
                    .field(order.field)
                    .is_none_or(|field| !field.value_type.supports_cursor())
                {
                    return Err(ParamError::InvalidValue {
                        field: "cursor".to_owned(),
                        reason: "ordering type is not eligible for cursor pagination".to_owned(),
                    }
                    .into());
                }
                if !projection.contains(&order.field) {
                    let name = E::META
                        .field(order.field)
                        .map_or("<unknown>", |field| field.public_name);
                    return Err(ParamError::NotSelectable(name.to_owned()).into());
                }
            }
        }

        if E::META.distinct
            && matches!(
                target,
                QueryTarget::TypedPage
                    | QueryTarget::MapPage
                    | QueryTarget::TypedList
                    | QueryTarget::First
            )
        {
            for order in &query.inner.order {
                if !projection.contains(&order.field) {
                    let name = E::META
                        .field(order.field)
                        .map_or("<unknown>", |field| field.public_name);
                    return Err(ParamError::DistinctOrderNotProjected(name.to_owned()).into());
                }
            }
        }

        let (row_predicate, group_predicate) = query.inner.predicate.partition()?;
        Self::validate_predicate_with_access(E::META, &scope.field_access, &row_predicate)?;
        Self::validate_predicate_with_access(E::META, &scope.field_access, &group_predicate)?;
        for order in &query.inner.order {
            Self::validate_fields_with_access(E::META, &scope.field_access, [order.field])?;
        }
        let paging = if target == QueryTarget::First {
            Paging {
                size: 1,
                ..Paging::default()
            }
        } else {
            query.inner.paging.clone()
        };
        let plan = Self {
            meta: E::META,
            server_predicate: scope.predicate.clone(),
            server_group_predicate: group_scope.predicate.clone(),
            row_predicate,
            group_predicate,
            projection,
            projection_aliases: None,
            order: query.inner.order.clone(),
            paging,
            target,
            distinct: E::META.distinct,
            force_empty: query.inner.force_empty,
            field_access: scope.field_access.clone(),
            policy_fingerprint: query.inner.policy_fingerprint,
            _entity: PhantomData,
        };
        for order in &plan.order {
            plan.condition_order_value(order.field)?;
        }
        Ok(plan)
    }

    pub fn condition_order_value(
        &self,
        field: FieldId,
    ) -> Result<Option<&crate::ScalarValue>, ParamError> {
        let Some(config) = E::condition_order(field) else {
            return Ok(None);
        };
        let invalid = || {
            ParamError::InvalidValue { field:"orderBy".to_owned(),reason:"computed ordering requires exactly one matching scalar query condition and an offset/page query".to_owned() }
        };
        if self.meta.is_grouped()
            || self.distinct
            || matches!(self.paging.mode, PagingMode::Cursor(_))
        {
            return Err(invalid());
        }
        let mut conditions = self
            .row_predicate
            .nodes()
            .iter()
            .filter_map(|node| match node {
                crate::PredicateNode::Condition(condition)
                    if condition.field == config.source
                        && condition.operator.short_name() == config.operator =>
                {
                    Some(condition)
                }
                _ => None,
            });
        let condition = conditions.next().ok_or_else(invalid)?;
        if conditions.next().is_some() || condition.values.len() != 1 || condition.ignore_case {
            return Err(invalid());
        }
        Ok(condition.values.first())
    }

    pub fn for_projection<P: SearchProjection<E>>(
        query: &UserQuery<E>,
        scope: &ServerScope<E>,
    ) -> Result<Self, PlanError> {
        if query.inner.projection.is_dynamic() {
            return Err(ParamError::ProjectionNotAllowed.into());
        }
        if P::FIELDS.is_empty() {
            return Err(ParamError::EmptyProjection.into());
        }
        let mut seen = std::collections::BTreeSet::new();
        let mut projection = Vec::with_capacity(P::FIELDS.len());
        let mut aliases = Vec::with_capacity(P::FIELDS.len());
        for item in P::FIELDS {
            let field = E::META
                .field(item.source)
                .ok_or_else(|| ParamError::UnknownField(format!("#{}", item.source.0)))?;
            if field.result_alias.is_none() {
                return Err(ParamError::NotSelectable(field.public_name.to_owned()).into());
            }
            if !seen.insert(item.source) {
                return Err(ParamError::DuplicateParameter(field.public_name.to_owned()).into());
            }
            projection.push(item.source);
            aliases.push(item.result_alias);
        }
        let mut plan = Self::new(query, scope, QueryTarget::Count)?;
        plan.target = QueryTarget::StaticPage;
        plan.projection = projection;
        plan.projection_aliases = Some(aliases);
        plan.validate_field_access(plan.projection.iter().copied())?;
        if matches!(plan.paging.mode, PagingMode::Cursor(_)) {
            for order in &plan.order {
                if !plan.projection.contains(&order.field) {
                    let name = E::META
                        .field(order.field)
                        .map_or("<unknown>", |field| field.public_name);
                    return Err(ParamError::NotSelectable(name.to_owned()).into());
                }
            }
        }
        if plan.distinct {
            for order in &plan.order {
                if !plan.projection.contains(&order.field) {
                    let name = E::META
                        .field(order.field)
                        .map_or("<unknown>", |field| field.public_name);
                    return Err(ParamError::DistinctOrderNotProjected(name.to_owned()).into());
                }
            }
        }
        Ok(plan)
    }

    #[must_use]
    pub const fn shape_fingerprint_seed(&self) -> u64 {
        self.meta.schema_fingerprint ^ self.policy_fingerprint.rotate_left(17)
    }

    #[must_use]
    pub fn field_access(&self) -> &FieldAccessPolicy<E> {
        &self.field_access
    }

    pub fn validate_field_access(
        &self,
        fields: impl IntoIterator<Item = FieldId>,
    ) -> Result<(), ParamError> {
        Self::validate_fields_with_access(self.meta, &self.field_access, fields)
    }

    pub fn validate_predicate_access(&self, predicate: &PredicateArena) -> Result<(), ParamError> {
        Self::validate_predicate_with_access(self.meta, &self.field_access, predicate)
    }

    fn validate_fields_with_access(
        meta: &'static crate::EntityMeta,
        access: &FieldAccessPolicy<E>,
        fields: impl IntoIterator<Item = FieldId>,
    ) -> Result<(), ParamError> {
        for field_id in fields {
            let field = meta
                .field(field_id)
                .ok_or_else(|| ParamError::UnknownField(format!("#{}", field_id.0)))?;
            if !access.allows(field_id) {
                return Err(ParamError::FieldAccessDenied(field.public_name.to_owned()));
            }
        }
        Ok(())
    }

    fn validate_predicate_with_access(
        meta: &'static crate::EntityMeta,
        access: &FieldAccessPolicy<E>,
        predicate: &PredicateArena,
    ) -> Result<(), ParamError> {
        let fields = predicate.nodes().iter().filter_map(|node| match node {
            crate::PredicateNode::Condition(condition) => Some(condition.field),
            crate::PredicateNode::And(_) | crate::PredicateNode::Or(_) => None,
        });
        Self::validate_fields_with_access(meta, access, fields)
    }
}
