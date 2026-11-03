use application::ports::MovieWebhookDelivery;
use application::ports::ReadRadarrWebhook;
use application::ports::UnreadableWebhook;
use domain::RadarrId;
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
                | RadarrEvent::MovieFileDelete { movie } => MovieWebhookDelivery::Movie {
                    radarr_id: RadarrId(movie.id),
                },
                RadarrEvent::Test
                | RadarrEvent::Grab
                | RadarrEvent::Health
                | RadarrEvent::HealthRestored
                | RadarrEvent::ApplicationUpdate
                | RadarrEvent::ManualInteractionRequired
                | RadarrEvent::Other => MovieWebhookDelivery::Ignored,
            },
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(name: &str) -> Result<MovieWebhookDelivery, UnreadableWebhook> {
        let body = std::fs::read(format!(
            "{}/../../test/fixtures/radarr/webhooks/{name}.json",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        read_webhook().call(&body)
    }

    #[test]
    fn library_events_name_the_movie() {
        for name in [
            "download",
            "download-upgrade",
            "rename",
            "movie-added",
            "movie-delete",
            "movie-file-delete",
        ] {
            assert_eq!(
                read(name).unwrap(),
                MovieWebhookDelivery::Movie {
                    radarr_id: RadarrId(42)
                },
                "{name}"
            );
        }
    }

    #[test]
    fn other_events_are_ignored() {
        for name in [
            "test",
            "grab",
            "health",
            "health-restored",
            "application-update",
            "manual-interaction-required",
        ] {
            assert_eq!(read(name).unwrap(), MovieWebhookDelivery::Ignored, "{name}");
        }
    }

    #[test]
    fn a_library_event_without_its_movie_is_unreadable() {
        assert!(read_webhook().call(br#"{"eventType":"Download"}"#).is_err());
    }
}
