use sea_orm_migration::MigratorTrait as _;

use crate::migration::Migrator;

pub async fn init_db() -> anyhow::Result<sea_orm::DatabaseConnection> {
    let path = std::path::Path::new("./china_unicom/data.db");
    let exist = path.exists();
    if !exist {
        let parent = path.parent().ok_or(anyhow::anyhow!("No parent Folder"))?;
        std::fs::create_dir_all(parent)?;
        std::fs::File::create(path)?;
    }
    let connect_options = sea_orm::ConnectOptions::new("sqlite://./china_unicom/data.db");
    let db = sea_orm::Database::connect(connect_options).await?;
    if !exist {
        Migrator::up(&db, None).await?;
    }
    Ok(db)
}
