use serde_json::Value;
use winbridge_protocol::*;
#[test]
fn shared_metadata_cases_and_frozen_contract_hashes() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/discovery/resource-scheduling-cases.json"
    ))
    .unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let mut info = fixture["baseInfo"].clone();
        if let Some(pointer) = case["pointer"].as_str() {
            if case["remove"] == true {
                let (parent, key) = pointer.rsplit_once('/').unwrap();
                info.pointer_mut(parent)
                    .unwrap()
                    .as_object_mut()
                    .unwrap()
                    .remove(key);
            } else if let Some(value) = info.pointer_mut(pointer) {
                *value = case["value"].clone();
            } else {
                let (parent, key) = pointer.rsplit_once('/').unwrap();
                info.pointer_mut(parent)
                    .unwrap()
                    .as_object_mut()
                    .unwrap()
                    .insert(key.into(), case["value"].clone());
            }
        }
        let info: WorkerInfo = serde_json::from_value(info).unwrap();
        assert_eq!(
            interpret_resource_scheduling(&info).is_some(),
            case["expected"].as_bool().unwrap(),
            "{}",
            case["name"]
        );
    }
    #[cfg(feature = "jsonschema")]
    {
        assert_eq!(
            fenced_rpc_schema_sha256(),
            "dd86d35b8b9ed59aca0bb438491e906b9c25c69970f0121226fdee99b98e9a12"
        );
        assert_eq!(
            operation_schema_sha256(),
            "7c078b1bfedbdf2b1488fe782f6ab52e35ba33f9b8f28e560aa683458d9c9749"
        );
        assert_eq!(
            work_scheduling_schema_sha256(),
            "d89abdc35d712b7cced4a8c922ece1c7dca469b330c7160ec1df356f12a00bc8"
        );
        assert_eq!(
            queue_control_schema_sha256(),
            "d33836aaf9d79201ccc19c21c56d3007de9cb46bcb47fd1f7e29e57a239e0722"
        );
        assert_eq!(resource_scheduling_schema_sha256().len(), 64);
    }
}
