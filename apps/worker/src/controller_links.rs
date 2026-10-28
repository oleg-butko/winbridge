//! One worker, independent controller observations and a shared network RPC budget.
use crate::{projection::CachedProjection, Result};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Instant,
};
use winbridge_core::controller_sessions::{Peer, Permit, Phase, Sessions, Ticket, Topology};
use winbridge_protocol::*;

#[derive(Default)]
struct Counters {
    admitted: u64,
    completed: u64,
    rejected: u64,
}
impl Counters {
    fn observe(&self, in_flight: usize, limit: u16) -> NetworkRpcObservation {
        NetworkRpcObservation {
            in_flight: in_flight as u16,
            limit,
            admitted: self.admitted.to_string(),
            completed: self.completed.to_string(),
            rejected: self.rejected.to_string(),
        }
    }
}

pub(crate) struct Link {
    pub connection: ControllerConnection,
    pub projection: Option<CachedProjection>,
    attempts: u64,
    last_connected_at_ms: Option<u64>,
    last_disconnected_at_ms: Option<u64>,
    last_error: Option<String>,
    rpc: Counters,
}
pub(crate) struct Links {
    pub registry: Sessions,
    pub primary: String,
    pub links: BTreeMap<String, Link>,
    pub clock: Instant,
    rpc: Counters,
}
impl Links {
    pub fn new(topology: &Topology, generation: String) -> Result<Self> {
        Ok(Self {
            registry: Sessions::new(
                topology,
                generation,
                NETWORK_RPC_LIMIT.into(),
                CONTROLLER_RPC_LIMIT.into(),
                10_000,
            )?,
            primary: topology.primary.clone(),
            clock: Instant::now(),
            rpc: Counters::default(),
            links: topology
                .controllers
                .iter()
                .map(|p| {
                    (
                        p.id.clone(),
                        Link {
                            projection: None,
                            attempts: 0,
                            last_connected_at_ms: None,
                            last_disconnected_at_ms: None,
                            last_error: None,
                            rpc: Counters::default(),
                            connection: ControllerConnection {
                                url: p.url.clone(),
                                state: ConnectionState::Connecting,
                                session_id: None,
                                controller_node_id: None,
                                controller_generation: None,
                                last_confirmed_at_ms: None,
                                error: None,
                            },
                        },
                    )
                })
                .collect(),
        })
    }
    pub fn begin(&mut self, id: &str) -> Result<Ticket> {
        let ticket = self.registry.begin(id)?;
        let link = self.links.get_mut(id).ok_or("Unknown controller")?;
        link.attempts = link.attempts.saturating_add(1);
        link.projection = None;
        link.connection.state = ConnectionState::Connecting;
        link.connection.session_id = None;
        link.connection.controller_generation = None;
        link.connection.controller_node_id = None;
        link.connection.last_confirmed_at_ms = None;
        link.connection.error = None;
        Ok(ticket)
    }
    pub fn connected(&mut self, ticket: &Ticket, peer: Peer) -> Result<bool> {
        if !self.registry.connected(ticket, peer.clone())? {
            return Ok(false);
        }
        let link = self
            .links
            .get_mut(ticket.controller_id())
            .ok_or("Unknown controller")?;
        let now = crate::store::now_ms();
        link.last_connected_at_ms = Some(now);
        let c = &mut link.connection;
        c.state = ConnectionState::Connected;
        c.error = None;
        c.session_id = Some(peer.session);
        c.controller_generation = Some(peer.generation);
        c.controller_node_id = Some(peer.node);
        c.last_confirmed_at_ms = Some(now);
        Ok(true)
    }
    pub fn disconnected(&mut self, ticket: &Ticket, mut reason: String) {
        if !self.registry.is_current(ticket)
            || self
                .registry
                .observe(ticket.controller_id())
                .is_some_and(|s| s.phase == Phase::Disconnected)
        {
            return;
        }
        if reason.len() > 2048 {
            let mut end = 2048;
            while !reason.is_char_boundary(end) {
                end -= 1
            }
            reason.truncate(end);
            reason.push_str(" [truncated]");
        }
        if !self.registry.disconnected(ticket, reason.clone()) {
            return;
        }
        if let Some(link) = self.links.get_mut(ticket.controller_id()) {
            link.last_disconnected_at_ms = Some(crate::store::now_ms());
            link.last_error = Some(reason.clone());
            link.projection = None;
            link.connection.state = ConnectionState::Disconnected;
            link.connection.error = Some(reason);
            link.connection.session_id = None;
            link.connection.controller_generation = None;
            link.connection.controller_node_id = None;
        }
    }
    pub fn heartbeat(&mut self, ticket: &Ticket) {
        if self.registry.is_current(ticket)
            && self
                .registry
                .observe(ticket.controller_id())
                .is_some_and(|s| s.phase == Phase::Connected)
        {
            if let Some(link) = self.links.get_mut(ticket.controller_id()) {
                link.connection.last_confirmed_at_ms = Some(crate::store::now_ms());
            }
        }
    }
    pub fn primary_connection(&self) -> Option<ControllerConnection> {
        self.links.get(&self.primary).map(|l| l.connection.clone())
    }
    fn admit(&mut self, ticket: &Ticket, id: String) -> Result<Permit> {
        let permit = self.registry.admit(ticket, id);
        if let Some(link) = self.links.get_mut(ticket.controller_id()) {
            if permit.is_ok() {
                link.rpc.admitted = link.rpc.admitted.saturating_add(1);
                self.rpc.admitted = self.rpc.admitted.saturating_add(1);
            } else {
                link.rpc.rejected = link.rpc.rejected.saturating_add(1);
                self.rpc.rejected = self.rpc.rejected.saturating_add(1);
            }
        }
        permit
    }
    pub fn reject(&mut self, ticket: &Ticket) {
        if let Some(link) = self.links.get_mut(ticket.controller_id()) {
            link.rpc.rejected = link.rpc.rejected.saturating_add(1);
            self.rpc.rejected = self.rpc.rejected.saturating_add(1);
        }
    }
    fn complete(&mut self, permit: &Permit) {
        if self.registry.complete(permit) {
            self.rpc.completed = self.rpc.completed.saturating_add(1);
            if let Some(link) = self.links.get_mut(&permit.origin().controller) {
                link.rpc.completed = link.rpc.completed.saturating_add(1);
            }
        }
    }
    pub(crate) fn observe(
        &self,
        context: RequestContext,
        generation: String,
    ) -> ControllerConnections {
        ControllerConnections {
            schema_version: 1,
            context,
            provider: build_identity(),
            worker_generation: generation,
            observed_at_ms: crate::store::now_ms(),
            primary: Some(self.primary.clone()),
            rpc: self
                .rpc
                .observe(self.registry.in_flight(), NETWORK_RPC_LIMIT),
            connections: self
                .links
                .iter()
                .map(|(id, l)| ControllerLinkObservation {
                    id: id.clone(),
                    connection: l.connection.clone(),
                    attempts: l.attempts.to_string(),
                    last_connected_at_ms: l.last_connected_at_ms,
                    last_disconnected_at_ms: l.last_disconnected_at_ms,
                    last_error: l.last_error.clone(),
                    rpc: l
                        .rpc
                        .observe(self.registry.in_flight_for(id), CONTROLLER_RPC_LIMIT),
                })
                .collect(),
        }
    }
}

impl crate::Worker {
    pub(crate) fn controller_connections_snapshot(
        &self,
        context: RequestContext,
    ) -> Result<ControllerConnections> {
        match &self.controllers {
            Some(links) => Ok(links
                .lock()
                .map_err(|_| "Controller state lock poisoned")?
                .observe(context, self.generation.clone())),
            None => Ok(ControllerConnections {
                schema_version: 1,
                context,
                provider: build_identity(),
                worker_generation: self.generation.clone(),
                observed_at_ms: crate::store::now_ms(),
                primary: None,
                connections: vec![],
                rpc: Counters::default().observe(0, NETWORK_RPC_LIMIT),
            }),
        }
    }
}

/// Moved into spawn_blocking: timeout/JoinSet/socket drops cannot release a
/// request that is still executing. This permit does not own accepted tasks.
pub(crate) struct RpcGuard {
    links: Arc<Mutex<Links>>,
    permit: Permit,
}
impl RpcGuard {
    pub fn creator(&self) -> OperationCreator {
        let origin = self.permit.origin();
        OperationCreator::Controller {
            worker_generation: origin.worker_generation.clone(),
            controller_id: origin.controller.clone(),
            controller_node_id: origin.peer.node.clone(),
            controller_generation: origin.peer.generation.clone(),
            session_id: origin.peer.session.clone(),
        }
    }
    pub fn admit(links: &Arc<Mutex<Links>>, ticket: &Ticket, id: String) -> Result<Self> {
        let permit = links
            .lock()
            .map_err(|_| "Controller state lock poisoned")?
            .admit(ticket, id)?;
        Ok(Self {
            links: links.clone(),
            permit,
        })
    }
}
impl Drop for RpcGuard {
    fn drop(&mut self) {
        if let Ok(mut links) = self.links.lock() {
            links.complete(&self.permit);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn timed_out_blocking_handler_keeps_its_permit_until_actual_completion() {
        let topology = Topology::legacy("ws://127.0.0.1:1143".into()).unwrap();
        let links = Arc::new(Mutex::new(Links::new(&topology, "worker".into()).unwrap()));
        let ticket = links.lock().unwrap().begin("legacy").unwrap();
        links
            .lock()
            .unwrap()
            .connected(
                &ticket,
                Peer {
                    node: "controller".into(),
                    generation: "generation".into(),
                    session: "session".into(),
                },
            )
            .unwrap();
        let guard = RpcGuard::admit(&links, &ticket, "rpc".into()).unwrap();
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (finish_tx, finish_rx) = std::sync::mpsc::channel();
        let (ended_tx, ended_rx) = tokio::sync::oneshot::channel();
        let handler = tokio::task::spawn_blocking(move || {
            let _ = started_tx.send(());
            let _ = finish_rx.recv();
            drop(guard);
            let _ = ended_tx.send(());
        });
        started_rx.await.unwrap();
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(5), handler)
                .await
                .is_err()
        );
        links
            .lock()
            .unwrap()
            .disconnected(&ticket, "socket lost".into());
        assert_eq!(links.lock().unwrap().registry.in_flight(), 1);
        {
            let links = links.lock().unwrap();
            assert_eq!(
                links
                    .rpc
                    .observe(links.registry.in_flight(), NETWORK_RPC_LIMIT)
                    .in_flight,
                1
            );
            assert_eq!((links.rpc.admitted, links.rpc.completed), (1, 0));
        }
        finish_tx.send(()).unwrap();
        ended_rx.await.unwrap();
        assert_eq!(links.lock().unwrap().registry.in_flight(), 0);
        assert_eq!(links.lock().unwrap().rpc.completed, 1);
    }

    #[test]
    fn counters_keep_old_handlers_across_reconnect_and_errors_are_bounded() {
        let topology: Topology = serde_json::from_value(
            serde_json::json!({"schemaVersion":1,"primary":"a","controllers":[
            {"id":"a","url":"ws://127.0.0.1:1143"},{"id":"b","url":"ws://127.0.0.1:1144"}]}),
        )
        .unwrap();
        let mut links = Links::new(&topology, "worker".into()).unwrap();
        let mut connected = |id: &str| {
            let t = links.begin(id).unwrap();
            links
                .connected(
                    &t,
                    Peer {
                        node: "same-node".into(),
                        generation: "same-generation".into(),
                        session: "same-session".into(),
                    },
                )
                .unwrap();
            t
        };
        let a = connected("a");
        let b = connected("b");
        let p = links.admit(&a, "same-id".into()).unwrap();
        let other = links.admit(&b, "same-id".into()).unwrap();
        assert!(links.admit(&a, "same-id".into()).is_err());
        links.reject(&b); // Also counts the session response-buffer admission limit.
        links.disconnected(&a, "Проверка ✓".repeat(1000));
        let retained = links.links["a"].last_error.clone().unwrap();
        assert!(retained.len() < 4096 && retained.ends_with("[truncated]"));
        let next = links.begin("a").unwrap();
        links
            .connected(
                &next,
                Peer {
                    node: "same-node".into(),
                    generation: "new-generation".into(),
                    session: "new-session".into(),
                },
            )
            .unwrap();
        links.disconnected(&a, "late stale error".into());
        assert_eq!(links.links["a"].last_error.as_ref(), Some(&retained));
        assert_eq!(links.registry.in_flight_for("a"), 1);
        links.complete(&p);
        links.complete(&p);
        links.complete(&other);
        assert_eq!(
            (links.rpc.admitted, links.rpc.completed, links.rpc.rejected),
            (2, 2, 2)
        );
        assert_eq!(
            (links.links["a"].attempts, links.links["b"].attempts),
            (2, 1)
        );
        assert_eq!(links.links["a"].rpc.completed, 1);
        assert_eq!(links.links["b"].rpc.completed, 1);
    }
}
