use jiff::Timestamp;
use topcoat::view::View;
use topcoat::view::component;
use topcoat::view::view;

#[derive(Clone, Copy, Debug)]
pub enum TimeFormat {
    Absolute,
    Relative,
}

impl TimeFormat {
    fn name(self) -> &'static str {
        match self {
            Self::Absolute => "absolute",
            Self::Relative => "relative",
        }
    }
}

#[component]
pub async fn local_time(
    at: Timestamp,
    format: TimeFormat,
    #[default] hour_precision: bool,
) -> topcoat::Result<impl View> {
    Ok(view! {
        <time
            datetime=(at.strftime("%Y-%m-%dT%H:%M:%SZ").to_string())
            data-format=(format.name())
            data-precision=(hour_precision.then_some("hour"))
        >
            (at.strftime("%Y-%m-%d %H:%M UTC").to_string())
        </time>
    })
}
