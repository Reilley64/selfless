pub async fn mock_database(models: toasty::ModelSet) -> toasty::Db {
    let db = toasty::Db::builder()
        .models(models)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    db
}
