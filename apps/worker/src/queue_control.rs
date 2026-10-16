//! Registered only in local IPC, never WebSocket/readonly extension transport.
use crate::Worker;
use std::sync::atomic::Ordering;
use winbridge_protocol::*;
impl Worker {
    pub fn handle_queue_control(&self, request: QueueControlRequest) -> QueueControlResponse {
        let rejected = |code, message: &str| QueueControlResponse {
            queue_control_version: 1,
            context: None,
            result: QueueControlResult::Rejected {
                code,
                message: message.into(),
            },
        };
        if request.queue_control_version != 1 {
            return rejected(
                QueueControlError::UnsupportedVersion,
                "Unsupported local queue control version; no fallback",
            );
        }
        let result = (|| {
            let mut store = self.store()?;
            let context = RequestContext {
                instance_id: self.config.instance_id.clone(),
                journal_incarnation: store.journal_authority().incarnation.clone(),
            };
            if context != request.context {
                return Ok(rejected(
                    QueueControlError::ScopeMismatch,
                    "Queue command belongs to another journal; no rebind",
                ));
            }
            if matches!(request.command, QueueControlCommand::Set { .. })
                && (self.stop.load(Ordering::Acquire)
                    || self.journal_recovery_required()?.is_some())
            {
                return Ok(rejected(
                    QueueControlError::Unavailable,
                    "Worker stopping/recovery required; no queue change",
                ));
            }
            let result = store.queue_control(context.clone(), request.command)?;
            Ok::<_, String>(QueueControlResponse {
                queue_control_version: 1,
                context: Some(context),
                result,
            })
        })();
        result.unwrap_or_else(|error|QueueControlResponse{queue_control_version:1,context:None,result:QueueControlResult::Unconfirmed{message:format!("Queue result unconfirmed: {error}; observe original command ID before deciding on a repeat")}})
    }
}
