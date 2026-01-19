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
}

impl Default for UsageProgressBar {
    fn default() -> Self {
        Self::new()
    }
}

mod imp {
    use super::*;
    use std::cell::RefCell;

    #[derive(Default)]
    pub struct UsageProgressBarPriv {
        pub progress: Cell<f64>,
        pub label: RefCell<String>,
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
            obj.set_height_request(8);
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
            let border_radius = height / 2.0;

            let bg_color = gtk4::gdk::RGBA::new(0.2, 0.2, 0.2, 0.3);
            let bg_rect = gtk4::graphene::Rect::new(0.0, 0.0, width as f32, height as f32);
            let bg_rounded = gtk4::gsk::RoundedRect::new(
                bg_rect,
                gtk4::graphene::Size::new(border_radius as f32, border_radius as f32),
                gtk4::graphene::Size::new(border_radius as f32, border_radius as f32),
                gtk4::graphene::Size::new(border_radius as f32, border_radius as f32),
                gtk4::graphene::Size::new(border_radius as f32, border_radius as f32),
            );
            snapshot.push_rounded_clip(&bg_rounded);
            snapshot.append_color(&bg_color, &bg_rect);
            snapshot.pop();

            if progress > 0.0 {
                let fill_width = (width * progress).max(height);
                let fill_color = gtk4::gdk::RGBA::new(0.96, 0.65, 0.14, 1.0);
                let fill_rect =
                    gtk4::graphene::Rect::new(0.0, 0.0, fill_width as f32, height as f32);
                let fill_rounded = gtk4::gsk::RoundedRect::new(
                    fill_rect,
                    gtk4::graphene::Size::new(border_radius as f32, border_radius as f32),
                    gtk4::graphene::Size::new(border_radius as f32, border_radius as f32),
                    gtk4::graphene::Size::new(border_radius as f32, border_radius as f32),
                    gtk4::graphene::Size::new(border_radius as f32, border_radius as f32),
                );
                snapshot.push_rounded_clip(&fill_rounded);
                snapshot.append_color(&fill_color, &fill_rect);
                snapshot.pop();
            }
        }

        fn measure(&self, orientation: gtk4::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            match orientation {
                gtk4::Orientation::Horizontal => (100, 200, -1, -1),
                gtk4::Orientation::Vertical => (8, 8, -1, -1),
                _ => (0, 0, -1, -1),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Once;

    static GTK_INIT: Once = Once::new();

    fn init_gtk() {
        GTK_INIT.call_once(|| {
            gtk4::init().expect("Failed to initialize GTK");
        });
    }

    #[test]
    fn test_progress_clamping() {
        init_gtk();

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
        init_gtk();

        let bar = UsageProgressBar::new();
        bar.set_label("78% used");
        assert_eq!(bar.label(), "78% used");
    }
}
