use crate::error::{Error, Result};
use std::sync::RwLock;
use std::{os::raw::c_void, sync::Mutex};

/// # Safety
///
/// set traffic status callback.
#[no_mangle]
pub unsafe extern "C" fn overtls_set_traffic_status_callback(
    send_interval_secs: u32,
    callback: Option<unsafe extern "C" fn(*const TrafficStatus, *mut c_void)>,
    ctx: *mut c_void,
) {
    if let Ok(mut cb) = TRAFFIC_STATUS_CALLBACK.lock() {
        *cb = Some(TrafficStatusCallback(callback, ctx));
    } else {
        log::error!("set traffic status callback failed");
    }
    if send_interval_secs > 0 {
        SEND_INTERVAL_SECS.store(send_interval_secs as u64, std::sync::atomic::Ordering::Relaxed);
    }
}

#[repr(C)]
#[derive(Debug, Default, Copy, Clone, serde::Serialize, serde::Deserialize)]
pub struct TrafficStatus {
    pub tx: u64,
    pub rx: u64,
}

#[derive(Clone)]
struct TrafficStatusCallback(Option<unsafe extern "C" fn(*const TrafficStatus, *mut c_void)>, *mut c_void);

impl TrafficStatusCallback {
    unsafe fn call(self, info: &TrafficStatus) {
        if let Some(cb) = self.0 {
            cb(info, self.1);
        }
    }
}

unsafe impl Send for TrafficStatusCallback {}
unsafe impl Sync for TrafficStatusCallback {}

static TRAFFIC_STATUS_CALLBACK: Mutex<Option<TrafficStatusCallback>> = Mutex::new(None);
static SEND_INTERVAL_SECS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

lazy_static::lazy_static! {
    static ref TRAFFIC_STATUS: RwLock<TrafficStatus> = RwLock::new(TrafficStatus::default());
    static ref TIME_STAMP: RwLock<std::time::Instant> = RwLock::new(std::time::Instant::now());
}

pub(crate) fn traffic_status_update(delta_tx: usize, delta_rx: usize) -> Result<()> {
    {
        let mut traffic_status = TRAFFIC_STATUS.write().map_err(|e| Error::from(e.to_string()))?;
        traffic_status.tx += delta_tx as u64;
        traffic_status.rx += delta_rx as u64;
    }
    let old_time = { *TIME_STAMP.read().map_err(|e| Error::from(e.to_string()))? };
    let interval_secs = SEND_INTERVAL_SECS.load(std::sync::atomic::Ordering::Relaxed);
    if std::time::Instant::now().duration_since(old_time).as_secs() >= interval_secs {
        send_traffic_stat()?;
        {
            let mut time_stamp = TIME_STAMP.write().map_err(|e| Error::from(e.to_string()))?;
            *time_stamp = std::time::Instant::now();
        }
    }
    Ok(())
}

fn send_traffic_stat() -> Result<()> {
    if let Ok(cb) = TRAFFIC_STATUS_CALLBACK.lock() {
        if let Some(cb) = cb.clone() {
            let traffic_status = { *TRAFFIC_STATUS.read().map_err(|e| Error::from(e.to_string()))? };
            unsafe { cb.call(&traffic_status) };
        }
    }
    Ok(())
}
