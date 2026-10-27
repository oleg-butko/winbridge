use crate::{Controller, RpcSender};
use winbridge_protocol::*;
impl Controller {
    /// Keep metadata negotiation and the query on the same ready session.
    async fn diagnostic_provider(
        &self,
        context: &RequestContext,
    ) -> Result<(RpcSender, Box<WorkerInfo>), String> {
        if context.instance_id != self.config.instance_id {
            return Err("Diagnostic instance differs; no transmission".into());
        }
        let sender = {
            let state = self.state.lock().map_err(|_| "Controller state poisoned")?;
            state
                .active
                .as_ref()
                .filter(|s| s.ready)
                .ok_or("Worker offline; diagnostic query not transmitted")?
                .sender
                .clone()
        };
        let LocalResponse::Info { info } = sender.call(LocalRequest::Info).await? else {
            return Err("Missing diagnostic provider Info".into());
        };
        Ok((sender, info))
    }

    pub async fn operation_cancellation(
        &self,
        request: OperationAttributionRequest,
    ) -> Result<OperationCancellation, String> {
        let (sender, info) = self.diagnostic_provider(&request.context).await?;
        let envelope = operation_cancellation_envelope(&info, &request)?;
        decode_operation_cancellation(sender.call_extension(envelope).await?, &info, &request)
    }
    pub async fn operation_attribution(
        &self,
        request: OperationAttributionRequest,
    ) -> Result<OperationAttribution, String> {
        let (sender, info) = self.diagnostic_provider(&request.context).await?;
        let envelope = operation_attribution_envelope(&info, &request)?;
        decode_operation_attribution(sender.call_extension(envelope).await?, &info, &request)
    }
    /// Same negotiated readonly route as local desktop IPC, over this session.
    pub async fn controller_connections(
        &self,
        request: ControllerConnectionsRequest,
    ) -> Result<ControllerConnections, String> {
        let (sender, info) = self.diagnostic_provider(&request.context).await?;
        let envelope = controller_connections_envelope(&info, &request)?;
        decode_controller_connections(sender.call_extension(envelope).await?, &info, &request)
    }
}
