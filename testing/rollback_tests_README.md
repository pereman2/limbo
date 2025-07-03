# Rollback Tests for Limbo Database

This directory contains comprehensive rollback tests designed to validate transaction rollback functionality with complex schemas and intricate operations, without using indexes or virtual tables.

## Test Files

### 1. `rollback_schema_tests.sql`
Contains 10 comprehensive test scenarios covering:

- **Basic Schema Rollback**: Simple table creation and rollback
- **Mixed Schema and Data Rollback**: Combined DDL and DML operations
- **Complex Column Modifications**: Multi-table schemas with constraints
- **Multiple Table Operations with Constraints**: Foreign keys and check constraints
- **Nested Transactions with Schema Changes**: Complex hierarchical operations
- **Transaction Rollback with Data Type Conversions**: Type coercion and conversions
- **Error Handling and Constraint Violations**: Constraint enforcement testing
- **Large Data Operations Rollback**: Bulk data operations
- **Complex Join Operations Before Rollback**: Multi-table joins and aggregations
- **Schema Modifications with Triggers**: Future trigger support preparation

### 2. `rollback_edge_cases.sql`
Contains 5 advanced test scenarios focusing on edge cases:

- **Cascading Operations Rollback**: Parent-child relationships and cascading updates
- **Complex Subquery Operations Rollback**: Nested queries and derived tables
- **Recursive Data Structures Rollback**: Self-referencing tables and hierarchies
- **Complex Aggregation and Window Function Rollback**: Analytical queries and time series
- **Complex Data Transformation Rollback**: Survey data analysis and transformations

## Test Coverage

### Schema Operations
- `CREATE TABLE` with various column types and constraints
- Foreign key relationships
- Check constraints
- Default values and generated columns
- Self-referencing tables (recursive structures)
- Complex table hierarchies

### Data Operations
- `INSERT` with various data types
- `UPDATE` with complex WHERE clauses and subqueries
- `DELETE` with cascading effects
- Bulk data operations
- Multi-table operations
- Complex joins and aggregations

### Advanced Features
- Computed/derived tables (`CREATE TABLE AS SELECT`)
- Recursive Common Table Expressions (CTEs)
- Window functions simulation
- Complex analytical queries
- Data type conversions and coercion
- Hierarchical data processing
- Time series data analysis

### Constraint Types Tested
- Primary keys
- Unique constraints
- Foreign key constraints
- Check constraints
- NOT NULL constraints
- DEFAULT value constraints
- Generated columns (where supported)

## Test Philosophy

### No Indexes or Virtual Tables
These tests intentionally avoid:
- CREATE INDEX statements
- Virtual table creation
- External data sources

### Focus Areas
- **Schema Complexity**: Multi-table relationships with constraints
- **Data Intricacy**: Complex data transformations and calculations
- **Transaction Scope**: Operations that span multiple tables and affect related data
- **Error Scenarios**: Constraint violations and edge cases
- **Performance Impact**: Large data sets and complex operations

## Usage Instructions

### Prerequisites
1. Rollback functionality must be implemented in the database engine
2. Transaction support (BEGIN, COMMIT, ROLLBACK) must be functional
3. Basic DDL and DML operations must be supported

### Running the Tests

#### Option 1: Manual Execution
```bash
# Run basic schema tests
limbo < testing/rollback_schema_tests.sql

# Run edge case tests  
limbo < testing/rollback_edge_cases.sql
```

#### Option 2: Automated Testing Framework
```bash
# When integrated into the test framework
cargo test rollback_tests
```

### Expected Behavior

Each test follows this pattern:
1. `BEGIN TRANSACTION`
2. Perform complex schema and data operations
3. Verify intermediate state (optional)
4. `ROLLBACK`
5. Verify that all changes are completely undone

### Verification Steps

After each `ROLLBACK`, the tests verify:
1. All created tables are removed from `sqlite_schema`
2. No residual data exists
3. Schema state is restored to pre-transaction state
4. No orphaned constraints or relationships remain

## Test Scenarios Explained

### Basic to Advanced Progression
The tests are ordered from simple to complex:

1. **Single table operations** → **Multi-table operations**
2. **Simple data types** → **Complex data transformations**
3. **Basic constraints** → **Complex relationships**
4. **Linear operations** → **Recursive and hierarchical structures**
5. **Simple queries** → **Advanced analytical operations**

### Real-World Scenarios
Tests simulate realistic use cases:
- E-commerce systems (products, orders, customers)
- HR systems (employees, departments, organizational hierarchy)
- Financial systems (accounts, transactions, customers)
- Analytics systems (time series, metrics, reporting)
- Survey systems (responses, demographics, analysis)

### Edge Cases Covered
- Self-referencing foreign keys
- Circular dependencies (where possible)
- Complex calculated fields
- Nested aggregations
- Hierarchical data structures
- Time-based data analysis
- Cross-table correlations

## Implementation Notes

### Transaction Isolation
Tests assume proper transaction isolation where:
- Changes within a transaction are visible to subsequent operations in the same transaction
- Changes are completely rolled back on ROLLBACK
- No partial rollback occurs

### Memory and Performance Considerations
- Tests include scenarios with multiple tables and significant data
- Rollback implementation should handle complex dependency graphs
- Tests verify that rollback is complete and doesn't leave partial state

### Error Handling
- Some tests include commented-out operations that would cause constraint violations
- These can be uncommented to test error handling during rollback
- Rollback should work even after constraint violations

## Future Enhancements

### When Additional Features Are Available
- **Triggers**: Uncomment trigger-related tests
- **Views**: Add view creation/rollback tests
- **Stored Procedures**: Add procedure rollback tests
- **User-Defined Functions**: Add UDF rollback tests

### Additional Test Categories
- **Savepoint Tests**: Partial rollback scenarios
- **Nested Transaction Tests**: When supported
- **Concurrent Transaction Tests**: Multi-user scenarios
- **Large Scale Tests**: Performance and memory stress tests

## Troubleshooting

### Common Issues
1. **Incomplete Rollback**: Check for lingering entries in `sqlite_schema`
2. **Memory Leaks**: Monitor memory usage during large data tests
3. **Constraint Violations**: Ensure proper dependency handling
4. **Performance Issues**: Check rollback performance on large datasets

### Debugging Tips
1. Run tests individually to isolate issues
2. Add intermediate `SELECT` statements to verify state
3. Check foreign key constraint enforcement
4. Verify that all table dependencies are properly handled

## Contributing

When adding new rollback tests:
1. Follow the existing naming convention
2. Include verification steps after each ROLLBACK
3. Add comments explaining the test scenario
4. Test both success and failure cases
5. Ensure tests don't rely on indexes or virtual tables
6. Include realistic data scenarios
7. Test constraint enforcement properly

## Integration with CI/CD

These tests should be integrated into:
1. **Unit Tests**: Individual transaction scenarios
2. **Integration Tests**: Multi-component rollback scenarios  
3. **Performance Tests**: Large data rollback scenarios
4. **Stress Tests**: Complex schema rollback scenarios
5. **Regression Tests**: Ensure rollback continues to work as features are added