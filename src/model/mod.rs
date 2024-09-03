pub mod config;
pub use config::ActiveModel as ConfigActiveModel;
pub use config::Entity as ConfigEntity;
pub use config::Model as ConfigModel;
pub mod today;
pub use today::ActiveModel as TodayActiveModel;
pub use today::Entity as TodayEntity;
pub use today::Model as TodayModel;
pub mod yesterday;
pub use yesterday::ActiveModel as YesterdayActiveModel;
pub use yesterday::Entity as YesterdayEntity;
pub use yesterday::Model as YesterdayModel;

#[cfg(test)]
pub mod data_test {
    use crate::model::today::build_today_data;
    use crate::model::{
        ConfigActiveModel, ConfigEntity, ConfigModel, TodayActiveModel, TodayEntity, TodayModel,
        YesterdayActiveModel, YesterdayEntity, YesterdayModel,
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
        let today_active: TodayActiveModel = TodayModel {
            user: "1".to_string(),
            ..Default::default()
        }
        .into();
        let yesterday_active: YesterdayActiveModel = YesterdayModel {
            user: "1".to_string(),
            ..Default::default()
        }
        .into();
        let config = ConfigEntity::insert(config_active).exec(&db).await.unwrap();
        let today = TodayEntity::insert(today_active).exec(&db).await.unwrap();
        let yesterday = YesterdayEntity::insert(yesterday_active)
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
        let today = TodayEntity::find().all(&db).await.unwrap();
        let yesterday = YesterdayEntity::find().all(&db).await.unwrap();
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
        let today = TodayEntity::find().one(&db).await.unwrap().unwrap();
        let yesterday = YesterdayEntity::find().one(&db).await.unwrap().unwrap();
        let config = ConfigEntity::find().one(&db).await.unwrap().unwrap();

        let today_config = today.find_related(ConfigEntity).all(&db).await.unwrap();
        let yesterday_config = yesterday
            .find_related(ConfigEntity)
            .one(&db)
            .await
            .unwrap()
            .unwrap();
        let config_today = config
            .find_related(TodayEntity)
            .one(&db)
            .await
            .unwrap()
            .unwrap();
        let config_yesterday = config
            .find_related(YesterdayEntity)
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
        let today = build_today_data(ChinaUnicomData::default(), "1".to_string(), "1".to_string());
        today.update(&db).await.unwrap();

        let yesterday =
            build_today_data(ChinaUnicomData::default(), "1".to_string(), "1".to_string());
        yesterday.update(&db).await.unwrap();

        let mut config: ConfigActiveModel = ConfigModel::default().into();
        config.user = Set("1".to_string());
        config.update(&db).await.unwrap();
    }
}
