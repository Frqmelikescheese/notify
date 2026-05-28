use gtk4 as gtk;
use gtk::prelude::*;
use gtk4_layer_shell::{Edge, Layer, LayerShell};
use std::rc::Rc;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use crate::config::{Config, State};
use crate::dbus::Notification;

pub struct NotificationWindow {
    pub window: gtk::Window,
    config: Config,
    #[allow(dead_code)]
    current_state_name: String,
    animation_start_time: Option<u64>,
    start_props: State,
    target_props: State,
    icon: gtk::Image,
    summary: gtk::Label,
    body: gtk::Label,
    text_box: gtk::Box,
    container: gtk::Box,
    
    pending_summary: String,
    pending_body: String,
}

fn resolve_icon_file(name: &str) -> Option<PathBuf> {
    let name = name.trim();
    if name.is_empty() { return None; }

    // file:// URI → strip scheme and use as path
    if name.starts_with("file://") {
        let path = Path::new(&name[7..]);
        return path.exists().then(|| path.to_path_buf());
    }

    // ~ expansion → replace with $HOME
    if name.starts_with('~') {
        let home = std::env::var("HOME").unwrap_or_default();
        let path_str = name.replacen('~', &home, 1);
        let path = Path::new(&path_str);
        return path.exists().then(|| path.to_path_buf());
    }

    // Path-like (contains '/') → use as-is (relative or absolute)
    if name.contains('/') {
        let path = Path::new(name);
        return path.exists().then(|| path.to_path_buf());
    }

    None
}

impl NotificationWindow {
    pub fn new(app: &gtk::Application, config: Config) -> Rc<RefCell<Self>> {
        let window = gtk::Window::builder()
            .application(app)
            .default_width(1)
            .default_height(1)
            .css_classes(["notification-window"])
            .build();

        window.init_layer_shell();
        window.set_layer(Layer::Overlay);
        window.set_namespace("notify");

        if let Some(ref monitor_name) = config.global.monitor {
            let display = gdk4::Display::default().expect("Could not connect to a display.");
            let monitors = display.monitors();
            for i in 0..monitors.n_items() {
                if let Some(monitor) = monitors.item(i).and_then(|m| m.downcast::<gdk4::Monitor>().ok()) {
                    if let Some(name) = monitor.connector() {
                        if name == *monitor_name {
                            window.set_monitor(&monitor);
                            break;
                        }
                    }
                }
            }
        }

        for anchor in &config.global.anchor {
            match anchor.as_str() {
                "top" => window.set_anchor(Edge::Top, true),
                "bottom" => window.set_anchor(Edge::Bottom, true),
                "left" => window.set_anchor(Edge::Left, true),
                "right" => window.set_anchor(Edge::Right, true),
                _ => {}
            }
        }

        window.set_margin(Edge::Top, config.global.margin_top);
        window.set_margin(Edge::Bottom, config.global.margin_bottom);
        window.set_margin(Edge::Left, config.global.margin_left);
        window.set_margin(Edge::Right, config.global.margin_right);

        let container = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .css_classes(["container"])
            .spacing(10)
            .build();

        let icon = gtk::Image::builder()
            .pixel_size(48)
            .css_classes(["icon"])
            .build();

        let text_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .valign(gtk::Align::Center)
            .css_classes(["text-box"])
            .build();

        let summary = gtk::Label::builder()
            .css_classes(["summary"])
            .halign(gtk::Align::Start)
            .build();

        let body = gtk::Label::builder()
            .css_classes(["body"])
            .halign(gtk::Align::Start)
            .build();

        text_box.append(&summary);
        text_box.append(&body);
        container.append(&icon);
        container.append(&text_box);
        window.set_child(Some(&container));

        let initial_state = config.states.get("idle").cloned().unwrap_or(State {
            width: 0, height: 0, opacity: 0.0, margin_top: 0, border_radius: 0,
            duration_ms: 0, wait_ms: 0, easing: "linear".to_string(), next_state: None,
        });

        let win_rc = Rc::new(RefCell::new(Self {
            window,
            config,
            current_state_name: "idle".to_string(),
            animation_start_time: None,
            start_props: initial_state.clone(),
            target_props: initial_state,
            icon,
            summary,
            body,
            text_box,
            container,
            pending_summary: String::new(),
            pending_body: String::new(),
        }));

        let win_clone = win_rc.clone();
        let window_handle = win_rc.borrow().window.clone();
        window_handle.add_tick_callback(move |_, clock| {
            win_clone.borrow_mut().tick(clock.frame_time() as u64 / 1000);
            glib::ControlFlow::Continue
        });

        win_rc
    }

    pub fn show_notification(&mut self, notif: Notification) {
        eprintln!("Showing notification: {} from {}", notif.summary, notif.app_name);
        self.update_icon(&notif);

        self.pending_summary = notif.summary;
        self.pending_body = notif.body;

        if self.current_state_name == "idle" {
            self.summary.set_text(&notif.app_name);
            self.body.set_text("");
            self.transition_to("pop_in");
        } else {
            match self.current_state_name.as_str() {
                "expand" => {
                    self.summary.set_text(&self.pending_summary);
                    self.body.set_text(&self.pending_body);
                }
                "collapse_to_circle" | "pop_out" => {
                    self.transition_to("expand");
                }
                _ => {}
            }
        }
    }

    fn update_icon(&mut self, notif: &Notification) {
        // Priority 1: image-path hint or app_icon field
        let icon_id = notif.hints.get("image-path")
            .and_then(|v| v.downcast_ref::<String>().ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .or_else(|| {
                let s = notif.app_icon.trim().to_string();
                if s.is_empty() { None } else { Some(s) }
            });

        if let Some(ref icon_id) = icon_id {
            // Resolve as file path (URI, ~, absolute/relative path)
            if let Some(path) = resolve_icon_file(icon_id) {
                eprintln!("Loading icon from file: {:?}", path);
                self.icon.set_from_file(Some(&path));
                return;
            }
            // Search icon_path directories with extension probing
            if let Some(path) = self.search_icon_path(icon_id) {
                eprintln!("Found icon '{}' in icon_path: {:?}", icon_id, path);
                self.icon.set_from_file(Some(&path));
                return;
            }
            // Use as icon name via GTK theme
            self.icon.set_icon_name(Some(icon_id));
            return;
        }

        // Priority 2: desktop-entry hint → use as icon name (Discord, etc.)
        if let Some(desktop_id) = notif.hints.get("desktop-entry")
            .and_then(|v| v.downcast_ref::<String>().ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
        {
            if let Some(path) = self.search_icon_path(&desktop_id) {
                self.icon.set_from_file(Some(&path));
                return;
            }
            self.icon.set_icon_name(Some(&desktop_id));
            return;
        }

        // Priority 3: app_name → use as icon name
        if !notif.app_name.is_empty() {
            if let Some(path) = self.search_icon_path(&notif.app_name) {
                self.icon.set_from_file(Some(&path));
                return;
            }
            self.icon.set_icon_name(Some(&notif.app_name));
            return;
        }

        // Fallback: generic information icon
        self.icon.set_icon_name(Some("dialog-information"));
    }

    fn search_icon_path(&self, name: &str) -> Option<PathBuf> {
        let icon_path = self.config.global.icon_path.as_ref()?;
        let suffixes = [".png", ".svg", ".svgz", ".xpm", ".ico"];
        for dir in icon_path.split(':') {
            let dir = dir.trim();
            if dir.is_empty() { continue; }
            for suffix in &suffixes {
                let icon_file = Path::new(dir).join(format!("{}{}", name, suffix));
                if icon_file.exists() {
                    return Some(icon_file);
                }
            }
        }
        None
    }

    fn transition_to(&mut self, state_name: &str) {
        if let Some(target) = self.config.states.get(state_name).cloned() {
            self.start_props = self.target_props.clone();
            self.target_props = target;
            self.current_state_name = state_name.to_string();
            self.animation_start_time = None;

            match state_name {
                "expand" => {
                    self.container.add_css_class("expanded");
                    self.text_box.set_visible(true);
                    self.summary.set_text(&self.pending_summary);
                    self.body.set_text(&self.pending_body);
                }
                "collapse_to_circle" => {
                    self.container.remove_css_class("expanded");
                    self.text_box.set_visible(false);
                }
                "idle" => {
                    self.container.remove_css_class("expanded");
                    self.text_box.set_visible(false);
                    self.window.set_visible(false);
                }
                "pop_in" => {
                    self.text_box.set_visible(true);
                    self.window.set_visible(true);
                    self.window.present();
                }
                _ => {}
            }
        }
    }

    fn tick(&mut self, now_ms: u64) {
        if self.animation_start_time.is_none() {
            self.animation_start_time = Some(now_ms);
        }

        let elapsed = now_ms - self.animation_start_time.unwrap();
        let duration = self.target_props.duration_ms;

        if elapsed < duration {
            let t = elapsed as f64 / duration as f64;
            let eased_t = self.apply_easing(t, &self.target_props.easing);
            self.apply_properties(eased_t);
        } else {
            self.apply_properties(1.0);
            
            let wait_elapsed = elapsed - duration;
            if wait_elapsed >= self.target_props.wait_ms {
                if let Some(next) = self.target_props.next_state.clone() {
                    self.transition_to(&next);
                }
            }
        }
    }

    fn apply_easing(&self, t: f64, easing: &str) -> f64 {
        match easing {
            "ease-in" => t * t,
            "ease-out" => t * (2.0 - t),
            "ease-in-out" => if t < 0.5 { 2.0 * t * t } else { -1.0 + (4.0 - 2.0 * t) * t },
            _ => t,
        }
    }

    fn apply_properties(&mut self, t: f64) {
        let w = self.interpolate(self.start_props.width, self.target_props.width, t);
        let h = self.interpolate(self.start_props.height, self.target_props.height, t);
        let opacity = self.interpolate_f64(self.start_props.opacity, self.target_props.opacity, t);
        let margin_top = self.interpolate(self.start_props.margin_top, self.target_props.margin_top, t);
        
        self.window.set_default_size(w as i32, h as i32);
        self.container.set_opacity(opacity);
        self.window.set_margin(Edge::Top, margin_top);
    }

    fn interpolate(&self, start: i32, end: i32, t: f64) -> i32 {
        (start as f64 + (end - start) as f64 * t) as i32
    }

    fn interpolate_f64(&self, start: f64, end: f64, t: f64) -> f64 {
        start + (end - start) * t
    }
}
