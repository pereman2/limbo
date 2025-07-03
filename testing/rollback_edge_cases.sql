-- Advanced Rollback Edge Cases and Complex Scenarios
-- Tests for intricate rollback situations and edge cases
-- without using indexes or virtual tables

-- =============================================
-- Test 1: Cascading Operations Rollback
-- =============================================
-- Test rollback with operations that affect multiple related tables

BEGIN TRANSACTION;

-- Create parent-child relationship tables
CREATE TABLE categories (
    id INTEGER PRIMARY KEY,
    name TEXT UNIQUE NOT NULL,
    parent_category_id INTEGER,
    description TEXT,
    is_active INTEGER DEFAULT 1,
    FOREIGN KEY (parent_category_id) REFERENCES categories(id)
);

CREATE TABLE products_advanced (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    category_id INTEGER NOT NULL,
    subcategory_id INTEGER,
    price REAL CHECK (price >= 0),
    cost REAL CHECK (cost >= 0),
    margin_percent REAL GENERATED ALWAYS AS ((price - cost) / NULLIF(cost, 0) * 100) STORED,
    stock_quantity INTEGER DEFAULT 0,
    min_stock_level INTEGER DEFAULT 5,
    created_date TEXT DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (category_id) REFERENCES categories(id),
    FOREIGN KEY (subcategory_id) REFERENCES categories(id)
);

-- Insert hierarchical categories
INSERT INTO categories (id, name, parent_category_id, description) VALUES 
    (1, 'Electronics', NULL, 'Electronic devices and accessories'),
    (2, 'Computers', 1, 'Desktop and laptop computers'),
    (3, 'Laptops', 2, 'Portable computers'),
    (4, 'Gaming Laptops', 3, 'High-performance gaming laptops'),
    (5, 'Clothing', NULL, 'Apparel and accessories'),
    (6, 'Mens Clothing', 5, 'Clothing for men'),
    (7, 'Shirts', 6, 'Various types of shirts');

-- Insert products with complex relationships
INSERT INTO products_advanced (name, category_id, subcategory_id, price, cost, stock_quantity) VALUES 
    ('Gaming Laptop Pro', 1, 4, 1500.00, 1200.00, 10),
    ('Office Laptop', 1, 3, 800.00, 600.00, 25),
    ('Desktop Computer', 1, 2, 1200.00, 900.00, 15),
    ('Cotton T-Shirt', 5, 7, 25.00, 12.00, 100),
    ('Dress Shirt', 5, 7, 45.00, 25.00, 50);

-- Complex cascading updates
UPDATE categories SET is_active = 0 WHERE id = 3; -- Deactivate Laptops category
UPDATE products_advanced SET price = price * 0.9 WHERE subcategory_id = 3; -- Discount laptops
UPDATE products_advanced SET stock_quantity = stock_quantity - 1 
WHERE category_id IN (SELECT id FROM categories WHERE is_active = 0);

-- Create derived tables based on the complex relationships
CREATE TABLE category_summary AS
SELECT 
    c1.name as main_category,
    c2.name as subcategory,
    COUNT(p.id) as product_count,
    AVG(p.price) as avg_price,
    SUM(p.stock_quantity) as total_stock
FROM categories c1
LEFT JOIN categories c2 ON c1.id = c2.parent_category_id
LEFT JOIN products_advanced p ON c2.id = p.subcategory_id
WHERE c1.parent_category_id IS NULL
GROUP BY c1.id, c2.id;

-- Verify complex state
SELECT * FROM category_summary;

ROLLBACK;

-- Verify all cascading changes are rolled back
SELECT name FROM sqlite_schema WHERE type='table' AND name IN ('categories', 'products_advanced', 'category_summary');

-- =============================================
-- Test 2: Complex Subquery Operations Rollback
-- =============================================
-- Test rollback with complex subqueries and derived data

BEGIN TRANSACTION;

CREATE TABLE sales_data (
    id INTEGER PRIMARY KEY,
    salesperson_id INTEGER,
    customer_id INTEGER,
    product_id INTEGER,
    sale_date TEXT,
    quantity INTEGER,
    unit_price REAL,
    total_amount REAL GENERATED ALWAYS AS (quantity * unit_price) STORED,
    commission_rate REAL DEFAULT 0.05
);

CREATE TABLE salesperson (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    region TEXT,
    hire_date TEXT,
    base_salary REAL,
    is_active INTEGER DEFAULT 1
);

CREATE TABLE customers_sales (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    email TEXT UNIQUE,
    registration_date TEXT,
    customer_type TEXT CHECK (customer_type IN ('individual', 'business')),
    credit_limit REAL DEFAULT 1000.0
);

-- Insert salespeople
INSERT INTO salesperson (id, name, region, hire_date, base_salary) VALUES 
    (1, 'Alice Smith', 'North', '2022-01-15', 50000),
    (2, 'Bob Johnson', 'South', '2022-03-20', 48000),
    (3, 'Carol Davis', 'East', '2021-11-10', 52000),
    (4, 'David Wilson', 'West', '2023-01-05', 46000);

-- Insert customers
INSERT INTO customers_sales (id, name, email, registration_date, customer_type, credit_limit) VALUES 
    (1, 'TechCorp Inc', 'orders@techcorp.com', '2023-01-01', 'business', 50000),
    (2, 'John Doe', 'john.doe@email.com', '2023-02-15', 'individual', 2000),
    (3, 'SmallBiz LLC', 'contact@smallbiz.com', '2023-03-01', 'business', 10000),
    (4, 'Jane Smith', 'jane.smith@email.com', '2023-03-20', 'individual', 1500);

-- Insert complex sales data
INSERT INTO sales_data (salesperson_id, customer_id, product_id, sale_date, quantity, unit_price) VALUES 
    (1, 1, 101, '2023-04-01', 5, 200.00),
    (1, 2, 102, '2023-04-02', 2, 150.00),
    (2, 3, 101, '2023-04-03', 3, 200.00),
    (2, 4, 103, '2023-04-04', 1, 500.00),
    (3, 1, 102, '2023-04-05', 10, 150.00),
    (3, 3, 103, '2023-04-06', 2, 500.00),
    (4, 2, 101, '2023-04-07', 1, 200.00),
    (4, 4, 102, '2023-04-08', 4, 150.00);

-- Complex operations with subqueries
UPDATE customers_sales 
SET credit_limit = credit_limit + 5000 
WHERE id IN (
    SELECT customer_id 
    FROM sales_data 
    GROUP BY customer_id 
    HAVING SUM(total_amount) > 1000
);

UPDATE salesperson 
SET base_salary = base_salary + 2000 
WHERE id IN (
    SELECT salesperson_id 
    FROM sales_data 
    GROUP BY salesperson_id 
    HAVING COUNT(*) > 2
);

-- Create complex derived tables
CREATE TABLE sales_performance AS
SELECT 
    sp.name as salesperson_name,
    sp.region,
    COUNT(sd.id) as total_sales,
    SUM(sd.total_amount) as total_revenue,
    AVG(sd.total_amount) as avg_sale_amount,
    SUM(sd.total_amount * sd.commission_rate) as total_commission,
    sp.base_salary + SUM(sd.total_amount * sd.commission_rate) as total_compensation
FROM salesperson sp
LEFT JOIN sales_data sd ON sp.id = sd.salesperson_id
GROUP BY sp.id, sp.name, sp.region, sp.base_salary;

CREATE TABLE customer_analytics AS
SELECT 
    c.name as customer_name,
    c.customer_type,
    c.credit_limit,
    COUNT(sd.id) as purchase_count,
    SUM(sd.total_amount) as total_spent,
    AVG(sd.total_amount) as avg_purchase,
    MAX(sd.total_amount) as largest_purchase,
    (c.credit_limit - COALESCE(SUM(sd.total_amount), 0)) as remaining_credit
FROM customers_sales c
LEFT JOIN sales_data sd ON c.id = sd.customer_id
GROUP BY c.id, c.name, c.customer_type, c.credit_limit;

-- Verify complex state before rollback
SELECT * FROM sales_performance WHERE total_sales > 0;
SELECT * FROM customer_analytics WHERE purchase_count > 0;

ROLLBACK;

-- Verify all complex data structures are gone
SELECT name FROM sqlite_schema WHERE type='table' AND name IN ('sales_data', 'salesperson', 'customers_sales', 'sales_performance', 'customer_analytics');

-- =============================================
-- Test 3: Recursive Data Structures Rollback
-- =============================================
-- Test rollback with self-referencing tables and recursive operations

BEGIN TRANSACTION;

CREATE TABLE organizational_units (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    parent_unit_id INTEGER,
    unit_type TEXT CHECK (unit_type IN ('company', 'division', 'department', 'team')),
    budget REAL DEFAULT 0,
    head_employee_id INTEGER,
    established_date TEXT,
    is_active INTEGER DEFAULT 1,
    FOREIGN KEY (parent_unit_id) REFERENCES organizational_units(id)
);

CREATE TABLE employees_org (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    email TEXT UNIQUE,
    unit_id INTEGER,
    manager_id INTEGER,
    position TEXT,
    salary REAL,
    hire_date TEXT,
    employment_status TEXT DEFAULT 'active',
    FOREIGN KEY (unit_id) REFERENCES organizational_units(id),
    FOREIGN KEY (manager_id) REFERENCES employees_org(id)
);

-- Insert organizational hierarchy
INSERT INTO organizational_units (id, name, parent_unit_id, unit_type, budget, established_date) VALUES 
    (1, 'ACME Corporation', NULL, 'company', 10000000, '2020-01-01'),
    (2, 'Technology Division', 1, 'division', 5000000, '2020-01-01'),
    (3, 'Sales Division', 1, 'division', 3000000, '2020-01-01'),
    (4, 'Engineering Department', 2, 'department', 2000000, '2020-02-01'),
    (5, 'QA Department', 2, 'department', 500000, '2020-02-01'),
    (6, 'Backend Team', 4, 'team', 800000, '2020-03-01'),
    (7, 'Frontend Team', 4, 'team', 600000, '2020-03-01'),
    (8, 'Sales Team North', 3, 'team', 1000000, '2020-02-15'),
    (9, 'Sales Team South', 3, 'team', 800000, '2020-02-15');

-- Insert employees with manager relationships
INSERT INTO employees_org (id, name, email, unit_id, manager_id, position, salary, hire_date) VALUES 
    (1, 'CEO John Smith', 'ceo@acme.com', 1, NULL, 'Chief Executive Officer', 250000, '2020-01-01'),
    (2, 'CTO Jane Doe', 'cto@acme.com', 2, 1, 'Chief Technology Officer', 200000, '2020-01-15'),
    (3, 'VP Sales Bob Wilson', 'vpsales@acme.com', 3, 1, 'VP of Sales', 180000, '2020-01-20'),
    (4, 'Eng Manager Alice Johnson', 'alice@acme.com', 4, 2, 'Engineering Manager', 140000, '2020-02-01'),
    (5, 'QA Manager Carol Davis', 'carol@acme.com', 5, 2, 'QA Manager', 120000, '2020-02-05'),
    (6, 'Backend Lead David Brown', 'david@acme.com', 6, 4, 'Senior Backend Engineer', 110000, '2020-03-01'),
    (7, 'Frontend Lead Eva Wilson', 'eva@acme.com', 7, 4, 'Senior Frontend Engineer', 105000, '2020-03-01'),
    (8, 'Sales Manager North Mike Johnson', 'mike@acme.com', 8, 3, 'Sales Manager', 95000, '2020-02-15'),
    (9, 'Sales Manager South Lisa Davis', 'lisa@acme.com', 9, 3, 'Sales Manager', 90000, '2020-02-15');

-- Update organizational heads
UPDATE organizational_units SET head_employee_id = 1 WHERE id = 1;
UPDATE organizational_units SET head_employee_id = 2 WHERE id = 2;
UPDATE organizational_units SET head_employee_id = 3 WHERE id = 3;
UPDATE organizational_units SET head_employee_id = 4 WHERE id = 4;
UPDATE organizational_units SET head_employee_id = 5 WHERE id = 5;

-- Complex hierarchical operations
UPDATE employees_org 
SET salary = salary * 1.1 
WHERE id IN (
    SELECT head_employee_id 
    FROM organizational_units 
    WHERE unit_type IN ('division', 'department')
);

-- Budget reallocation based on team size
UPDATE organizational_units 
SET budget = budget + 50000 
WHERE id IN (
    SELECT unit_id 
    FROM employees_org 
    GROUP BY unit_id 
    HAVING COUNT(*) > 1
);

-- Create complex hierarchical analysis
CREATE TABLE org_hierarchy_analysis AS
WITH RECURSIVE unit_hierarchy AS (
    -- Base case: top-level units
    SELECT id, name, parent_unit_id, unit_type, budget, 0 as level
    FROM organizational_units 
    WHERE parent_unit_id IS NULL
    
    UNION ALL
    
    -- Recursive case: child units
    SELECT ou.id, ou.name, ou.parent_unit_id, ou.unit_type, ou.budget, uh.level + 1
    FROM organizational_units ou
    JOIN unit_hierarchy uh ON ou.parent_unit_id = uh.id
)
SELECT 
    uh.name as unit_name,
    uh.unit_type,
    uh.level,
    uh.budget,
    COUNT(e.id) as employee_count,
    AVG(e.salary) as avg_salary,
    SUM(e.salary) as total_payroll
FROM unit_hierarchy uh
LEFT JOIN employees_org e ON uh.id = e.unit_id
GROUP BY uh.id, uh.name, uh.unit_type, uh.level, uh.budget
ORDER BY uh.level, uh.name;

-- Employee hierarchy analysis
CREATE TABLE employee_hierarchy_analysis AS
WITH RECURSIVE emp_hierarchy AS (
    -- Base case: top-level managers (CEO)
    SELECT id, name, manager_id, position, salary, unit_id, 0 as level,
           CAST(name AS TEXT) as hierarchy_path
    FROM employees_org 
    WHERE manager_id IS NULL
    
    UNION ALL
    
    -- Recursive case: subordinates
    SELECT e.id, e.name, e.manager_id, e.position, e.salary, e.unit_id, eh.level + 1,
           eh.hierarchy_path || ' -> ' || e.name
    FROM employees_org e
    JOIN emp_hierarchy eh ON e.manager_id = eh.id
)
SELECT 
    name,
    position,
    level,
    salary,
    hierarchy_path,
    (SELECT ou.name FROM organizational_units ou WHERE ou.id = unit_id) as unit_name
FROM emp_hierarchy
ORDER BY level, name;

-- Verify complex hierarchical state
SELECT * FROM org_hierarchy_analysis WHERE level <= 2;
SELECT * FROM employee_hierarchy_analysis WHERE level <= 3;

ROLLBACK;

-- Verify all hierarchical structures are gone
SELECT name FROM sqlite_schema WHERE type='table' AND name IN ('organizational_units', 'employees_org', 'org_hierarchy_analysis', 'employee_hierarchy_analysis');

-- =============================================
-- Test 4: Complex Aggregation and Window Function Rollback
-- =============================================
-- Test rollback with complex analytical queries and derived tables

BEGIN TRANSACTION;

CREATE TABLE time_series_data (
    id INTEGER PRIMARY KEY,
    timestamp TEXT NOT NULL,
    metric_name TEXT NOT NULL,
    metric_value REAL,
    metric_category TEXT,
    data_source TEXT,
    quality_score REAL CHECK (quality_score >= 0 AND quality_score <= 1)
);

-- Insert time series data
INSERT INTO time_series_data (timestamp, metric_name, metric_value, metric_category, data_source, quality_score) VALUES 
    ('2023-01-01 00:00:00', 'cpu_usage', 45.5, 'system', 'server_1', 0.95),
    ('2023-01-01 01:00:00', 'cpu_usage', 52.3, 'system', 'server_1', 0.92),
    ('2023-01-01 02:00:00', 'cpu_usage', 38.7, 'system', 'server_1', 0.98),
    ('2023-01-01 00:00:00', 'memory_usage', 67.2, 'system', 'server_1', 0.89),
    ('2023-01-01 01:00:00', 'memory_usage', 71.8, 'system', 'server_1', 0.91),
    ('2023-01-01 02:00:00', 'memory_usage', 69.4, 'system', 'server_1', 0.94),
    ('2023-01-01 00:00:00', 'request_rate', 150.0, 'application', 'web_server', 1.0),
    ('2023-01-01 01:00:00', 'request_rate', 180.5, 'application', 'web_server', 0.97),
    ('2023-01-01 02:00:00', 'request_rate', 165.2, 'application', 'web_server', 0.99),
    ('2023-01-01 00:00:00', 'error_rate', 2.1, 'application', 'web_server', 0.85),
    ('2023-01-01 01:00:00', 'error_rate', 3.4, 'application', 'web_server', 0.87),
    ('2023-01-01 02:00:00', 'error_rate', 1.8, 'application', 'web_server', 0.92);

-- Create complex analytical tables with window functions and aggregations
CREATE TABLE metric_analytics AS
SELECT 
    metric_name,
    metric_category,
    data_source,
    COUNT(*) as measurement_count,
    AVG(metric_value) as avg_value,
    MIN(metric_value) as min_value,
    MAX(metric_value) as max_value,
    (MAX(metric_value) - MIN(metric_value)) as value_range,
    AVG(quality_score) as avg_quality,
    SUM(CASE WHEN quality_score > 0.9 THEN 1 ELSE 0 END) as high_quality_count,
    -- Simulating percentile calculations
    (SELECT metric_value FROM time_series_data t2 
     WHERE t2.metric_name = t1.metric_name 
     ORDER BY metric_value 
     LIMIT 1 OFFSET (COUNT(*) / 2)) as median_approx
FROM time_series_data t1
GROUP BY metric_name, metric_category, data_source
HAVING COUNT(*) > 1;

-- Create time-based analysis
CREATE TABLE temporal_analysis AS
SELECT 
    SUBSTR(timestamp, 1, 13) as hour_bucket, -- Extract hour
    metric_category,
    COUNT(DISTINCT metric_name) as unique_metrics,
    COUNT(*) as total_measurements,
    AVG(metric_value) as avg_value_per_hour,
    SUM(metric_value) as total_value_per_hour,
    AVG(quality_score) as avg_quality_per_hour
FROM time_series_data
GROUP BY SUBSTR(timestamp, 1, 13), metric_category
ORDER BY hour_bucket;

-- Create correlation analysis table
CREATE TABLE metric_correlations AS
SELECT 
    t1.metric_name as metric_1,
    t2.metric_name as metric_2,
    t1.data_source,
    COUNT(*) as common_measurements,
    AVG(t1.metric_value) as avg_metric_1,
    AVG(t2.metric_value) as avg_metric_2,
    -- Simple correlation approximation
    (AVG(t1.metric_value * t2.metric_value) - AVG(t1.metric_value) * AVG(t2.metric_value)) as covariance_approx
FROM time_series_data t1
JOIN time_series_data t2 ON t1.timestamp = t2.timestamp 
                          AND t1.data_source = t2.data_source
                          AND t1.metric_name < t2.metric_name  -- Avoid duplicates
GROUP BY t1.metric_name, t2.metric_name, t1.data_source
HAVING COUNT(*) >= 2;

-- Perform complex updates based on analytical results
UPDATE time_series_data 
SET quality_score = quality_score * 0.9 
WHERE metric_name IN (
    SELECT metric_name 
    FROM metric_analytics 
    WHERE avg_quality < 0.9
);

-- Create final summary with nested aggregations
CREATE TABLE final_summary AS
SELECT 
    'System Metrics' as category,
    (SELECT COUNT(DISTINCT metric_name) FROM metric_analytics WHERE metric_category = 'system') as unique_metrics,
    (SELECT AVG(avg_value) FROM metric_analytics WHERE metric_category = 'system') as overall_avg,
    (SELECT SUM(measurement_count) FROM metric_analytics WHERE metric_category = 'system') as total_measurements
UNION ALL
SELECT 
    'Application Metrics' as category,
    (SELECT COUNT(DISTINCT metric_name) FROM metric_analytics WHERE metric_category = 'application') as unique_metrics,
    (SELECT AVG(avg_value) FROM metric_analytics WHERE metric_category = 'application') as overall_avg,
    (SELECT SUM(measurement_count) FROM metric_analytics WHERE metric_category = 'application') as total_measurements;

-- Verify complex analytical state
SELECT * FROM metric_analytics;
SELECT * FROM temporal_analysis;
SELECT * FROM metric_correlations;
SELECT * FROM final_summary;

ROLLBACK;

-- Verify all analytical structures are gone
SELECT name FROM sqlite_schema WHERE type='table' AND name IN ('time_series_data', 'metric_analytics', 'temporal_analysis', 'metric_correlations', 'final_summary');

-- =============================================
-- Test 5: Complex Data Transformation Rollback
-- =============================================
-- Test rollback with complex data transformations and computed columns

BEGIN TRANSACTION;

CREATE TABLE raw_survey_data (
    id INTEGER PRIMARY KEY,
    respondent_id TEXT NOT NULL,
    survey_date TEXT,
    age_group TEXT CHECK (age_group IN ('18-25', '26-35', '36-45', '46-55', '56-65', '65+')),
    gender TEXT CHECK (gender IN ('male', 'female', 'other', 'prefer_not_to_say')),
    income_bracket TEXT CHECK (income_bracket IN ('under_30k', '30k-50k', '50k-75k', '75k-100k', 'over_100k')),
    education_level TEXT CHECK (education_level IN ('high_school', 'some_college', 'bachelors', 'masters', 'doctorate')),
    satisfaction_score INTEGER CHECK (satisfaction_score >= 1 AND satisfaction_score <= 10),
    recommendation_score INTEGER CHECK (recommendation_score >= 1 AND recommendation_score <= 10),
    response_time_seconds INTEGER,
    survey_version TEXT DEFAULT 'v1.0'
);

-- Insert survey responses
INSERT INTO raw_survey_data (respondent_id, survey_date, age_group, gender, income_bracket, education_level, satisfaction_score, recommendation_score, response_time_seconds) VALUES 
    ('RESP001', '2023-01-15', '26-35', 'female', '50k-75k', 'bachelors', 8, 9, 245),
    ('RESP002', '2023-01-16', '36-45', 'male', '75k-100k', 'masters', 7, 8, 189),
    ('RESP003', '2023-01-17', '18-25', 'other', 'under_30k', 'some_college', 6, 7, 312),
    ('RESP004', '2023-01-18', '46-55', 'female', 'over_100k', 'doctorate', 9, 10, 156),
    ('RESP005', '2023-01-19', '26-35', 'male', '30k-50k', 'bachelors', 5, 6, 278),
    ('RESP006', '2023-01-20', '56-65', 'prefer_not_to_say', '50k-75k', 'high_school', 8, 8, 201),
    ('RESP007', '2023-01-21', '36-45', 'female', '75k-100k', 'masters', 7, 9, 167),
    ('RESP008', '2023-01-22', '18-25', 'male', 'under_30k', 'some_college', 4, 5, 389);

-- Create normalized/transformed data tables
CREATE TABLE demographic_segments AS
SELECT 
    age_group,
    gender,
    income_bracket,
    education_level,
    COUNT(*) as segment_size,
    AVG(satisfaction_score) as avg_satisfaction,
    AVG(recommendation_score) as avg_recommendation,
    AVG(response_time_seconds) as avg_response_time,
    -- Net Promoter Score calculation (simplified)
    (SUM(CASE WHEN recommendation_score >= 9 THEN 1 ELSE 0 END) * 100.0 / COUNT(*)) -
    (SUM(CASE WHEN recommendation_score <= 6 THEN 1 ELSE 0 END) * 100.0 / COUNT(*)) as nps_score
FROM raw_survey_data
GROUP BY age_group, gender, income_bracket, education_level
HAVING COUNT(*) >= 1;

-- Create satisfaction analysis
CREATE TABLE satisfaction_analysis AS
SELECT 
    satisfaction_score,
    recommendation_score,
    COUNT(*) as response_count,
    AVG(response_time_seconds) as avg_time_for_score,
    -- Create satisfaction categories
    CASE 
        WHEN satisfaction_score >= 8 THEN 'Highly Satisfied'
        WHEN satisfaction_score >= 6 THEN 'Satisfied' 
        WHEN satisfaction_score >= 4 THEN 'Neutral'
        ELSE 'Dissatisfied'
    END as satisfaction_category,
    CASE 
        WHEN recommendation_score >= 9 THEN 'Promoter'
        WHEN recommendation_score >= 7 THEN 'Passive'
        ELSE 'Detractor'
    END as nps_category
FROM raw_survey_data
GROUP BY satisfaction_score, recommendation_score;

-- Create complex crosstab analysis
CREATE TABLE demographic_crosstab AS
SELECT 
    age_group,
    COUNT(CASE WHEN gender = 'male' THEN 1 END) as male_count,
    COUNT(CASE WHEN gender = 'female' THEN 1 END) as female_count,
    COUNT(CASE WHEN gender = 'other' THEN 1 END) as other_count,
    COUNT(CASE WHEN gender = 'prefer_not_to_say' THEN 1 END) as no_answer_count,
    COUNT(CASE WHEN income_bracket IN ('under_30k', '30k-50k') THEN 1 END) as lower_income,
    COUNT(CASE WHEN income_bracket IN ('50k-75k', '75k-100k') THEN 1 END) as middle_income,
    COUNT(CASE WHEN income_bracket = 'over_100k' THEN 1 END) as higher_income,
    AVG(CASE WHEN education_level IN ('bachelors', 'masters', 'doctorate') THEN satisfaction_score END) as college_plus_satisfaction,
    AVG(CASE WHEN education_level IN ('high_school', 'some_college') THEN satisfaction_score END) as no_degree_satisfaction
FROM raw_survey_data
GROUP BY age_group;

-- Data quality and outlier analysis
CREATE TABLE data_quality_report AS
SELECT 
    'Response Time Outliers' as metric_type,
    COUNT(CASE WHEN response_time_seconds > 300 THEN 1 END) as outlier_count,
    COUNT(*) as total_count,
    (COUNT(CASE WHEN response_time_seconds > 300 THEN 1 END) * 100.0 / COUNT(*)) as outlier_percentage
FROM raw_survey_data
UNION ALL
SELECT 
    'Low Satisfaction Responses' as metric_type,
    COUNT(CASE WHEN satisfaction_score <= 3 THEN 1 END) as outlier_count,
    COUNT(*) as total_count,
    (COUNT(CASE WHEN satisfaction_score <= 3 THEN 1 END) * 100.0 / COUNT(*)) as outlier_percentage
FROM raw_survey_data
UNION ALL
SELECT 
    'Score Inconsistency' as metric_type,
    COUNT(CASE WHEN ABS(satisfaction_score - recommendation_score) > 3 THEN 1 END) as outlier_count,
    COUNT(*) as total_count,
    (COUNT(CASE WHEN ABS(satisfaction_score - recommendation_score) > 3 THEN 1 END) * 100.0 / COUNT(*)) as outlier_percentage
FROM raw_survey_data;

-- Complex transformations based on analysis
UPDATE raw_survey_data 
SET response_time_seconds = response_time_seconds * 0.9 
WHERE response_time_seconds > (
    SELECT AVG(response_time_seconds) + 2 * (
        SELECT AVG((response_time_seconds - sub_avg) * (response_time_seconds - sub_avg))
        FROM (SELECT AVG(response_time_seconds) as sub_avg FROM raw_survey_data) 
        CROSS JOIN raw_survey_data
    )
    FROM raw_survey_data
);

-- Final comprehensive report
CREATE TABLE final_survey_report AS
SELECT 
    (SELECT COUNT(DISTINCT respondent_id) FROM raw_survey_data) as total_respondents,
    (SELECT AVG(satisfaction_score) FROM raw_survey_data) as overall_satisfaction,
    (SELECT AVG(recommendation_score) FROM raw_survey_data) as overall_recommendation,
    (SELECT COUNT(*) FROM demographic_segments) as unique_segments,
    (SELECT AVG(nps_score) FROM demographic_segments WHERE segment_size >= 1) as weighted_nps,
    (SELECT COUNT(*) FROM satisfaction_analysis WHERE satisfaction_category = 'Highly Satisfied') as high_satisfaction_groups,
    (SELECT SUM(outlier_count) FROM data_quality_report) as total_outliers_detected;

-- Verify complex transformation state
SELECT * FROM demographic_segments;
SELECT * FROM satisfaction_analysis WHERE response_count > 0;
SELECT * FROM demographic_crosstab;
SELECT * FROM data_quality_report;
SELECT * FROM final_survey_report;

ROLLBACK;

-- Verify all transformation structures are gone
SELECT name FROM sqlite_schema WHERE type='table' AND name IN ('raw_survey_data', 'demographic_segments', 'satisfaction_analysis', 'demographic_crosstab', 'data_quality_report', 'final_survey_report');

-- End of advanced rollback edge cases