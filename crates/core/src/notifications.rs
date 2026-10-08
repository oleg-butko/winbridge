//! Pure notification policy. Wall time controls quiet hours; elapsed time controls rate.
use crate::config::Result;
use winbridge_protocol::{AttentionEvent, AttentionSeverity, NotificationPreferences};

pub fn validate(p: &NotificationPreferences) -> Result<()> {
    if p.volume_percent > 100
        || !(1..=3600).contains(&p.minimum_interval_seconds)
        || p.quiet_start_minute >= 1440
        || p.quiet_end_minute >= 1440
        || (p.quiet_hours_enabled && p.quiet_start_minute == p.quiet_end_minute)
    {
        return Err("Notification settings: volume 0–100%, interval 1–3600 seconds; quiet hours require two different times within a day".into());
    }
    Ok(())
}

pub fn includes(p: &NotificationPreferences, severity: AttentionSeverity) -> bool {
    fn rank(s: AttentionSeverity) -> u8 {
        match s {
            AttentionSeverity::Info => 0,
            AttentionSeverity::Warning => 1,
            AttentionSeverity::Error => 2,
        }
    }
    rank(severity) >= rank(p.minimum_severity)
}

pub fn quiet(p: &NotificationPreferences, minute: u16) -> bool {
    if !p.quiet_hours_enabled {
        return false;
    }
    let (start, end) = (p.quiet_start_minute, p.quiet_end_minute);
    if start < end {
        minute >= start && minute < end
    } else {
        minute >= start || minute < end
    }
}

pub fn may_signal(p: &NotificationPreferences, minute: u16, elapsed_seconds: Option<u64>) -> bool {
    (p.popup_enabled || (p.sound_enabled && p.volume_percent > 0))
        && !quiet(p, minute)
        && elapsed_seconds.is_none_or(|s| s >= u64::from(p.minimum_interval_seconds))
}

/// A deferred batch keeps its cursor. Re-reading pending records suppresses an ACKed
/// event even if it was previously deferred; a restart cannot lose the batch.
pub fn plan(
    p: &NotificationPreferences,
    minute: u16,
    elapsed_seconds: Option<u64>,
    after: u64,
    scanned: u64,
    records: &[AttentionEvent],
) -> (u64, Option<AttentionEvent>) {
    let latest = records
        .iter()
        .rev()
        .find(|r| r.acknowledged_at_ms.is_none() && includes(p, r.severity));
    match latest {
        None => (scanned, None),
        Some(r) if may_signal(p, minute, elapsed_seconds) => (scanned, Some(r.clone())),
        Some(_) => (after, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quiet_midnight_boundaries_rate_and_disabled_channels() {
        let mut p = NotificationPreferences {
            quiet_hours_enabled: true,
            ..Default::default()
        };
        for m in [1320, 1439, 0, 479] {
            assert!(quiet(&p, m));
        }
        for m in [480, 1319] {
            assert!(!quiet(&p, m));
        }
        assert!(!may_signal(&p, 480, Some(29)));
        assert!(may_signal(&p, 480, Some(30)));
        assert!(may_signal(&p, 480, None));
        p.quiet_start_minute = 60;
        p.quiet_end_minute = 120;
        assert!(!quiet(&p, 59));
        assert!(quiet(&p, 60));
        assert!(!quiet(&p, 120));
        p.popup_enabled = false;
        p.volume_percent = 0;
        assert!(!may_signal(&p, 480, None));
        p.quiet_end_minute = p.quiet_start_minute;
        assert!(validate(&p).is_err());
    }
    #[test]
    fn severity_threshold_is_explicit() {
        let p = NotificationPreferences::default();
        assert!(!includes(&p, AttentionSeverity::Info));
        assert!(includes(&p, AttentionSeverity::Warning));
        assert!(includes(&p, AttentionSeverity::Error));
    }
    #[test]
    fn deferred_cursor_survives_restart_and_ack_during_quiet_removes_signal() {
        let p = NotificationPreferences {
            quiet_hours_enabled: true,
            ..Default::default()
        };
        let mut r = AttentionEvent {
            attention_id: "a".into(),
            sequence: 7,
            project_id: "p".into(),
            target: winbridge_protocol::AttentionTarget::Task {
                task_id: "t".into(),
            },
            severity: AttentionSeverity::Error,
            message: "failure".into(),
            origin: "task_outcome".into(),
            created_at_ms: 1,
            acknowledged_at_ms: None,
        };
        assert_eq!(plan(&p, 0, None, 3, 7, &[r.clone()]).0, 3);
        assert!(plan(&p, 480, None, 3, 7, &[r.clone()]).1.is_some());
        assert_eq!(plan(&p, 480, Some(1), 3, 7, &[r.clone()]).0, 3);
        r.acknowledged_at_ms = Some(2);
        let (cursor, signal) = plan(&p, 480, None, 3, 7, &[r]);
        assert_eq!(cursor, 7);
        assert!(signal.is_none());
    }
}
