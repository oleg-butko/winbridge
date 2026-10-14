use serde_json::Value;
use winbridge_protocol::*;

#[test]
fn readonly_projection_readers_agree_on_foreign_future_and_inconsistent_frames() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/readiness/projection-cases.json"
    ))
    .unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let mut raw = fixture["baseInfo"].clone();
        if let Some(pointer) = case["pointer"].as_str() {
            let (parent, key) = pointer.rsplit_once('/').unwrap();
            let parent = raw.pointer_mut(parent).unwrap();
            if case["remove"] == true {
                parent.as_object_mut().unwrap().remove(key);
            } else {
                let value = if let Some(repeat) = case["repeat"].as_u64() {
                    Value::Array(
                        (0..repeat)
                            .flat_map(|_| parent[key].as_array().unwrap().clone())
                            .collect(),
                    )
                } else {
                    case["value"].clone()
                };
                if let Some(array) = parent.as_array_mut() {
                    array[key.parse::<usize>().unwrap()] = value;
                } else {
                    parent.as_object_mut().unwrap().insert(key.into(), value);
                }
            }
        }
        let info: WorkerInfo = serde_json::from_value(raw).unwrap();
        let actual = match interpret_controller_projection(&info) {
            ProjectionObservation::Missing => "missing",
            ProjectionObservation::Known(_) => "known",
            ProjectionObservation::Unsupported => "unsupported",
        };
        assert_eq!(
            actual,
            case["expected"].as_str().unwrap(),
            "{}",
            case["name"]
        );
    }
    assert_eq!(
        fenced_rpc_schema_sha256(),
        include_str!("../../../tests/fixtures/readiness/fenced-rpc-2.sha256").trim()
    );
}
