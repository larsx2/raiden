use std::ptr::NonNull;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use block2::RcBlock;
use objc2_app_kit::{NSWorkspace, NSWorkspaceDidWakeNotification};
use objc2_foundation::{NSNotification, NSOperationQueue};

pub fn watch(woke: Arc<AtomicBool>) {
    let block = RcBlock::new(move |_: NonNull<NSNotification>| {
        woke.store(true, Ordering::SeqCst);
    });
    let center = NSWorkspace::sharedWorkspace().notificationCenter();
    let observer = unsafe {
        center.addObserverForName_object_queue_usingBlock(
            Some(NSWorkspaceDidWakeNotification),
            None,
            Some(&NSOperationQueue::mainQueue()),
            &block,
        )
    };
    std::mem::forget(observer);
}
