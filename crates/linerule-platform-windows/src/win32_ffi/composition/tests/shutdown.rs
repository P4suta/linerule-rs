use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use windows::Foundation::TypedEventHandler;
use windows::System::{DispatcherQueue, DispatcherQueueHandler};

use super::super::{DISPATCHER_QUEUE, ensure_dispatcher_queue};
use crate::{DesktopRuntime, LaunchIntent, RuntimeOptions};
use linerule_core::Preferences;

#[test]
fn failed_runtime_drains_owned_composition_queue_before_returning() {
    ensure_dispatcher_queue().expect("fixture dispatcher");
    let controller = DISPATCHER_QUEUE.with(|cell| cell.get().expect("owned controller").clone());
    let queue = controller.DispatcherQueue().expect("owned queue");
    let work_completed = Arc::new(AtomicBool::new(false));
    let work_observed = Arc::clone(&work_completed);
    let work = DispatcherQueueHandler::new(move || {
        work_observed.store(true, Ordering::SeqCst);
        Ok(())
    });
    assert!(queue.TryEnqueue(&work).expect("queue fixture work"));
    let shutdown_completed = Arc::new(AtomicBool::new(false));
    let shutdown_observed = Arc::clone(&shutdown_completed);
    let _token = queue
        .ShutdownCompleted(&TypedEventHandler::<
            DispatcherQueue,
            windows::core::IInspectable,
        >::new(move |_, _| {
            shutdown_observed.store(true, Ordering::SeqCst);
            Ok(())
        }))
        .expect("observe queue shutdown");

    let result = DesktopRuntime::run(
        LaunchIntent::Resident,
        RuntimeOptions::new(Preferences {
            schema_version: 0,
            ..Preferences::default()
        }),
    );
    let work_finished_at_return = work_completed.load(Ordering::SeqCst);
    let shutdown_finished_at_return = shutdown_completed.load(Ordering::SeqCst);
    let accepted_after_return = queue.TryEnqueue(&work).expect("query queue acceptance");

    if !shutdown_finished_at_return {
        let action = controller.ShutdownQueueAsync().expect("fixture cleanup");
        let deadline = Instant::now() + Duration::from_secs(2);
        while action.Status().expect("cleanup status").0 == 0 && Instant::now() < deadline {
            let _ = crate::win32_ffi::core::pump_ready(10).expect("fixture message pump");
        }
        action
            .GetResults()
            .expect("fixture queue was fully drained");
    }

    assert!(matches!(
        result,
        Err(crate::error::PlatformError::Preferences(_))
    ));
    assert!(
        work_finished_at_return,
        "runtime returned before queued work completed"
    );
    assert!(
        shutdown_finished_at_return,
        "runtime returned before the shutdown callback"
    );
    assert!(
        !accepted_after_return,
        "runtime left its queue accepting work"
    );
}
