//! Background session connection and handshake profile selection.

use ssp_client::{Session, SessionBuilder};
use std::sync::{Arc, Mutex};
use std::thread;

pub(crate) type SharedSession = Arc<tokio::sync::Mutex<Session>>;
pub(crate) type SessionSlot = Arc<Mutex<Option<(SharedSession, tokio::runtime::Runtime)>>>;

pub(crate) struct ConnectHandle {
    pub session_slot: SessionSlot,
    pub _cancel: Arc<()>,
}

pub(crate) fn handshake_profile() -> ((u64, u64, u64), (u64, u64, u64)) {
    if let Ok(p) = std::env::var("SLPAUTH_PROFILE") {
        match p.to_lowercase().as_str() {
            "tight" => return ((30, 60, 90), (15, 30, 60)),
            "wide" => return ((60, 240, 480), (30, 120, 240)),
            "mismatch" => return ((500, 600, 700), (250, 300, 350)),
            _ => {}
        }
    }

    #[cfg(feature = "profile-tight")]
    return ((30, 60, 90), (15, 30, 60));
    #[cfg(all(not(feature = "profile-tight"), feature = "profile-wide"))]
    return ((60, 240, 480), (30, 120, 240));
    #[cfg(all(
        not(feature = "profile-tight"),
        not(feature = "profile-wide"),
        feature = "profile-mismatch"
    ))]
    return ((500, 600, 700), (250, 300, 350));

    #[allow(unreachable_code)]
    ((60, 120, 240), (30, 60, 120))
}

pub(crate) fn spawn_connect() -> ConnectHandle {
    let session_slot: SessionSlot = Arc::new(Mutex::new(None));
    let slot = session_slot.clone();
    let cancel = Arc::new(());
    let weak_cancel = Arc::downgrade(&cancel);

    thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
        let session = rt.block_on(async {
            let ((roll_min, roll_pref, roll_max), (off_min, off_pref, off_max)) = handshake_profile();
            SessionBuilder::new()
                .set_encryption(true)
                .set_rollover_range(roll_min, roll_pref, roll_max)
                .set_offset_range(off_min, off_pref, off_max)
                .connect()
                .await
        });

        if weak_cancel.upgrade().is_some() {
            *slot.lock().unwrap() = Some((Arc::new(tokio::sync::Mutex::new(session)), rt));
        }
    });

    ConnectHandle {
        session_slot,
        _cancel: cancel,
    }
}
