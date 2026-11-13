use crate::EntityId;
use std::{fmt, future::Future};

/// Comparison applied by a [`StoragePredicate`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoragePredicateOperator {
    Eq,
    NotEq,
    Like,
    ILike,
    Gt,
    Gte,
    Lt,
    Lte,
    In,
    NotIn,
    /// Substring match; rendered as `LIKE` (the caller wraps the value in `%`).
    Contains,
}

impl StoragePredicateOperator {
    /// The SQL operator this variant stands for.
    pub fn as_sql(&self) -> &'static str {
        match self {
            Self::Eq => "=",
            Self::NotEq => "!=",
            Self::Like | Self::Contains => "LIKE",
            Self::ILike => "ILIKE",
            Self::Gt => ">",
            Self::Gte => ">=",
            Self::Lt => "<",
            Self::Lte => "<=",
            Self::In => "IN",
            Self::NotIn => "NOT IN",
        }
    }
}

impl fmt::Display for StoragePredicateOperator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_sql())
    }
}

/// One `field <operator> value` filter of a [`StorageListQuery`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoragePredicate {
    pub field: String,
    pub operator: StoragePredicateOperator,
    pub value: String,
}

/// A paged, filtered listing request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageListQuery {
    pub limit: i64,
    pub offset: i64,
    pub predicates: Vec<StoragePredicate>,
}

/// CRUD contract every generated storage (Postgres, gRPC client, ...) implements.
pub trait Storage<T, E = String> {
    fn save_one(&self, data: T) -> impl Future<Output = Result<T, E>> + Send;
    fn save_many(&self, data: Vec<T>) -> impl Future<Output = Result<Vec<T>, E>> + Send;
    fn find_one(&self, id: EntityId) -> impl Future<Output = Result<Option<T>, E>> + Send;
    fn find_many(&self, ids: Vec<EntityId>) -> impl Future<Output = Result<Vec<T>, E>> + Send;
    fn list(&self, query: StorageListQuery) -> impl Future<Output = Result<Vec<T>, E>> + Send;
    fn update_one(&self, id: EntityId, data: T) -> impl Future<Output = Result<T, E>> + Send;
    fn delete(&self, id: EntityId) -> impl Future<Output = Result<T, E>> + Send;
    fn delete_many(&self, ids: Vec<EntityId>) -> impl Future<Output = Result<Vec<T>, E>> + Send;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operators_render_as_sql() {
        assert_eq!(StoragePredicateOperator::Eq.to_string(), "=");
        assert_eq!(StoragePredicateOperator::Contains.to_string(), "LIKE");
        assert_eq!(StoragePredicateOperator::NotIn.to_string(), "NOT IN");
    }
}
