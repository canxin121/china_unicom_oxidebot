use sea_orm_migration::{async_trait, MigrationTrait, MigratorTrait};

mod create_config_table;
mod create_today_table;
mod create_yesterday_table;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(create_config_table::Migration),
            Box::new(create_today_table::Migration),
            Box::new(create_yesterday_table::Migration),
        ]
    }
}
