//! Only the original actor can move its live finalized owner to retirement.
use super::*;
impl FilesActor {
    pub(in crate::supervisor) fn keeper_preparing(&self) -> bool {
        #[cfg(windows)]
        {
            matches!(self.state, State::PreparingKeeper(_))
        }
        #[cfg(not(windows))]
        {
            false
        }
    }
    pub(in crate::supervisor) fn can_retire_keeper(&self) -> bool {
        #[cfg(windows)]
        {
            matches!(&self.state,State::FinalizationHeld(g) if matches!(g.record().state,crate::activation::finalization::ActivationFinalizationState::Confirmed{..})&&g.delivery_attempted())
        }
        #[cfg(not(windows))]
        {
            false
        }
    }
    #[cfg(windows)]
    pub(in crate::supervisor) fn take_for_keeper(
        &mut self,
    ) -> Result<Box<crate::activation::ActivationFinalizationGuard>> {
        if !self.can_retire_keeper() {
            return Err("Keeper retirement requires the original confirmed Active guard".into());
        }
        let pending = match &self.state {
            State::FinalizationHeld(g) => Box::new(g.activation().clone()),
            _ => unreachable!(),
        };
        match std::mem::replace(&mut self.state, State::PreparingKeeper(pending)) {
            State::FinalizationHeld(g) => Ok(g),
            _ => unreachable!(),
        }
    }
    #[cfg(windows)]
    pub(in crate::supervisor) fn fail_keeper_preparation(
        &mut self,
        f: crate::activation::ActivationFinalizationFailure,
    ) -> Result<()> {
        let matches =
            matches!(&self.state,State::PreparingKeeper(r) if r.intent==f.custody_record().intent);
        let reason = if matches {
            f.reason.clone()
        } else {
            "Keeper preparation changed original intent".into()
        };
        self.state = State::FinalizationHeld(f.into_custody());
        Err(reason)
    }
}
