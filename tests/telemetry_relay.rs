mod support;

use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceResponse;
use opentelemetry_proto::tonic::collector::trace::v1::trace_service_server::TraceService;
use opentelemetry_proto::tonic::collector::trace::v1::trace_service_server::TraceServiceServer;
use opentelemetry_proto::tonic::common::v1::AnyValue;
use opentelemetry_proto::tonic::common::v1::KeyValue;
use opentelemetry_proto::tonic::common::v1::any_value::Value;
use opentelemetry_proto::tonic::resource::v1::Resource;
use opentelemetry_proto::tonic::trace::v1::ResourceSpans;
use opentelemetry_proto::tonic::trace::v1::ScopeSpans;
use opentelemetry_proto::tonic::trace::v1::Span;
use prost::Message;
use reqwest::StatusCode;
use std::sync::Arc;
use std::sync::Mutex;
use support::TestDatabase;
use support::TestServer;
use tokio::net::TcpListener;
use tonic::Request;
use tonic::Response;
use tonic::Status;
use tonic::transport::Server;
use tonic::transport::server::TcpIncoming;

#[tokio::test]
async fn relay_forwards_traces_to_the_collector() {
    let collector = FakeCollector::default();
    let collector_url = collector.start().await;
    let database = TestDatabase::create().await;
    let server = TestServer::start_with_collector(database.url(), &collector_url).await;
    let request = ExportTraceServiceRequest {
        resource_spans: vec![ResourceSpans {
            resource: Some(Resource {
                attributes: vec![KeyValue {
                    key: "service.name".to_owned(),
                    value: Some(AnyValue {
                        value: Some(Value::StringValue("selfless-web".to_owned())),
                    }),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            scope_spans: vec![ScopeSpans {
                spans: vec![Span {
                    trace_id: vec![0x4b; 16],
                    span_id: vec![0x00; 8],
                    name: "GET".to_owned(),
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        }],
    };

    let (status, body) = server
        .post_protobuf("/api/telemetry/traces/v1", request.encode_to_vec())
        .await;

    assert_eq!(status, StatusCode::OK);
    ExportTraceServiceResponse::decode(body.as_slice()).unwrap();
    assert_eq!(*collector.requests.lock().unwrap(), vec![request]);
    drop(server);
    database.drop().await;
}

#[derive(Clone, Default)]
struct FakeCollector {
    requests: Arc<Mutex<Vec<ExportTraceServiceRequest>>>,
}

impl FakeCollector {
    async fn start(&self) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(
            Server::builder()
                .add_service(TraceServiceServer::new(self.clone()))
                .serve_with_incoming(TcpIncoming::from(listener)),
        );
        format!("http://{address}")
    }
}

#[tonic::async_trait]
impl TraceService for FakeCollector {
    async fn export(
        &self,
        request: Request<ExportTraceServiceRequest>,
    ) -> Result<Response<ExportTraceServiceResponse>, Status> {
        self.requests.lock().unwrap().push(request.into_inner());
        Ok(Response::new(ExportTraceServiceResponse::default()))
    }
}
