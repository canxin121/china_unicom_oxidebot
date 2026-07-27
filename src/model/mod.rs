pub mod account;
pub mod account_state;

// Legacy entities remain available only for forward database migration.
pub mod config;
pub mod daily;
pub mod last;
pub mod usage_state;

pub use account::{
    ActiveModel as AccountActiveModel, Entity as AccountEntity, Model as AccountModel,
};
pub use account_state::{
    ActiveModel as AccountStateActiveModel, Entity as AccountStateEntity,
    Model as AccountStateModel,
};
pub use config::Entity as LegacyConfigEntity;
pub use daily::Entity as LegacyDailyEntity;
pub use last::Entity as LegacyLastEntity;
pub use usage_state::Entity as LegacyUsageStateEntity;
