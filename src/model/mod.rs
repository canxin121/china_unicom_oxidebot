pub mod config;
pub use config::ActiveModel as ConfigActiveModel;
pub use config::Entity as ConfigEntity;
pub use config::Model as ConfigModel;
pub mod last;
pub use last::ActiveModel as LastActiveModel;
pub use last::Entity as LastEntity;
pub use last::Model as LastModel;
pub mod daily;
pub use daily::ActiveModel as DailyActiveModel;
pub use daily::Entity as DailyEntity;
pub use daily::Model as DailyModel;

#[cfg(test)]
pub mod data_test {
    use crate::model::last::build_last_active;
    use crate::model::{
        ConfigActiveModel, ConfigEntity, ConfigModel, DailyActiveModel, DailyEntity, DailyModel,
        LastActiveModel, LastEntity, LastModel,
    };
    use crate::utils::db::init_db;
    use china_unicom_rs::data::ChinaUnicomData;
    use sea_orm::{ActiveModelTrait, EntityTrait, ModelTrait, Set};

    #[tokio::test]
    async fn init_tables() {
        let _db = init_db().await.unwrap();
    }

    #[tokio::test]
    async fn insert_all() {
        // delete the already exist file
        let path = std::path::Path::new("./china_unicom/data.db");
        if path.exists() {
            std::fs::remove_file(path).unwrap();
        }
        let db = init_db().await.unwrap();
        let config_active: ConfigActiveModel = ConfigModel {
            user: "1".to_string(),
            ..Default::default()
        }
        .into();
        let today_active: LastActiveModel = LastModel {
            user: "1".to_string(),
            ..Default::default()
        }
        .into();
        let yesterday_active: DailyActiveModel = DailyModel {
            user: "1".to_string(),
            ..Default::default()
        }
        .into();
        let config = ConfigEntity::insert(config_active).exec(&db).await.unwrap();
        let today = LastEntity::insert(today_active).exec(&db).await.unwrap();
        let yesterday = DailyEntity::insert(yesterday_active)
            .exec(&db)
            .await
            .unwrap();

        println!("{:?}", config);
        println!("{:?}", today);
        println!("{:?}", yesterday);
    }

    #[tokio::test]
    async fn get_all() {
        let db = init_db().await.unwrap();
        let config = ConfigEntity::find().all(&db).await.unwrap();
        let today = LastEntity::find().all(&db).await.unwrap();
        let yesterday = DailyEntity::find().all(&db).await.unwrap();
        assert!(config.len() > 0);
        assert!(today.len() > 0);
        assert!(yesterday.len() > 0);
        println!("{:?}", config);
        println!("{:?}", today);
        println!("{:?}", yesterday);
    }

    #[tokio::test]
    async fn relation_all() {
        let db = init_db().await.unwrap();
        let today = LastEntity::find().one(&db).await.unwrap().unwrap();
        let yesterday = DailyEntity::find().one(&db).await.unwrap().unwrap();
        let config = ConfigEntity::find().one(&db).await.unwrap().unwrap();

        let today_config = today.find_related(ConfigEntity).all(&db).await.unwrap();
        let yesterday_config = yesterday
            .find_related(ConfigEntity)
            .one(&db)
            .await
            .unwrap()
            .unwrap();
        let config_today = config
            .find_related(LastEntity)
            .one(&db)
            .await
            .unwrap()
            .unwrap();
        let config_yesterday = config
            .find_related(DailyEntity)
            .one(&db)
            .await
            .unwrap()
            .unwrap();
        println!("{:?}", today_config);
        println!("{:?}", yesterday_config);
        println!("{:?}", config_today);
        println!("{:?}", config_yesterday);
    }

    #[tokio::test]
    async fn update_all() {
        let db = init_db().await.unwrap();
        let today = build_last_active(ChinaUnicomData::default(), "1".to_string(), "1".to_string());
        today.update(&db).await.unwrap();

        let yesterday =
            build_last_active(ChinaUnicomData::default(), "1".to_string(), "1".to_string());
        yesterday.update(&db).await.unwrap();

        let mut config: ConfigActiveModel = ConfigModel::default().into();
        config.user = Set("1".to_string());
        config.update(&db).await.unwrap();
    }
}
