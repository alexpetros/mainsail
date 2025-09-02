use std::fmt::Debug;
use std::ops::{Deref, DerefMut};
use rusqlite::{Connection, Error, Params};
use tracing;
use uuid::Uuid;

use crate::local::InternalResult;
use crate::activitypub::signature::generate_pkey_string;

const CREATE_MIGRATIONS_TABLE: &str = "
    CREATE TABLE IF NOT EXISTS _migrations (
        filename TEXT,
        timestamp TEXT DEFAULT CURRENT_TIMESTAMP
    ) STRICT;";

const MIGRATIONS: &[(&str, &str)] = &include!(concat!(env!("OUT_DIR"), "/migrations.rs"));

pub struct Database {
    pub db: Connection,
    path_string: String,
}

impl Debug for Database {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.path_string)
    }
}

impl Database {
    pub fn new(path: &str) -> Result<Self, Error> {
        let db = Connection::open(path)?;
        db.pragma_update(None, "journal_mode", "WAL")?;
        db.pragma_update(None, "foreign_keys", "ON")?;
        db.pragma_update(None, "synchronous", "NORMAL")?;

        Ok(Database { db, path_string: path.to_owned() })
    }

    pub fn new_in_memory() -> Result<Self, Error> {
        let name = Uuid::new_v4().to_string();
        let path = format!("file:{name}?mode=memory&cache=shared");
        Database::new(&path)
    }

    /// Execute a single statement with defer_foreign_keys set to ON
    ///
    /// This allows you to use INSERT OR REPLACE without triggering an update or delete until after
    /// everything is over.
    pub fn execute_deferred_foreign_keys<P>(&mut self, sql: &str, params: P) -> Result<usize, Error>
    where
        P: Params,
    {
        let tx = self.db.transaction()?;
        tx.pragma_update(None, "defer_foreign_keys", "ON")?;
        let res = tx.execute(sql, params)?;
        tx.commit()?;
        Ok(res)
    }

    pub fn run_migrations(&self) -> InternalResult<()> {
        // Run migrations
        // TODO this should be in a transaction
        for migration in MIGRATIONS {
            let exists: bool = self.query_row(
                "SELECT EXISTS (SELECT filename FROM _migrations WHERE filename = ?1)",
                (migration.0,),
                |row| { row.get(0) },
            )?;
            if exists {
                tracing::debug!("Skipping previously-applied migration {}", migration.0);
            } else {
                let (name, code) = migration;
                tracing::debug!("Applying migration {}", name);
                self.execute_batch(code)?;
                self.execute("INSERT INTO _migrations (filename) VALUES (?1)", [name])?;
            }
        }

        Ok(())
    }

    pub fn init(&self) -> InternalResult<()> {
        self.execute(CREATE_MIGRATIONS_TABLE, ())?;
        self.run_migrations()?;
        let pkey = generate_pkey_string()?;
        self.execute("INSERT OR REPLACE INTO globals (key, value) VALUES ('server_pkey', ?1)", [pkey])?;
        Ok(())
    }

    pub fn set_domain(&self, domain: &str) -> InternalResult<()> {
        self.execute("INSERT OR REPLACE INTO globals (key, value) VALUES ('domain', ?1)", [domain])?;
        Ok(())
    }

    pub fn execute<P: Params>(&self, sql: &str, params: P) -> rusqlite::Result<usize> {
        let res = self.db.execute(sql, params);
        if let Err(err) = res.as_ref() {
            tracing::error!("Failed execute SQL query: {}\nerror: {}", sql, err);
        }
        res
    }

    pub fn get_path(&self) -> &str {
        &self.path_string
    }

    pub fn clone_conn(&self) -> InternalResult<Database> {
        let res = Database::new(self.get_path())?;
        Ok(res)
    }
}

impl Deref for Database {
    type Target = Connection;
    fn deref(&self) -> &Self::Target {
        &self.db
    }
}

impl DerefMut for Database {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.db
    }
}

pub fn initialize_db(path: &str) -> InternalResult<()> {
    let db = Database::new(path)?;
    db.init()?;
    Ok(())
}

#[macro_export]
macro_rules! query_row {
    (
        $db:expr_2021,
        $struct_name:ident { $( $field_name:ident : $type:ty ),+ },
        $query:literal,
        $params:expr_2021
    ) => {{
        #[derive(Debug, serde::Serialize, serde::Deserialize)]
        struct $struct_name {
            $( $field_name : $type),+
        }
        let query_str = concat!("SELECT ", $crate::join_with_commas!($( $field_name ),+), " ", $query);
        let mut query = $db.prepare(query_str)?;
        let row = query.query_row($params, |row| {
            let item = $crate::make_struct!(row, $struct_name, $( $field_name ),+);
            Ok(item)
        });
        row
    }}
}

#[macro_export]
macro_rules! query_row_custom {
    (
        $db:expr_2021,
        $struct_name:ident { $( $field_name:ident : $type:ty ),+ },
        $query:literal,
        $params:expr_2021
    ) => {{
        #[derive(Debug, serde::Serialize, serde::Deserialize)]
        struct $struct_name {
            $( $field_name : $type),+
        }
        let mut query = $db.prepare($query)?;
        let row = query.query_row($params, |row| {
            let item = $crate::make_struct!(row, $struct_name, $( $field_name ),+);
            Ok(item)
        });
        row
    }}
}

#[macro_export]
macro_rules! query_map {
    (
        $db:expr_2021,
        $struct_name:ident { $( $field_name:ident : $type:ty ),+ },
        $query:literal,
        $params:expr_2021
    ) => {{
        #[derive(Debug, serde::Serialize, serde::Deserialize)]
        struct $struct_name {
            $( $field_name : $type),+
        }
        let query_str = concat!("SELECT ", $crate::join_with_commas!($( $field_name ),+), " ", $query);
        let mut query = $db.prepare(query_str)?;
        let rows = query.query_map($params, |row| {
            let item = $crate::make_struct!(row, $struct_name, $( $field_name ),+);
            Ok(item)
        })?;

        let rows: Vec<$struct_name> = rows.collect::<Result<_, _>>()?;
        rows
    }}
}

#[macro_export]
macro_rules! query_map_custom {
    (
        $db:expr_2021,
        $struct_name:ident { $( $field_name:ident : $type:ty ),+ },
        $query:literal,
        $params:expr_2021
    ) => {{
        #[derive(Debug, serde::Serialize, serde::Deserialize)]
        struct $struct_name {
            $( $field_name : $type),+
        }
        let mut query = $db.prepare($query)?;
        let rows = query.query_map($params, |row| {
            let item = $crate::make_struct!(row, $struct_name, $( $field_name ),+);
            Ok(item)
        })?;

        let rows: Vec<$struct_name> = rows.collect::<Result<_, _>>()?;
        rows
    }}
}

/// Create a struct recursively
/// Rust does now allow helper macros to populate the body of a struct definition
/// So instead we turn each row.get(x) into a token, recursively, and then populate the struct
/// with those tokens.
#[macro_export]
macro_rules! make_struct {
    (@ $row:expr_2021, $_count:expr_2021, $struct_name:ident, { } [ $($result:tt)* ]) => {
        $struct_name {
            $($result)*
        }
    };

    (@
     $row:expr_2021,
     $count:expr_2021,
     $struct_name:ident,
     { $first_field_name:ident $($field_name:ident)* }
     [ $($result:tt)* ]) => {
        $crate::make_struct!(
            @
            $row,
            $count + 1,
            $struct_name,
            { $($field_name)* }
            [$($result)* $first_field_name: $row.get($count)?, ])
    };

    ($row:expr_2021, $struct_name:ident, $( $field_name:ident ),+ )  => {
        $crate::make_struct!(@ $row, 0, $struct_name, { $($field_name)* } [])
    };
}

/// Stringify and join a series of identifiers with commas
#[macro_export]
macro_rules! join_with_commas {
    ( $name:ident ) => {
        stringify!($name)
    };
    ( $first:ident, $( $name:ident ),+ ) => {
        concat!(stringify!($first), ", ", $crate::join_with_commas!( $( $name ),+ ))
    };
}
