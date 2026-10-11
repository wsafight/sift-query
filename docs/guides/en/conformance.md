# Conformance

This English guide summarizes the [complete Chinese contract](../../07-BEAN-SEARCHER-CONFORMANCE.md). Switch the page language to Chinese to read the full specification.

## Product reference

Bean Searcher 4.8.12 inspired the parameter-driven read-only query model. Compatibility is an explicit set of behaviors and test mappings, not a claim that all Java behavior has been copied.

## Covered concepts

Field mappings, operator selection, list/summary queries, grouped conditions, RPC encoding, reusable mappings, named data sources, temporal/list conversion, labels and export have corresponding Rust contracts. Rust uses static metadata, typed values, bounded input and SQLx drivers.

## Intentional differences

SiftQuery rejects unknown or incompatible fields/operators and does not silently fall back to loose Java behavior. Mapping reuse occurs at compile time. Routing uses trusted context. The client cannot provide SQL fragments, tables, columns or authorization scopes.

WHERE/HAVING phase safety, projection/DISTINCT semantics, stable paging and aggregate output types follow SiftQuery's explicit contracts. Compatibility policy does not remove field allowlists, authorization or budgets.

## Evidence and limits

Read [RPC and routing](rpc-and-routing.md), [parameter protocol](protocol.md) and [query semantics](semantics.md). Cross-language conformance tests define the actual comparison scope; production and fault-matrix validation remain separate from product similarity.
