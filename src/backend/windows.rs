//! Windows: toast notifications, read through UI Automation. No permission is needed.
//!
//! Toasts live in top-level windows of class `Windows.UI.Core.CoreWindow` (ShellExperienceHost);
//! run `yesyesyes --dump` while a toast is shown to check what your Windows version exposes.

use super::{Access, Backend};
use crate::core::Node;
use uiautomation::controls::ControlType;
use uiautomation::patterns::UIInvokePattern;
use uiautomation::{UIAutomation, UIElement, UITreeWalker};

const TOAST_WINDOW_CLASS: &str = "Windows.UI.Core.CoreWindow";

pub struct Platform {
    automation: Option<(UIAutomation, UITreeWalker)>,
}

impl Platform {
    pub fn new() -> Self {
        let automation = UIAutomation::new().ok().and_then(|a| {
            let walker = a.get_control_view_walker().ok()?;
            Some((a, walker))
        });
        Platform { automation }
    }
}

impl Backend for Platform {
    type Node = WinNode;

    fn access(&self, _prompt: bool) -> Access {
        match self.automation {
            Some(_) => Access::Ready,
            None => Access::Missing("UI Automation is not available on this system.".into()),
        }
    }

    fn roots(&self) -> Vec<WinNode> {
        let Some((automation, walker)) = &self.automation else { return Vec::new() };
        let Ok(desktop) = automation.get_root_element() else { return Vec::new() };
        let desktop = WinNode { element: desktop, walker: walker.clone() };
        desktop
            .children()
            .into_iter()
            .filter(|w| w.element.get_classname().is_ok_and(|c| c == TOAST_WINDOW_CLASS))
            .collect()
    }
}

#[derive(Clone)]
pub struct WinNode {
    element: UIElement,
    walker: UITreeWalker,
}

impl Node for WinNode {
    fn is_button(&self) -> bool {
        self.element.get_control_type().is_ok_and(|t| t == ControlType::Button)
    }

    fn labels(&self) -> Vec<String> {
        [self.element.get_name(), self.element.get_help_text()]
            .into_iter()
            .filter_map(Result::ok)
            .filter(|s| !s.is_empty())
            .collect()
    }

    fn children(&self) -> Vec<WinNode> {
        let mut children = Vec::new();
        let mut next = self.walker.get_first_child(&self.element).ok();
        while let Some(element) = next {
            next = self.walker.get_next_sibling(&element).ok();
            children.push(WinNode { element, walker: self.walker.clone() });
        }
        children
    }

    fn named_actions(&self) -> Vec<String> {
        Vec::new()
    }

    fn press(&self) -> bool {
        self.element.get_pattern::<UIInvokePattern>().and_then(|p| p.invoke()).is_ok()
    }

    fn perform_named(&self, _label: &str) -> bool {
        false
    }
}
