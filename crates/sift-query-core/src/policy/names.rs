use crate::ConfigError;
pub(crate) use sift_query_algorithm::is_ascii_name as is_name;

/// Names of protocol-level control parameters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParamNames {
    pub page: String,
    pub size: String,
    pub offset: String,
    pub cursor: String,
    pub sort: String,
    pub order: String,
    pub order_by: String,
    pub group_expr: String,
    pub only_select: String,
    pub select_exclude: String,
}

impl Default for ParamNames {
    fn default() -> Self {
        Self {
            page: "page".to_owned(),
            size: "size".to_owned(),
            offset: "offset".to_owned(),
            cursor: "cursor".to_owned(),
            sort: "sort".to_owned(),
            order: "order".to_owned(),
            order_by: "orderBy".to_owned(),
            group_expr: "gexpr".to_owned(),
            only_select: "onlySelect".to_owned(),
            select_exclude: "selectExclude".to_owned(),
        }
    }
}

impl ParamNames {
    pub fn validate(&self) -> Result<(), ConfigError> {
        let names = self.all();
        for name in names {
            if !is_name(name) {
                return Err(ConfigError::Invalid(format!(
                    "invalid control parameter name: {name}"
                )));
            }
        }
        for (index, name) in names.iter().enumerate() {
            if names[index + 1..].contains(name) {
                return Err(ConfigError::Invalid(format!(
                    "duplicate control parameter name: {name}"
                )));
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn all(&self) -> [&str; 10] {
        [
            &self.page,
            &self.size,
            &self.offset,
            &self.cursor,
            &self.sort,
            &self.order,
            &self.order_by,
            &self.group_expr,
            &self.only_select,
            &self.select_exclude,
        ]
    }
}
