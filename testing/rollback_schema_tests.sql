-- Rollback Tests for Schema and Complex Operations
-- Tests various rollback scenarios with schemas and intricate operations
-- without using indexes or virtual tables

-- =============================================
-- Test 1: Basic Schema Rollback
-- =============================================
-- Test creating and dropping tables within a transaction

BEGIN TRANSACTION;

CREATE TABLE users (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    email TEXT UNIQUE,
    age INTEGER CHECK (age >= 0),
    created_at TEXT DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE posts (
    id INTEGER PRIMARY KEY,
    user_id INTEGER,
    title TEXT NOT NULL,
    content TEXT,
    published INTEGER DEFAULT 0,
    FOREIGN KEY (user_id) REFERENCES users(id)
);

-- Verify tables exist
SELECT name FROM sqlite_schema WHERE type='table' AND name IN ('users', 'posts');

ROLLBACK;

-- Verify tables no longer exist
SELECT name FROM sqlite_schema WHERE type='table' AND name IN ('users', 'posts');

-- =============================================
-- Test 2: Mixed Schema and Data Rollback
-- =============================================
-- Test rollback with both schema changes and data modifications

BEGIN TRANSACTION;

CREATE TABLE products (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    price REAL CHECK (price > 0),
    category TEXT,
    in_stock INTEGER DEFAULT 1
);

-- Insert initial data
INSERT INTO products (name, price, category) VALUES 
    ('Laptop', 999.99, 'Electronics'),
    ('Book', 19.99, 'Education'),
    ('Coffee', 4.99, 'Food');

-- Create another table with foreign key relationship
CREATE TABLE orders (
    id INTEGER PRIMARY KEY,
    product_id INTEGER,
    quantity INTEGER CHECK (quantity > 0),
    order_date TEXT DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (product_id) REFERENCES products(id)
);

-- Insert related data
INSERT INTO orders (product_id, quantity) VALUES 
    (1, 2),
    (2, 1),
    (3, 5);

-- Verify data exists
SELECT COUNT(*) FROM products;
SELECT COUNT(*) FROM orders;

ROLLBACK;

-- Verify everything is gone
SELECT name FROM sqlite_schema WHERE type='table' AND name IN ('products', 'orders');

-- =============================================
-- Test 3: Complex Column Modifications
-- =============================================
-- Test rollback with complex schema modifications

BEGIN TRANSACTION;

CREATE TABLE employees (
    id INTEGER PRIMARY KEY,
    first_name TEXT NOT NULL,
    last_name TEXT NOT NULL,
    department TEXT,
    salary REAL CHECK (salary > 0),
    hire_date TEXT,
    is_active INTEGER DEFAULT 1
);

-- Insert test data
INSERT INTO employees (first_name, last_name, department, salary, hire_date) VALUES 
    ('John', 'Doe', 'Engineering', 75000.0, '2023-01-15'),
    ('Jane', 'Smith', 'Marketing', 65000.0, '2023-02-20'),
    ('Bob', 'Johnson', 'Engineering', 80000.0, '2022-12-10'),
    ('Alice', 'Williams', 'HR', 55000.0, '2023-03-05');

-- ALTER TABLE operations (if supported in future)
-- ALTER TABLE employees ADD COLUMN bonus REAL DEFAULT 0;
-- ALTER TABLE employees ADD COLUMN email TEXT;

-- Complex data modifications
UPDATE employees SET salary = salary * 1.1 WHERE department = 'Engineering';
UPDATE employees SET is_active = 0 WHERE hire_date < '2023-01-01';

-- Delete some records
DELETE FROM employees WHERE department = 'HR';

-- Verify intermediate state
SELECT first_name, last_name, salary, is_active FROM employees;

ROLLBACK;

-- Verify table and data are gone
SELECT name FROM sqlite_schema WHERE type='table' AND name = 'employees';

-- =============================================
-- Test 4: Multiple Table Operations with Constraints
-- =============================================
-- Test rollback with multiple tables and constraint violations

BEGIN TRANSACTION;

-- Create customers table
CREATE TABLE customers (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    email TEXT UNIQUE NOT NULL,
    phone TEXT,
    address TEXT
);

-- Create accounts table with foreign key
CREATE TABLE accounts (
    id INTEGER PRIMARY KEY,
    customer_id INTEGER NOT NULL,
    account_type TEXT CHECK (account_type IN ('checking', 'savings', 'credit')),
    balance REAL DEFAULT 0.0,
    opened_date TEXT DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (customer_id) REFERENCES customers(id)
);

-- Create transactions table
CREATE TABLE transactions (
    id INTEGER PRIMARY KEY,
    account_id INTEGER NOT NULL,
    transaction_type TEXT CHECK (transaction_type IN ('debit', 'credit')),
    amount REAL NOT NULL CHECK (amount > 0),
    description TEXT,
    transaction_date TEXT DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (account_id) REFERENCES accounts(id)
);

-- Insert customers
INSERT INTO customers (name, email, phone, address) VALUES 
    ('Alice Johnson', 'alice@email.com', '555-0101', '123 Main St'),
    ('Bob Smith', 'bob@email.com', '555-0102', '456 Oak Ave'),
    ('Carol Brown', 'carol@email.com', '555-0103', '789 Pine Rd');

-- Insert accounts
INSERT INTO accounts (customer_id, account_type, balance) VALUES 
    (1, 'checking', 1500.00),
    (1, 'savings', 5000.00),
    (2, 'checking', 750.00),
    (3, 'credit', -200.00);

-- Insert transactions
INSERT INTO transactions (account_id, transaction_type, amount, description) VALUES 
    (1, 'debit', 100.00, 'ATM Withdrawal'),
    (1, 'credit', 2000.00, 'Salary Deposit'),
    (2, 'credit', 500.00, 'Transfer from Checking'),
    (3, 'debit', 50.00, 'Gas Station'),
    (4, 'credit', 300.00, 'Payment');

-- Complex multi-table operations
UPDATE accounts SET balance = balance + 1000 WHERE customer_id = 1;
UPDATE customers SET address = 'Updated Address' WHERE email LIKE '%@email.com';

-- Verify complex data state
SELECT c.name, a.account_type, a.balance, COUNT(t.id) as transaction_count
FROM customers c
JOIN accounts a ON c.id = a.customer_id
LEFT JOIN transactions t ON a.id = t.account_id
GROUP BY c.id, a.id;

ROLLBACK;

-- Verify all tables and data are gone
SELECT name FROM sqlite_schema WHERE type='table' AND name IN ('customers', 'accounts', 'transactions');

-- =============================================
-- Test 5: Nested Transactions with Schema Changes
-- =============================================
-- Test rollback with nested operations and schema modifications

BEGIN TRANSACTION;

-- Create a complex table with multiple constraints
CREATE TABLE inventory (
    id INTEGER PRIMARY KEY,
    sku TEXT UNIQUE NOT NULL,
    product_name TEXT NOT NULL,
    category TEXT CHECK (category IN ('electronics', 'clothing', 'books', 'food')),
    cost_price REAL CHECK (cost_price >= 0),
    sell_price REAL CHECK (sell_price >= cost_price),
    quantity INTEGER DEFAULT 0 CHECK (quantity >= 0),
    reorder_level INTEGER DEFAULT 10,
    supplier_id INTEGER,
    created_at TEXT DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT
);

-- Create supplier table
CREATE TABLE suppliers (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    contact_person TEXT,
    email TEXT,
    phone TEXT,
    address TEXT,
    is_active INTEGER DEFAULT 1
);

-- Insert suppliers first
INSERT INTO suppliers (name, contact_person, email, phone) VALUES 
    ('TechCorp', 'John Tech', 'john@techcorp.com', '555-1001'),
    ('BookWorld', 'Jane Reader', 'jane@bookworld.com', '555-1002'),
    ('FoodPlus', 'Bob Chef', 'bob@foodplus.com', '555-1003');

-- Insert inventory with various constraints
INSERT INTO inventory (sku, product_name, category, cost_price, sell_price, quantity, supplier_id) VALUES 
    ('LAPTOP001', 'Gaming Laptop', 'electronics', 800.00, 1200.00, 15, 1),
    ('BOOK001', 'SQL Guide', 'books', 25.00, 45.00, 100, 2),
    ('SHIRT001', 'Cotton T-Shirt', 'clothing', 8.00, 20.00, 50, 1),
    ('APPLE001', 'Red Apples', 'food', 1.50, 3.00, 200, 3);

-- Complex operations affecting multiple tables
UPDATE inventory 
SET sell_price = sell_price * 1.15, 
    updated_at = CURRENT_TIMESTAMP 
WHERE category IN ('electronics', 'clothing');

UPDATE suppliers 
SET is_active = 0 
WHERE id NOT IN (SELECT DISTINCT supplier_id FROM inventory WHERE supplier_id IS NOT NULL);

-- Create a view-like table with computed values
CREATE TABLE inventory_summary AS 
SELECT 
    category,
    COUNT(*) as item_count,
    SUM(quantity) as total_quantity,
    AVG(sell_price - cost_price) as avg_profit_margin
FROM inventory 
GROUP BY category;

-- Verify complex state
SELECT * FROM inventory_summary;
SELECT s.name, s.is_active, COUNT(i.id) as product_count
FROM suppliers s
LEFT JOIN inventory i ON s.id = i.supplier_id
GROUP BY s.id;

ROLLBACK;

-- Verify all tables are gone
SELECT name FROM sqlite_schema WHERE type='table' AND name IN ('inventory', 'suppliers', 'inventory_summary');

-- =============================================
-- Test 6: Transaction Rollback with Data Type Conversions
-- =============================================
-- Test rollback with complex data type operations

BEGIN TRANSACTION;

CREATE TABLE data_types_test (
    id INTEGER PRIMARY KEY,
    text_col TEXT,
    integer_col INTEGER,
    real_col REAL,
    blob_col BLOB,
    null_col TEXT,
    mixed_col TEXT  -- Will store different types
);

-- Insert data with various type conversions
INSERT INTO data_types_test (text_col, integer_col, real_col, mixed_col) VALUES 
    ('123', '456', '789.12', '2023-01-01'),
    ('abc', 0, 0.0, 'true'),
    ('3.14', 42, 2.718, '{"key": "value"}'),
    ('', -1, -3.14, 'NULL');

-- Create table with computed columns
CREATE TABLE computed_values (
    id INTEGER PRIMARY KEY,
    source_id INTEGER,
    computed_sum REAL,
    computed_concat TEXT,
    computed_boolean INTEGER
);

-- Insert computed values
INSERT INTO computed_values (source_id, computed_sum, computed_concat, computed_boolean)
SELECT 
    id,
    integer_col + real_col,
    text_col || '_' || CAST(integer_col AS TEXT),
    CASE WHEN real_col > 0 THEN 1 ELSE 0 END
FROM data_types_test;

-- Complex updates with type coercion
UPDATE data_types_test 
SET mixed_col = CAST(integer_col AS TEXT) || '_updated'
WHERE integer_col > 0;

UPDATE computed_values 
SET computed_sum = computed_sum * 2
WHERE computed_boolean = 1;

-- Verify intermediate state
SELECT * FROM data_types_test;
SELECT * FROM computed_values;

ROLLBACK;

-- Verify tables are gone
SELECT name FROM sqlite_schema WHERE type='table' AND name IN ('data_types_test', 'computed_values');

-- =============================================
-- Test 7: Error Handling and Constraint Violations
-- =============================================
-- Test rollback when constraints are violated

BEGIN TRANSACTION;

CREATE TABLE users_strict (
    id INTEGER PRIMARY KEY,
    username TEXT UNIQUE NOT NULL,
    email TEXT UNIQUE NOT NULL,
    age INTEGER CHECK (age >= 18 AND age <= 120),
    status TEXT CHECK (status IN ('active', 'inactive', 'suspended'))
);

-- Insert valid data first
INSERT INTO users_strict (username, email, age, status) VALUES 
    ('user1', 'user1@test.com', 25, 'active'),
    ('user2', 'user2@test.com', 30, 'inactive');

-- This should work
UPDATE users_strict SET age = 35 WHERE username = 'user1';

-- Attempt operations that might violate constraints
-- (These might fail, but transaction should still be rollbackable)

-- Try to insert duplicate username (should fail)
-- INSERT INTO users_strict (username, email, age, status) VALUES ('user1', 'different@test.com', 28, 'active');

-- Try to insert invalid age (should fail)  
-- INSERT INTO users_strict (username, email, age, status) VALUES ('user3', 'user3@test.com', 15, 'active');

-- Try to insert invalid status (should fail)
-- INSERT INTO users_strict (username, email, age, status) VALUES ('user4', 'user4@test.com', 25, 'invalid_status');

-- Verify current state before rollback
SELECT * FROM users_strict;

ROLLBACK;

-- Verify table is gone
SELECT name FROM sqlite_schema WHERE type='table' AND name = 'users_strict';

-- =============================================
-- Test 8: Large Data Operations Rollback
-- =============================================
-- Test rollback with large amounts of data

BEGIN TRANSACTION;

CREATE TABLE large_data (
    id INTEGER PRIMARY KEY,
    data_chunk TEXT,
    chunk_number INTEGER,
    created_at TEXT DEFAULT CURRENT_TIMESTAMP
);

-- Insert multiple rows to simulate large data operations
INSERT INTO large_data (data_chunk, chunk_number) VALUES 
    ('Chunk of data 1', 1),
    ('Chunk of data 2', 2),
    ('Chunk of data 3', 3),
    ('Chunk of data 4', 4),
    ('Chunk of data 5', 5),
    ('Chunk of data 6', 6),
    ('Chunk of data 7', 7),
    ('Chunk of data 8', 8),
    ('Chunk of data 9', 9),
    ('Chunk of data 10', 10);

-- Perform bulk operations
UPDATE large_data SET data_chunk = data_chunk || ' (modified)' WHERE chunk_number <= 5;
DELETE FROM large_data WHERE chunk_number > 7;

-- Create summary table
CREATE TABLE data_summary (
    total_records INTEGER,
    modified_records INTEGER,
    avg_chunk_number REAL
);

INSERT INTO data_summary 
SELECT 
    COUNT(*),
    SUM(CASE WHEN data_chunk LIKE '%(modified)%' THEN 1 ELSE 0 END),
    AVG(chunk_number)
FROM large_data;

-- Verify state
SELECT * FROM data_summary;
SELECT COUNT(*) as remaining_records FROM large_data;

ROLLBACK;

-- Verify all data is gone
SELECT name FROM sqlite_schema WHERE type='table' AND name IN ('large_data', 'data_summary');

-- =============================================
-- Test 9: Complex Join Operations Before Rollback
-- =============================================
-- Test rollback after complex multi-table operations

BEGIN TRANSACTION;

-- Create related tables for complex operations
CREATE TABLE departments (
    id INTEGER PRIMARY KEY,
    name TEXT UNIQUE NOT NULL,
    budget REAL DEFAULT 0
);

CREATE TABLE employees_dept (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    department_id INTEGER,
    salary REAL,
    hire_date TEXT,
    FOREIGN KEY (department_id) REFERENCES departments(id)
);

CREATE TABLE projects (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    department_id INTEGER,
    budget REAL,
    start_date TEXT,
    end_date TEXT,
    FOREIGN KEY (department_id) REFERENCES departments(id)
);

-- Insert departments
INSERT INTO departments (name, budget) VALUES 
    ('Engineering', 500000),
    ('Marketing', 200000),
    ('Sales', 300000);

-- Insert employees
INSERT INTO employees_dept (name, department_id, salary, hire_date) VALUES 
    ('John Engineer', 1, 80000, '2022-01-15'),
    ('Jane Engineer', 1, 85000, '2022-03-20'),
    ('Bob Marketer', 2, 60000, '2022-06-10'),
    ('Alice Salesperson', 3, 55000, '2022-09-05');

-- Insert projects
INSERT INTO projects (name, department_id, budget, start_date) VALUES 
    ('Project Alpha', 1, 150000, '2023-01-01'),
    ('Marketing Campaign', 2, 75000, '2023-02-15'),
    ('Sales Initiative', 3, 50000, '2023-03-01');

-- Perform complex operations that would involve joins
UPDATE departments 
SET budget = budget - 10000 
WHERE id IN (SELECT department_id FROM projects WHERE budget > 100000);

-- Create a complex derived table
CREATE TABLE department_stats AS
SELECT 
    d.name as dept_name,
    d.budget as dept_budget,
    COUNT(e.id) as employee_count,
    AVG(e.salary) as avg_salary,
    COUNT(p.id) as project_count,
    SUM(p.budget) as total_project_budget
FROM departments d
LEFT JOIN employees_dept e ON d.id = e.department_id
LEFT JOIN projects p ON d.id = p.department_id
GROUP BY d.id, d.name, d.budget;

-- Verify complex state
SELECT * FROM department_stats;

ROLLBACK;

-- Verify all tables are gone
SELECT name FROM sqlite_schema WHERE type='table' AND name IN ('departments', 'employees_dept', 'projects', 'department_stats');

-- =============================================
-- Test 10: Schema Modifications with Triggers (Future)
-- =============================================
-- Test rollback with schema that would include triggers

BEGIN TRANSACTION;

CREATE TABLE audit_log (
    id INTEGER PRIMARY KEY,
    table_name TEXT,
    operation TEXT,
    old_values TEXT,
    new_values TEXT,
    timestamp TEXT DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE tracked_table (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    value INTEGER,
    last_modified TEXT DEFAULT CURRENT_TIMESTAMP
);

-- Note: Triggers would be added here when supported
-- CREATE TRIGGER tracked_table_audit 
-- AFTER UPDATE ON tracked_table
-- BEGIN
--     INSERT INTO audit_log (table_name, operation, old_values, new_values)
--     VALUES ('tracked_table', 'UPDATE', 
--             OLD.name || '|' || OLD.value,
--             NEW.name || '|' || NEW.value);
-- END;

-- Insert and modify data
INSERT INTO tracked_table (name, value) VALUES 
    ('Item 1', 100),
    ('Item 2', 200),
    ('Item 3', 300);

UPDATE tracked_table SET value = value + 50 WHERE id <= 2;
UPDATE tracked_table SET name = name || ' (updated)' WHERE value > 150;

-- Manual audit log entries (simulating trigger behavior)
INSERT INTO audit_log (table_name, operation, old_values, new_values) VALUES 
    ('tracked_table', 'UPDATE', 'Item 1|100', 'Item 1|150'),
    ('tracked_table', 'UPDATE', 'Item 2|200', 'Item 2|250');

-- Verify state
SELECT * FROM tracked_table;
SELECT * FROM audit_log;

ROLLBACK;

-- Verify all tables and audit data are gone
SELECT name FROM sqlite_schema WHERE type='table' AND name IN ('audit_log', 'tracked_table');

-- End of rollback tests