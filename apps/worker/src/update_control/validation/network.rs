//! Firewall wait for the original normal Main, never an activation permission.
use super::*;
use winbridge_protocol::{ConnectionState, ControllerConnection};

const FIREWALL_WAIT: Duration = Duration::from_secs(900);
const MAX_CONFIRMATION_AGE_MS: u64 = 10_000;

fn ready(connection: Option<&ControllerConnection>, url: &str, now: u64) -> bool {
    connection.is_some_and(|c| {
        c.url == url
            && matches!(c.state, ConnectionState::Connected)
            && c.error.is_none()
            && c.session_id.as_ref().is_some_and(|s| !s.is_empty())
            && c.controller_node_id.as_ref().is_some_and(|s| !s.is_empty())
            && c.controller_generation
                .as_ref()
                .is_some_and(|s| !s.is_empty())
            && c.last_confirmed_at_ms.is_some_and(|t| {
                now.checked_sub(t)
                    .is_some_and(|age| age <= MAX_CONFIRMATION_AGE_MS)
            })
    })
}

// Main25 remains a declared single-route fixture. Never silently choose a
// primary from a production topology when validating its firewall preflight.
fn connection(worker: &Worker, url: &str) -> Result<Option<ControllerConnection>> {
    if worker.config.worker.controller_url.as_deref() != Some(url)
        || worker.config.worker.controller_topology.is_some()
    {
        return Err("Frozen Main fixture requires its exact single-route controller URL".into());
    }
    worker
        .controllers
        .as_ref()
        .map(|links| {
            links
                .lock()
                .map_err(|_| "Controller connection lock poisoned".to_string())
                .map(|links| links.primary_connection())
        })
        .transpose()
        .map(Option::flatten)
}

pub(super) async fn wait_before_admission(worker: &Worker, input: &Input) -> Result<()> {
    let Some(url) = input.controller_url.as_deref() else {
        return Ok(());
    };
    let started = Instant::now();
    let mut last_progress = None;
    loop {
        let connection = connection(worker, url)?;
        if ready(connection.as_ref(), url, crate::store::now_ms()) {
            publish(
                &input.root,
                "00-controller-preflight.json",
                &serde_json::json!({
                    "schemaVersion":1,"kind":"winbridge_main_controller_preflight",
                    "instanceId":worker.config.instance_id,"workerGeneration":worker.generation,
                    "connection":connection,"elapsedMs":started.elapsed().as_millis(),
                    "beforeSupervisorAdmission":true,"activationDeadlineRenewed":false
                }),
            )?;
            return Ok(());
        }
        if started.elapsed() >= FIREWALL_WAIT {
            return Err(format!("Test controller {url} was not confirmed within 900s before update admission; preserve this runtime and check its executable firewall permission"));
        }
        if last_progress.is_none_or(|at: Instant| at.elapsed() >= Duration::from_secs(15)) {
            eprintln!("Main fixture: waiting for controller {url} before update admission ({}s/900s); same executable/config, no activation started", started.elapsed().as_secs());
            last_progress = Some(Instant::now());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn connected() -> ControllerConnection {
        ControllerConnection {
            url: "ws://192.168.44.203:1144".into(),
            state: ConnectionState::Connected,
            session_id: Some("session".into()),
            controller_node_id: Some("test-controller".into()),
            controller_generation: Some("generation".into()),
            last_confirmed_at_ms: Some(20_000),
            error: None,
        }
    }
    #[test]
    fn preflight_reads_legacy_link_but_never_adopts_a_topology_primary() {
        use winbridge_core::controller_sessions::Peer;
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        std::fs::create_dir(&project).unwrap();
        let url = "ws://127.0.0.1:1144";
        for topology in [false, true] {
            let settings = if topology {
                serde_json::json!({"controllerTopology":{"schemaVersion":1,"primary":"c","controllers":[
                    {"id":"a","url":"ws://127.0.0.1:1143"},
                    {"id":"b","url":url},{"id":"c","url":"ws://127.0.0.1:1145"}]},"keepRunningOnUiExit":true})
            } else {
                serde_json::json!({"controllerUrl":url,"keepRunningOnUiExit":true})
            };
            let worker = Worker::open(serde_json::from_value(serde_json::json!({
                "schemaVersion":1,"instanceId":"network-preflight","nodeId":"worker",
                "stateDirectory":root.path().join(if topology {"multi"} else {"legacy"}),"worker":settings,
                "projects":[{"id":"fixture","workspaceRoot":project,"allowDirectCommands":false,"maxConcurrentMutations":1,"recipes":[]}]
            })).unwrap()).unwrap();
            if topology {
                assert!(connection(&worker, url)
                    .unwrap_err()
                    .contains("single-route"));
            } else {
                assert!(matches!(
                    connection(&worker, url).unwrap().unwrap().state,
                    ConnectionState::Connecting
                ));
                assert!(connection(&worker, "ws://127.0.0.1:1143").is_err());
                let mut links = worker.controllers.as_ref().unwrap().lock().unwrap();
                let ticket = links.begin("legacy").unwrap();
                links
                    .connected(
                        &ticket,
                        Peer {
                            node: "test-controller".into(),
                            generation: "generation".into(),
                            session: "session".into(),
                        },
                    )
                    .unwrap();
                drop(links);
                assert!(ready(
                    connection(&worker, url).unwrap().as_ref(),
                    url,
                    crate::store::now_ms()
                ));
            }
        }
    }

    #[test]
    fn fresh_exact_session_is_required_before_admission() {
        let c = connected();
        assert!(ready(Some(&c), &c.url, 30_000));
        assert!(!ready(Some(&c), &c.url, 30_001));
        assert!(!ready(Some(&c), &c.url, 19_999));
        assert!(!ready(Some(&c), "ws://192.168.44.203:1143", 20_000));
        assert!(!ready(None, &c.url, 20_000));
        for field in 0..6 {
            let mut bad = c.clone();
            match field {
                0 => bad.state = ConnectionState::Connecting,
                1 => bad.session_id = None,
                2 => bad.controller_node_id = Some(String::new()),
                3 => bad.controller_generation = None,
                4 => bad.last_confirmed_at_ms = None,
                _ => bad.error = Some("blocked".into()),
            }
            assert!(!ready(Some(&bad), &c.url, 20_000));
        }
    }
}
