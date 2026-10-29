pub mod check_database;
pub mod confirm_held_removal;
pub mod delete_old_inbox_rows;
mod library_items;
pub mod list_episodes;
pub mod list_movies;
pub mod list_seasons;
pub mod list_shows;
mod media_files;
pub mod next_inbox_attempt;
pub mod ports;
pub mod process_inbox_row;
pub mod reconcile;
pub mod relay_telemetry;
pub mod release_inbox_rows;
pub mod retry_failed_row;
pub mod store_webhook_delivery;
pub mod sync_movie;
pub mod sync_now;
pub mod sync_show;
pub mod sync_status;
mod unsampled;
mod webhook_deliveries;

pub mod attributes {
    include!(concat!(env!("OUT_DIR"), "/attributes.rs"));
}

pub mod metrics {
    include!(concat!(env!("OUT_DIR"), "/metrics.rs"));
}

use async_trait::async_trait;
use domain::Source;
use opentelemetry::KeyValue;
use opentelemetry::global;
use opentelemetry::metrics::Histogram;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Instant;
use sync_status::ReconcileProgress;
use toasty_core::Schema;
use toasty_core::driver::Capability;
use toasty_core::driver::ExecResponse;
use toasty_core::driver::operation::RawSql;
use toasty_core::stmt::ExprSet;
use toasty_core::stmt::InsertTarget;
use toasty_core::stmt::Statement;
use toasty_core::stmt::UpdateTarget;
use tokio::sync::Notify;
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

#[derive(Clone)]
pub struct BuildInfo {
    pub name: &'static str,
    pub version: &'static str,
    pub git_branch: &'static str,
    pub git_commit_id: &'static str,
    pub git_commit_time: &'static str,
}

#[derive(Clone)]
pub struct WebhookPasswords {
    pub radarr: String,
    pub sonarr: String,
}

#[derive(Clone, Default)]
pub struct InboxWakeup(Arc<Notify>);

impl InboxWakeup {
    pub async fn notified(&self) {
        self.0.notified().await;
    }

    fn wake(&self) {
        self.0.notify_one();
    }
}

#[derive(Clone, Default)]
pub struct Reconciles {
    requested: Arc<Notify>,
    radarr: SourceReconcile,
    sonarr: SourceReconcile,
}

#[derive(Clone, Default)]
struct SourceReconcile {
    running: Arc<tokio::sync::Mutex<()>>,
    progress: Arc<Mutex<Option<ReconcileProgress>>>,
}

impl Reconciles {
    pub async fn requested(&self) {
        self.requested.notified().await;
    }

    fn request(&self) {
        self.requested.notify_one();
    }

    fn source(&self, source: Source) -> &SourceReconcile {
        match source {
            Source::Radarr => &self.radarr,
            Source::Sonarr => &self.sonarr,
        }
    }
}

#[derive(Clone)]
pub struct DbContext {
    executor: InstrumentedDb,
}

impl DbContext {
    pub fn new(db: toasty::Db, location: DbLocation) -> Self {
        Self {
            executor: InstrumentedDb {
                db,
                instruments: DbInstruments {
                    location,
                    duration: global::meter("selfless")
                        .f64_histogram("db.client.operation.duration")
                        .with_unit("s")
                        .with_boundaries(vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0, 10.0])
                        .build(),
                },
            },
        }
    }

    pub fn executor_mut(&mut self) -> &mut dyn toasty::Executor {
        &mut self.executor
    }

    pub async fn transaction(&mut self) -> toasty::Result<DbTransaction<'_>> {
        Ok(DbTransaction {
            transaction: self.executor.db.transaction().await?,
            instruments: &self.executor.instruments,
        })
    }
}

pub struct DbTransaction<'a> {
    transaction: toasty::Transaction<'a>,
    instruments: &'a DbInstruments,
}

impl DbTransaction<'_> {
    pub fn executor_mut(&mut self) -> &mut dyn toasty::Executor {
        self
    }

    pub async fn commit(self) -> toasty::Result<()> {
        self.transaction.commit().await
    }
}

#[async_trait]
impl toasty::Executor for DbTransaction<'_> {
    async fn transaction(&mut self) -> toasty::Result<toasty::Transaction<'_>> {
        self.transaction.transaction().await
    }

    async fn exec_untyped(&mut self, stmt: Statement) -> toasty::Result<ExecResponse> {
        self.instruments
            .exec_untyped(&mut self.transaction, stmt)
            .await
    }

    async fn exec_raw_sql(&mut self, raw: RawSql) -> toasty::Result<ExecResponse> {
        self.instruments
            .exec_raw_sql(&mut self.transaction, raw)
            .await
    }

    fn capability(&mut self) -> &Capability {
        toasty::Executor::capability(&mut self.transaction)
    }

    fn schema(&mut self) -> &Arc<Schema> {
        toasty::Executor::schema(&mut self.transaction)
    }
}

#[derive(Clone)]
struct InstrumentedDb {
    db: toasty::Db,
    instruments: DbInstruments,
}

#[async_trait]
impl toasty::Executor for InstrumentedDb {
    async fn transaction(&mut self) -> toasty::Result<toasty::Transaction<'_>> {
        self.db.transaction().await
    }

    async fn exec_untyped(&mut self, stmt: Statement) -> toasty::Result<ExecResponse> {
        self.instruments.exec_untyped(&mut self.db, stmt).await
    }

    async fn exec_raw_sql(&mut self, raw: RawSql) -> toasty::Result<ExecResponse> {
        self.instruments.exec_raw_sql(&mut self.db, raw).await
    }

    fn capability(&mut self) -> &Capability {
        self.db.capability()
    }

    fn schema(&mut self) -> &Arc<Schema> {
        self.db.schema()
    }
}

#[derive(Clone)]
struct DbInstruments {
    location: DbLocation,
    duration: Histogram<f64>,
}

impl DbInstruments {
    async fn exec_untyped(
        &self,
        executor: &mut dyn toasty::Executor,
        stmt: Statement,
    ) -> toasty::Result<ExecResponse> {
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
            Statement::Update(x) => (
                "UPDATE",
                match x.target {
                    UpdateTarget::Model(x) => Some(x),
                    _ => None,
                },
            ),
            Statement::Delete(x) => ("DELETE", x.from.as_model().map(|x| x.id)),
        };
        let collection = model.map(|x| executor.schema().table_for(x).name.clone());
        self.instrument(
            Some(operation),
            collection,
            None,
            executor.exec_untyped(stmt),
        )
        .await
    }

    async fn exec_raw_sql(
        &self,
        executor: &mut dyn toasty::Executor,
        raw: RawSql,
    ) -> toasty::Result<ExecResponse> {
        let query_text = raw.sql.clone();
        self.instrument(None, None, Some(query_text), executor.exec_raw_sql(raw))
            .await
    }

    async fn instrument(
        &self,
        operation: Option<&'static str>,
        collection: Option<String>,
        query_text: Option<String>,
        execute: impl Future<Output = toasty::Result<ExecResponse>>,
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
        let result = execute.instrument(span.clone()).await;
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
