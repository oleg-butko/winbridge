//! Portable multi-controller foundation. Not wired to worker config or transport.
//! Session observations never grant execution authority or release durable tasks.
use crate::{config::valid_id, controller_config::validate_controller_url};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Endpoint {
    pub id: String,
    pub url: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Topology {
    pub schema_version: u16,
    pub primary: String,
    pub controllers: Vec<Endpoint>,
}
impl Topology {
    pub fn legacy(url: String) -> Result<Self, String> {
        let topology = Self {
            schema_version: 1,
            primary: "legacy".into(),
            controllers: vec![Endpoint {
                id: "legacy".into(),
                url,
            }],
        };
        topology.validate()?;
        Ok(topology)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1 || !(1..=4).contains(&self.controllers.len()) {
            return Err("Topology requires schema 1 and 1..4 controllers".into());
        }
        let mut ids = BTreeSet::new();
        let mut addresses = BTreeSet::new();
        for endpoint in &self.controllers {
            if !valid_id(&endpoint.id)
                || !ids.insert(&endpoint.id)
                || !addresses.insert(validate_controller_url(&endpoint.url)?)
            {
                return Err("Controller IDs and normalized addresses must be unique".into());
            }
        }
        if !ids.contains(&self.primary) {
            return Err("Primary controller is not configured".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Phase {
    Disconnected,
    Connecting,
    Connected,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Peer {
    pub node: String,
    pub generation: String,
    pub session: String,
}
/// Captured from the live connection, not from caller-supplied task arguments.
/// Attribution only: does not grant task, cancellation or update authority.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RpcOrigin {
    pub worker_generation: String,
    pub controller: String,
    pub peer: Peer,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Observation {
    pub phase: Phase,
    pub peer: Option<Peer>,
    pub error: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Ticket {
    worker_generation: String,
    controller: String,
    attempt: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Permit {
    ticket: Ticket,
    request: String,
    origin: RpcOrigin,
}
impl Permit {
    pub fn origin(&self) -> &RpcOrigin {
        &self.origin
    }
    pub fn request_id(&self) -> &str {
        &self.request
    }
}
struct Session {
    attempt: u64,
    observation: Observation,
    projection: Option<(u64, u64)>, // sequence, injected monotonic receive time
}
pub struct Sessions {
    worker_generation: String,
    primary: String,
    sessions: BTreeMap<String, Session>,
    in_flight: BTreeSet<Permit>,
    total_limit: usize,
    peer_limit: usize,
    projection_ttl_ms: u64,
}
impl Sessions {
    pub fn new(
        topology: &Topology,
        worker_generation: String,
        total_limit: usize,
        peer_limit: usize,
        projection_ttl_ms: u64,
    ) -> Result<Self, String> {
        topology.validate()?;
        if !valid_id(&worker_generation)
            || total_limit == 0
            || peer_limit == 0
            || peer_limit > total_limit
            || projection_ttl_ms == 0
        {
            return Err("Invalid worker generation, RPC limits or projection TTL".into());
        }
        Ok(Self {
            worker_generation,
            primary: topology.primary.clone(),
            sessions: topology
                .controllers
                .iter()
                .map(|p| {
                    (
                        p.id.clone(),
                        Session {
                            attempt: 0,
                            observation: Observation {
                                phase: Phase::Disconnected,
                                peer: None,
                                error: None,
                            },
                            projection: None,
                        },
                    )
                })
                .collect(),
            in_flight: BTreeSet::new(),
            total_limit,
            peer_limit,
            projection_ttl_ms,
        })
    }
    pub fn begin(&mut self, controller: &str) -> Result<Ticket, String> {
        let state = self
            .sessions
            .get_mut(controller)
            .ok_or("Unknown controller")?;
        state.attempt = state
            .attempt
            .checked_add(1)
            .ok_or("Connection attempt exhausted")?;
        state.observation = Observation {
            phase: Phase::Connecting,
            peer: None,
            error: None,
        };
        state.projection = None;
        Ok(Ticket {
            worker_generation: self.worker_generation.clone(),
            controller: controller.into(),
            attempt: state.attempt,
        })
    }
    fn current(&self, ticket: &Ticket) -> bool {
        ticket.worker_generation == self.worker_generation
            && self
                .sessions
                .get(&ticket.controller)
                .is_some_and(|s| s.attempt == ticket.attempt)
    }
    pub fn connected(&mut self, ticket: &Ticket, peer: Peer) -> Result<bool, String> {
        if !self.current(ticket) {
            return Ok(false);
        }
        if ![&peer.node, &peer.generation, &peer.session]
            .into_iter()
            .all(|v| valid_id(v))
        {
            return Err("Invalid peer identity".into());
        }
        let state = self.sessions.get_mut(&ticket.controller).unwrap();
        if state.observation.phase == Phase::Connected
            && state.observation.peer.as_ref() == Some(&peer)
        {
            return Ok(false); // duplicate observation must not erase projection
        }
        if state.observation.phase != Phase::Connecting {
            return Err("Begin a fresh connection attempt first".into());
        }
        state.observation = Observation {
            phase: Phase::Connected,
            peer: Some(peer),
            error: None,
        };
        Ok(true)
    }
    pub fn disconnected(&mut self, ticket: &Ticket, reason: String) -> bool {
        if !self.current(ticket) {
            return false;
        }
        let state = self.sessions.get_mut(&ticket.controller).unwrap();
        state.observation = Observation {
            phase: Phase::Disconnected,
            peer: None,
            error: Some(reason),
        };
        state.projection = None;
        // Blocking handlers and accepted tasks may outlive the socket. Only an
        // explicit handler completion releases its RPC permit; tasks use Store.
        true
    }
    pub fn observe(&self, controller: &str) -> Option<&Observation> {
        self.sessions.get(controller).map(|s| &s.observation)
    }
    pub fn project(&mut self, ticket: &Ticket, session: &str, sequence: u64, now_ms: u64) -> bool {
        if !self.current(ticket) || sequence == 0 {
            return false;
        }
        let state = self.sessions.get_mut(&ticket.controller).unwrap();
        if state.observation.phase != Phase::Connected
            || !state
                .observation
                .peer
                .as_ref()
                .is_some_and(|p| p.session == session)
            || state
                .projection
                .is_some_and(|(old, time)| sequence <= old || now_ms < time)
        {
            return false;
        }
        state.projection = Some((sequence, now_ms));
        true
    }
    pub fn projection_sequence(&self, controller: &str, now_ms: u64) -> Option<u64> {
        let state = self.sessions.get(controller)?;
        if state.observation.phase != Phase::Connected {
            return None;
        }
        let (sequence, received) = state.projection?;
        now_ms
            .checked_sub(received)
            .filter(|age| *age < self.projection_ttl_ms)
            .map(|_| sequence)
    }
    pub fn primary_projection_sequence(&self, now_ms: u64) -> Option<u64> {
        self.projection_sequence(&self.primary, now_ms)
    }
    pub fn admit(&mut self, ticket: &Ticket, request: String) -> Result<Permit, String> {
        if !valid_id(&request)
            || !self.current(ticket)
            || self.sessions[&ticket.controller].observation.phase != Phase::Connected
        {
            return Err("RPC needs a live session and valid request ID".into());
        }
        let permit = Permit {
            ticket: ticket.clone(),
            request,
            origin: RpcOrigin {
                worker_generation: self.worker_generation.clone(),
                controller: ticket.controller.clone(),
                peer: self.sessions[&ticket.controller]
                    .observation
                    .peer
                    .clone()
                    .ok_or("Connected session has no peer identity")?,
            },
        };
        if self.in_flight.contains(&permit) {
            return Err("Duplicate in-flight RPC ID".into());
        }
        if self.in_flight.len() >= self.total_limit
            || self
                .in_flight
                .iter()
                .filter(|p| p.ticket.controller == ticket.controller)
                .count()
                >= self.peer_limit
        {
            return Err("RPC capacity busy".into());
        }
        self.in_flight.insert(permit.clone());
        Ok(permit)
    }
    pub fn complete(&mut self, permit: &Permit) -> bool {
        self.in_flight.remove(permit)
    }
    pub fn in_flight(&self) -> usize {
        self.in_flight.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn topology() -> Topology {
        serde_json::from_str(
            r#"{"schemaVersion":1,"primary":"debian-203","controllers":[
            {"id":"debian-203","url":"ws://192.168.44.203:1143"},
            {"id":"debian-204","url":"ws://192.168.44.204:1143"}]}"#,
        )
        .unwrap()
    }
    fn registry() -> Sessions {
        Sessions::new(&topology(), "worker-generation".into(), 3, 2, 1000).unwrap()
    }
    fn connect(s: &mut Sessions, id: &str) -> Ticket {
        let t = s.begin(id).unwrap();
        s.connected(
            &t,
            Peer {
                node: id.into(),
                generation: "gen".into(),
                session: "session".into(),
            },
        )
        .unwrap();
        t
    }
    #[test]
    fn topology_is_strict_and_normalizes_addresses_and_preserves_legacy() {
        Topology::legacy("ws://192.168.44.203:1143".into()).unwrap();
        let mut t = topology();
        t.controllers[1].url = "ws://192.168.44.203:1143/".into();
        assert!(t.validate().is_err());
        let mut t = topology();
        t.primary = "missing".into();
        assert!(t.validate().is_err());
        let mut t = topology();
        t.controllers[1].url = "wss://192.168.44.204:1143".into();
        assert!(t.validate().is_err());
        let mut t = topology();
        t.controllers[1].id = t.controllers[0].id.clone();
        assert!(t.validate().is_err());
        assert!(serde_json::from_str::<Topology>(
            r#"{"schemaVersion":1,"primary":"x","controllers":[],"extra":true}"#
        )
        .is_err());
    }
    #[test]
    fn one_disconnect_cannot_clear_other_projection_or_replace_primary() {
        let mut s = registry();
        let a = connect(&mut s, "debian-203");
        let b = connect(&mut s, "debian-204");
        assert!(s.project(&a, "session", 9, 10));
        assert!(s.project(&b, "session", 5, 10));
        assert!(s.disconnected(&a, "offline".into()));
        assert_eq!(s.projection_sequence("debian-204", 11), Some(5));
        assert_eq!(s.primary_projection_sequence(11), None);
        assert_eq!(s.observe("debian-204").unwrap().phase, Phase::Connected);
    }
    #[test]
    fn stale_callbacks_and_foreign_worker_generation_cannot_mutate_new_session() {
        let mut s = registry();
        let old = connect(&mut s, "debian-203");
        let fresh = connect(&mut s, "debian-203");
        assert!(!s.disconnected(&old, "late EOF".into()));
        assert!(!s.project(&old, "session", 99, 0));
        assert!(s.project(&fresh, "session", 1, 1));
        let mut other = Sessions::new(&topology(), "other-worker".into(), 3, 2, 1000).unwrap();
        connect(&mut other, "debian-203");
        assert!(!other.disconnected(&fresh, "foreign".into()));
        assert!(!s
            .connected(
                &old,
                Peer {
                    node: "x".into(),
                    generation: "g".into(),
                    session: "s".into()
                }
            )
            .unwrap());
    }
    #[test]
    fn projections_reject_reordering_wrong_session_expiry_and_clock_rollback() {
        let mut s = registry();
        let a = connect(&mut s, "debian-203");
        assert!(!s.project(&a, "foreign", 1, 10));
        assert!(s.project(&a, "session", 2, 10));
        assert!(!s.project(&a, "session", 1, 11));
        assert!(!s.project(&a, "session", 3, 9));
        assert_eq!(s.projection_sequence("debian-203", 9), None);
        assert_eq!(s.projection_sequence("debian-203", 1010), None);
        let peer = s.observe("debian-203").unwrap().peer.clone().unwrap();
        assert!(!s.connected(&a, peer).unwrap());
        assert_eq!(s.projection_sequence("debian-203", 11), Some(2));
    }
    #[test]
    fn disconnected_rpc_still_counts_and_old_completion_cannot_free_new_slot() {
        let mut s = registry();
        let a = connect(&mut s, "debian-203");
        let b = connect(&mut s, "debian-204");
        let p = s.admit(&a, "same-id".into()).unwrap();
        assert!(s.admit(&a, "same-id".into()).is_err());
        let q = s.admit(&a, "second".into()).unwrap();
        let r = s.admit(&b, "same-id".into()).unwrap();
        assert!(s.admit(&b, "full".into()).is_err());
        s.disconnected(&a, "EOF".into());
        let fresh = connect(&mut s, "debian-203");
        assert!(s.admit(&fresh, "third".into()).is_err());
        assert_eq!(s.in_flight(), 3);
        assert!(s.complete(&p));
        let new = s.admit(&fresh, "same-id".into()).unwrap();
        assert!(!s.complete(&p));
        assert_eq!(s.in_flight(), 3);
        assert!(s.complete(&new));
        assert!(s.complete(&q));
        assert!(s.complete(&r));
        assert_eq!(s.in_flight(), 0);
    }

    #[test]
    fn rpc_origin_survives_disconnect_and_cannot_be_rebound_by_reconnect() {
        let mut s = registry();
        let a = connect(&mut s, "debian-203");
        let b = connect(&mut s, "debian-204");
        let original = s.admit(&a, "same-rpc-id".into()).unwrap();
        let other = s.admit(&b, "same-rpc-id".into()).unwrap();
        assert_ne!(original.origin().controller, other.origin().controller);
        assert_eq!(original.request_id(), other.request_id());
        s.disconnected(&a, "network lost".into());
        let next = s.begin("debian-203").unwrap();
        s.connected(
            &next,
            Peer {
                node: "node".into(),
                generation: "new-generation".into(),
                session: "new-session".into(),
            },
        )
        .unwrap();
        assert_eq!(original.origin().peer.session, "session");
        assert_eq!(
            s.observe("debian-203")
                .unwrap()
                .peer
                .as_ref()
                .unwrap()
                .session,
            "new-session"
        );
        assert!(!s.disconnected(&a, "late EOF".into()));
        assert!(s.complete(&original));
        assert!(s.complete(&other));
    }
}
