//! Atomic kill switch. When engaged, no outbound traffic is allowed.

use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Default)]
pub struct KillSwitch {
    flag: AtomicBool,
}

impl KillSwitch {
    pub const fn new() -> Self {
        Self {
            flag: AtomicBool::new(false),
        }
    }

    pub fn engage(&self) {
        self.flag.store(true, Ordering::SeqCst);
    }

    pub fn disengage(&self) {
        self.flag.store(false, Ordering::SeqCst);
    }

    pub fn is_engaged(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_state_is_disengaged() {
        let ks = KillSwitch::new();
        assert!(!ks.is_engaged());
    }

    #[test]
    fn engage_disengage_round_trip() {
        let ks = KillSwitch::new();
        ks.engage();
        assert!(ks.is_engaged());
        ks.disengage();
        assert!(!ks.is_engaged());
    }

    #[test]
    fn shared_across_threads() {
        use std::sync::Arc;
        use std::thread;
        let ks = Arc::new(KillSwitch::new());
        let ks2 = ks.clone();
        let h = thread::spawn(move || ks2.engage());
        h.join().unwrap();
        assert!(ks.is_engaged());
    }
}
