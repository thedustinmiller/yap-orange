//! Safe wrapper over the raw `sqlite-wasm-rs` FFI.
//!
//! Provides `WasmDb` — a safe, single-connection SQLite handle that wraps
//! the prepare/bind/step/finalize lifecycle. All columns are TEXT or NULL
//! (matching the yap-orange SQLite schema where UUIDs, timestamps, and JSON
//! are stored as TEXT).

use std::ffi::CString;
use std::ffi::{c_char, c_int};
use std::ptr;

use sqlite_wasm_rs::{
    sqlite3, sqlite3_stmt,
    sqlite3_bind_int, sqlite3_bind_null, sqlite3_bind_text,
    sqlite3_changes, sqlite3_close, sqlite3_column_count, sqlite3_column_int,
    sqlite3_column_text, sqlite3_column_type, sqlite3_errmsg, sqlite3_exec,
    sqlite3_finalize, sqlite3_open_v2, sqlite3_prepare_v2, sqlite3_step,
    SQLITE_DONE, SQLITE_NULL, SQLITE_OK, SQLITE_OPEN_CREATE, SQLITE_OPEN_READWRITE,
    SQLITE_ROW, SQLITE_TRANSIENT,
};

use yap_core::error::Error;

type Result<T> = yap_core::error::Result<T>;

/// A single value that can be bound to a SQLite parameter.
pub enum Value<'a> {
    Text(&'a str),
    Int(i32),
    Null,
}

impl<'a> From<&'a str> for Value<'a> {
    fn from(s: &'a str) -> Self {
        Value::Text(s)
    }
}

impl<'a> From<&'a String> for Value<'a> {
    fn from(s: &'a String) -> Self {
        Value::Text(s.as_str())
    }
}

impl<'a> From<i32> for Value<'a> {
    fn from(i: i32) -> Self {
        Value::Int(i)
    }
}

impl<'a> From<Option<&'a str>> for Value<'a> {
    fn from(s: Option<&'a str>) -> Self {
        match s {
            Some(s) => Value::Text(s),
            None => Value::Null,
        }
    }
}

impl<'a> From<Option<&'a String>> for Value<'a> {
    fn from(s: Option<&'a String>) -> Self {
        match s {
            Some(s) => Value::Text(s.as_str()),
            None => Value::Null,
        }
    }
}

/// A row from a SQLite query result, providing column access.
pub struct Row {
    texts: Vec<Option<String>>,
    ints: Vec<i32>,
    types: Vec<c_int>,
}

impl Row {
    /// Get a TEXT column value. Returns empty string for NULL.
    pub fn get_text(&self, col: usize) -> &str {
        self.texts
            .get(col)
            .and_then(|v| v.as_deref())
            .unwrap_or("")
    }

    /// Get a TEXT column value, returning None for NULL.
    pub fn get_opt_text(&self, col: usize) -> Option<&str> {
        self.texts.get(col).and_then(|v| v.as_deref())
    }

    /// Get an INTEGER column value.
    pub fn get_int(&self, col: usize) -> i32 {
        self.ints.get(col).copied().unwrap_or(0)
    }

    /// Check if a column is NULL.
    pub fn is_null(&self, col: usize) -> bool {
        self.types.get(col).copied() == Some(SQLITE_NULL)
    }
}

/// Safe wrapper around a raw `sqlite3` database handle.
pub struct WasmDb {
    db: *mut sqlite3,
}

// SAFETY: WASM is single-threaded. The raw pointer is only accessed from
// the main thread (or the single Service Worker thread).
unsafe impl Send for WasmDb {}
unsafe impl Sync for WasmDb {}

impl WasmDb {
    /// Open (or create) a SQLite database at the given path.
    pub fn open(path: &str) -> Result<Self> {
        let c_path =
            CString::new(path).map_err(|e| Error::Database(format!("invalid path: {}", e)))?;
        let mut db: *mut sqlite3 = ptr::null_mut();
        let rc = unsafe {
            sqlite3_open_v2(
                c_path.as_ptr(),
                &mut db as *mut _,
                SQLITE_OPEN_READWRITE | SQLITE_OPEN_CREATE,
                ptr::null(),
            )
        };
        if rc != SQLITE_OK {
            let msg = Self::errmsg_raw(db);
            return Err(Error::Database(format!("sqlite3_open_v2 failed: {}", msg)));
        }
        Ok(Self { db })
    }

    /// Execute a SQL statement that returns no rows.
    /// Returns the number of rows changed.
    pub fn execute(&self, sql: &str, params: &[Value]) -> Result<u64> {
        let stmt = self.prepare(sql)?;
        self.bind_params(stmt, params)?;

        let rc = unsafe { sqlite3_step(stmt) };
        unsafe { sqlite3_finalize(stmt) };

        match rc {
            SQLITE_DONE => {
                let changes = unsafe { sqlite3_changes(self.db) };
                Ok(changes as u64)
            }
            SQLITE_ROW => {
                // RETURNING clause — we already finalized, just report success
                Ok(1)
            }
            _ => Err(self.last_error("sqlite3_step")),
        }
    }

    /// Execute raw SQL (possibly multiple statements). Used for migrations.
    pub fn exec(&self, sql: &str) -> Result<()> {
        let c_sql =
            CString::new(sql).map_err(|e| Error::Database(format!("invalid SQL: {}", e)))?;
        let rc = unsafe {
            sqlite3_exec(
                self.db,
                c_sql.as_ptr(),
                None,
                ptr::null_mut(),
                ptr::null_mut(),
            )
        };
        if rc != SQLITE_OK {
            return Err(self.last_error("sqlite3_exec"));
        }
        Ok(())
    }

    /// Execute a query and collect all result rows.
    pub fn query_rows<T>(
        &self,
        sql: &str,
        params: &[Value],
        map: impl Fn(&Row) -> T,
    ) -> Result<Vec<T>> {
        let stmt = self.prepare(sql)?;
        self.bind_params(stmt, params)?;

        let col_count = unsafe { sqlite3_column_count(stmt) } as usize;
        let mut results = Vec::new();

        loop {
            let rc = unsafe { sqlite3_step(stmt) };
            match rc {
                SQLITE_ROW => {
                    let row = self.read_row(stmt, col_count);
                    results.push(map(&row));
                }
                SQLITE_DONE => break,
                _ => {
                    unsafe { sqlite3_finalize(stmt) };
                    return Err(self.last_error("sqlite3_step"));
                }
            }
        }

        unsafe { sqlite3_finalize(stmt) };
        Ok(results)
    }

    /// Execute a query and return the first row, or None.
    pub fn query_optional<T>(
        &self,
        sql: &str,
        params: &[Value],
        map: impl Fn(&Row) -> T,
    ) -> Result<Option<T>> {
        let stmt = self.prepare(sql)?;
        self.bind_params(stmt, params)?;

        let col_count = unsafe { sqlite3_column_count(stmt) } as usize;
        let rc = unsafe { sqlite3_step(stmt) };

        let result = match rc {
            SQLITE_ROW => {
                let row = self.read_row(stmt, col_count);
                Ok(Some(map(&row)))
            }
            SQLITE_DONE => Ok(None),
            _ => Err(self.last_error("sqlite3_step")),
        };

        unsafe { sqlite3_finalize(stmt) };
        result
    }

    /// Execute a query and return exactly one row, or error.
    pub fn query_one<T>(
        &self,
        sql: &str,
        params: &[Value],
        map: impl Fn(&Row) -> T,
    ) -> Result<T> {
        self.query_optional(sql, params, map)?
            .ok_or_else(|| Error::NotFound("query returned no rows".to_string()))
    }

    /// Execute a query and return the first column of the first row as i32.
    pub fn query_scalar_int(&self, sql: &str, params: &[Value]) -> Result<i32> {
        self.query_one(sql, params, |r| r.get_int(0))
    }

    /// Execute a query and return the first column of the first row as optional text.
    pub fn query_scalar_opt_text(&self, sql: &str, params: &[Value]) -> Result<Option<String>> {
        let stmt = self.prepare(sql)?;
        self.bind_params(stmt, params)?;

        let rc = unsafe { sqlite3_step(stmt) };
        let result = match rc {
            SQLITE_ROW => {
                let col_type = unsafe { sqlite3_column_type(stmt, 0) };
                if col_type == SQLITE_NULL {
                    Ok(None)
                } else {
                    let ptr = unsafe { sqlite3_column_text(stmt, 0) };
                    if ptr.is_null() {
                        Ok(None)
                    } else {
                        let cstr = unsafe { core::ffi::CStr::from_ptr(ptr as *const c_char) };
                        Ok(Some(cstr.to_string_lossy().into_owned()))
                    }
                }
            }
            SQLITE_DONE => Ok(None),
            _ => Err(self.last_error("sqlite3_step")),
        };

        unsafe { sqlite3_finalize(stmt) };
        result
    }

    // ── Private helpers ──────────────────────────────────────────────────

    fn prepare(&self, sql: &str) -> Result<*mut sqlite3_stmt> {
        let c_sql =
            CString::new(sql).map_err(|e| Error::Database(format!("invalid SQL: {}", e)))?;
        let mut stmt: *mut sqlite3_stmt = ptr::null_mut();
        let rc = unsafe {
            sqlite3_prepare_v2(
                self.db,
                c_sql.as_ptr(),
                -1,
                &mut stmt as *mut _,
                ptr::null_mut(),
            )
        };
        if rc != SQLITE_OK {
            return Err(self.last_error("sqlite3_prepare_v2"));
        }
        Ok(stmt)
    }

    fn bind_params(&self, stmt: *mut sqlite3_stmt, params: &[Value]) -> Result<()> {
        for (i, param) in params.iter().enumerate() {
            let idx = (i + 1) as c_int; // SQLite params are 1-indexed
            let rc = match param {
                Value::Text(s) => {
                    let c_str = CString::new(*s)
                        .map_err(|e| Error::Database(format!("invalid param: {}", e)))?;
                    unsafe {
                        sqlite3_bind_text(
                            stmt,
                            idx,
                            c_str.as_ptr(),
                            s.len() as c_int,
                            SQLITE_TRANSIENT(),
                        )
                    }
                }
                Value::Int(v) => unsafe { sqlite3_bind_int(stmt, idx, *v) },
                Value::Null => unsafe { sqlite3_bind_null(stmt, idx) },
            };
            if rc != SQLITE_OK {
                return Err(self.last_error("sqlite3_bind"));
            }
        }
        Ok(())
    }

    fn read_row(&self, stmt: *mut sqlite3_stmt, col_count: usize) -> Row {
        let mut texts = Vec::with_capacity(col_count);
        let mut ints = Vec::with_capacity(col_count);
        let mut types = Vec::with_capacity(col_count);

        for i in 0..col_count {
            let col = i as c_int;
            let col_type = unsafe { sqlite3_column_type(stmt, col) };
            types.push(col_type);

            if col_type == SQLITE_NULL {
                texts.push(None);
                ints.push(0);
            } else {
                // Read as text (primary format for UUID/datetime/JSON columns)
                let ptr = unsafe { sqlite3_column_text(stmt, col) };
                if ptr.is_null() {
                    texts.push(None);
                } else {
                    let cstr = unsafe { core::ffi::CStr::from_ptr(ptr as *const c_char) };
                    texts.push(Some(cstr.to_string_lossy().into_owned()));
                }
                // Also read as int (for COUNT, EXISTS, etc.)
                ints.push(unsafe { sqlite3_column_int(stmt, col) });
            }
        }

        Row { texts, ints, types }
    }

    fn last_error(&self, context: &str) -> Error {
        let msg = Self::errmsg_raw(self.db);
        Error::Database(format!("{}: {}", context, msg))
    }

    fn errmsg_raw(db: *mut sqlite3) -> String {
        if db.is_null() {
            return "null db handle".to_string();
        }
        let ptr = unsafe { sqlite3_errmsg(db) };
        if ptr.is_null() {
            return "unknown error".to_string();
        }
        let cstr = unsafe { core::ffi::CStr::from_ptr(ptr) };
        cstr.to_string_lossy().into_owned()
    }
}

impl Drop for WasmDb {
    fn drop(&mut self) {
        if !self.db.is_null() {
            unsafe { sqlite3_close(self.db) };
        }
    }
}
