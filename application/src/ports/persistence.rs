use async_trait::async_trait;
use domain::Actor;
use domain::Admin;
use domain::AdminSession;
use domain::Artwork;
use domain::Episode;
use domain::HeldRemoval;
use domain::Inbox;
use domain::Invite;
use domain::MediaFile;
use domain::Movie;
use domain::Outbox;
use domain::Season;
use domain::Session;
use domain::Show;
use domain::SourceState;
use domain::SyncProblem;
use domain::User;
use opentelemetry::KeyValue;
use opentelemetry::global;
use opentelemetry::metrics::Histogram;
use std::marker::PhantomData;
use std::sync::Arc;
use std::time::Instant;
use toasty::schema::Model;
use toasty::stmt::IntoExpr;
use toasty_core::Schema;
use toasty_core::driver::Capability;
use toasty_core::driver::ExecResponse;
use toasty_core::driver::operation::RawSql;
use toasty_core::stmt::ExprSet;
use toasty_core::stmt::InsertTarget;
use toasty_core::stmt::Statement;
use tracing::Instrument;
use tracing::field;

pub fn models() -> toasty::ModelSet {
    toasty::models!(domain::*)
}

#[derive(Clone)]
pub struct DbLocation {
    pub database: String,
    pub address: String,
    pub port: u16,
}

pub struct DbSet<T>(PhantomData<fn() -> T>);

impl<T: Model> Default for DbSet<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Model> DbSet<T> {
    pub const fn new() -> Self {
        Self(PhantomData)
    }

    pub fn all(&self) -> T::Query<toasty::stmt::List<T>> {
        T::wrap_query(toasty::stmt::Query::all())
    }

    pub fn filter(&self, expression: toasty::stmt::Expr<bool>) -> T::Query<toasty::stmt::List<T>> {
        T::wrap_query(toasty::stmt::Query::all().filter(expression))
    }

    pub fn find(&self, primary_key: impl IntoExpr<T::PrimaryKey>) -> T::Query<Option<T>> {
        T::query_first(T::find_by_primary_key(primary_key.into_expr()))
    }

    pub fn create(&self) -> T::Create {
        T::new_create()
    }
}

pub trait DbScope {
    fn actors(&self) -> DbSet<Actor> {
        DbSet::new()
    }

    fn admins(&self) -> DbSet<Admin> {
        DbSet::new()
    }

    fn admin_sessions(&self) -> DbSet<AdminSession> {
        DbSet::new()
    }

    fn artwork(&self) -> DbSet<Artwork> {
        DbSet::new()
    }

    fn episodes(&self) -> DbSet<Episode> {
        DbSet::new()
    }

    fn held_removals(&self) -> DbSet<HeldRemoval> {
        DbSet::new()
    }

    fn inboxes(&self) -> DbSet<Inbox> {
        DbSet::new()
    }

    fn invites(&self) -> DbSet<Invite> {
        DbSet::new()
    }

    fn media_files(&self) -> DbSet<MediaFile> {
        DbSet::new()
    }

    fn movies(&self) -> DbSet<Movie> {
        DbSet::new()
    }

    fn outboxes(&self) -> DbSet<Outbox> {
        DbSet::new()
    }

    fn seasons(&self) -> DbSet<Season> {
        DbSet::new()
    }

    fn sessions(&self) -> DbSet<Session> {
        DbSet::new()
    }

    fn shows(&self) -> DbSet<Show> {
        DbSet::new()
    }

    fn source_states(&self) -> DbSet<SourceState> {
        DbSet::new()
    }

    fn sync_problems(&self) -> DbSet<SyncProblem> {
        DbSet::new()
    }

    fn users(&self) -> DbSet<User> {
        DbSet::new()
    }

    fn executor_mut(&mut self) -> &mut dyn toasty::Executor;
}

#[derive(Clone)]
pub struct DbContext {
    executor: Instrumented<toasty::Db>,
}

impl DbContext {
    pub fn new(db: toasty::Db, location: DbLocation) -> Self {
        Self {
            executor: Instrumented {
                inner: db,
                location,
                duration: global::meter("selfless")
                    .f64_histogram("db.client.operation.duration")
                    .with_unit("s")
                    .with_boundaries(vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0, 10.0])
                    .build(),
            },
        }
    }

    pub async fn transaction(&mut self) -> toasty::Result<DbTransaction<'_>> {
        let Instrumented {
            inner,
            location,
            duration,
        } = &mut self.executor;
        Ok(DbTransaction {
            executor: Instrumented {
                inner: inner.transaction().await?,
                location: location.clone(),
                duration: duration.clone(),
            },
        })
    }
}

impl DbScope for DbContext {
    fn executor_mut(&mut self) -> &mut dyn toasty::Executor {
        &mut self.executor
    }
}

pub struct DbTransaction<'a> {
    executor: Instrumented<toasty::Transaction<'a>>,
}

impl DbTransaction<'_> {
    pub async fn commit(self) -> toasty::Result<()> {
        self.executor.inner.commit().await
    }

    pub async fn rollback(self) -> toasty::Result<()> {
        self.executor.inner.rollback().await
    }
}

impl DbScope for DbTransaction<'_> {
    fn executor_mut(&mut self) -> &mut dyn toasty::Executor {
        &mut self.executor
    }
}

#[derive(Clone)]
struct Instrumented<E> {
    inner: E,
    location: DbLocation,
    duration: Histogram<f64>,
}

#[async_trait]
impl<E: toasty::Executor> toasty::Executor for Instrumented<E> {
    #[expect(
        clippy::disallowed_methods,
        reason = "the Executor trait requires it, and use cases start transactions through DbContext"
    )]
    async fn transaction(&mut self) -> toasty::Result<toasty::Transaction<'_>> {
        self.inner.transaction().await
    }

    async fn exec_untyped(&mut self, stmt: Statement) -> toasty::Result<ExecResponse> {
        let (operation, model) = match &stmt {
            Statement::Query(x) => (
                "SELECT",
                match &x.body {
                    ExprSet::Select(x) => x.source.as_model().map(|x| x.id),
                    _ => None,
                },
            ),
            Statement::Insert(x) => (
                "INSERT",
                match x.target {
                    InsertTarget::Model(x) => Some(x),
                    _ => None,
                },
            ),
            Statement::Update(x) => ("UPDATE", x.target.model_id()),
            Statement::Delete(x) => ("DELETE", x.from.as_model().map(|x| x.id)),
        };
        let collection = model.map(|x| self.inner.schema().table_for(x).name.clone());
        self.instrument(Some(operation), collection, None, async |db| {
            db.exec_untyped(stmt).await
        })
        .await
    }

    async fn exec_raw_sql(&mut self, raw: RawSql) -> toasty::Result<ExecResponse> {
        self.instrument(None, None, Some(raw.sql.clone()), async |db| {
            db.exec_raw_sql(raw).await
        })
        .await
    }

    fn capability(&mut self) -> &Capability {
        self.inner.capability()
    }

    fn schema(&mut self) -> &Arc<Schema> {
        self.inner.schema()
    }
}

impl<E: toasty::Executor> Instrumented<E> {
    async fn instrument(
        &mut self,
        operation: Option<&'static str>,
        collection: Option<String>,
        query_text: Option<String>,
        execute: impl AsyncFnOnce(&mut E) -> toasty::Result<ExecResponse>,
    ) -> toasty::Result<ExecResponse> {
        let start = Instant::now();
        let target = collection.as_deref().unwrap_or(&self.location.database);
        let span = tracing::info_span!(
            "db_operation",
            otel.name = %operation.map_or_else(|| target.to_owned(), |x| format!("{x} {target}")),
            otel.kind = "client",
            otel.status_code = field::Empty,
            db.system.name = "postgresql",
            db.namespace = self.location.database,
            db.operation.name = operation,
            db.collection.name = collection,
            db.query.text = query_text,
            server.address = self.location.address,
            server.port = self.location.port,
            error.type = field::Empty,
        );
        let result = execute(&mut self.inner).instrument(span.clone()).await;
        let mut attributes = vec![
            KeyValue::new("db.system.name", "postgresql"),
            KeyValue::new("db.namespace", self.location.database.clone()),
            KeyValue::new("server.address", self.location.address.clone()),
            KeyValue::new("server.port", i64::from(self.location.port)),
        ];
        if let Some(operation) = operation {
            attributes.push(KeyValue::new("db.operation.name", operation));
        }
        if let Some(collection) = collection {
            attributes.push(KeyValue::new("db.collection.name", collection));
        }
        if let Err(error) = &result {
            let error_type = if error.is_connection_lost() {
                "connection_lost"
            } else if error.is_connection_pool() {
                "connection_pool"
            } else if error.is_transaction_timeout() {
                "timeout"
            } else {
                "_OTHER"
            };
            span.record("otel.status_code", "ERROR");
            span.record("error.type", error_type);
            attributes.push(KeyValue::new("error.type", error_type));
        }
        self.duration
            .record(start.elapsed().as_secs_f64(), &attributes);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::mock_db_context;
    use domain::EmailAddress;
    use opentelemetry::trace::SpanKind;
    use opentelemetry::trace::TracerProvider;
    use opentelemetry_sdk::trace::InMemorySpanExporter;
    use opentelemetry_sdk::trace::SdkTracerProvider;
    use tracing::level_filters::LevelFilter;
    use tracing_subscriber::layer::SubscriberExt;

    #[tokio::test]
    async fn statements_inside_a_transaction_export_database_spans_under_the_current_span() {
        let mut db_context = mock_db_context().await;
        let exporter = InMemorySpanExporter::default();
        let provider = SdkTracerProvider::builder()
            .with_simple_exporter(exporter.clone())
            .build();
        let _tracing = test_fixtures::set_default_subscriber(
            tracing_subscriber::registry()
                .with(LevelFilter::INFO)
                .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("selfless"))),
        );
        let request = tracing::info_span!("request");

        async {
            let mut transaction = db_context.transaction().await.unwrap();
            transaction
                .users()
                .filter(User::fields().name().eq("Noor"))
                .update()
                .email(EmailAddress::new("noor@example.com".to_owned()).unwrap())
                .exec(transaction.executor_mut())
                .await
                .unwrap();
            transaction
                .sessions()
                .filter(Session::fields().device_name().eq("Living Room"))
                .delete()
                .exec(transaction.executor_mut())
                .await
                .unwrap();
            transaction.commit().await.unwrap();
        }
        .instrument(request.clone())
        .await;

        drop(request);
        let spans = exporter.get_finished_spans().unwrap();
        let request = spans.iter().find(|x| x.name == "request").unwrap();
        let database_spans = spans
            .iter()
            .filter(|x| {
                x.span_kind == SpanKind::Client
                    && x.parent_span_id == request.span_context.span_id()
            })
            .map(|x| x.name.as_ref())
            .collect::<Vec<_>>();
        assert_eq!(database_spans, ["UPDATE users", "DELETE sessions"]);
    }
}
