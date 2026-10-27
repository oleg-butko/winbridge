use crate::Controller;
use winbridge_protocol::*;
impl Controller {
    pub async fn operation_attribution(
        &self,
        request: OperationAttributionRequest,
    ) -> Result<OperationAttribution, String> {
        if request.context.instance_id != self.config.instance_id {
            return Err("Attribution instance differs; no transmission".into());
        }
        let sender = {
            let state = self.state.lock().map_err(|_| "Controller state poisoned")?;
            state
                .active
                .as_ref()
                .filter(|s| s.ready)
                .ok_or("Worker offline")?
                .sender
                .clone()
        };
        let LocalResponse::Info { info } = sender.call(LocalRequest::Info).await? else {
            return Err("Missing attribution provider Info".into());
        };
        let envelope = operation_attribution_envelope(&info, &request)?;
        decode_operation_attribution(sender.call_extension(envelope).await?, &info, &request)
    }
    /// Same negotiated readonly route as local desktop IPC, over this session.
    pub async fn controller_connections(
        &self,
        request: ControllerConnectionsRequest,
    ) -> Result<ControllerConnections, String> {
        if request.context.instance_id != self.config.instance_id {
            return Err("Connection query instance differs; no transmission".into());
        }
        let sender = {
            let state = self.state.lock().map_err(|_| "Controller state poisoned")?;
            state
                .active
                .as_ref()
                .filter(|s| s.ready)
                .ok_or("Worker offline; connection query not transmitted")?
                .sender
                .clone()
        };
        let LocalResponse::Info { info } = sender.call(LocalRequest::Info).await? else {
            return Err("Missing connection provider metadata".into());
        };
        let envelope = controller_connections_envelope(&info, &request)?;
        decode_controller_connections(sender.call_extension(envelope).await?, &info, &request)
    }
}
