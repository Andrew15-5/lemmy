use activitystreams::{
  base::BaseExt,
  object::{kind::ImageType, Tombstone, TombstoneExt},
};
use anyhow::anyhow;
use chrono::NaiveDateTime;
use lemmy_apub_lib::values::MediaTypeMarkdown;
use lemmy_utils::{utils::convert_datetime, LemmyError};
use url::Url;

pub mod comment;
pub mod community;
pub mod person;
pub mod post;
pub mod private_message;

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
  content: String,
  media_type: MediaTypeMarkdown,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageObject {
  #[serde(rename = "type")]
  kind: ImageType,
  url: Url,
}

/// Updated is actually the deletion time
fn create_tombstone<T>(
  deleted: bool,
  object_id: Url,
  updated: Option<NaiveDateTime>,
  former_type: T,
) -> Result<Tombstone, LemmyError>
where
  T: ToString,
{
  if deleted {
    if let Some(updated) = updated {
      let mut tombstone = Tombstone::new();
      tombstone.set_id(object_id);
      tombstone.set_former_type(former_type.to_string());
      tombstone.set_deleted(convert_datetime(updated));
      Ok(tombstone)
    } else {
      Err(anyhow!("Cant convert to tombstone because updated time was None.").into())
    }
  } else {
    Err(anyhow!("Cant convert object to tombstone if it wasnt deleted").into())
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use actix::Actor;
  use diesel::{
    r2d2::{ConnectionManager, Pool},
    PgConnection,
  };
  use lemmy_apub_lib::activity_queue::create_activity_queue;
  use lemmy_db_schema::{
    establish_unpooled_connection,
    get_database_url_from_env,
    source::secret::Secret,
  };
  use lemmy_utils::{
    rate_limit::{rate_limiter::RateLimiter, RateLimit},
    request::build_user_agent,
    settings::structs::Settings,
  };
  use lemmy_websocket::{chat_server::ChatServer, LemmyContext};
  use reqwest::Client;
  use serde::de::DeserializeOwned;
  use std::{fs::File, io::BufReader, sync::Arc};
  use tokio::sync::Mutex;

  // TODO: would be nice if we didnt have to use a full context for tests.
  //       or at least write a helper function so this code is shared with main.rs
  pub(crate) fn init_context() -> LemmyContext {
    // call this to run migrations
    establish_unpooled_connection();
    let settings = Settings::init().unwrap();
    let rate_limiter = RateLimit {
      rate_limiter: Arc::new(Mutex::new(RateLimiter::default())),
      rate_limit_config: settings.rate_limit.to_owned().unwrap_or_default(),
    };
    let client = Client::builder()
      .user_agent(build_user_agent(&settings))
      .build()
      .unwrap();
    let activity_queue = create_activity_queue();
    let secret = Secret {
      id: 0,
      jwt_secret: "".to_string(),
    };
    let db_url = match get_database_url_from_env() {
      Ok(url) => url,
      Err(_) => settings.get_database_url(),
    };
    let manager = ConnectionManager::<PgConnection>::new(&db_url);
    let pool = Pool::builder()
      .max_size(settings.database.pool_size)
      .build(manager)
      .unwrap_or_else(|_| panic!("Error connecting to {}", db_url));
    async fn x() -> Result<String, LemmyError> {
      Ok("".to_string())
    }
    let chat_server = ChatServer::startup(
      pool.clone(),
      rate_limiter.clone(),
      |_, _, _, _| Box::pin(x()),
      |_, _, _, _| Box::pin(x()),
      client.clone(),
      activity_queue.clone(),
      settings.clone(),
      secret.clone(),
    )
    .start();
    LemmyContext::create(pool, chat_server, client, activity_queue, settings, secret)
  }

  pub(crate) fn file_to_json_object<T: DeserializeOwned>(path: &str) -> T {
    let file = File::open(path).unwrap();
    let reader = BufReader::new(file);
    serde_json::from_reader(reader).unwrap()
  }
}
