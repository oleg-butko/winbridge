use winbridge_core::operation_query::*;
use winbridge_protocol::*;
#[test]
fn decimal_range_and_cursor_scope_filter_are_exact() {
    for value in ["0", "9007199254740993", "9223372036854775807"] {
        assert_eq!(decimal_counter(value).unwrap().to_string(), value);
    }
    for value in ["-1", "01", "1.0", "1e3", "9223372036854775808", ""] {
        assert!(decimal_counter(value).is_err());
    }
    let ctx = RequestContext {
        instance_id: "query".into(),
        journal_incarnation: "one".into(),
    };
    let mut query = OperationQuery {
        project_id: None,
        kind: None,
        search: String::new(),
        limit: 25,
        cursor: None,
    };
    query.cursor = Some(OperationCursor {
        context: ctx.clone(),
        query_sha256: query_sha256(&query).unwrap(),
        watermarks: AdmissionWatermarks {
            tasks: "9007199254740993".into(),
            imports: "0".into(),
            workflows: "0".into(),
            applications: "0".into(),
        },
        before: HistoryCursor {
            created_at_ms: 100,
            kind: HistoryKind::Task,
            operation_id: "one".into(),
        },
    });
    validate(&ctx, &query).unwrap();
    query.limit = 50;
    validate(&ctx, &query).unwrap();
    query.search = "changed".into();
    assert!(validate(&ctx, &query).is_err());
    query.search.clear();
    let mut other = ctx.clone();
    other.journal_incarnation = "two".into();
    assert!(validate(&other, &query).is_err());
}
