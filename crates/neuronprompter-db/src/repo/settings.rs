// =============================================================================
// Settings repository operations.
//
// Manages the app_settings key-value store and per-user user_settings records.
// App settings store global preferences (e.g., last_user_id). User settings
// store per-user UI preferences, Ollama configuration, and sorting options.
// =============================================================================

use neuronprompter_core::CoreError;
use neuronprompter_core::constants::DEFAULT_OLLAMA_URL;
use neuronprompter_core::domain::settings::{
    AppSetting, SortDirection, SortField, Theme, UserSettings,
};
use rusqlite::params;

use crate::DbError;

/// Retrieves a global application setting by key. Returns None if the key
/// does not exist.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn get_app_setting(
    conn: &rusqlite::Connection,
    key: &str,
) -> Result<Option<AppSetting>, DbError> {
    let result = conn.query_row(
        "SELECT key, value FROM app_settings WHERE key = ?1",
        params![key],
        |row| {
            Ok(AppSetting {
                key: row.get("key")?,
                value: row.get("value")?,
            })
        },
    );
    match result {
        Ok(setting) => Ok(Some(setting)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(DbError::Query {
            operation: "get_app_setting".to_owned(),
            source: e,
        }),
    }
}

/// Inserts or updates a global application setting.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn set_app_setting(conn: &rusqlite::Connection, key: &str, value: &str) -> Result<(), DbError> {
    conn.execute(
        "INSERT INTO app_settings (key, value) VALUES (?1, ?2) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )
    .map_err(|e| DbError::Query {
        operation: "set_app_setting".to_owned(),
        source: e,
    })?;
    Ok(())
}

/// Retrieves per-user settings by user id.
///
/// # Errors
///
/// Returns `DbError::Core` with `CoreError::NotFound` if no settings exist
/// for the given user id.
/// Returns `DbError::Query` if the SQL statement fails.
pub fn get_user_settings(
    conn: &rusqlite::Connection,
    user_id: i64,
) -> Result<UserSettings, DbError> {
    conn.query_row(
        "SELECT user_id, theme, last_collection_id, sidebar_collapsed, \
         sort_field, sort_direction, ollama_base_url, ollama_model, extra \
         FROM user_settings WHERE user_id = ?1",
        params![user_id],
        row_to_user_settings,
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => DbError::Core(CoreError::NotFound {
            entity: "UserSettings".to_owned(),
            id: user_id,
        }),
        other => DbError::Query {
            operation: "get_user_settings".to_owned(),
            source: other,
        },
    })
}

/// Inserts or updates per-user settings. Uses INSERT OR REPLACE to handle
/// both creation and update in a single statement.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn upsert_user_settings(
    conn: &rusqlite::Connection,
    settings: &UserSettings,
) -> Result<(), DbError> {
    let theme_str = serialize_theme(&settings.theme);
    let sort_field_str = serialize_sort_field(&settings.sort_field);
    let sort_dir_str = serialize_sort_direction(&settings.sort_direction);

    conn.execute(
        "INSERT INTO user_settings \
         (user_id, theme, last_collection_id, sidebar_collapsed, \
          sort_field, sort_direction, ollama_base_url, ollama_model, extra) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) \
         ON CONFLICT(user_id) DO UPDATE SET \
           theme = excluded.theme, \
           last_collection_id = excluded.last_collection_id, \
           sidebar_collapsed = excluded.sidebar_collapsed, \
           sort_field = excluded.sort_field, \
           sort_direction = excluded.sort_direction, \
           ollama_base_url = excluded.ollama_base_url, \
           ollama_model = excluded.ollama_model, \
           extra = excluded.extra",
        params![
            settings.user_id,
            theme_str,
            settings.last_collection_id,
            settings.sidebar_collapsed,
            sort_field_str,
            sort_dir_str,
            settings.ollama_base_url,
            settings.ollama_model,
            settings.extra,
        ],
    )
    .map_err(|e| DbError::Query {
        operation: "upsert_user_settings".to_owned(),
        source: e,
    })?;
    Ok(())
}

/// Creates a default settings row for a user with standard values:
/// theme=dark, `ollama_base_url`=[`DEFAULT_OLLAMA_URL`],
/// `sort_field`=`updated_at`, `sort_direction`=desc.
///
/// # Errors
///
/// Returns `DbError::Query` if the SQL statement fails.
pub fn create_default_user_settings(
    conn: &rusqlite::Connection,
    user_id: i64,
) -> Result<(), DbError> {
    conn.execute(
        "INSERT INTO user_settings \
         (user_id, theme, sidebar_collapsed, sort_field, sort_direction, \
          ollama_base_url, extra) \
         VALUES (?1, 'dark', 0, 'updated_at', 'desc', ?2, '{}')",
        params![user_id, DEFAULT_OLLAMA_URL],
    )
    .map_err(|e| DbError::Query {
        operation: "create_default_user_settings".to_owned(),
        source: e,
    })?;
    Ok(())
}

/// Maps a `rusqlite` row to a `UserSettings` struct, parsing enum fields from
/// their string representations.
fn row_to_user_settings(row: &rusqlite::Row<'_>) -> rusqlite::Result<UserSettings> {
    let theme_str: String = row.get("theme")?;
    let sort_field_str: String = row.get("sort_field")?;
    let sort_dir_str: String = row.get("sort_direction")?;

    Ok(UserSettings {
        user_id: row.get("user_id")?,
        theme: parse_theme(&theme_str),
        last_collection_id: row.get("last_collection_id")?,
        sidebar_collapsed: row.get("sidebar_collapsed")?,
        sort_field: parse_sort_field(&sort_field_str),
        sort_direction: parse_sort_direction(&sort_dir_str),
        ollama_base_url: row.get("ollama_base_url")?,
        ollama_model: row.get("ollama_model")?,
        extra: row.get("extra")?,
    })
}

/// Converts a `Theme` enum to its database string representation.
fn serialize_theme(theme: &Theme) -> &'static str {
    match theme {
        Theme::Light => "light",
        Theme::Dark => "dark",
        Theme::System => "system",
    }
}

/// Parses a theme string from the database. Defaults to `Dark` for
/// unrecognized values.
fn parse_theme(s: &str) -> Theme {
    match s {
        "light" => Theme::Light,
        "dark" => Theme::Dark,
        "system" => Theme::System,
        other => {
            tracing::warn!(
                value = other,
                "unrecognized theme value, falling back to Dark"
            );
            Theme::Dark
        }
    }
}

/// Converts a `SortField` enum to its database string representation.
fn serialize_sort_field(field: &SortField) -> &'static str {
    match field {
        SortField::UpdatedAt => "updated_at",
        SortField::CreatedAt => "created_at",
        SortField::Title => "title",
    }
}

/// Parses a sort field string from the database. Defaults to `UpdatedAt` for
/// unrecognized values.
fn parse_sort_field(s: &str) -> SortField {
    match s {
        "created_at" => SortField::CreatedAt,
        "title" => SortField::Title,
        "updated_at" => SortField::UpdatedAt,
        other => {
            tracing::warn!(
                value = other,
                "unrecognized sort_field value, falling back to UpdatedAt"
            );
            SortField::UpdatedAt
        }
    }
}

/// Converts a `SortDirection` enum to its database string representation.
fn serialize_sort_direction(dir: &SortDirection) -> &'static str {
    match dir {
        SortDirection::Asc => "asc",
        SortDirection::Desc => "desc",
    }
}

/// Parses a sort direction string from the database. Defaults to `Desc` for
/// unrecognized values.
fn parse_sort_direction(s: &str) -> SortDirection {
    match s {
        "asc" => SortDirection::Asc,
        "desc" => SortDirection::Desc,
        other => {
            tracing::warn!(
                value = other,
                "unrecognized sort_direction value, falling back to Desc"
            );
            SortDirection::Desc
        }
    }
}
