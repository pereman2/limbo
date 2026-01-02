mod opts;

use clap::Parser;
use core::panic;
use opts::Opts;
#[cfg(not(feature = "antithesis"))]
use rand::rngs::StdRng;
#[cfg(not(feature = "antithesis"))]
use rand::{Rng, SeedableRng};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::Arc;
#[cfg(not(feature = "antithesis"))]
use std::sync::Mutex as StdMutex;
use tokio::sync::Mutex;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::reload;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;
use turso::Builder;

#[derive(Debug, Clone)]
pub enum ProbSchedule {
    /// Always returns the same value.
    Constant { p: f64 },

    /// Linearly ramps from `start` to `end` over progress in [0, 1].
    ///
    /// NOTE: The generator should ensure `end` is sufficiently far from `start` to avoid near-constant ramps.
    Ramp { start: f64, end: f64 },

    /// Triangle wave: rises from `min` to `max` by `peak_at` then falls back to `min` by 1.0.
    ///
    /// NOTE: The generator should avoid `peak_at` too close to 0.0 or 1.0 to avoid sudden changes.
    Triangle { min: f64, max: f64, peak_at: f64 },

    /// Smooth sinusoidal schedule oscillating between `min` and `max`.
    /// `cycles` is the number of full oscillations over progress in [0, 1].
    /// `phase` is in radians.
    Sin {
        min: f64,
        max: f64,
        cycles: f64,
        phase: f64,
    },
}

fn clamp01(x: f64) -> f64 {
    if x < 0.0 {
        0.0
    } else if x > 1.0 {
        1.0
    } else {
        x
    }
}

impl ProbSchedule {
    pub fn value_at(&self, progress_0_1: f64) -> f64 {
        let t = clamp01(progress_0_1);
        match *self {
            ProbSchedule::Constant { p } => clamp01(p),
            ProbSchedule::Ramp { start, end } => clamp01(start + (end - start) * t),
            ProbSchedule::Triangle { min, max, peak_at } => {
                let peak = clamp01(peak_at.max(1e-9)); // avoid div by zero
                if t <= peak {
                    let u = t / peak;
                    clamp01(min + (max - min) * u)
                } else {
                    let denom = (1.0 - peak).max(1e-9);
                    let u = (t - peak) / denom;
                    clamp01(max + (min - max) * u)
                }
            }
            ProbSchedule::Sin {
                min,
                max,
                cycles,
                phase,
            } => {
                // Map sin output [-1, 1] -> [0, 1] then to [min, max]
                let amp = (max - min) / 2.0;
                let mid = (max + min) / 2.0;
                let angle = (2.0 * std::f64::consts::PI) * cycles * t + phase;
                let v = mid + amp * angle.sin();
                clamp01(v)
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct GenConfig {
    /// Probability schedule: INSERT attempts a constraint error.
    pub insert_attempt_constraint_error: ProbSchedule,

    /// Probability schedule: given we're attempting a constraint error, reuse an existing PK.
    pub insert_reuse_existing_pk_on_error: ProbSchedule,

    /// Probability schedule: given we're attempting a constraint error and column is UNIQUE, steal existing UNIQUE value.
    pub insert_steal_unique_value_on_error: ProbSchedule,

    /// Probability schedule: DELETE targets a missing/random PK (i.e. delete maybe-nonexistent).
    pub delete_target_missing_pk: ProbSchedule,

    /// Probability schedule: UPDATE targets a missing/random PK (i.e. update maybe-nonexistent).
    pub update_target_missing_pk: ProbSchedule,

    /// Probability schedule: weight for selecting INSERT vs UPDATE vs DELETE (can vary over time).
    pub w_insert: ProbSchedule,
    pub w_update: ProbSchedule,
    pub w_delete: ProbSchedule,
}

/// Draw a startup config by randomizing schedules once.
fn gen_startup_config() -> GenConfig {
    // Helper to pick one of the schedule shapes at startup.
    // Shapes:
    // - Constant: never changes.
    // - Ramp: increases or decreases over time.
    // - Triangle: increases then decreases.
    fn pick_schedule(min_p: f64, max_p: f64) -> ProbSchedule {
        let min_p = clamp01(min_p);
        let max_p = clamp01(max_p);
        let (lo, hi) = if min_p <= max_p {
            (min_p, max_p)
        } else {
            (max_p, min_p)
        };

        // Helper to sample in [lo, hi]
        let sample = || lo + (hi - lo) * ((get_random() % 1000) as f64 / 999.0);
        // Enforce a minimum delta so ramps/triangles don't become "almost constant".
        let min_delta = (hi - lo) * 0.15;

        match get_random() % 4 {
            0 => {
                let p = sample();
                ProbSchedule::Constant { p }
            }
            1 => {
                // Ramp with enforced separation between start and end.
                let start = sample();
                let mut end = sample();
                let mut tries = 0;
                while (end - start).abs() < min_delta && tries < 16 {
                    end = sample();
                    tries += 1;
                }
                // If still too close, force end to be at least min_delta away (clamped to bounds).
                if (end - start).abs() < min_delta {
                    end = if start + min_delta <= hi {
                        start + min_delta
                    } else if start - min_delta >= lo {
                        start - min_delta
                    } else {
                        // Fallback: push to far end
                        if start < (lo + hi) / 2.0 {
                            hi
                        } else {
                            lo
                        }
                    };
                }
                ProbSchedule::Ramp { start, end }
            }
            2 => {
                // Triangle with peak away from edges and with meaningful amplitude.
                let min = lo;
                let max = hi;
                let peak_at = 0.20 + 0.60 * ((get_random() % 1000) as f64 / 999.0); // keep away from extremes
                ProbSchedule::Triangle { min, max, peak_at }
            }
            _ => {
                // Sin: smooth oscillation, avoids sudden edges by design.
                let min = lo;
                let max = hi;
                let cycles = 0.5 + 2.5 * ((get_random() % 1000) as f64 / 999.0); // 0.5 .. 3.0 cycles
                let phase = (2.0 * std::f64::consts::PI) * ((get_random() % 1000) as f64 / 999.0);
                ProbSchedule::Sin {
                    min,
                    max,
                    cycles,
                    phase,
                }
            }
        }
    }

    // Keep ranges reasonable; tweak as desired.
    let insert_attempt_constraint_error = pick_schedule(0.00, 0.30);
    let insert_reuse_existing_pk_on_error = pick_schedule(0.30, 0.90);
    let insert_steal_unique_value_on_error = pick_schedule(0.00, 0.80);
    let delete_target_missing_pk = pick_schedule(0.00, 0.20);
    let update_target_missing_pk = pick_schedule(0.00, 0.20);

    // Operation-selection weights.
    // These are stored as schedules so they can be constant or time-varying, and are chosen at startup.
    //
    // We intentionally use a "weight" range (not necessarily summing to 1.0). The picker uses them proportionally.
    let w_insert = pick_schedule(1.0, 100.0);
    let w_update = pick_schedule(1.0, 100.0);
    let w_delete = pick_schedule(1.0, 100.0);

    GenConfig {
        insert_attempt_constraint_error,
        insert_reuse_existing_pk_on_error,
        insert_steal_unique_value_on_error,
        delete_target_missing_pk,
        update_target_missing_pk,
        w_insert,
        w_update,
        w_delete,
    }
}

#[cfg(not(feature = "antithesis"))]
static RNG: std::sync::OnceLock<StdMutex<StdRng>> = std::sync::OnceLock::new();

#[cfg(not(feature = "antithesis"))]
fn init_rng(seed: u64) {
    RNG.get_or_init(|| StdMutex::new(StdRng::seed_from_u64(seed)));
}

#[cfg(not(feature = "antithesis"))]
fn get_random() -> u64 {
    RNG.get()
        .expect("RNG not initialized")
        .lock()
        .unwrap()
        .random()
}

#[cfg(feature = "antithesis")]
fn get_random() -> u64 {
    antithesis_sdk::random::get_random()
}

#[derive(Debug)]
pub struct Plan {
    pub ddl_statements: Vec<String>,
    pub queries_per_thread: Vec<Vec<String>>,
    pub nr_iterations: usize,
    pub nr_threads: usize,
}

/// Represents a column in a SQLite table
#[derive(Debug, Clone)]
pub struct Column {
    pub name: String,
    pub data_type: DataType,
    pub constraints: Vec<Constraint>,
}

/// Represents SQLite data types
#[derive(Debug, Clone, PartialEq)]
pub enum DataType {
    Integer,
    Real,
    Text,
    Blob,
    Numeric,
}

/// Represents column constraints
#[derive(Debug, Clone, PartialEq)]
pub enum Constraint {
    PrimaryKey,
    NotNull,
    Unique,
}

/// Represents a table in a SQLite schema
#[derive(Debug, Clone)]
pub struct Table {
    pub name: String,
    pub columns: Vec<Column>,
    pub pk_values: Vec<String>,
}

/// In-memory representation of a row (SQL-literal strings per column name).
#[derive(Debug, Clone)]
pub struct RowState {
    pub values: BTreeMap<String, String>,
}

/// In-memory tracker of expected rows, keyed by primary key SQL-literal.
#[derive(Debug, Default, Clone)]
pub struct TableState {
    pub rows_by_pk: HashMap<String, RowState>,
}

#[derive(Debug, Default, Clone)]
pub struct ExpectedState {
    pub tables: HashMap<String, TableState>,
}

impl ExpectedState {
    pub fn new_from_schema(schema: &ArbitrarySchema) -> Self {
        let mut tables = HashMap::new();
        for t in &schema.tables {
            tables.insert(t.name.clone(), TableState::default());
        }
        Self { tables }
    }
}

/// Represents a complete SQLite schema
#[derive(Debug, Clone)]
pub struct ArbitrarySchema {
    pub tables: Vec<Table>,
}

// Word lists for generating readable identifiers
const ADJECTIVES: &[&str] = &[
    "red", "blue", "green", "fast", "slow", "big", "small", "old", "new", "hot", "cold", "dark",
    "light", "soft", "hard", "loud", "quiet", "sweet", "sour", "fresh", "dry", "wet", "clean",
    "dirty", "empty", "full", "happy", "sad", "angry", "calm", "brave", "shy", "smart", "wild",
];

const NOUNS: &[&str] = &[
    "cat", "dog", "bird", "fish", "tree", "rock", "lake", "river", "cloud", "star", "moon", "sun",
    "book", "desk", "chair", "door", "wall", "roof", "floor", "road", "path", "hill", "cave",
    "leaf", "root", "seed", "fruit", "flower", "grass", "stone", "sand", "wave", "wind", "rain",
];

// Helper functions for generating random data
fn generate_random_identifier() -> String {
    let adj = ADJECTIVES[get_random() as usize % ADJECTIVES.len()];
    let noun = NOUNS[get_random() as usize % NOUNS.len()];
    let num = get_random() % 1000;
    format!("{adj}_{noun}_{num}")
}

fn generate_random_data_type() -> DataType {
    match get_random() % 5 {
        0 => DataType::Integer,
        1 => DataType::Real,
        2 => DataType::Text,
        3 => DataType::Blob,
        _ => DataType::Numeric,
    }
}

fn generate_random_constraint() -> Constraint {
    match get_random() % 2 {
        0 => Constraint::NotNull,
        _ => Constraint::Unique,
    }
}

fn generate_random_column() -> Column {
    let name = generate_random_identifier();
    let data_type = generate_random_data_type();

    let constraint_count = (get_random() % 2) as usize;
    let mut constraints = Vec::with_capacity(constraint_count);

    for _ in 0..constraint_count {
        constraints.push(generate_random_constraint());
    }

    Column {
        name,
        data_type,
        constraints,
    }
}

fn generate_random_table() -> Table {
    let name = generate_random_identifier();
    let column_count = (get_random() % 10 + 1) as usize;
    let mut columns = Vec::with_capacity(column_count);
    let mut column_names = HashSet::new();

    // First, generate all columns without primary keys
    for _ in 0..column_count {
        let mut column = generate_random_column();

        // Ensure column names are unique within the table
        while column_names.contains(&column.name) {
            column.name = generate_random_identifier();
        }

        column_names.insert(column.name.clone());
        columns.push(column);
    }

    // Then, randomly select one column to be the primary key
    let pk_index = (get_random() % column_count as u64) as usize;
    columns[pk_index].constraints.push(Constraint::PrimaryKey);
    Table {
        name,
        columns,
        pk_values: vec![],
    }
}

pub fn gen_bool(probability_true: f64) -> bool {
    (get_random() as f64 / u64::MAX as f64) < probability_true
}

pub fn gen_schema(table_count: Option<usize>) -> ArbitrarySchema {
    let table_count = table_count.unwrap_or((get_random() % 10 + 1) as usize);
    let mut tables = Vec::with_capacity(table_count);
    let mut table_names = HashSet::new();

    for _ in 0..table_count {
        let mut table = generate_random_table();

        // Ensure table names are unique
        while table_names.contains(&table.name) {
            table.name = generate_random_identifier();
        }

        table_names.insert(table.name.clone());
        tables.push(table);
    }

    ArbitrarySchema { tables }
}

impl ArbitrarySchema {
    /// Convert the schema to a vector of SQL DDL statements
    pub fn to_sql(&self) -> Vec<String> {
        self.tables
            .iter()
            .map(|table| {
                let columns = table
                    .columns
                    .iter()
                    .map(|col| {
                        let mut col_def =
                            format!("  {} {}", col.name, data_type_to_sql(&col.data_type));
                        for constraint in &col.constraints {
                            col_def.push(' ');
                            col_def.push_str(&constraint_to_sql(constraint));
                        }
                        col_def
                    })
                    .collect::<Vec<_>>()
                    .join(",");

                format!("CREATE TABLE IF NOT EXISTS {} ({});", table.name, columns)
            })
            .collect()
    }
}

fn data_type_to_sql(data_type: &DataType) -> &'static str {
    match data_type {
        DataType::Integer => "INTEGER",
        DataType::Real => "REAL",
        DataType::Text => "TEXT",
        DataType::Blob => "BLOB",
        DataType::Numeric => "NUMERIC",
    }
}

fn constraint_to_sql(constraint: &Constraint) -> String {
    match constraint {
        Constraint::PrimaryKey => "PRIMARY KEY".to_string(),
        Constraint::NotNull => "NOT NULL".to_string(),
        Constraint::Unique => "UNIQUE".to_string(),
    }
}

/// Generate a random value for a given data type
fn generate_random_value(data_type: &DataType) -> String {
    match data_type {
        DataType::Integer => (get_random() % 1000).to_string(),
        DataType::Real => format!("{:.2}", (get_random() % 1000) as f64 / 100.0),
        DataType::Text => format!("'{}'", generate_random_identifier()),
        DataType::Blob => format!("x'{}'", hex::encode(generate_random_identifier())),
        DataType::Numeric => (get_random() % 1000).to_string(),
    }
}

/// Find the primary key column for a table.
fn pk_column<'a>(table: &'a Table) -> &'a Column {
    table
        .columns
        .iter()
        .find(|col| col.constraints.contains(&Constraint::PrimaryKey))
        .expect("Table should have a primary key")
}

/// Attempt to parse a simple equality predicate like `col = literal` and return the literal.
fn parse_simple_eq_literal(where_clause: &str, col_name: &str) -> Option<String> {
    let s = where_clause.trim();
    // Expected patterns: "{col} = {lit}" (with arbitrary spaces), no trailing semicolon.
    // We'll do a conservative parse: must start with col name, contain '=', and have non-empty RHS.
    let prefix = col_name;
    let s = s.strip_prefix(prefix)?;
    let s = s.trim_start();
    let s = s.strip_prefix('=')?;
    let rhs = s.trim();
    if rhs.is_empty() {
        None
    } else {
        Some(rhs.to_string())
    }
}

/// Generate an INSERT statement and (if it should succeed) update in-memory expected state.
fn generate_insert(
    table: &Table,
    expected: &mut ExpectedState,
    cfg: &GenConfig,
    progress_0_1: f64,
) -> String {
    let pk = pk_column(table);

    let table_state = expected
        .tables
        .get_mut(&table.name)
        .expect("expected table state missing");

    // Decide whether we attempt a constraint failure.
    let attempt_constraint_error =
        gen_bool(cfg.insert_attempt_constraint_error.value_at(progress_0_1))
            && !table_state.rows_by_pk.is_empty();

    // Choose PK value:
    // - If attempting constraint error: reuse existing PK with high probability.
    // - Otherwise: generate a fresh-ish value, but still might collide by chance.
    let pk_value = if attempt_constraint_error
        && gen_bool(cfg.insert_reuse_existing_pk_on_error.value_at(progress_0_1))
    {
        // Reuse an existing PK to trigger PRIMARY KEY/UNIQUE constraint.
        let idx = get_random() as usize % table_state.rows_by_pk.len();
        table_state
            .rows_by_pk
            .keys()
            .nth(idx)
            .expect("non-empty")
            .clone()
    } else {
        generate_random_value(&pk.data_type)
    };

    // Build row values.
    let mut values_by_col: BTreeMap<String, String> = BTreeMap::new();
    for col in &table.columns {
        let v = if col.name == pk.name {
            pk_value.clone()
        } else if attempt_constraint_error
            && col.constraints.contains(&Constraint::Unique)
            && gen_bool(
                cfg.insert_steal_unique_value_on_error
                    .value_at(progress_0_1),
            )
            && !table_state.rows_by_pk.is_empty()
        {
            // Try to violate a UNIQUE column by stealing an existing value from some row.
            let idx = get_random() as usize % table_state.rows_by_pk.len();
            let some_row = table_state.rows_by_pk.values().nth(idx).expect("non-empty");
            some_row
                .values
                .get(&col.name)
                .cloned()
                .unwrap_or_else(|| generate_random_value(&col.data_type))
        } else {
            generate_random_value(&col.data_type)
        };
        values_by_col.insert(col.name.clone(), v);
    }

    // Decide whether we expect the INSERT to succeed.
    // If we're attempting a constraint error, only mark success if the PK isn't already present.
    let expect_success = if attempt_constraint_error {
        !table_state.rows_by_pk.contains_key(&pk_value)
    } else {
        !table_state.rows_by_pk.contains_key(&pk_value)
    };

    if expect_success {
        table_state.rows_by_pk.insert(
            pk_value.clone(),
            RowState {
                values: values_by_col.clone(),
            },
        );
    }

    let columns = table
        .columns
        .iter()
        .map(|col| col.name.clone())
        .collect::<Vec<_>>()
        .join(", ");

    let values = table
        .columns
        .iter()
        .map(|col| {
            values_by_col
                .get(&col.name)
                .expect("missing col value")
                .clone()
        })
        .collect::<Vec<_>>()
        .join(", ");

    format!(
        "INSERT INTO {} ({}) VALUES ({});",
        table.name, columns, values
    )
}

/// Generate an UPDATE statement and update in-memory expected state.
/// Behavior:
/// - Prefer updating an existing row; with configurable probability use a random PK that likely doesn't exist.
fn generate_update(
    table: &Table,
    expected: &mut ExpectedState,
    cfg: &GenConfig,
    progress_0_1: f64,
) -> String {
    let pk = pk_column(table);

    let table_state = expected
        .tables
        .get_mut(&table.name)
        .expect("expected table state missing");

    let target_missing = gen_bool(cfg.update_target_missing_pk.value_at(progress_0_1))
        || table_state.rows_by_pk.is_empty();

    let target_pk = if !target_missing {
        let idx = get_random() as usize % table_state.rows_by_pk.len();
        table_state
            .rows_by_pk
            .keys()
            .nth(idx)
            .expect("non-empty")
            .clone()
    } else {
        generate_random_value(&pk.data_type)
    };

    // Update only non-PK columns (avoid changing PK to keep tracking sane).
    let non_pk_columns: Vec<_> = table.columns.iter().filter(|c| c.name != pk.name).collect();

    // If there are no non-PK columns, do a no-op update on PK.
    let mut assignments: Vec<(String, String)> = Vec::new();
    if non_pk_columns.is_empty() {
        assignments.push((pk.name.clone(), target_pk.clone()));
    } else {
        let nr_to_update = 1 + (get_random() as usize % non_pk_columns.len());
        for _ in 0..nr_to_update {
            let col = non_pk_columns[get_random() as usize % non_pk_columns.len()];
            assignments.push((col.name.clone(), generate_random_value(&col.data_type)));
        }
        // De-dup by last-write-wins
        let mut dedup: HashMap<String, String> = HashMap::new();
        for (k, v) in assignments {
            dedup.insert(k, v);
        }
        assignments = dedup.into_iter().collect();
        assignments.sort_by(|a, b| a.0.cmp(&b.0));
    }

    // Apply to expected state if row exists.
    if let Some(row) = table_state.rows_by_pk.get_mut(&target_pk) {
        for (col, val) in &assignments {
            row.values.insert(col.clone(), val.clone());
        }
    }

    let set_clause = assignments
        .iter()
        .map(|(c, v)| format!("{c} = {v}"))
        .collect::<Vec<_>>()
        .join(", ");

    let where_clause = format!("{} = {}", pk.name, target_pk);

    format!(
        "UPDATE {} SET {} WHERE {};",
        table.name, set_clause, where_clause
    )
}

/// Generate a DELETE statement and update in-memory expected state.
/// Behavior:
/// - Prefer deleting an existing row; with configurable probability use a random PK that likely doesn't exist.
fn generate_delete(
    table: &Table,
    expected: &mut ExpectedState,
    cfg: &GenConfig,
    progress_0_1: f64,
) -> String {
    let pk = pk_column(table);

    let table_state = expected
        .tables
        .get_mut(&table.name)
        .expect("expected table state missing");

    let target_missing = gen_bool(cfg.delete_target_missing_pk.value_at(progress_0_1))
        || table_state.rows_by_pk.is_empty();

    let target_pk = if !target_missing {
        let idx = get_random() as usize % table_state.rows_by_pk.len();
        table_state
            .rows_by_pk
            .keys()
            .nth(idx)
            .expect("non-empty")
            .clone()
    } else {
        generate_random_value(&pk.data_type)
    };

    // Apply to expected state if row exists.
    let _ = table_state.rows_by_pk.remove(&target_pk);

    format!(
        "DELETE FROM {} WHERE {} = {};",
        table.name, pk.name, target_pk
    )
}

/// Generate a random SQL statement for a schema while tracking expected in-memory state.
///
/// The probabilities for choosing INSERT vs UPDATE vs DELETE are randomized per-statement:
/// we generate 3 random weights and pick an operation proportionally.
///
/// The "constraint-ish" and "missing row targeting" probabilities used inside INSERT/UPDATE/DELETE
/// are chosen once at startup via `GenConfig` and then kept constant for the run.
fn generate_random_statement(
    schema: &ArbitrarySchema,
    expected: &mut ExpectedState,
    cfg: &GenConfig,
    progress_0_1: f64,
) -> String {
    let table = &schema.tables[get_random() as usize % schema.tables.len()];

    // Profile-controlled weights (may vary over time).
    let w_insert = cfg.w_insert.value_at(progress_0_1);
    let w_update = cfg.w_update.value_at(progress_0_1);
    let w_delete = cfg.w_delete.value_at(progress_0_1);

    let sum = w_insert + w_update + w_delete;
    let roll = (get_random() as f64 / u64::MAX as f64) * sum;

    if roll < w_insert {
        generate_insert(table, expected, cfg, progress_0_1)
    } else if roll < w_insert + w_update {
        generate_update(table, expected, cfg, progress_0_1)
    } else {
        generate_delete(table, expected, cfg, progress_0_1)
    }
}

/// Convert SQLite type string to DataType
fn map_sqlite_type(type_str: &str) -> DataType {
    let t = type_str.to_uppercase();

    if t.contains("INT") {
        DataType::Integer
    } else if t.contains("CHAR") || t.contains("CLOB") || t.contains("TEXT") {
        DataType::Text
    } else if t.contains("BLOB") {
        DataType::Blob
    } else if t.contains("REAL") || t.contains("FLOA") || t.contains("DOUB") {
        DataType::Real
    } else {
        DataType::Numeric
    }
}

/// Load full schema from SQLite database
pub fn load_schema(
    db_path: &Path,
) -> Result<ArbitrarySchema, Box<dyn std::error::Error + Send + Sync>> {
    let conn = rusqlite::Connection::open(db_path)?;

    // Fetch user tables (ignore sqlite internal tables)
    let mut stmt = conn.prepare(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
    )?;

    let table_names: Vec<String> = stmt
        .query_map([], |row| row.get(0))?
        .collect::<Result<_, _>>()?;

    let mut tables = Vec::new();

    for table_name in table_names {
        let pragma = format!("PRAGMA table_info({table_name})");
        let mut pragma_stmt = conn.prepare(&pragma)?;

        let columns = pragma_stmt
            .query_map([], |row| {
                let name: String = row.get(1)?;
                let type_str: String = row.get(2)?;
                let not_null: bool = row.get::<_, i32>(3)? != 0;
                let is_pk: bool = row.get::<_, i32>(5)? != 0;

                let mut constraints = Vec::new();

                if is_pk {
                    constraints.push(Constraint::PrimaryKey);
                }
                if not_null {
                    constraints.push(Constraint::NotNull);
                }

                Ok(Column {
                    name,
                    data_type: map_sqlite_type(&type_str),
                    constraints,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let pk_column = columns
            .iter()
            .find(|col| col.constraints.contains(&Constraint::PrimaryKey))
            .expect("Table should have a primary key");
        let mut select_stmt =
            conn.prepare(&format!("SELECT {} FROM {table_name}", pk_column.name))?;
        let mut rows = select_stmt.query(())?;
        let mut pk_values = Vec::new();
        while let Some(row) = rows.next()? {
            let value = match row.get_ref(0)? {
                rusqlite::types::ValueRef::Null => "NULL".to_string(),
                rusqlite::types::ValueRef::Integer(x) => x.to_string(),
                rusqlite::types::ValueRef::Real(x) => x.to_string(),
                rusqlite::types::ValueRef::Text(text) => {
                    format!("'{}'", std::str::from_utf8(text)?)
                }
                rusqlite::types::ValueRef::Blob(blob) => format!("x'{}'", hex::encode(blob)),
            };
            pk_values.push(value);
        }
        tables.push(Table {
            name: table_name,
            columns,
            pk_values,
        });
    }

    Ok(ArbitrarySchema { tables })
}

fn generate_plan(opts: &Opts) -> Result<Plan, Box<dyn std::error::Error + Send + Sync>> {
    let mut log_file = File::create(&opts.log_file)?;
    if !opts.skip_log {
        writeln!(log_file, "{}", opts.nr_threads)?;
        writeln!(log_file, "{}", opts.nr_iterations)?;
    }
    let mut plan = Plan {
        ddl_statements: vec![],
        queries_per_thread: vec![],
        nr_iterations: opts.nr_iterations,
        nr_threads: opts.nr_threads,
    };
    let schema = if let Some(db_ref) = &opts.db_ref {
        writeln!(log_file, "{}", 0)?;
        load_schema(db_ref)?
    } else {
        let schema = gen_schema(opts.tables);
        let ddl_statements = schema.to_sql();
        if !opts.skip_log {
            writeln!(log_file, "{}", ddl_statements.len())?;
            for stmt in &ddl_statements {
                writeln!(log_file, "{stmt}")?;
            }
        }
        plan.ddl_statements = ddl_statements;
        schema
    };

    // Choose randomized generator behavior once at startup (per run).
    let gen_cfg = gen_startup_config();
    println!("Generator config: {gen_cfg:?}");

    // Per-thread expected state (kept in-memory during plan generation).
    // NOTE: Because execution can COMMIT/ROLLBACK, this expected state is best-effort
    // and currently only tracks autocommit-level effects (see TODO below).
    let mut expected_per_thread: Vec<ExpectedState> = (0..opts.nr_threads)
        .map(|_| ExpectedState::new_from_schema(&schema))
        .collect();

    // Write DDL statements to log file
    for id in 0..opts.nr_threads {
        writeln!(log_file, "{id}",)?;
        let mut queries = vec![];
        let mut push = |sql: &str| {
            queries.push(sql.to_string());
            if !opts.skip_log {
                writeln!(log_file, "{sql}").unwrap();
            }
        };

        for i in 0..opts.nr_iterations {
            if !opts.silent && !opts.verbose && i % 100 == 0 {
                print!(
                    "\r{} %",
                    (i as f64 / opts.nr_iterations as f64 * 100.0) as usize
                );
                std::io::stdout().flush().unwrap();
            }

            let tx = if get_random() % 2 == 0 {
                Some("BEGIN")
            } else {
                None
            };

            // TODO: To make expected state exact across COMMIT/ROLLBACK, maintain a transactional
            // shadow copy of ExpectedState per thread and only merge on COMMIT.
            if let Some(tx) = tx {
                push(tx);
            }

            let progress_0_1 = if opts.nr_iterations <= 1 {
                1.0
            } else {
                i as f64 / (opts.nr_iterations - 1) as f64
            };

            let sql = generate_random_statement(
                &schema,
                &mut expected_per_thread[id],
                &gen_cfg,
                progress_0_1,
            );
            push(&sql);

            if tx.is_some() {
                if get_random() % 2 == 0 {
                    push("COMMIT");
                } else {
                    push("ROLLBACK");
                }
            }
        }

        plan.queries_per_thread.push(queries);
    }
    Ok(plan)
}

fn read_plan_from_log_file(opts: &Opts) -> Result<Plan, Box<dyn std::error::Error + Send + Sync>> {
    let mut file = File::open(&opts.log_file)?;
    let mut buf = String::new();
    let mut plan = Plan {
        ddl_statements: vec![],
        queries_per_thread: vec![],
        nr_iterations: 0,
        nr_threads: 0,
    };
    file.read_to_string(&mut buf).unwrap();
    let mut lines = buf.lines();
    plan.nr_threads = lines.next().expect("missing threads").parse().unwrap();
    plan.nr_iterations = lines
        .next()
        .expect("missing nr_iterations")
        .parse()
        .unwrap();
    let nr_ddl = lines
        .next()
        .expect("number of ddl statements")
        .parse()
        .unwrap();
    for _ in 0..nr_ddl {
        plan.ddl_statements
            .push(lines.next().expect("expected ddl statement").to_string());
    }
    for line in lines {
        if line.parse::<i64>().is_ok() {
            plan.queries_per_thread.push(Vec::new());
        } else {
            plan.queries_per_thread
                .last_mut()
                .unwrap()
                .push(line.to_string());
        }
    }
    Ok(plan)
}

pub type LogLevelReloadHandle = reload::Handle<EnvFilter, tracing_subscriber::Registry>;

pub fn init_tracing() -> Result<(WorkerGuard, LogLevelReloadHandle), std::io::Error> {
    let (non_blocking, guard) = tracing_appender::non_blocking(std::io::stderr());
    let filter = EnvFilter::from_default_env();
    let (filter_layer, reload_handle) = reload::Layer::new(filter);

    if let Err(e) = tracing_subscriber::registry()
        .with(filter_layer)
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(non_blocking)
                .with_ansi(false)
                .with_line_number(true)
                .with_thread_ids(true),
        )
        .try_init()
    {
        println!("Unable to setup tracing appender: {e:?}");
    }
    Ok((guard, reload_handle))
}

const LOG_LEVEL_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(100);

const LOG_LEVEL_FILE: &str = "RUST_LOG";

/// Spawns a background thread that watches for a RUST_LOG file and dynamically
/// updates the log level when the file contents change.
///
/// The file should contain a valid tracing filter string (e.g., "debug", "info",
/// "limbo_core=trace,warn"). If the file is removed or contains an invalid filter,
/// the current log level is preserved.
pub fn spawn_log_level_watcher(reload_handle: LogLevelReloadHandle) {
    std::thread::spawn(move || {
        let mut last_content: Option<String> = None;

        loop {
            std::thread::sleep(LOG_LEVEL_POLL_INTERVAL);

            let content = match std::fs::read_to_string(LOG_LEVEL_FILE) {
                Ok(content) => content.trim().to_string(),
                Err(_) => {
                    continue;
                }
            };

            if last_content.as_ref() == Some(&content) {
                continue;
            }

            match content.parse::<EnvFilter>() {
                Ok(new_filter) => {
                    if let Err(e) = reload_handle.reload(new_filter) {
                        eprintln!("Failed to reload log filter: {e}");
                    } else {
                        last_content = Some(content);
                    }
                }
                Err(e) => {
                    eprintln!("Invalid log filter in {LOG_LEVEL_FILE}: {e}");
                    last_content = Some(content);
                }
            }
        }
    });
}

fn sqlite_integrity_check(
    db_path: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    assert!(db_path.exists());
    let conn = rusqlite::Connection::open(db_path)?;
    let mut stmt = conn.prepare("SELECT * FROM pragma_integrity_check;")?;
    let mut rows = stmt.query(())?;
    let mut result: Vec<String> = Vec::new();

    while let Some(row) = rows.next()? {
        result.push(row.get(0)?);
    }
    assert!(!result.is_empty());
    if !result[0].eq_ignore_ascii_case("ok") {
        // Build a list of problems
        result.iter_mut().for_each(|row| *row = format!("- {row}"));
        return Err(format!("SQLite integrity check failed: {}", result.join("\n")).into());
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    rt.block_on(async_main())
}

async fn async_main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (_guard, reload_handle) = init_tracing()?;

    spawn_log_level_watcher(reload_handle);

    let opts = Opts::parse();

    // Initialize RNG
    #[cfg(feature = "antithesis")]
    {
        if opts.seed.is_some() {
            eprintln!("Error: --seed is not supported under Antithesis");
            std::process::exit(1);
        }
        println!("Using randomness from Antithesis");
    }
    #[cfg(not(feature = "antithesis"))]
    {
        let seed = opts.seed.unwrap_or_else(rand::random);
        init_rng(seed);
        println!("Using seed: {seed}");
    }
    if opts.nr_threads > 1 {
        println!("WARNING: Multi-threaded data access is not yet supported: https://github.com/tursodatabase/turso/issues/1552");
    }

    let plan = if opts.load_log {
        println!("Loading plan from log file...");
        read_plan_from_log_file(&opts)?
    } else {
        println!("Generating plan...");
        generate_plan(&opts)?
    };

    let mut handles = Vec::with_capacity(opts.nr_threads);
    let plan = Arc::new(plan);

    let tempfile = tempfile::NamedTempFile::new()?;
    let (_, path) = tempfile.keep().unwrap();
    let db_file = if let Some(db_file) = opts.db_file {
        db_file
    } else {
        if let Some(db_ref) = opts.db_ref {
            std::fs::copy(db_ref, &path)?;
        }
        path.to_string_lossy().to_string()
    };

    println!("db_file={db_file}");

    let vfs_option = opts.vfs.clone();

    for thread in 0..opts.nr_threads {
        let db_file = db_file.clone();
        let mut builder = Builder::new_local(&db_file);
        if let Some(ref vfs) = vfs_option {
            builder = builder.with_io(vfs.clone());
        }
        let db = Arc::new(Mutex::new(builder.build().await?));
        let plan = plan.clone();
        let conn = db.lock().await.connect()?;

        conn.busy_timeout(std::time::Duration::from_millis(opts.busy_timeout))?;

        conn.execute("PRAGMA data_sync_retry = 1", ()).await?;

        // Apply each DDL statement individually
        for stmt in &plan.ddl_statements {
            if opts.verbose {
                println!("executing ddl {stmt}");
            }
            if let Err(e) = conn.execute(stmt, ()).await {
                match e {
                    turso::Error::Corrupt(e) => {
                        panic!("Error creating table: {e}");
                    }
                    turso::Error::Error(e) => {
                        println!("Error creating table: {e}");
                    }
                    _ => {
                        println!("Error creating table: {e}");
                        // Exit on any other error during table creation
                        std::process::exit(1);
                    }
                }
            }
        }

        let nr_iterations = plan.nr_iterations;
        let db = db.clone();
        let vfs_for_task = vfs_option.clone();

        let handle = tokio::spawn(async move {
            let mut conn = db.lock().await.connect()?;

            conn.busy_timeout(std::time::Duration::from_millis(opts.busy_timeout))?;

            conn.execute("PRAGMA data_sync_retry = 1", ()).await?;

            println!("\rExecuting queries...");
            for query_index in 0..nr_iterations {
                if gen_bool(0.0) {
                    // disabled
                    if opts.verbose {
                        println!("Reopening database");
                    }
                    // Reopen the database
                    let mut db_guard = db.lock().await;
                    let mut builder = Builder::new_local(&db_file);
                    if let Some(ref vfs) = vfs_for_task {
                        builder = builder.with_io(vfs.clone());
                    }
                    *db_guard = builder.build().await?;
                    conn = db_guard.connect()?;
                    conn.busy_timeout(std::time::Duration::from_millis(opts.busy_timeout))?;
                } else if gen_bool(0.0) {
                    // disabled
                    // Reconnect to the database
                    if opts.verbose {
                        println!("Reconnecting to database");
                    }
                    let db_guard = db.lock().await;
                    conn = db_guard.connect()?;
                    conn.busy_timeout(std::time::Duration::from_millis(opts.busy_timeout))?;
                }
                let sql = &plan.queries_per_thread[thread][query_index];
                if !opts.silent {
                    if opts.verbose {
                        println!("executing query {sql}");
                    } else if query_index % 100 == 0 {
                        print!(
                            "\r{:.2} %",
                            (query_index as f64 / nr_iterations as f64 * 100.0)
                        );
                        std::io::stdout().flush().unwrap();
                    }
                }
                if opts.verbose {
                    eprintln!("thread#{thread}(start): {sql}");
                }
                if let Err(e) = conn.execute(sql, ()).await {
                    match e {
                        turso::Error::Corrupt(e) => {
                            panic!("Error[FATAL] executing query: {}", e);
                        }
                        turso::Error::Constraint(e) => {
                            if opts.verbose {
                                println!("Error[WARNING] executing query: {e}");
                            }
                        }
                        turso::Error::Busy(e) => {
                            println!("Error[WARNING] executing query: {e}");
                        }
                        turso::Error::Error(e) => {
                            if opts.verbose {
                                println!("Error executing query: {e}");
                            }
                        }
                        _ => panic!("Error[FATAL] executing query: {}", e),
                    }
                }
                if opts.verbose {
                    eprintln!("thread#{thread}(end): {sql}");
                }
                const INTEGRITY_CHECK_INTERVAL: usize = 100;
                if query_index % INTEGRITY_CHECK_INTERVAL == 0 {
                    if opts.verbose {
                        eprintln!("thread#{thread}(start): PRAGMA integrity_check");
                    }
                    let mut res = conn.query("PRAGMA integrity_check", ()).await.unwrap();
                    match res.next().await {
                        Ok(Some(row)) => {
                            let value = row.get_value(0).unwrap();
                            if value != "ok".into() {
                                panic!("integrity check failed: {:?}", value);
                            }
                        }
                        Ok(None) => {
                            panic!("integrity check failed: no rows");
                        }
                        Err(e) => {
                            println!("Error performing integrity check: {e}");
                        }
                    }
                    match res.next().await {
                        Ok(Some(_)) => panic!("integrity check failed: more than 1 row"),
                        Err(e) => println!("Error performing integrity check: {e}"),
                        _ => {}
                    }
                    if opts.verbose {
                        eprintln!("thread#{thread}(end): PRAGMA integrity_check");
                    }
                }
            }
            // In case this thread is running an exclusive transaction, commit it so that it doesn't block other threads.
            let _ = conn.execute("COMMIT", ()).await;
            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await??;
    }
    println!("Done. SQL statements written to {}", opts.log_file);
    println!("Database file: {db_file}");

    #[cfg(not(miri))]
    {
        println!("Running SQLite Integrity check");
        sqlite_integrity_check(std::path::Path::new(&db_file))?;
    }

    Ok(())
}
