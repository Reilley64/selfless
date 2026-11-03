use application::ports::RadarrWebhook;
use application::ports::ReadRadarrWebhook;
use application::ports::UnreadableWebhook;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(tag = "eventType")]
enum RadarrEvent {
    Download {
        movie: RadarrMovieRef,
    },
    Rename {
        movie: RadarrMovieRef,
    },
    MovieAdded {
        movie: RadarrMovieRef,
    },
    MovieDelete {
        movie: RadarrMovieRef,
    },
    MovieFileDelete {
        movie: RadarrMovieRef,
    },
    Test,
    Grab,
    Health,
    HealthRestored,
    ApplicationUpdate,
    ManualInteractionRequired,
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
struct RadarrMovieRef {
    id: i64,
}

pub fn read_webhook() -> ReadRadarrWebhook {
    ReadRadarrWebhook::new(|body| {
        Ok(
            match serde_json::from_slice(body).map_err(UnreadableWebhook::caused_by)? {
                RadarrEvent::Download { movie }
                | RadarrEvent::Rename { movie }
                | RadarrEvent::MovieAdded { movie }
                | RadarrEvent::MovieDelete { movie }
                | RadarrEvent::MovieFileDelete { movie } => RadarrWebhook::Movie {
                    radarr_id: movie.id,
                },
                RadarrEvent::Test
                | RadarrEvent::Grab
                | RadarrEvent::Health
                | RadarrEvent::HealthRestored
                | RadarrEvent::ApplicationUpdate
                | RadarrEvent::ManualInteractionRequired
                | RadarrEvent::Other => RadarrWebhook::Ignored,
            },
        )
    })
}
