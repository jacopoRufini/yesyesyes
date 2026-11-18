//! Linux (GNOME): notification banners are drawn by gnome-shell, read through AT-SPI on the
//! accessibility D-Bus. Needs accessibility turned on for the session.

use super::{Access, Backend};
use crate::core::Node;
use zbus::blocking::Connection;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

const ACCESSIBLE: &str = "org.a11y.atspi.Accessible";
const ACTION: &str = "org.a11y.atspi.Action";
const REGISTRY: &str = "org.a11y.atspi.Registry";
const ROOT: &str = "/org/a11y/atspi/accessible/root";
const SHELL: &str = "gnome-shell";
const HINT: &str = "Turn on accessibility, then log out and back in: \
gsettings set org.gnome.desktop.interface toolkit-accessibility true";

pub struct Platform {
    bus: Option<Connection>,
}

impl Platform {
    pub fn new() -> Self {
        Platform { bus: accessibility_bus().ok() }
    }

    fn shell(&self) -> Option<LinuxNode> {
        let bus = self.bus.as_ref()?;
        let root = LinuxNode { bus: bus.clone(), name: REGISTRY.into(), path: OwnedObjectPath::try_from(ROOT).ok()? };
        root.children().into_iter().find(|app| app.name_property().as_deref() == Some(SHELL))
    }
}

/// The accessibility bus is separate from the session bus; the session bus tells where it is.
fn accessibility_bus() -> zbus::Result<Connection> {
    let session = Connection::session()?;
    let reply = session.call_method(Some("org.a11y.Bus"), "/org/a11y/bus", Some("org.a11y.Bus"), "GetAddress", &())?;
    let address: String = reply.body().deserialize()?;
    zbus::blocking::connection::Builder::address(address.as_str())?.build()
}

impl Backend for Platform {
    type Node = LinuxNode;

    fn access(&self, _prompt: bool) -> Access {
        match self.shell() {
            Some(_) => Access::Ready,
            None => Access::Missing(HINT.into()),
        }
    }

    fn roots(&self) -> Vec<LinuxNode> {
        self.shell().into_iter().collect()
    }
}

#[derive(Clone)]
pub struct LinuxNode {
    bus: Connection,
    name: String,
    path: OwnedObjectPath,
}

impl LinuxNode {
    fn call<B, R>(&self, interface: &str, method: &str, body: &B) -> Option<R>
    where
        B: serde::Serialize + zbus::zvariant::DynamicType,
        R: for<'d> zbus::zvariant::DynamicDeserialize<'d>,
    {
        let reply = self.bus.call_method(Some(self.name.as_str()), self.path.as_str(), Some(interface), method, body).ok()?;
        reply.body().deserialize().ok()
    }

    fn property(&self, name: &str) -> Option<String> {
        let value: OwnedValue = self.call("org.freedesktop.DBus.Properties", "Get", &(ACCESSIBLE, name))?;
        String::try_from(value).ok().filter(|s| !s.is_empty())
    }

    fn name_property(&self) -> Option<String> {
        self.property("Name")
    }
}

impl Node for LinuxNode {
    fn is_button(&self) -> bool {
        let role: Option<String> = self.call(ACCESSIBLE, "GetRoleName", &());
        matches!(role.as_deref(), Some("push button") | Some("button"))
    }

    fn labels(&self) -> Vec<String> {
        ["Name", "Description"].iter().filter_map(|p| self.property(p)).collect()
    }

    fn children(&self) -> Vec<LinuxNode> {
        let children: Vec<(String, OwnedObjectPath)> = self.call(ACCESSIBLE, "GetChildren", &()).unwrap_or_default();
        children.into_iter().map(|(name, path)| LinuxNode { bus: self.bus.clone(), name, path }).collect()
    }

    fn named_actions(&self) -> Vec<String> {
        Vec::new()
    }

    fn press(&self) -> bool {
        self.call::<_, bool>(ACTION, "DoAction", &(0i32,)).unwrap_or(false)
    }

    fn perform_named(&self, _label: &str) -> bool {
        false
    }
}
