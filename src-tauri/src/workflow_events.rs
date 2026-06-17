use crate::domain::{WorkflowEvent, WorkflowEventLevel, WorkflowEventPhase};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
#[cfg(not(test))]
use tauri::{AppHandle, Emitter};

#[cfg(not(test))]
pub const WORKFLOW_EVENT_NAME: &str = "rush-patch://workflow-event";
const PROGRESS_THROTTLE_WINDOW: Duration = Duration::from_millis(400);

#[derive(Clone)]
pub struct WorkflowReporter {
    target: ReporterTarget,
    debug_enabled: bool,
    progress_state: Arc<Mutex<HashMap<String, ProgressThrottleState>>>,
}

#[derive(Clone)]
enum ReporterTarget {
    Noop,
    #[cfg(not(test))]
    Tauri(AppHandle),
    #[cfg(test)]
    Collector(Arc<Mutex<Vec<WorkflowEvent>>>),
}

#[derive(Clone)]
struct ProgressThrottleState {
    last_bucket: usize,
    last_at: Instant,
}

impl WorkflowReporter {
    pub fn noop() -> Self {
        Self {
            target: ReporterTarget::Noop,
            debug_enabled: false,
            progress_state: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    #[cfg(not(test))]
    pub fn tauri(app: AppHandle, debug_enabled: bool) -> Self {
        Self {
            target: ReporterTarget::Tauri(app),
            debug_enabled,
            progress_state: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    #[cfg(test)]
    pub fn collector(debug_enabled: bool) -> (Self, Arc<Mutex<Vec<WorkflowEvent>>>) {
        let events = Arc::new(Mutex::new(Vec::new()));
        (
            Self {
                target: ReporterTarget::Collector(events.clone()),
                debug_enabled,
                progress_state: Arc::new(Mutex::new(HashMap::new())),
            },
            events,
        )
    }

    pub fn info(
        &self,
        phase: WorkflowEventPhase,
        message: impl Into<String>,
        detail: Option<String>,
    ) {
        self.emit(event(
            phase,
            WorkflowEventLevel::Info,
            None,
            message.into(),
            None,
            None,
            detail,
            None,
        ));
    }

    pub fn info_key(
        &self,
        phase: WorkflowEventPhase,
        message_key: impl Into<String>,
        fallback_message: impl Into<String>,
        detail: Option<String>,
    ) {
        self.emit(event(
            phase,
            WorkflowEventLevel::Info,
            Some(message_key.into()),
            fallback_message.into(),
            None,
            None,
            detail,
            None,
        ));
    }

    pub fn warn(
        &self,
        phase: WorkflowEventPhase,
        message: impl Into<String>,
        detail: Option<String>,
    ) {
        self.emit(event(
            phase,
            WorkflowEventLevel::Warn,
            None,
            message.into(),
            None,
            None,
            detail,
            None,
        ));
    }

    pub fn warn_key(
        &self,
        phase: WorkflowEventPhase,
        message_key: impl Into<String>,
        fallback_message: impl Into<String>,
        detail: Option<String>,
    ) {
        self.emit(event(
            phase,
            WorkflowEventLevel::Warn,
            Some(message_key.into()),
            fallback_message.into(),
            None,
            None,
            detail,
            None,
        ));
    }

    pub fn error(
        &self,
        phase: WorkflowEventPhase,
        message: impl Into<String>,
        detail: Option<String>,
    ) {
        self.emit(event(
            phase,
            WorkflowEventLevel::Error,
            None,
            message.into(),
            None,
            None,
            detail,
            None,
        ));
    }

    pub fn error_key(
        &self,
        phase: WorkflowEventPhase,
        message_key: impl Into<String>,
        fallback_message: impl Into<String>,
        detail: Option<String>,
    ) {
        self.emit(event(
            phase,
            WorkflowEventLevel::Error,
            Some(message_key.into()),
            fallback_message.into(),
            None,
            None,
            detail,
            None,
        ));
    }

    pub fn debug<I, K, V>(
        &self,
        phase: WorkflowEventPhase,
        message: impl Into<String>,
        detail: Option<String>,
        debug: I,
    ) where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        if !self.debug_enabled {
            return;
        }

        self.emit(event(
            phase,
            WorkflowEventLevel::Debug,
            None,
            message.into(),
            None,
            None,
            detail,
            Some(
                debug
                    .into_iter()
                    .map(|(k, v)| (k.into(), v.into()))
                    .collect(),
            ),
        ));
    }

    pub fn progress(
        &self,
        phase: WorkflowEventPhase,
        message: impl Into<String>,
        current: usize,
        total: usize,
        detail: Option<String>,
    ) {
        self.emit(event(
            phase,
            WorkflowEventLevel::Info,
            None,
            message.into(),
            Some(current),
            Some(total),
            detail,
            None,
        ));
    }

    pub fn progress_key(
        &self,
        phase: WorkflowEventPhase,
        message_key: impl Into<String>,
        fallback_message: impl Into<String>,
        current: usize,
        total: usize,
        detail: Option<String>,
    ) {
        self.emit(event(
            phase,
            WorkflowEventLevel::Info,
            Some(message_key.into()),
            fallback_message.into(),
            Some(current),
            Some(total),
            detail,
            None,
        ));
    }

    pub fn progress_throttled(
        &self,
        throttle_key: &str,
        phase: WorkflowEventPhase,
        message: impl Into<String>,
        current: usize,
        total: usize,
        detail: Option<String>,
    ) {
        if !self.should_emit_progress(throttle_key, current, total) {
            return;
        }
        self.progress(phase, message, current, total, detail);
    }

    pub fn progress_throttled_key(
        &self,
        throttle_key: &str,
        phase: WorkflowEventPhase,
        message_key: impl Into<String>,
        fallback_message: impl Into<String>,
        current: usize,
        total: usize,
        detail: Option<String>,
    ) {
        if !self.should_emit_progress(throttle_key, current, total) {
            return;
        }
        self.progress_key(
            phase,
            message_key,
            fallback_message,
            current,
            total,
            detail,
        );
    }

    fn should_emit_progress(&self, throttle_key: &str, current: usize, total: usize) -> bool {
        if total <= 1 || current == 0 || current >= total {
            return true;
        }

        let bucket = current.saturating_mul(100) / total.max(1);
        let Ok(mut states) = self.progress_state.lock() else {
            return true;
        };
        let now = Instant::now();
        let state = states
            .entry(throttle_key.to_owned())
            .or_insert(ProgressThrottleState {
                last_bucket: usize::MAX,
                last_at: now.checked_sub(PROGRESS_THROTTLE_WINDOW).unwrap_or(now),
            });
        if state.last_bucket != bucket
            || now.duration_since(state.last_at) >= PROGRESS_THROTTLE_WINDOW
        {
            state.last_bucket = bucket;
            state.last_at = now;
            return true;
        }
        false
    }

    fn emit(&self, workflow_event: WorkflowEvent) {
        match &self.target {
            ReporterTarget::Noop => {}
            #[cfg(not(test))]
            ReporterTarget::Tauri(app) => {
                let _ = app.emit(WORKFLOW_EVENT_NAME, workflow_event);
            }
            #[cfg(test)]
            ReporterTarget::Collector(events) => {
                if let Ok(mut items) = events.lock() {
                    items.push(workflow_event);
                }
            }
        }
    }
}

fn event(
    phase: WorkflowEventPhase,
    level: WorkflowEventLevel,
    message_key: Option<String>,
    message: String,
    current: Option<usize>,
    total: Option<usize>,
    detail: Option<String>,
    debug: Option<BTreeMap<String, String>>,
) -> WorkflowEvent {
    WorkflowEvent {
        phase,
        level,
        message_key,
        message,
        current,
        total,
        detail,
        timestamp_ms: timestamp_ms(),
        debug,
    }
}

fn timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noop_reporter_accepts_all_event_types() {
        let reporter = WorkflowReporter::noop();
        reporter.info(WorkflowEventPhase::Catalog, "cache ready", None);
        reporter.warn(WorkflowEventPhase::Translate, "retrying request", None);
        reporter.error(WorkflowEventPhase::Translate, "request failed", None);
        reporter.debug(
            WorkflowEventPhase::Translate,
            "request details",
            None,
            [("model", "gpt-4.1-mini")],
        );
        reporter.progress(
            WorkflowEventPhase::Patch,
            "writing files",
            1,
            4,
            Some("file 1/4".to_owned()),
        );
    }

    #[test]
    fn workflow_event_round_trips_via_json() {
        let workflow_event = WorkflowEvent {
            phase: WorkflowEventPhase::Translate,
            level: WorkflowEventLevel::Debug,
            message_key: None,
            message: "batch retry".to_owned(),
            current: Some(2),
            total: Some(5),
            detail: Some("attempt 2/3".to_owned()),
            timestamp_ms: 42,
            debug: Some(BTreeMap::from([(
                "model".to_owned(),
                "gpt-4.1-mini".to_owned(),
            )])),
        };

        let payload = serde_json::to_string(&workflow_event).expect("serialize event");
        let decoded: WorkflowEvent = serde_json::from_str(&payload).expect("deserialize event");

        assert_eq!(decoded.phase, WorkflowEventPhase::Translate);
        assert_eq!(decoded.level, WorkflowEventLevel::Debug);
        assert_eq!(decoded.current, Some(2));
        assert_eq!(
            decoded.debug.as_ref().and_then(|debug| debug.get("model")),
            Some(&"gpt-4.1-mini".to_owned())
        );
    }
}
