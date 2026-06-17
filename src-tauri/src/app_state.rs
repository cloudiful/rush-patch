use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[derive(Debug, Clone, Default)]
pub struct CancellationFlag {
    cancelled: Arc<AtomicBool>,
}

impl CancellationFlag {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn reset(&self) {
        self.cancelled.store(false, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

#[derive(Debug, Default)]
pub struct AppState {
    translation_cancel: CancellationFlag,
}

impl AppState {
    pub fn reset_translation_cancel(&self) -> CancellationFlag {
        self.translation_cancel.reset();
        self.translation_cancel.clone()
    }

    pub fn cancel_translation(&self) {
        self.translation_cancel.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_and_cancel_share_same_translation_flag() {
        let state = AppState::default();
        let flag = state.reset_translation_cancel();

        assert!(!flag.is_cancelled());

        state.cancel_translation();
        assert!(flag.is_cancelled());

        let reset = state.reset_translation_cancel();
        assert!(!flag.is_cancelled());
        assert!(!reset.is_cancelled());
    }
}
