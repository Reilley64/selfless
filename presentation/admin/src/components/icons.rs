use topcoat::icon::IconData;
use topcoat::view::svg::ViewBox;

const fn lucide(body: &'static str) -> IconData {
    IconData::unescaped_unchecked(ViewBox::new(0.0, 0.0, 24.0, 24.0), body)
}

pub const CHECK: IconData = lucide(
    r#"<path fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M20 6 9 17l-5-5"/>"#,
);
pub const CHEVRON_DOWN: IconData = lucide(
    r#"<path fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="m6 9 6 6 6-6"/>"#,
);
pub const CHEVRON_LEFT: IconData = lucide(
    r#"<path fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="m15 18-6-6 6-6"/>"#,
);
pub const CHEVRON_RIGHT: IconData = lucide(
    r#"<path fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="m9 18 6-6-6-6"/>"#,
);
pub const ELLIPSIS: IconData = lucide(
    r#"<g fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2"><circle cx="12" cy="12" r="1"/><circle cx="19" cy="12" r="1"/><circle cx="5" cy="12" r="1"/></g>"#,
);
pub const PANEL_LEFT: IconData = lucide(
    r#"<g fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2"><rect width="18" height="18" x="3" y="3" rx="2"/><path d="M9 3v18"/></g>"#,
);
pub const X: IconData = lucide(
    r#"<path fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M18 6 6 18M6 6l12 12"/>"#,
);
