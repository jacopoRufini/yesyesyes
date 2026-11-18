//! macOS: the Notification Center process, read through the Accessibility API (AXUIElement).

use super::{Access, Backend};
use crate::core::Node;
use core_foundation::array::{CFArray, CFArrayRef};
use core_foundation::base::{CFType, CFTypeRef, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::string::{CFString, CFStringRef};
use objc2_app_kit::NSRunningApplication;
use objc2_foundation::NSString;
use std::ffi::c_void;
use std::ptr;

type AXUIElementRef = *const c_void;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
    fn AXUIElementCopyAttributeValue(element: AXUIElementRef, attribute: CFStringRef, value: *mut CFTypeRef) -> i32;
    fn AXUIElementCopyActionNames(element: AXUIElementRef, names: *mut CFArrayRef) -> i32;
    fn AXUIElementPerformAction(element: AXUIElementRef, action: CFStringRef) -> i32;
    fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> bool;
}

const NOTIFICATION_CENTER: &str = "com.apple.notificationcenterui";

pub struct Platform;

impl Platform {
    pub fn new() -> Self {
        Platform
    }
}

impl Backend for Platform {
    type Node = MacNode;

    fn access(&self, prompt: bool) -> Access {
        let options = CFDictionary::from_CFType_pairs(&[(
            CFString::new("AXTrustedCheckOptionPrompt").as_CFType(),
            CFBoolean::from(prompt).as_CFType(),
        )]);
        if unsafe { AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef()) } {
            Access::Ready
        } else {
            Access::Missing(
                "Allow yesyesyes in System Settings > Privacy & Security > Accessibility, then restart it.".into(),
            )
        }
    }

    fn roots(&self) -> Vec<MacNode> {
        let apps = NSRunningApplication::runningApplicationsWithBundleIdentifier(&NSString::from_str(NOTIFICATION_CENTER));
        apps.iter()
            .map(|app| {
                let element = unsafe { AXUIElementCreateApplication(app.processIdentifier()) };
                MacNode(unsafe { CFType::wrap_under_create_rule(element) })
            })
            .collect()
    }
}

#[derive(Clone)]
pub struct MacNode(CFType);

impl MacNode {
    fn attribute(&self, name: &str) -> Option<CFType> {
        let mut value: CFTypeRef = ptr::null();
        let name = CFString::new(name);
        let error = unsafe { AXUIElementCopyAttributeValue(self.0.as_CFTypeRef(), name.as_concrete_TypeRef(), &mut value) };
        (error == 0 && !value.is_null()).then(|| unsafe { CFType::wrap_under_create_rule(value) })
    }

    fn string(&self, name: &str) -> Option<String> {
        self.attribute(name)?.downcast::<CFString>().map(|s| s.to_string())
    }

    /// Full AX names of the custom actions, e.g. "Name:Allow once\nTarget:0x0\nSelector:(null)".
    fn raw_actions(&self) -> Vec<String> {
        let mut names: CFArrayRef = ptr::null();
        if unsafe { AXUIElementCopyActionNames(self.0.as_CFTypeRef(), &mut names) } != 0 || names.is_null() {
            return Vec::new();
        }
        let names: CFArray<CFString> = unsafe { CFArray::wrap_under_create_rule(names) };
        names.iter().map(|n| n.to_string()).collect()
    }

    fn perform(&self, action: &str) -> bool {
        let action = CFString::new(action);
        unsafe { AXUIElementPerformAction(self.0.as_CFTypeRef(), action.as_concrete_TypeRef()) == 0 }
    }
}

fn custom_label(raw: &str) -> Option<String> {
    raw.strip_prefix("Name:").map(|rest| rest.lines().next().unwrap_or("").to_string())
}

impl Node for MacNode {
    fn is_button(&self) -> bool {
        self.string("AXRole").as_deref() == Some("AXButton")
    }

    fn labels(&self) -> Vec<String> {
        ["AXTitle", "AXDescription", "AXValue"].iter().filter_map(|a| self.string(a)).collect()
    }

    fn children(&self) -> Vec<MacNode> {
        let Some(value) = self.attribute("AXChildren") else { return Vec::new() };
        if value.type_of() != CFArray::<CFType>::type_id() {
            return Vec::new();
        }
        let array: CFArray<CFType> = unsafe { CFArray::wrap_under_get_rule(value.as_CFTypeRef() as CFArrayRef) };
        array.iter().map(|child| MacNode(child.clone())).collect()
    }

    fn named_actions(&self) -> Vec<String> {
        self.raw_actions().iter().filter_map(|a| custom_label(a)).collect()
    }

    fn press(&self) -> bool {
        self.perform("AXPress")
    }

    fn perform_named(&self, label: &str) -> bool {
        let raw = self.raw_actions().into_iter().find(|a| custom_label(a).as_deref() == Some(label));
        raw.is_some_and(|raw| self.perform(&raw))
    }
}

#[cfg(test)]
mod tests {
    use super::custom_label;

    #[test]
    fn reads_the_label_of_a_custom_action() {
        assert_eq!(custom_label("Name:Allow once\nTarget:0x0\nSelector:(null)").as_deref(), Some("Allow once"));
        assert_eq!(custom_label("AXPress"), None);
    }
}
