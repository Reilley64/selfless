use application::ports::MalformedTelemetry;
use application::ports::TelemetrySignal;
use application::telemetry::relay_telemetry;
use infrastructure::AppState;
use ohno::ErrorExt;
use std::error::Error;
use topcoat::context::Cx;
use topcoat::context::app_context;
use topcoat::router::StatusCode;
use topcoat::router::header::CONTENT_TYPE;
use topcoat::router::request::Bytes;
use topcoat::router::response::IntoResponse;
use topcoat::router::response::Response;
use topcoat::router::route;

#[route(POST "/api/(user)/telemetry/traces/v1")]
pub(crate) async fn traces(cx: &Cx, request: Bytes) -> topcoat::Result<Response> {
    relay(cx, TelemetrySignal::Traces, request).await
}

#[route(POST "/api/(user)/telemetry/metrics/v1")]
pub(crate) async fn metrics(cx: &Cx, request: Bytes) -> topcoat::Result<Response> {
    relay(cx, TelemetrySignal::Metrics, request).await
}

#[route(POST "/api/(user)/telemetry/logs/v1")]
pub(crate) async fn logs(cx: &Cx, request: Bytes) -> topcoat::Result<Response> {
    relay(cx, TelemetrySignal::Logs, request).await
}

async fn relay(cx: &Cx, signal: TelemetrySignal, request: Bytes) -> topcoat::Result<Response> {
    match relay_telemetry::call(app_context::<AppState>(cx), signal, request).await {
        Ok(output) => (
            StatusCode::OK,
            [(CONTENT_TYPE, "application/x-protobuf")],
            output.response,
        )
            .into_response(cx),
        Err(error) if error.find_source::<MalformedTelemetry>().is_some() => {
            StatusCode::BAD_REQUEST.into_response(cx)
        }
        Err(error) => {
            tracing::warn!(error = &error as &dyn Error, "failed to relay telemetry");
            StatusCode::BAD_GATEWAY.into_response(cx)
        }
    }
}


#[cfg(test)]
mod tests {
    use crate::test::serve;
    use crate::test::signed_in_user;
    use application::ports::ExportTelemetry;
    use application::ports::TelemetrySignal;
    use http::Method;
    use http::StatusCode;
    use std::sync::Arc;
    use std::sync::Mutex;
    use test_app::TestApp;
    use topcoat::router::request::Bytes;

    const SIGNALS: [(&str, TelemetrySignal); 3] = [
        ("traces", TelemetrySignal::Traces),
        ("metrics", TelemetrySignal::Metrics),
        ("logs", TelemetrySignal::Logs),
    ];
    const REPLY: &[u8] = b"the collector's reply";

    #[derive(Clone, Default)]
    struct RecordingExporter {
        calls: Arc<Mutex<Vec<(TelemetrySignal, Bytes)>>>,
    }

    impl RecordingExporter {
        fn port(&self) -> ExportTelemetry {
            let calls = self.calls.clone();
            ExportTelemetry::new(move |signal, request| {
                calls.lock().unwrap().push((signal, request));
                Box::pin(async { Ok(Bytes::from_static(REPLY)) })
            })
        }

        fn take(&self) -> Vec<(TelemetrySignal, Bytes)> {
            std::mem::take(&mut self.calls.lock().unwrap())
        }
    }

    #[tokio::test]
    async fn relay_passes_the_request_to_the_exporter_and_returns_its_reply() {
        let mut app = TestApp::new().await;
        let exporter = RecordingExporter::default();
        app.state.export_telemetry = exporter.port();
        let (_, token) = signed_in_user(&app, "Noor", "a long passphrase about tea").await;
        let server = serve(&app).await;

        for (path, signal) in SIGNALS {
            let request = format!("a {path} export request").into_bytes();

            let (status, body) = server
                .post_protobuf(
                    &format!("/api/telemetry/{path}/v1"),
                    &token,
                    request.clone(),
                )
                .await;

            assert_eq!(status, StatusCode::OK);
            assert_eq!(body, REPLY);
            assert_eq!(exporter.take(), [(signal, Bytes::from(request))]);
        }
    }

    #[tokio::test]
    async fn relay_rejects_requests_without_a_session_token() {
        let mut app = TestApp::new().await;
        let exporter = RecordingExporter::default();
        app.state.export_telemetry = exporter.port();
        let server = serve(&app).await;

        for (path, _) in SIGNALS {
            let (status, problem) = server
                .request(
                    Method::POST,
                    &format!("/api/telemetry/{path}/v1"),
                    None,
                    None,
                )
                .await;

            assert_eq!(status, StatusCode::UNAUTHORIZED);
            assert_eq!(problem["type"], "urn:selfless:problem:unauthenticated");
        }
        assert_eq!(exporter.take(), []);
    }
}
