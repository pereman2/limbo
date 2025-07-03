-- Basic Rollback Validation Tests
-- Simple tests for initial rollback implementation validation
-- These tests focus on fundamental rollback behavior

-- =============================================
-- Test 1: Single Table Creation Rollback
-- =============================================
-- Most basic rollback test

BEGIN TRANSACTION;

CREATE TABLE simple_test (
    id INTEGER PRIMARY KEY,
    name TEXT,
    value INTEGER
);

-- Verify table exists
SELECT name FROM sqlite_schema WHERE type='table' AND name='simple_test';
-- Expected: simple_test

ROLLBACK;

-- Verify table is gone
SELECT name FROM sqlite_schema WHERE type='table' AND name='simple_test';
-- Expected: No results

-- =============================================
-- Test 2: Data Insert Rollback
-- =============================================
-- Test rollback with data operations

-- First create a table outside transaction
CREATE TABLE persistent_table (
    id INTEGER PRIMARY KEY,
    data TEXT
);

-- Insert some initial data
INSERT INTO persistent_table (data) VALUES ('initial');

-- Verify initial state
SELECT COUNT(*) FROM persistent_table;
-- Expected: 1

BEGIN TRANSACTION;

-- Insert data within transaction
INSERT INTO persistent_table (data) VALUES ('transactional');
INSERT INTO persistent_table (data) VALUES ('more_data');

-- Verify data exists within transaction
SELECT COUNT(*) FROM persistent_table;
-- Expected: 3

ROLLBACK;

-- Verify rollback worked
SELECT COUNT(*) FROM persistent_table;
-- Expected: 1

SELECT data FROM persistent_table;
-- Expected: initial

-- Clean up
DROP TABLE persistent_table;

-- =============================================
-- Test 3: Update Operations Rollback
-- =============================================
-- Test rollback with update operations

CREATE TABLE update_test (
    id INTEGER PRIMARY KEY,
    value INTEGER,
    status TEXT DEFAULT 'active'
);

-- Insert initial data
INSERT INTO update_test (value, status) VALUES 
    (100, 'active'),
    (200, 'inactive'),
    (300, 'active');

-- Verify initial state
SELECT SUM(value) FROM update_test WHERE status = 'active';
-- Expected: 400

BEGIN TRANSACTION;

-- Perform updates
UPDATE update_test SET value = value * 2 WHERE status = 'active';
UPDATE update_test SET status = 'archived' WHERE value < 500;

-- Verify intermediate state
SELECT SUM(value) FROM update_test WHERE status = 'active';
-- Expected: 600

SELECT COUNT(*) FROM update_test WHERE status = 'archived';
-- Expected: 1

ROLLBACK;

-- Verify rollback worked
SELECT SUM(value) FROM update_test WHERE status = 'active';
-- Expected: 400

SELECT COUNT(*) FROM update_test WHERE status = 'archived';
-- Expected: 0

-- Clean up
DROP TABLE update_test;

-- =============================================
-- Test 4: Delete Operations Rollback
-- =============================================
-- Test rollback with delete operations

CREATE TABLE delete_test (
    id INTEGER PRIMARY KEY,
    category TEXT,
    amount REAL
);

-- Insert test data
INSERT INTO delete_test (category, amount) VALUES 
    ('A', 10.5),
    ('B', 20.0),
    ('A', 15.5),
    ('C', 30.0),
    ('B', 25.0);

-- Verify initial state
SELECT COUNT(*) FROM delete_test;
-- Expected: 5

SELECT COUNT(*) FROM delete_test WHERE category = 'A';
-- Expected: 2

BEGIN TRANSACTION;

-- Perform deletes
DELETE FROM delete_test WHERE category = 'A';
DELETE FROM delete_test WHERE amount > 25.0;

-- Verify intermediate state
SELECT COUNT(*) FROM delete_test;
-- Expected: 2

SELECT COUNT(*) FROM delete_test WHERE category = 'A';
-- Expected: 0

ROLLBACK;

-- Verify rollback worked
SELECT COUNT(*) FROM delete_test;
-- Expected: 5

SELECT COUNT(*) FROM delete_test WHERE category = 'A';
-- Expected: 2

-- Clean up
DROP TABLE delete_test;

-- =============================================
-- Test 5: Mixed DDL and DML Rollback
-- =============================================
-- Test rollback with both schema and data changes

BEGIN TRANSACTION;

-- Create table
CREATE TABLE mixed_test (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    score INTEGER CHECK (score >= 0)
);

-- Insert data
INSERT INTO mixed_test (name, score) VALUES 
    ('Alice', 85),
    ('Bob', 92),
    ('Carol', 78);

-- Update data
UPDATE mixed_test SET score = score + 5 WHERE score < 80;

-- Verify intermediate state
SELECT COUNT(*) FROM mixed_test;
-- Expected: 3

SELECT score FROM mixed_test WHERE name = 'Carol';
-- Expected: 83

ROLLBACK;

-- Verify complete rollback
SELECT name FROM sqlite_schema WHERE type='table' AND name='mixed_test';
-- Expected: No results

-- =============================================
-- Test 6: Multiple Table Rollback
-- =============================================
-- Test rollback with multiple related tables

BEGIN TRANSACTION;

-- Create first table
CREATE TABLE authors (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    country TEXT
);

-- Create second table with foreign key
CREATE TABLE books (
    id INTEGER PRIMARY KEY,
    title TEXT NOT NULL,
    author_id INTEGER,
    publication_year INTEGER,
    FOREIGN KEY (author_id) REFERENCES authors(id)
);

-- Insert data into both tables
INSERT INTO authors (name, country) VALUES 
    ('George Orwell', 'UK'),
    ('Isaac Asimov', 'USA');

INSERT INTO books (title, author_id, publication_year) VALUES 
    ('1984', 1, 1949),
    ('Animal Farm', 1, 1945),
    ('Foundation', 2, 1951);

-- Verify data exists
SELECT COUNT(*) FROM authors;
-- Expected: 2

SELECT COUNT(*) FROM books;
-- Expected: 3

-- Verify relationship
SELECT a.name, COUNT(b.id) as book_count
FROM authors a
LEFT JOIN books b ON a.id = b.author_id
GROUP BY a.id, a.name;
-- Expected: George Orwell 2, Isaac Asimov 1

ROLLBACK;

-- Verify both tables are gone
SELECT name FROM sqlite_schema WHERE type='table' AND name IN ('authors', 'books');
-- Expected: No results

-- =============================================
-- Test 7: Constraint Violation Rollback
-- =============================================
-- Test that rollback works even after constraint violations

CREATE TABLE constraint_test (
    id INTEGER PRIMARY KEY,
    unique_value TEXT UNIQUE NOT NULL,
    positive_number INTEGER CHECK (positive_number > 0)
);

-- Insert valid initial data
INSERT INTO constraint_test (unique_value, positive_number) VALUES ('valid', 10);

BEGIN TRANSACTION;

-- Insert more valid data
INSERT INTO constraint_test (unique_value, positive_number) VALUES ('another', 20);

-- Verify transaction state
SELECT COUNT(*) FROM constraint_test;
-- Expected: 2

-- Now we would try constraint violations (commented out as they would cause errors)
-- INSERT INTO constraint_test (unique_value, positive_number) VALUES ('valid', 30); -- Would fail: unique constraint
-- INSERT INTO constraint_test (unique_value, positive_number) VALUES ('negative', -5); -- Would fail: check constraint

ROLLBACK;

-- Verify rollback worked
SELECT COUNT(*) FROM constraint_test;
-- Expected: 1

SELECT unique_value FROM constraint_test;
-- Expected: valid

-- Clean up
DROP TABLE constraint_test;

-- =============================================
-- Test 8: Large Data Rollback
-- =============================================
-- Test rollback with more substantial data

CREATE TABLE large_data_test (
    id INTEGER PRIMARY KEY,
    batch_id INTEGER,
    data_value REAL,
    created_timestamp TEXT DEFAULT CURRENT_TIMESTAMP
);

-- Insert some initial data
INSERT INTO large_data_test (batch_id, data_value) VALUES (1, 100.0);

BEGIN TRANSACTION;

-- Insert larger batch of data
INSERT INTO large_data_test (batch_id, data_value) VALUES 
    (2, 200.1), (2, 200.2), (2, 200.3), (2, 200.4), (2, 200.5),
    (2, 200.6), (2, 200.7), (2, 200.8), (2, 200.9), (2, 201.0),
    (3, 300.1), (3, 300.2), (3, 300.3), (3, 300.4), (3, 300.5),
    (3, 300.6), (3, 300.7), (3, 300.8), (3, 300.9), (3, 301.0);

-- Verify transaction state
SELECT COUNT(*) FROM large_data_test;
-- Expected: 21

SELECT COUNT(DISTINCT batch_id) FROM large_data_test;
-- Expected: 3

-- Perform bulk update
UPDATE large_data_test SET data_value = data_value * 1.1 WHERE batch_id > 1;

-- Verify update
SELECT COUNT(*) FROM large_data_test WHERE data_value > 300;
-- Expected: 20

ROLLBACK;

-- Verify rollback
SELECT COUNT(*) FROM large_data_test;
-- Expected: 1

SELECT batch_id FROM large_data_test;
-- Expected: 1

-- Clean up
DROP TABLE large_data_test;

-- =============================================
-- Test 9: Schema Modification Rollback
-- =============================================
-- Test rollback after schema changes

-- Create base table
CREATE TABLE schema_test (
    id INTEGER PRIMARY KEY,
    name TEXT,
    value INTEGER
);

-- Insert initial data
INSERT INTO schema_test (name, value) VALUES ('test', 42);

BEGIN TRANSACTION;

-- Note: ALTER TABLE might not be supported yet, so we create new tables instead
CREATE TABLE schema_test_extended (
    id INTEGER PRIMARY KEY,
    name TEXT,
    value INTEGER,
    extra_field TEXT,
    computed_value INTEGER
);

-- Copy data to new table
INSERT INTO schema_test_extended (id, name, value, extra_field, computed_value)
SELECT id, name, value, 'extended', value * 2
FROM schema_test;

-- Verify extended table
SELECT COUNT(*) FROM schema_test_extended;
-- Expected: 1

SELECT computed_value FROM schema_test_extended WHERE name = 'test';
-- Expected: 84

ROLLBACK;

-- Verify rollback
SELECT name FROM sqlite_schema WHERE type='table' AND name='schema_test_extended';
-- Expected: No results

-- Original table should still exist
SELECT COUNT(*) FROM schema_test;
-- Expected: 1

-- Clean up
DROP TABLE schema_test;

-- =============================================
-- Test 10: Nested Operations Rollback
-- =============================================
-- Test rollback with complex nested operations

BEGIN TRANSACTION;

-- Create hierarchical tables
CREATE TABLE categories (
    id INTEGER PRIMARY KEY,
    name TEXT,
    parent_id INTEGER,
    FOREIGN KEY (parent_id) REFERENCES categories(id)
);

CREATE TABLE items (
    id INTEGER PRIMARY KEY,
    name TEXT,
    category_id INTEGER,
    price REAL,
    FOREIGN KEY (category_id) REFERENCES categories(id)
);

-- Insert hierarchical data
INSERT INTO categories (id, name, parent_id) VALUES 
    (1, 'Electronics', NULL),
    (2, 'Computers', 1),
    (3, 'Laptops', 2);

INSERT INTO items (name, category_id, price) VALUES 
    ('Gaming Laptop', 3, 1500.00),
    ('Office Laptop', 3, 800.00);

-- Perform complex operations
UPDATE items SET price = price * 0.9 WHERE category_id IN (
    SELECT id FROM categories WHERE parent_id = 2
);

-- Create summary data
CREATE TABLE category_summary AS
SELECT 
    c.name as category_name,
    COUNT(i.id) as item_count,
    AVG(i.price) as avg_price
FROM categories c
LEFT JOIN items i ON c.id = i.category_id
GROUP BY c.id, c.name;

-- Verify complex state
SELECT * FROM category_summary WHERE item_count > 0;

ROLLBACK;

-- Verify complete rollback
SELECT name FROM sqlite_schema WHERE type='table' AND name IN ('categories', 'items', 'category_summary');
-- Expected: No results

-- End of basic validation tests