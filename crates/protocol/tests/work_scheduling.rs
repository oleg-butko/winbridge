use winbridge_protocol::*;
#[test]
fn scheduling_metadata_matches_the_shared_typed_reader_cases() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/discovery/work-scheduling-cases.json"
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
            } else {
                let (parent, key) = pointer.rsplit_once('/').unwrap();
                let target = info.pointer_mut(parent).unwrap();
                if let Some(object) = target.as_object_mut() {
                    object.insert(key.into(), case["value"].clone());
                } else {
                    *target.get_mut(key.parse::<usize>().unwrap()).unwrap() = case["value"].clone();
                }
            }
        }
        let info: WorkerInfo = serde_json::from_value(info).unwrap();
        assert_eq!(
            interpret_work_scheduling(&info).is_some(),
            case["expected"].as_bool().unwrap(),
            "{}",
            case["name"]
        );
    }
}
#[test]
fn scheduling_does_not_change_the_frozen_rpc_two_schema() {
    #[cfg(feature = "jsonschema")]
    assert_eq!(
        fenced_rpc_schema_sha256(),
        "dd86d35b8b9ed59aca0bb438491e906b9c25c69970f0121226fdee99b98e9a12"
    );
}
