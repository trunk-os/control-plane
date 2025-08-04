use anyhow::Result;
use sqlx::migrate::Migrator;

static MIGRATOR: Migrator = sqlx::migrate!();
//
// boilerplate for migration execution
//

pub(crate) async fn migrate(filename: std::path::PathBuf) -> Result<()> {
    let conn =
        sqlx::Pool::<sqlx::Sqlite>::connect(&format!("sqlite:{}", filename.display())).await?;
    Ok(MIGRATOR.run(&conn).await?)
}
