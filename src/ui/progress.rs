use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use std::cell::Cell;

glib::wrapper! {
    pub struct UsageProgressBar(ObjectSubclass<imp::UsageProgressBarPriv>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl UsageProgressBar {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn set_progress(&self, progress: f64) {
        self.imp().progress.set(progress.clamp(0.0, 1.0));
        self.queue_draw();
    }

    pub fn progress(&self) -> f64 {
        self.imp().progress.get()
    }

    pub fn set_label(&self, label: &str) {
        self.imp().label.replace(label.to_string());
        self.queue_draw();
    }

    pub fn label(&self) -> String {
        self.imp().label.borrow().clone()
    }

    pub fn set_colors(&self, accent: gdk::RGBA, trough: gdk::RGBA) {
        let imp = self.imp();
        imp.accent.replace(accent);
        imp.trough.replace(trough);
        self.queue_draw();
    }

    pub fn set_pace_marker(&self, marker_progress: Option<f64>, is_deficit: bool) {
        let imp = self.imp();
        imp.pace_marker.set(marker_progress.unwrap_or(-1.0));
        imp.pace_deficit.set(is_deficit);
        self.queue_draw();
    }
}

impl Default for UsageProgressBar {
    fn default() -> Self {
        Self::new()
    }
}

fn draw_rounded_bar(
    snapshot: &gtk4::Snapshot,
    width: f32,
    height: f32,
    radius: f32,
    color: gtk4::gdk::RGBA,
) {
    let rect = gtk4::graphene::Rect::new(0.0, 0.0, width, height);
    let corner = gtk4::graphene::Size::new(radius, radius);
    let rounded = gtk4::gsk::RoundedRect::new(rect, corner, corner, corner, corner);
    snapshot.push_rounded_clip(&rounded);
    snapshot.append_color(&color, &rect);
    snapshot.pop();
}

mod imp {
    use super::*;
    use std::cell::RefCell;

    pub struct UsageProgressBarPriv {
        pub progress: Cell<f64>,
        pub label: RefCell<String>,
        pub accent: RefCell<gdk::RGBA>,
        pub trough: RefCell<gdk::RGBA>,
        pub pace_marker: Cell<f64>,
        pub pace_deficit: Cell<bool>,
    }

    impl Default for UsageProgressBarPriv {
        fn default() -> Self {
            Self {
                progress: Cell::new(0.0),
                label: RefCell::new(String::new()),
                accent: RefCell::new(gdk::RGBA::new(0.96, 0.65, 0.14, 0.85)),
                trough: RefCell::new(gdk::RGBA::new(0.25, 0.25, 0.25, 0.2)),
                pace_marker: Cell::new(-1.0),
                pace_deficit: Cell::new(false),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for UsageProgressBarPriv {
        const NAME: &'static str = "ClaudeBarUsageProgressBar";
        type Type = super::UsageProgressBar;
        type ParentType = gtk4::Widget;
    }

    impl ObjectImpl for UsageProgressBarPriv {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.set_height_request(5);
            obj.add_css_class("usage-progress-bar");
        }
    }

    impl WidgetImpl for UsageProgressBarPriv {
        fn snapshot(&self, snapshot: &gtk4::Snapshot) {
            let widget = self.obj();
            let width = widget.width() as f64;
            let height = widget.height() as f64;

            if width <= 0.0 || height <= 0.0 {
                return;
            }

            let progress = self.progress.get();
            let radius = (height / 2.0) as f32;

            draw_rounded_bar(
                snapshot,
                width as f32,
                height as f32,
                radius,
                *self.trough.borrow(),
            );

            if progress > 0.0 {
                let fill_width = (width * progress).max(height) as f32;
                draw_rounded_bar(
                    snapshot,
                    fill_width,
                    height as f32,
                    radius,
                    *self.accent.borrow(),
                );
            }

            let marker = self.pace_marker.get();
            if (0.0..=1.0).contains(&marker) {
                let x = (width * marker) as f32;
                let color = if self.pace_deficit.get() {
                    gdk::RGBA::new(0.9, 0.3, 0.3, 0.7)
                } else {
                    gdk::RGBA::new(0.3, 0.7, 0.4, 0.7)
                };
                let rect = gtk4::graphene::Rect::new(x - 0.75, 0.0, 1.5, height as f32);
                snapshot.append_color(&color, &rect);
            }
        }

        fn measure(&self, orientation: gtk4::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            match orientation {
                gtk4::Orientation::Horizontal => (100, 200, -1, -1),
                gtk4::Orientation::Vertical => (5, 5, -1, -1),
                _ => (0, 0, -1, -1),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::OnceLock;

    static GTK_INIT: OnceLock<bool> = OnceLock::new();

    fn init_gtk() -> bool {
        if std::env::var("CLAUDE_BAR_GTK_TESTS").is_err() {
            eprintln!("Skipping GTK-dependent test: set CLAUDE_BAR_GTK_TESTS=1 to enable.");
            return false;
        }
        *GTK_INIT.get_or_init(|| gtk4::init().is_ok())
    }

    #[test]
    fn test_progress_clamping() {
        if !init_gtk() {
            eprintln!("Skipping GTK-dependent test: GTK init failed.");
            return;
        }

        let bar = UsageProgressBar::new();

        bar.set_progress(0.5);
        assert!((bar.progress() - 0.5).abs() < f64::EPSILON);

        bar.set_progress(1.5);
        assert!((bar.progress() - 1.0).abs() < f64::EPSILON);

        bar.set_progress(-0.5);
        assert!((bar.progress() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_label() {
        if !init_gtk() {
            eprintln!("Skipping GTK-dependent test: GTK init failed.");
            return;
        }

        let bar = UsageProgressBar::new();
        bar.set_label("78% used");
        assert_eq!(bar.label(), "78% used");
    }
}
