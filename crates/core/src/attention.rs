//! Attention policy does not launch processes or notify the operating system.
use winbridge_protocol::{AttentionSeverity, AttentionSubmission, TaskStatus};

pub fn validate(request: &AttentionSubmission) -> Result<(), String> {
    for (name, value, max) in [
        ("requestId", request.request_id.as_str(), 200),
        ("idempotencyKey", request.idempotency_key.as_str(), 200),
        ("taskId", request.task_id.as_str(), 200),
        ("message", request.message.as_str(), 1024),
    ] {
        if value.trim().is_empty() || value.len() > max || value.chars().any(|c| c.is_control()) {
            return Err(format!("Attention {name} must be nonempty, at most {max} UTF-8 bytes and contain no control characters"));
        }
    }
    Ok(())
}

pub fn outcome(status: TaskStatus) -> Option<(AttentionSeverity, &'static str)> {
    match status {
        TaskStatus::Failed => Some((
            AttentionSeverity::Error,
            "Задача завершилась с ошибкой. Откройте задачу для просмотра причины и вывода.",
        )),
        TaskStatus::Interrupted | TaskStatus::Indeterminate => Some((
            AttentionSeverity::Warning,
            "Исход задачи требует проверки. Автоматический повтор не выполняется.",
        )),
        _ => None,
    }
}
