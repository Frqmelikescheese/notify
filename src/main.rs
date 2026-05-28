mod config;
mod dbus;
mod window;

use gtk4 as gtk;
use gtk::prelude::*;
use std::env;
use std::rc::Rc;
use std::cell::RefCell;
use tokio::sync::mpsc;
use crate::window::NotificationWindow;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("Starting notify...");
    env_logger::init();

    let app = gtk::Application::builder()
        .application_id("io.github.notify.daemon")
        .build();

    let (tx, rx) = mpsc::unbounded_channel();
    let rx_holder = Rc::new(RefCell::new(Some(rx)));

    // Start DBus server
    tokio::spawn(async move {
        if let Err(e) = dbus::start_dbus_server(tx).await {
            eprintln!("DBus server error: {}", e);
        }
    });

    app.connect_activate(move |app| {
        std::mem::forget(app.hold());
        eprintln!("Application activated");
        let rx_opt = rx_holder.borrow_mut().take();
        if rx_opt.is_none() {
            return; // Already initialized
        }
        let mut rx = rx_opt.unwrap();

        let config_path = env::var("NOTIFY_CONFIG")
            .unwrap_or_else(|_| "config.toml".to_string());
        
        let config = config::Config::load(&config_path)
            .unwrap_or_else(|e| {
                eprintln!("Failed to load config: {}. Using default.", e);
                panic!("Config file not found!");
            });

        let css_path = env::var("NOTIFY_CSS")
            .unwrap_or_else(|_| "style.css".to_string());
        
        let provider = gtk::CssProvider::new();
        if let Ok(css) = std::fs::read_to_string(css_path) {
            provider.load_from_data(&css);
            gtk::style_context_add_provider_for_display(
                &gdk4::Display::default().expect("Could not connect to a display."),
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }

        let win_rc = NotificationWindow::new(app, config);

        let win_clone = win_rc.clone();
        glib::MainContext::default().spawn_local(async move {
            while let Some(notif) = rx.recv().await {
                win_clone.borrow_mut().show_notification(notif);
            }
        });

        win_rc.borrow().window.present();
    });

    app.run_with_args::<&str>(&[]);
    Ok(())
}
