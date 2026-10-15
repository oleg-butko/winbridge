//! Validation is independent of SQLite, transport and the native UI.
use winbridge_protocol::*;
pub fn decimal_counter(value: &str) -> Result<i64, String> {
    if value.is_empty()
        || value.len() > 19
        || (value.len() > 1 && value.starts_with('0'))
        || !value.bytes().all(|b| b.is_ascii_digit())
    {
        return Err("Invalid decimal journal counter".into());
    }
    value
        .parse::<i64>()
        .map_err(|_| "Journal counter exceeds signed SQLite range".into())
}
pub fn query_sha256(query: &OperationQuery) -> Result<String, String> {
    operation_query_sha256(query)
}
pub fn validate(context: &RequestContext, query: &OperationQuery) -> Result<(), String> {
    if !(1..=50).contains(&query.limit)
        || query.search.len() > 200
        || query.search.chars().any(char::is_control)
        || query
            .project_id
            .as_ref()
            .is_some_and(|id| !crate::config::valid_id(id))
    {
        return Err("Operation query requires limit 1..50 and bounded project/search".into());
    }
    if let Some(cursor) = &query.cursor {
        if &cursor.context != context || cursor.query_sha256 != query_sha256(query)? {
            return Err(
                "Operation cursor belongs to another scope or filter; start a new view explicitly"
                    .into(),
            );
        }
        if cursor.before.operation_id.is_empty()
            || cursor.before.operation_id.len() > 200
            || cursor.before.operation_id.chars().any(char::is_control)
            || cursor.before.created_at_ms > i64::MAX as u64
        {
            return Err("Invalid operation cursor key".into());
        }
        for counter in [
            &cursor.watermarks.tasks,
            &cursor.watermarks.imports,
            &cursor.watermarks.workflows,
            &cursor.watermarks.applications,
        ] {
            decimal_counter(counter)?;
        }
    }
    Ok(())
}
