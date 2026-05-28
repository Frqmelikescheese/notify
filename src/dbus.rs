use zbus::{interface, Connection};
use zbus::names::{BusName, WellKnownName};
use std::collections::HashMap;
use tokio::sync::mpsc;
use zbus::zvariant::OwnedValue;
use zbus::fdo::RequestNameFlags;

#[derive(Debug)]
pub struct Notification {
    pub app_name: String,
    pub id: u32,
    pub app_icon: String,
    pub summary: String,
    pub body: String,
    pub hints: HashMap<String, OwnedValue>,
    pub expire_timeout: i32,
}

pub struct NotificationServer {
    tx: mpsc::UnboundedSender<Notification>,
}

#[interface(name = "org.freedesktop.Notifications")]
impl NotificationServer {
    async fn notify(
        &self,
        app_name: String,
        replaces_id: u32,
        app_icon: String,
        summary: String,
        body: String,
        _actions: Vec<String>,
        hints: HashMap<String, zbus::zvariant::Value<'_>>,
        expire_timeout: i32,
    ) -> u32 {
        eprintln!("Received notification: {} - {}", summary, body);
        eprintln!("  app_name={} app_icon={} hints={:?}", app_name, app_icon, hints);
        let notification = Notification {
            app_name,
            id: replaces_id,
            app_icon,
            summary,
            body,
            hints: hints.into_iter().map(|(k, v)| (k, v.try_to_owned().unwrap())).collect(),
            expire_timeout,
        };

        let _ = self.tx.send(notification);
        1
    }

    fn get_capabilities(&self) -> Vec<String> {
        vec!["body".to_string(), "icon-static".to_string()]
    }

    fn get_server_information(&self) -> (String, String, String, String) {
        (
            "notify".to_string(),
            "notify-project".to_string(),
            "0.1.0".to_string(),
            "1.2".to_string(),
        )
    }

    fn close_notification(&self, _id: u32) {}
}

pub async fn start_dbus_server(tx: mpsc::UnboundedSender<Notification>) -> Result<(), Box<dyn std::error::Error>> {
    let server = NotificationServer { tx };

    let conn = Connection::session().await?;

    // Try to take over the notification service name
    match conn.request_name_with_flags(
        "org.freedesktop.Notifications",
        RequestNameFlags::ReplaceExisting | RequestNameFlags::DoNotQueue,
    ).await {
        Ok(reply) => {
            eprintln!("Acquired notification name: {:?}", reply);
        }
        Err(e) => {
            // NameTaken means the name is owned by someone who doesn't allow replacement
            eprintln!("Could not acquire notification service name: {}", e);
            if let Ok(dbus_proxy) = zbus::fdo::DBusProxy::builder(&conn).build().await {
                if let Ok(name) = WellKnownName::from_static_str("org.freedesktop.Notifications") {
                    if let Ok(owner) = dbus_proxy.get_name_owner(name.into()).await {
                        eprintln!("Current owner: {}", owner);
                        if let Ok(pid) = dbus_proxy.get_connection_unix_process_id(BusName::from(owner)).await {
                            eprintln!("Owner PID: {} — kill it or disable its notification component", pid);
                        }
                    }
                }
            }
            return Err(e.into());
        }
    }

    conn.object_server()
        .at("/org/freedesktop/Notifications", server)
        .await?;

    eprintln!("Notification service ready");

    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(3600)).await;
    }
}
