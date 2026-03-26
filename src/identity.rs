use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TwitchIdentity {
    pub user_id: String,
    pub login: String,
    pub display_name: String,
}

impl TwitchIdentity {
    pub fn new(
        user_id: impl Into<String>,
        login: impl Into<String>,
        display_name: impl Into<String>,
    ) -> Self {
        Self {
            user_id: user_id.into(),
            login: login.into(),
            display_name: display_name.into(),
        }
    }
}
