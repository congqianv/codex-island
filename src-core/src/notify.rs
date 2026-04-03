#![allow(deprecated)]

#[cfg(target_os = "macos")]
use objc2_foundation::{ns_string, NSUserNotification, NSUserNotificationCenter};

#[cfg(target_os = "macos")]
pub fn notify_attention(title: &str, body: &str) {
    let notification = NSUserNotification::new();
    notification.setTitle(Some(&*objc2_foundation::NSString::from_str(title)));
    notification.setInformativeText(Some(&*objc2_foundation::NSString::from_str(body)));
    notification.setHasActionButton(false);
    notification.setSoundName(Some(ns_string!("NSUserNotificationDefaultSoundName")));

    let center = NSUserNotificationCenter::defaultUserNotificationCenter();
    center.deliverNotification(&notification);
}

#[cfg(not(target_os = "macos"))]
pub fn notify_attention(_title: &str, _body: &str) {}
