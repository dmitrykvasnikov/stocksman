use std::{
    fmt, fs,
    path::Path,
    sync::{Mutex, MutexGuard},
};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    name: "user configuration",
    sql: include_str!("../migrations/0001_user_configuration.sql"),
}];

const CONFIGURATION_ID: i64 = 1;
const MAX_PREFERENCE_LENGTH: usize = 128;

struct Migration {
    version: i64,
    name: &'static str,
    sql: &'static str,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemePreference {
    System,
    Dark,
    Light,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct UserConfiguration {
    pub theme: ThemePreference,
    pub locale: Option<String>,
    pub time_zone: Option<String>,
}

impl Default for UserConfiguration {
    fn default() -> Self {
        Self {
            theme: ThemePreference::System,
            locale: None,
            time_zone: None,
        }
    }
}

impl UserConfiguration {
    pub fn validate(&self) -> Result<(), PersistenceError> {
        validate_optional_preference("locale", self.locale.as_deref())?;
        validate_optional_preference("time_zone", self.time_zone.as_deref())
    }
}

pub struct ConfigurationStore {
    connection: Mutex<Connection>,
}

impl ConfigurationStore {
    pub fn open(path: &Path) -> Result<Self, PersistenceError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let connection = Connection::open(path)?;
        Self::initialize(connection)
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self, PersistenceError> {
        Self::initialize(Connection::open_in_memory()?)
    }

    fn initialize(mut connection: Connection) -> Result<Self, PersistenceError> {
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        connection.execute_batch("PRAGMA foreign_keys = ON;")?;
        run_migrations(&mut connection)?;

        let store = Self {
            connection: Mutex::new(connection),
        };
        store.ensure_default_configuration()?;
        Ok(store)
    }

    pub fn load(&self) -> Result<UserConfiguration, PersistenceError> {
        let connection = self.connection()?;
        let document: String = connection
            .query_row(
                "SELECT document FROM user_configuration WHERE id = ?1",
                [CONFIGURATION_ID],
                |row| row.get(0),
            )
            .optional()?
            .ok_or(PersistenceError::MissingConfiguration)?;

        let configuration = serde_json::from_str::<UserConfiguration>(&document)?;
        configuration.validate()?;
        Ok(configuration)
    }

    pub fn save(
        &self,
        configuration: &UserConfiguration,
    ) -> Result<UserConfiguration, PersistenceError> {
        configuration.validate()?;
        let document = serde_json::to_string(configuration)?;
        let connection = self.connection()?;
        connection.execute(
            "INSERT INTO user_configuration (id, document, updated_at)
             VALUES (?1, ?2, unixepoch())
             ON CONFLICT(id) DO UPDATE SET
                 document = excluded.document,
                 updated_at = excluded.updated_at",
            params![CONFIGURATION_ID, document],
        )?;
        Ok(configuration.clone())
    }

    fn ensure_default_configuration(&self) -> Result<(), PersistenceError> {
        let document = serde_json::to_string(&UserConfiguration::default())?;
        let connection = self.connection()?;
        connection.execute(
            "INSERT OR IGNORE INTO user_configuration (id, document) VALUES (?1, ?2)",
            params![CONFIGURATION_ID, document],
        )?;
        Ok(())
    }

    fn connection(&self) -> Result<MutexGuard<'_, Connection>, PersistenceError> {
        self.connection
            .lock()
            .map_err(|_| PersistenceError::LockPoisoned)
    }
}

fn run_migrations(connection: &mut Connection) -> Result<(), PersistenceError> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            applied_at INTEGER NOT NULL DEFAULT (unixepoch())
        ) STRICT;",
    )?;

    let latest_supported = MIGRATIONS.last().map_or(0, |migration| migration.version);
    let latest_applied: i64 = connection.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        [],
        |row| row.get(0),
    )?;
    if latest_applied > latest_supported {
        return Err(PersistenceError::DatabaseTooNew {
            found: latest_applied,
            supported: latest_supported,
        });
    }

    for migration in MIGRATIONS {
        let already_applied: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version = ?1)",
            [migration.version],
            |row| row.get(0),
        )?;
        if already_applied {
            continue;
        }

        let transaction = connection.transaction()?;
        transaction.execute_batch(migration.sql)?;
        transaction.execute(
            "INSERT INTO schema_migrations (version, name) VALUES (?1, ?2)",
            params![migration.version, migration.name],
        )?;
        transaction.commit()?;
    }

    Ok(())
}

fn validate_optional_preference(
    field: &'static str,
    value: Option<&str>,
) -> Result<(), PersistenceError> {
    let Some(value) = value else {
        return Ok(());
    };

    if value.is_empty()
        || value.trim() != value
        || value.len() > MAX_PREFERENCE_LENGTH
        || value.chars().any(char::is_control)
    {
        return Err(PersistenceError::InvalidPreference(field));
    }

    Ok(())
}

#[derive(Debug)]
pub enum PersistenceError {
    Database(rusqlite::Error),
    DatabaseTooNew { found: i64, supported: i64 },
    InvalidPreference(&'static str),
    Io(std::io::Error),
    Json(serde_json::Error),
    LockPoisoned,
    MissingConfiguration,
}

impl fmt::Display for PersistenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Database(error) => write!(formatter, "database error: {error}"),
            Self::DatabaseTooNew { found, supported } => write!(
                formatter,
                "database schema version {found} is newer than supported version {supported}"
            ),
            Self::InvalidPreference(field) => {
                write!(formatter, "invalid user configuration field: {field}")
            }
            Self::Io(error) => write!(formatter, "database directory error: {error}"),
            Self::Json(error) => write!(formatter, "configuration encoding error: {error}"),
            Self::LockPoisoned => write!(formatter, "database lock is unavailable"),
            Self::MissingConfiguration => write!(formatter, "user configuration is missing"),
        }
    }
}

impl std::error::Error for PersistenceError {}

impl PersistenceError {
    pub fn is_invalid_configuration(&self) -> bool {
        matches!(self, Self::InvalidPreference(_))
    }
}

impl From<rusqlite::Error> for PersistenceError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(error)
    }
}

impl From<std::io::Error> for PersistenceError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for PersistenceError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;

    struct TestDatabase {
        path: PathBuf,
    }

    impl TestDatabase {
        fn new() -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "stocksman-configuration-{}-{unique}.sqlite3",
                std::process::id()
            ));
            Self { path }
        }
    }

    impl Drop for TestDatabase {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.path);
            let _ = fs::remove_file(self.path.with_extension("sqlite3-shm"));
            let _ = fs::remove_file(self.path.with_extension("sqlite3-wal"));
        }
    }

    #[test]
    fn migrations_are_idempotent_and_create_defaults() {
        let store = ConfigurationStore::open_in_memory().expect("open database");
        assert_eq!(
            store.load().expect("load defaults"),
            UserConfiguration::default()
        );

        let mut connection = store.connection().expect("database lock");
        run_migrations(&mut connection).expect("reapply migrations");
        let migrations: i64 = connection
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .expect("count migrations");
        assert_eq!(migrations, 1);
    }

    #[test]
    fn configuration_survives_reopening_the_database() {
        let database = TestDatabase::new();
        let expected = UserConfiguration {
            theme: ThemePreference::Dark,
            locale: Some("en-GB".to_owned()),
            time_zone: Some("Europe/Kaliningrad".to_owned()),
        };

        {
            let store = ConfigurationStore::open(&database.path).expect("open database");
            store.save(&expected).expect("save configuration");
        }

        let reopened = ConfigurationStore::open(&database.path).expect("reopen database");
        assert_eq!(reopened.load().expect("load configuration"), expected);
    }

    #[test]
    fn invalid_preferences_are_not_saved() {
        let store = ConfigurationStore::open_in_memory().expect("open database");
        let invalid = UserConfiguration {
            locale: Some(String::new()),
            ..UserConfiguration::default()
        };

        assert!(matches!(
            store.save(&invalid),
            Err(PersistenceError::InvalidPreference("locale"))
        ));
        assert_eq!(
            store.load().expect("load defaults"),
            UserConfiguration::default()
        );
    }
}
