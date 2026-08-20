//! Reusable presentation primitives, editing mechanics, common widgets, and theme tokens.
//!
//! Dependency direction is one-way: application features may compose this UI layer, while this
//! layer remains independent of feature state, orchestration, and product-specific entities.
//! Stable model value types may be used when a reusable control needs them.

pub mod components;
pub mod theme;
