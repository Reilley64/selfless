use async_trait::async_trait;
use domain::Admin;
use domain::AdminSession;
use domain::Invite;
use domain::Session;
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
use toasty_core::stmt::UpdateTarget;
use tracing::Instrument;
use tracing::field;

#[derive(Clone)]
pub struct DbLocation {
    pub database: String,
    pub address: String,
    pub port: u16,
}

pub struct DbSet<T>(PhantomData<fn() -> T>);

impl<T: Model> DbSet<T> {
    const fn new() -> Self {
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
    fn admins(&self) -> DbSet<Admin> {
        DbSet::new()
    }

    fn admin_sessions(&self) -> DbSet<AdminSession> {
        DbSet::new()
    }

    fn invites(&self) -> DbSet<Invite> {
        DbSet::new()
    }

    fn sessions(&self) -> DbSet<Session> {
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
            Statement::Update(x) => (
                "UPDATE",
                match x.target {
                    UpdateTarget::Model(x) => Some(x),
                    _ => None,
                },
            ),
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
