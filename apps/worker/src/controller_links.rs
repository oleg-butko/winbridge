//! One worker, independent controller observations and a shared network RPC budget.
use crate::{projection::CachedProjection, Result};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Instant,
};
use winbridge_core::controller_sessions::{Peer, Permit, Phase, Sessions, Ticket, Topology};
use winbridge_protocol::{ConnectionState, ControllerConnection};

pub(crate) struct Link {
    pub connection: ControllerConnection,
    pub projection: Option<CachedProjection>,
}
pub(crate) struct Links {
    pub registry: Sessions,
    pub primary: String,
    pub links: BTreeMap<String, Link>,
    pub clock: Instant,
}
impl Links {
    pub fn new(topology: &Topology, generation: String) -> Result<Self> {
        Ok(Self {
            registry: Sessions::new(topology, generation, 32, 16, 10_000)?,
            primary: topology.primary.clone(),
            clock: Instant::now(),
            links: topology
                .controllers
                .iter()
                .map(|p| {
                    (
                        p.id.clone(),
                        Link {
                            projection: None,
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
        let c = &mut self
            .links
            .get_mut(ticket.controller_id())
            .ok_or("Unknown controller")?
            .connection;
        c.state = ConnectionState::Connected;
        c.error = None;
        c.session_id = Some(peer.session);
        c.controller_generation = Some(peer.generation);
        c.controller_node_id = Some(peer.node);
        c.last_confirmed_at_ms = Some(crate::store::now_ms());
        Ok(true)
    }
    pub fn disconnected(&mut self, ticket: &Ticket, reason: String) {
        if !self.registry.is_current(ticket)
            || self
                .registry
                .observe(ticket.controller_id())
                .is_some_and(|s| s.phase == Phase::Disconnected)
        {
            return;
        }
        if !self.registry.disconnected(ticket, reason.clone()) {
            return;
        }
        if let Some(link) = self.links.get_mut(ticket.controller_id()) {
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
}

/// Moved into spawn_blocking: timeout/JoinSet/socket drops cannot release a
/// request that is still executing. This permit does not own accepted tasks.
pub(crate) struct RpcGuard {
    links: Arc<Mutex<Links>>,
    permit: Permit,
}
impl RpcGuard {
    pub fn admit(links: &Arc<Mutex<Links>>, ticket: &Ticket, id: String) -> Result<Self> {
        let permit = links
            .lock()
            .map_err(|_| "Controller state lock poisoned")?
            .registry
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
            links.registry.complete(&self.permit);
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
        finish_tx.send(()).unwrap();
        ended_rx.await.unwrap();
        assert_eq!(links.lock().unwrap().registry.in_flight(), 0);
    }
}
