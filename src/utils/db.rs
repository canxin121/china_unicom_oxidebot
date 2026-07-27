use sea_orm_migration::MigratorTrait as _;

use crate::migration::Migrator;

pub async fn init_db() -> anyhow::Result<sea_orm::DatabaseConnection> {
    let path = std::path::Path::new("./china_unicom/data.db");
    if !path.exists() {
        let parent = path.parent().ok_or(anyhow::anyhow!("No parent Folder"))?;
        std::fs::create_dir_all(parent)?;
        std::fs::File::create(path)?;
    }
    set_private_permissions(path)?;
    let mut connect_options = sea_orm::ConnectOptions::new("sqlite://./china_unicom/data.db");
    // SQL statements involving credentials must not be emitted to application logs.
    connect_options.sqlx_logging(false);
    let db = sea_orm::Database::connect(connect_options).await?;
    Migrator::up(&db, None).await?;
    Ok(db)
}

fn set_private_permissions(path: &std::path::Path) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        if let Some(parent) = path.parent() {
            std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
        }
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}
