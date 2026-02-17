//! Symbol-level locking system for coordinating parallel writes.
//!
//! Provides dependency-aware locking at the symbol level:
//! - When you lock a symbol, its callers are also locked
//! - Two agents can edit different functions in the same file
//! - File-level locking still works as backward-compatible convenience
//!
//! # Example
//! ```ignore
//! let manager = LockManager::new();
//!
//! // Lock a specific symbol - also locks its callers
//! let key = SymbolKey::new("src/auth.rs", "login");
//! manager.try_acquire_symbol(&key, &graph)?;
//!
//! // ... do write operation ...
//!
//! manager.release_symbol(&key);
//! ```

pub mod guard;
pub mod manager;
pub mod types;
pub mod write;

pub use guard::LockGuard;
pub use manager::LockManager;
pub use types::{LockInfo, LockResult, LockStatus, SymbolKey};
