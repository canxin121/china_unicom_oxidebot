use china_unicom_rs::data::ChinaUnicomData;
use chrono::{DateTime, Local};
use sea_orm::{entity::prelude::*, Set, Unchanged};

use super::DailyModel;

#[derive(Clone, Debug, Default, DeriveEntityModel)]
#[sea_orm(table_name = "last")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub user: String,

    pub bot: String,

    // 套餐名称
    pub package_name: String,

    // 查询时间
    pub time: DateTime<Local>,

    // 已用流量
    pub sum_flow_used: f64,

    // 已用定向流量
    pub limit_flow_used: f64,
    // 已用通用流量
    pub non_limit_flow_used: f64,
    // 已用免费流量
    pub free_flow_used: f64,
    // 已用非免费流量
    pub non_free_flow_used: f64,

    // 总流量
    pub sum_flow: f64,
    // 总定向流量
    pub limit_flow: f64,
    // 总通用流量
    pub non_limit_flow: f64,

    // 已用通话
    pub sum_voice_used: i64,
    // 已用定向通话
    pub limit_voice_used: i64,
    // 已用通用通话
    pub non_limit_voice_used: i64,

    // 总通话
    pub sum_voice: i64,
    // 总定向通话
    pub limit_voice: i64,
    // 总通用通话
    pub non_limit_voice: i64,
}

impl Into<DailyModel> for Model {
    fn into(self) -> DailyModel {
        DailyModel {
            user: self.user,
            bot: self.bot,
            package_name: self.package_name,
            time: self.time,
            sum_flow_used: self.sum_flow_used,
            limit_flow_used: self.limit_flow_used,
            non_limit_flow_used: self.non_limit_flow_used,
            free_flow_used: self.free_flow_used,
            non_free_flow_used: self.non_free_flow_used,
            sum_flow: self.sum_flow,
            limit_flow: self.limit_flow,
            non_limit_flow: self.non_limit_flow,
            sum_voice_used: self.sum_voice_used,
            limit_voice_used: self.limit_voice_used,
            non_limit_voice_used: self.non_limit_voice_used,
            sum_voice: self.sum_voice,
            limit_voice: self.limit_voice,
            non_limit_voice: self.non_limit_voice,
        }
    }
}

impl Into<ChinaUnicomData> for Model {
    fn into(self) -> ChinaUnicomData {
        ChinaUnicomData {
            package_name: self.package_name,
            time: self.time,
            sum_flow_used: self.sum_flow_used,
            limit_flow_used: self.limit_flow_used,
            non_limit_flow_used: self.non_limit_flow_used,
            free_flow_used: self.free_flow_used,
            non_free_flow_used: self.non_free_flow_used,
            sum_flow: self.sum_flow,
            limit_flow: self.limit_flow,
            non_limit_flow: self.non_limit_flow,
            sum_voice_used: self.sum_voice_used,
            limit_voice_used: self.limit_voice_used,
            non_limit_voice_used: self.non_limit_voice_used,
            sum_voice: self.sum_voice,
            limit_voice: self.limit_voice,
            non_limit_voice: self.non_limit_voice,
        }
    }
}

pub fn build_last(data: ChinaUnicomData, user: String, bot: String) -> super::LastModel {
    super::LastModel {
        user,
        bot,
        package_name: data.package_name,
        time: data.time,
        sum_flow_used: data.sum_flow_used,
        limit_flow_used: data.limit_flow_used,
        non_limit_flow_used: data.non_limit_flow_used,
        free_flow_used: data.free_flow_used,
        non_free_flow_used: data.non_free_flow_used,
        sum_flow: data.sum_flow,
        limit_flow: data.limit_flow,
        non_limit_flow: data.non_limit_flow,
        sum_voice_used: data.sum_voice_used,
        limit_voice_used: data.limit_voice_used,
        non_limit_voice_used: data.non_limit_voice_used,
        sum_voice: data.sum_voice,
        limit_voice: data.limit_voice,
        non_limit_voice: data.non_limit_voice,
    }
}

pub fn build_last_active(
    data: ChinaUnicomData,
    user: String,
    bot: String,
) -> super::LastActiveModel {
    super::LastActiveModel {
        user: Unchanged(user),
        bot: Unchanged(bot),
        package_name: Set(data.package_name),
        time: Set(data.time),
        sum_flow_used: Set(data.sum_flow_used),
        limit_flow_used: Set(data.limit_flow_used),
        non_limit_flow_used: Set(data.non_limit_flow_used),
        free_flow_used: Set(data.free_flow_used),
        non_free_flow_used: Set(data.non_free_flow_used),
        sum_flow: Set(data.sum_flow),
        limit_flow: Set(data.limit_flow),
        non_limit_flow: Set(data.non_limit_flow),
        sum_voice_used: Set(data.sum_voice_used),
        limit_voice_used: Set(data.limit_voice_used),
        non_limit_voice_used: Set(data.non_limit_voice_used),
        sum_voice: Set(data.sum_voice),
        limit_voice: Set(data.limit_voice),
        non_limit_voice: Set(data.non_limit_voice),
    }
}

#[derive(Copy, Clone, Debug, EnumIter)]
pub enum Relation {
    Config,
}

impl RelationTrait for Relation {
    fn def(&self) -> RelationDef {
        match self {
            Relation::Config => Entity::belongs_to(super::config::Entity)
                .from(Column::User)
                .to(super::config::Column::User)
                .into(),
        }
    }
}

impl Related<super::config::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Config.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
