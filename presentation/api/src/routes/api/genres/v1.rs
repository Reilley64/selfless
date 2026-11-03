use application::browsing::list_genres;
use domain::TitleKind;
use infrastructure::AppState;
use serde::Deserialize;
use serde::Serialize;
use topcoat::context::Cx;
use topcoat::context::app_context;
use topcoat::router::content::Json;
use topcoat::router::query_params;
use topcoat::router::response::IntoResponse;
use topcoat::router::response::Response;
use topcoat::router::route;

#[query_params(error = bad_request)]
struct Query {
    kind: Kind,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
enum Kind {
    Movie,
    Show,
}

#[derive(Serialize)]
struct GenresBody {
    genres: Vec<GenreBody>,
}

#[derive(Serialize)]
struct GenreBody {
    name: String,
    count: i64,
}

#[route(GET "/api/(user)/genres/v1")]
pub(crate) async fn genres(cx: &Cx) -> topcoat::Result<Response> {
    let query = query_params::<Query>(cx)?;
    let kind = match query.kind {
        Kind::Movie => TitleKind::Movie,
        Kind::Show => TitleKind::Show,
    };
    let output = list_genres::call(app_context::<AppState>(cx), kind).await?;
    Json(&GenresBody {
        genres: output
            .genres
            .into_iter()
            .map(|x| GenreBody {
                name: x.name,
                count: x.count,
            })
            .collect(),
    })
    .into_response(cx)
}

#[cfg(test)]
mod tests {
    use crate::test::add_movie;
    use crate::test::add_show;
    use crate::test::serve;
    use crate::test::signed_in_user;
    use http::Method;
    use http::StatusCode;
    use serde_json::json;
    use test_app::TestApp;

    #[tokio::test]
    async fn the_genres_of_each_kind_come_with_their_counts() {
        let app = TestApp::new().await;
        let (_, token) = signed_in_user(&app, "Noor", "a long passphrase about tea").await;
        add_movie(&app, "Arrival").await;
        add_show(&app, "Severance", &[1]).await;
        let server = serve(&app).await;

        let (movie_status, movie_genres) = server
            .request(Method::GET, "/api/genres/v1?kind=movie", Some(&token), None)
            .await;
        let (show_status, show_genres) = server
            .request(Method::GET, "/api/genres/v1?kind=show", Some(&token), None)
            .await;

        assert_eq!(
            (movie_status, show_status),
            (StatusCode::OK, StatusCode::OK)
        );
        assert_eq!(
            movie_genres,
            json!({ "genres": [
                { "name": "Drama", "count": 1 },
                { "name": "Science Fiction", "count": 1 },
            ] })
        );
        assert_eq!(
            show_genres,
            json!({ "genres": [
                { "name": "Drama", "count": 1 },
                { "name": "Mystery", "count": 1 },
            ] })
        );
    }

    #[tokio::test]
    async fn a_missing_or_unknown_kind_returns_a_bad_request_problem() {
        let app = TestApp::new().await;
        let (_, token) = signed_in_user(&app, "Noor", "a long passphrase about tea").await;
        let server = serve(&app).await;

        for path in ["/api/genres/v1", "/api/genres/v1?kind=episode"] {
            let (status, problem) = server.request(Method::GET, path, Some(&token), None).await;

            assert_eq!(status, StatusCode::BAD_REQUEST, "{path}");
            assert_eq!(
                problem["type"], "urn:selfless:problem:bad-request",
                "{path}"
            );
        }
    }
}
